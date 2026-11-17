//! Independent LLVM measurement gates. Branch completeness is not MC/DC;
//! unavailable instrumentation, warnings and empty denominators reject evidence.
use crate::{
    assurance::{self, Campaign},
    instrumentation,
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub const EXCLUSIONS: &str =
    r"/(tests|examples|tools|xtask)/|/\.rustup/|/\.cargo/registry/|^/rustc/";
pub fn exclusions(run: &Run) -> Result<String> {
    let mut pattern = EXCLUSIONS.to_owned();
    // Cargo images and custom installations need not use a directory named
    // .cargo. Bind the actual home explicitly instead of widening source scope.
    if let Some(home) = run.env.get(&path("CARGO_HOME")) {
        let registry = assurance::absolute(&run.root, Path::new(home)).join("registry");
        append_exclusion(&mut pattern, &registry)?;
    }
    Ok(pattern)
}
fn append_exclusion(pattern: &mut String, directory: &Path) -> Result<()> {
    let mut roots = vec![directory.to_owned()];
    if let Ok(canonical) = dunce::canonicalize(directory) {
        roots.push(canonical);
    }
    roots.sort();
    roots.dedup();
    for root in roots {
        let value = root.to_str().context("UTF-8 excluded source path")?;
        pattern.push('|');
        for ch in value.chars() {
            if r"\.^$|?*+()[]{}".contains(ch) {
                pattern.push('\\');
            }
            pattern.push(ch);
        }
        pattern.push('/');
    }
    Ok(())
}
pub fn with_sysroot(run: &Run, sysroot: &Path) -> Result<String> {
    let mut pattern = exclusions(run)?;
    append_exclusion(&mut pattern, &sysroot.join("lib/rustlib/src/rust"))?;
    Ok(pattern)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Line,
    Branch,
    Condition,
    Mcdc,
}
impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Branch => "branch",
            Self::Condition => "condition",
            Self::Mcdc => "mcdc",
        }
    }
    pub fn flags(self) -> Vec<String> {
        let mut flags = vec!["-C".into(), "instrument-coverage".into()];
        if self != Self::Line {
            flags.extend(["-Z".into(), format!("coverage-options={}", self.name())]);
        }
        flags
    }
    pub fn metric(self) -> &'static str {
        match self {
            Self::Line => "lines",
            Self::Mcdc => "mcdc",
            _ => "branches",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    Workspace,
    Library,
}
#[derive(Debug, Args)]
pub struct Options {
    #[arg(long, value_enum, default_value = "line")]
    pub mode: Mode,
    #[arg(long)]
    pub toolchain: Option<String>,
    #[arg(long, value_enum, default_value = "workspace")]
    pub scope: Scope,
    #[arg(long)]
    pub fixture_only: bool,
    #[arg(long)]
    pub output: Option<PathBuf>,
}
pub fn metric_counts(totals: &Value, metric: &str) -> Result<(u64, u64)> {
    let value = totals.get(metric).context("missing requested metric")?;
    let count = value
        .get("count")
        .and_then(Value::as_u64)
        .context("invalid metric count")?;
    let covered = value
        .get("covered")
        .and_then(Value::as_u64)
        .context("invalid covered count")?;
    ensure!(
        count > 0 && covered <= count,
        "empty or impossible metric denominator"
    );
    Ok((count, covered))
}
pub fn export_data(exported: &Value) -> Result<&Value> {
    let data = exported
        .get("data")
        .and_then(Value::as_array)
        .context("coverage data missing")?;
    ensure!(
        data.len() == 1
            && data[0].is_object()
            && data[0].get("totals").is_some_and(Value::is_object),
        "expected exactly one merged coverage export with totals"
    );
    Ok(&data[0])
}
pub fn coverage_files(root: &Path, exported: &Value, scope: Scope) -> Result<Vec<String>> {
    let files = export_data(exported)?
        .get("files")
        .and_then(Value::as_array)
        .context("coverage files missing")?;
    ensure!(!files.is_empty(), "no measured source files");
    let prefixes: &[&str] = if scope == Scope::Library {
        &["src", "crates"]
    } else {
        &["src", "crates", "apps"]
    };
    files
        .iter()
        .map(|row| {
            let name = row
                .get("filename")
                .and_then(Value::as_str)
                .context("coverage filename missing")?;
            let resolved = crate::notices::resolve(&assurance::absolute(root, Path::new(name)))?;
            ensure!(
                prefixes.iter().any(|p| resolved.starts_with(root.join(p))),
                "coverage outside requested repository scope: {name}"
            );
            Ok(name.to_owned())
        })
        .collect()
}
pub fn validate_calibration(totals: &Value, mode: Mode, case: &str) -> Result<()> {
    let covered = match case {
        "partial" => 2,
        "branch-complete" | "complete" => 6,
        _ => anyhow::bail!("unknown calibration case"),
    };
    ensure!(
        metric_counts(totals, "branches")? == (6, covered),
        "calibration must measure exactly six branch outcomes"
    );
    if mode == Mode::Mcdc {
        let covered = match case {
            "partial" => 0,
            "branch-complete" => 2,
            _ => 3,
        };
        ensure!(
            metric_counts(totals, "mcdc")? == (3, covered),
            "calibration independent condition effects differ"
        );
    }
    Ok(())
}
pub fn llvm_diagnostics(stderr: &str) -> bool {
    stderr.to_ascii_lowercase().contains("warning:")
}
pub fn checked_llvm(
    c: &mut Campaign<'_>,
    command: Vec<std::ffi::OsString>,
    stage: &str,
) -> Result<String> {
    let result = c.command(command, stage, None, 600.);
    let stderr = fs::read(c.output.join(format!("{stage}.stderr.log"))).unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr);
    let warning = llvm_diagnostics(&stderr);
    let entry = c.evidence["commands"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    if !c.evidence["llvm_diagnostics"].is_array() {
        c.evidence["llvm_diagnostics"] = json!([]);
    }
    c.evidence["llvm_diagnostics"].as_array_mut().unwrap().push(json!({"stage":stage,"command":entry["argv"],"returncode":entry["exit_code"],"stderr":stderr}));
    if result.is_err() || warning {
        c.evidence["status"] = json!(if result.is_ok() { "degraded" } else { "failed" });
        c.save()?;
        anyhow::bail!(
            "LLVM rejected during {stage}, or emitted mapping diagnostics; measurement rejected"
        );
    }
    result
}
pub fn calibrate(
    c: &mut Campaign<'_>,
    rustc: &[std::ffi::OsString],
    flags: &[String],
    cov: &Path,
    profdata: &Path,
    mode: Mode,
) -> Result<Value> {
    let directory = c.output.join("calibration");
    fs::create_dir(&directory)?;
    let binary = directory.join("fixture");
    let fixture = c.run.root.join("tools/coverage-fixture.rs");
    let before = sha(&fixture)?;
    c.evidence["fixture_sha256"] = json!(before);
    let mut command = rustc.to_vec();
    command.extend(flags.iter().map(path));
    command.extend([path(&fixture), path("-o"), path(&binary)]);
    c.command(command, "calibration-build", None, 120.)?;
    let mut result = json!({});
    for case in ["partial", "branch-complete", "complete"] {
        let raw = directory.join(format!("{case}.profraw"));
        let merged = directory.join(format!("{case}.profdata"));
        let old = c.env.insert(path("LLVM_PROFILE_FILE"), path(&raw));
        let outcome = c.command(
            vec![path(&binary), path(case)],
            &format!("calibration-{case}"),
            None,
            120.,
        );
        match old {
            Some(v) => {
                c.env.insert(path("LLVM_PROFILE_FILE"), v);
            }
            None => {
                c.env.remove(&path("LLVM_PROFILE_FILE"));
            }
        }
        outcome?;
        checked_llvm(
            c,
            vec![
                path(profdata),
                path("merge"),
                path("-sparse"),
                path(raw),
                path("-o"),
                path(&merged),
            ],
            &format!("calibration-{case}-merge"),
        )?;
        let exported = checked_llvm(
            c,
            vec![
                path(cov),
                path("export"),
                path(&binary),
                path(format!("-instr-profile={}", merged.display())),
            ],
            &format!("calibration-{case}-export"),
        )?;
        fs::write(directory.join(format!("{case}.json")), &exported)?;
        let value: Value = serde_json::from_str(&exported)?;
        let totals = &export_data(&value)?["totals"];
        validate_calibration(totals, mode, case)?;
        result[case] = totals.clone();
    }
    ensure!(sha(&fixture)? == before, "coverage fixture changed");
    Ok(result)
}
pub fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        !options.fixture_only || options.mode != Mode::Line,
        "fixture-only requires branch/condition/MC/DC instrumentation"
    );
    let toolchain = options
        .toolchain
        .or_else(|| (options.mode != Mode::Line).then(|| "nightly".into()));
    let mut rustc = cmd(["rustc"]);
    let mut cargo = cmd(["cargo"]);
    if let Some(t) = &toolchain {
        rustc.push(path(format!("+{t}")));
        cargo.push(path(format!("+{t}")));
    }
    let output = options.output.unwrap_or_else(|| {
        PathBuf::from(if options.mode == Mode::Line {
            "target/coverage".into()
        } else {
            format!("target/coverage-{}", options.mode.name())
        })
    });
    let mut c = Campaign::new(
        run,
        &output,
        "LLVM source coverage; metrics require compiler support, calibration and warning-free exports",
    )?;
    c.evidence["mode"] = json!(options.mode.name());
    c.evidence["scope"] = json!(if options.scope == Scope::Library {
        "library"
    } else {
        "workspace"
    });
    c.evidence["status"] = json!("incomplete");
    let exclusions = exclusions(run)?;
    c.evidence["exclusions"] = json!(exclusions);
    let result = (|| {
        c.evidence["fixture_sha256"] = json!(sha(&run.root.join("tools/coverage-fixture.rs"))?);
        let flags = options.mode.flags();
        c.evidence["rustflags"] = json!(flags);
        let mut command = rustc.clone();
        command.push(path("-vV"));
        let version = c.command(command, "rustc-version", None, 30.)?;
        let host = assurance::host(&version)?.to_owned();
        c.evidence["compiler"] = json!(version);
        let mut probe = rustc.clone();
        probe.extend(flags.iter().map(path));
        probe.extend(cmd(["--print", "cfg"]));
        if let Err(e) = c.command(probe, "instrumentation-probe", None, 30.) {
            c.evidence["status"] = json!("unsupported");
            return Err(e);
        }
        let mut command = rustc.clone();
        command.extend(cmd(["--print", "sysroot"]));
        let sysroot = PathBuf::from(c.command(command, "sysroot", None, 30.)?);
        let exclusions = with_sysroot(run, &sysroot)?;
        c.evidence["exclusions"] = json!(exclusions);
        let llvm = sysroot.join("lib/rustlib").join(host).join("bin");
        let cov = llvm.join("llvm-cov");
        let profdata = llvm.join("llvm-profdata");
        ensure!(
            cov.is_file() && profdata.is_file(),
            "install matching llvm-tools-preview for selected toolchain"
        );
        let version = c.command(
            vec![path(&cov), path("--version")],
            "llvm-version",
            None,
            30.,
        )?;
        c.evidence["llvm_cov"] = json!(version);
        c.env = instrumentation::environment(&run.env, &c.output.join("build"), &flags);
        let profiles = c.output.join("profiles");
        fs::create_dir(&profiles)?;
        c.env.insert(
            path("LLVM_PROFILE_FILE"),
            path(profiles.join("%p-%m.profraw")),
        );
        if options.mode != Mode::Line {
            let calibration = calibrate(&mut c, &rustc, &flags, &cov, &profdata, options.mode)?;
            c.evidence["calibration"] = calibration;
            c.save()?;
        }
        if options.fixture_only {
            c.evidence["status"] = json!("calibrated");
            return Ok(());
        }
        let mut test = cargo.clone();
        test.extend(cmd(["test", "--offline", "--locked"]));
        test.extend(if options.scope == Scope::Library {
            cmd(["-p", "weekend-db", "--lib"])
        } else {
            cmd(["--workspace"])
        });
        test.push(path("--all-features"));
        let mut build = test.clone();
        build.extend(cmd(["--no-run", "--message-format=json"]));
        let messages = c.command(build, "test-build", None, 600.)?;
        let mut objects = std::collections::BTreeSet::new();
        for line in messages.lines() {
            let value: Value = serde_json::from_str(line)?;
            if value["reason"] == "compiler-artifact"
                && let Some(s) = value["executable"].as_str()
            {
                objects.insert(PathBuf::from(s));
            }
        }
        ensure!(!objects.is_empty(), "no executable test artifacts");
        c.command(test, "tests", None, 1800.)?;
        let raws = fs::read_dir(&profiles)?
            .map(|e| Ok(e?.path()))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|p| p.extension().is_some_and(|s| s == "profraw"))
            .collect::<Vec<_>>();
        ensure!(!raws.is_empty(), "no raw profiles");
        let merged = c.output.join("tests.profdata");
        let mut command = vec![path(&profdata), path("merge"), path("-sparse")];
        command.extend(raws.iter().map(path));
        command.extend([path("-o"), path(&merged)]);
        checked_llvm(&mut c, command, "merge")?;
        let mut args = vec![
            path(objects.first().unwrap()),
            path(format!("-instr-profile={}", merged.display())),
            path(format!("-ignore-filename-regex={exclusions}")),
        ];
        for object in objects.iter().skip(1) {
            args.extend([path("-object"), path(object)]);
        }
        let mut command = vec![path(&cov), path("report")];
        command.extend(args.clone());
        if options.mode != Mode::Line {
            command.push(path("--show-branch-summary"));
        }
        if options.mode == Mode::Mcdc {
            command.push(path("--show-mcdc-summary"));
        }
        let report = checked_llvm(&mut c, command, "report")?;
        fs::write(c.output.join("report.txt"), &report)?;
        let mut command = vec![path(&cov), path("export")];
        command.extend(args.clone());
        let exported = checked_llvm(&mut c, command, "export")?;
        fs::write(c.output.join("export.json"), &exported)?;
        let exported: Value = serde_json::from_str(&exported)?;
        let totals = &export_data(&exported)?["totals"];
        metric_counts(totals, options.mode.metric())?;
        c.evidence["totals"] = totals.clone();
        c.evidence["measured_files"] = json!(coverage_files(&run.root, &exported, options.scope)?);
        c.evidence["objects"] = json!(objects);
        let mut command = vec![path(&cov), path("show")];
        command.extend(args);
        if options.mode != Mode::Line {
            command.push(path("--show-branches=count"));
        }
        if options.mode == Mode::Mcdc {
            command.push(path("--show-mcdc"));
        }
        command.extend([
            path("-format=html"),
            path(format!("-output-dir={}", c.output.join("html").display())),
        ]);
        checked_llvm(&mut c, command, "html")?;
        c.evidence["status"] = json!("measured");
        eprintln!("{report}");
        Ok(())
    })();
    c.finish(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_cargo_home_is_literal_and_does_not_expand_product_scope() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut run = Run::new(root.to_owned());
        let home = std::env::temp_dir().join("cargo-home.with+[regex]");
        run.env.insert(path("CARGO_HOME"), path(&home));
        let pattern = exclusions(&run).unwrap();
        let re = regex::Regex::new(&pattern).unwrap();
        assert!(
            re.is_match(
                home.join("registry/src/hash/dependency/src/lib.rs")
                    .to_str()
                    .unwrap()
            )
        );
        assert!(!re.is_match(root.join("src/lib.rs").to_str().unwrap()));
        assert!(
            !re.is_match(
                home.with_file_name("cargo-home.withXregex")
                    .join("registry/src/hash/lib.rs")
                    .to_str()
                    .unwrap()
            )
        );
        run.env.insert(path("CARGO_HOME"), path("/usr/local/cargo"));
        let re = regex::Regex::new(&exclusions(&run).unwrap()).unwrap();
        assert!(re.is_match("/usr/local/cargo/registry/src/hash/block-buffer/src/lib.rs"));
        assert!(!re.is_match("/usr/local/cargo-sibling/registry/src/hash/lib.rs"));
        let re = regex::Regex::new(
            &with_sysroot(
                &run,
                Path::new("/usr/local/rustup/toolchains/custom+[nightly]"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(re.is_match("/usr/local/rustup/toolchains/custom+[nightly]/lib/rustlib/src/rust/library/std/src/thread_local.rs"));
        assert!(!re.is_match("/usr/local/rustup/toolchains/customXnightly/lib/rustlib/src/rust/library/std/src/thread_local.rs"));
        assert!(!re.is_match(root.join("src/lib.rs").to_str().unwrap()));
    }
    fn totals(case: &str, mcdc: bool) -> Value {
        let mut out = json!({"branches":{"count":6,"covered":if case=="partial" {2} else {6}}});
        if mcdc {
            out["mcdc"] =
                json!({"count":3,"covered":match case {"partial"=>0,"branch-complete"=>2,_=>3}});
        }
        out
    }
    #[test]
    fn independent_counts_and_complete_branches_are_distinct() {
        for case in ["partial", "branch-complete", "complete"] {
            validate_calibration(&totals(case, true), Mode::Mcdc, case).unwrap();
        }
        assert!(
            validate_calibration(&totals("branch-complete", true), Mode::Mcdc, "complete").is_err()
        );
        assert!(
            validate_calibration(&totals("complete", true), Mode::Mcdc, "branch-complete").is_err()
        );
    }
    #[test]
    fn partial_mcdc_rejects_missing_or_arbitrary_counts() {
        for covered in [
            Value::Null,
            json!(-1),
            json!(1),
            json!(2),
            json!(3),
            json!(4),
            json!(true),
            json!("0"),
        ] {
            let mut t = totals("partial", true);
            t["mcdc"]["covered"] = covered;
            assert!(validate_calibration(&t, Mode::Mcdc, "partial").is_err());
        }
    }
    #[test]
    fn requested_metric_absent_or_zero_is_not_measurement() {
        for metric in ["lines", "branches", "mcdc"] {
            for value in [
                Value::Null,
                json!({}),
                json!({"count":0,"covered":0}),
                json!({"count":2,"covered":3}),
                json!({"count":true,"covered":1}),
                json!({"count":2.0,"covered":1}),
            ] {
                let mut t = json!({});
                t[metric] = value;
                assert!(metric_counts(&t, metric).is_err());
            }
        }
    }
    #[test]
    fn branch_and_condition_do_not_claim_mcdc() {
        for mode in [Mode::Branch, Mode::Condition] {
            for case in ["partial", "branch-complete", "complete"] {
                let mut t = totals(case, false);
                t["mcdc"] = json!({"count":0,"covered":0});
                validate_calibration(&t, mode, case).unwrap();
            }
        }
        assert!(validate_calibration(&totals("partial", false), Mode::Mcdc, "partial").is_err());
    }
    #[test]
    fn incomplete_branch_calibration_rejects() {
        for case in ["branch-complete", "complete"] {
            assert!(validate_calibration(&totals("partial", true), Mode::Condition, case).is_err());
        }
    }
    #[test]
    fn export_cannot_silently_ignore_other_data_sets() {
        let t = totals("complete", true);
        assert_eq!(
            export_data(&json!({"data":[{"totals":t}]})).unwrap()["totals"],
            t
        );
        for data in [
            json!([]),
            json!([null]),
            json!([{"totals":t},{"totals":t}]),
            json!([{}]),
        ] {
            assert!(export_data(&json!({"data":data})).is_err());
        }
    }
    #[test]
    fn remapped_sysroot_and_wrong_target_are_outside_measured_scope() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert!(
            regex::Regex::new(EXCLUSIONS)
                .unwrap()
                .is_match("/rustc/hash/library/std/src/thread_local.rs")
        );
        assert!(
            !regex::Regex::new(EXCLUSIONS)
                .unwrap()
                .is_match(root.join("src/lib.rs").to_str().unwrap())
        );
        let exported = |p: &Path| json!({"data":[{"totals":{},"files":[{"filename":p}]}]});
        let local = root.join("src/lib.rs");
        assert_eq!(
            coverage_files(root, &exported(&local), Scope::Library).unwrap(),
            [local.to_str().unwrap()]
        );
        for name in [
            PathBuf::from("/rustc/hash/library/std/src/thread_local.rs"),
            PathBuf::from("/outside/source.rs"),
            root.join("apps/example.rs"),
            root.join("src-sibling/stray.rs"),
        ] {
            assert!(coverage_files(root, &exported(&name), Scope::Library).is_err());
        }
        assert!(
            coverage_files(
                root,
                &json!({"data":[{"totals":{},"files":[]}]}),
                Scope::Workspace
            )
            .is_err()
        );
    }
    #[test]
    fn llvm_zero_exit_with_mapping_warning_is_degraded() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let run = Run::new(root.to_owned());
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "llvm-warning").unwrap();
        let mut c = Campaign::new(&run, &dir.path().join("evidence"), "test").unwrap();
        c.evidence["status"] = json!("incomplete");
        assert!(
            checked_llvm(
                &mut c,
                cmd([
                    "/bin/sh",
                    "-c",
                    r"printf 'untrusted report'; printf '\033[0;31mwarning: mismatched data\n' >&2"
                ]),
                "report"
            )
            .is_err()
        );
        assert_eq!(c.evidence["status"], "degraded");
        assert!(
            c.evidence["llvm_diagnostics"][0]["stderr"]
                .as_str()
                .unwrap()
                .contains("warning:")
        );
        assert_eq!(
            fs::read_to_string(c.output.join("report.stdout.log")).unwrap(),
            "untrusted report"
        );
        assert_eq!(
            checked_llvm(&mut c, cmd(["/bin/sh", "-c", "printf trusted"]), "clean").unwrap(),
            "trusted"
        );
        assert!(checked_llvm(&mut c, cmd(["/bin/sh", "-c", "exit 1"]), "failed").is_err());
        assert_eq!(c.evidence["status"], "failed");
    }
    #[test]
    fn fixture_and_runner_sources_are_bound() {
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "coverage-source").unwrap();
        fs::create_dir(dir.path().join("tools")).unwrap();
        fs::create_dir_all(dir.path().join("xtask/src")).unwrap();
        fs::write(dir.path().join("tools/coverage-fixture.rs"), "first").unwrap();
        fs::write(dir.path().join("xtask/src/coverage.rs"), "runner").unwrap();
        let before = assurance::snapshot(dir.path()).unwrap();
        fs::write(dir.path().join("tools/coverage-fixture.rs"), "second").unwrap();
        assert_ne!(before, assurance::snapshot(dir.path()).unwrap());
        fs::write(dir.path().join("tools/coverage-fixture.rs"), "first").unwrap();
        assert_eq!(before, assurance::snapshot(dir.path()).unwrap());
        fs::write(dir.path().join("xtask/src/coverage.rs"), "changed").unwrap();
        assert_ne!(before, assurance::snapshot(dir.path()).unwrap());
    }
}
