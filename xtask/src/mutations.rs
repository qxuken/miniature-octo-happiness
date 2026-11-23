//! Safe guard mutations in an owned source copy. Compilation failures, empty
//! selections and supervision failures never count as a detected mutant.
use crate::support::{Run, sha};
#[cfg(unix)]
use crate::support::{cmd, path};
#[cfg(unix)]
use crate::{
    assurance::{self, Campaign},
    instrumentation,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};
#[cfg(unix)]
const FIXTURE: &str = include_str!("../tests/mutations.json");
#[cfg(windows)]
const FIXTURE: &str = include_str!("../tests/mutations-windows.json");
#[cfg(any(windows, test))]
#[path = "mutations_windows.rs"]
mod windows;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    file: PathBuf,
    old: String,
    new: String,
    test: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    exact_test: Option<String>,
    target: String,
    occurrence: usize,
    matches: Option<usize>,
    package: String,
    manifest: Option<PathBuf>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    source_commit: String,
    source_sha256: String,
    cases: Vec<Case>,
}
#[derive(Debug, Args)]
pub struct Options {
    #[arg(long = "case")]
    pub prefixes: Vec<String>,
    #[arg(long)]
    pub list: bool,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub toolchain: Option<String>,
    /// Use optimized tests on Unix (Windows already qualifies both profiles).
    #[arg(long)]
    pub release: bool,
    /// Zero-based Unix shard within the prefix-filtered registry.
    #[arg(long, default_value_t = 0)]
    pub shard_index: usize,
    /// Number of disjoint Unix shards; defaults to the complete selection.
    #[arg(long, default_value_t = 1)]
    pub shard_count: usize,
}
fn parse_inventory(fixture: &str) -> Result<Inventory> {
    let data: Inventory = serde_json::from_str(fixture)?;
    ensure!(
        !data.source_commit.is_empty() && data.source_sha256.len() == 64,
        "mutation fixture provenance"
    );
    let mut names = BTreeSet::new();
    for c in &data.cases {
        ensure!(
            names.insert(&c.name)
                && !c.name.is_empty()
                && c.name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid or duplicate mutation name"
        );
        ensure!(
            !c.file.as_os_str().is_empty()
                && c.file
                    .components()
                    .all(|p| matches!(p, Component::Normal(_)))
                && c.file.extension().is_some_and(|s| s == "rs"),
            "unsafe source path"
        );
        ensure!(
            !c.old.is_empty()
                && c.old != c.new
                && c.occurrence > 0
                && c.matches.is_none_or(|n| n > 0)
                && matches!(c.target.as_str(), "--lib" | "--test" | "--bin" | "--bins")
                && match (&c.manifest, c.package.as_str()) {
                    (None, "weekend-db" | "weekend-db-typed" | "study-peer") => true,
                    (Some(manifest), "weekend-decision-inventory") =>
                        manifest == Path::new("tools/decision-inventory/Cargo.toml"),
                    _ => false,
                }
                && !c.test.is_empty(),
            "invalid mutation contract"
        );
        ensure!(
            c.exact_test.as_deref().is_none_or(|name| {
                c.target == "--test"
                    && name.split("::").all(|part| {
                        !part.is_empty()
                            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
            }),
            "invalid exact integration test for {}",
            c.name
        );
    }
    ensure!(!data.cases.is_empty(), "empty mutation inventory");
    Ok(data)
}
fn inventory() -> Result<Inventory> {
    parse_inventory(FIXTURE)
}
fn apply(case: &Case, original: &str) -> Result<String> {
    let marker = "#[cfg(test)]\nmod ";
    // An external `mod tests;` does not end production source. Only an inline
    // test module owns the remaining body in the supported source layout.
    let boundary = original.match_indices(marker).find_map(|(at, _)| {
        let tail = &original[at + marker.len()..];
        tail.find(['{', ';'])
            .filter(|end| tail.as_bytes()[*end] == b'{')
            .map(|_| at)
    });
    let (production, tests) = boundary.map_or((original, None), |at| {
        (&original[..at], Some(&original[at + marker.len()..]))
    });
    let parts: Vec<_> = production.split(&case.old).collect();
    let expected = case
        .matches
        .unwrap_or(if case.file == Path::new("src/storage/mod.rs") {
            2
        } else {
            1
        });
    ensure!(
        parts.len() == expected + 1 && case.occurrence < parts.len(),
        "source expression changed for {}: expected {expected}, found {}",
        case.name,
        parts.len() - 1
    );
    let n = case.occurrence;
    let mut out = parts[..n].join(&case.old);
    out.push_str(&case.new);
    out.push_str(&parts[n..].join(&case.old));
    if let Some(tests) = tests {
        out.push_str(marker);
        out.push_str(tests);
    }
    Ok(out)
}
fn command(case: &Case, toolchain: Option<&str>, release: bool) -> Vec<String> {
    let mut args = vec!["cargo".into()];
    if let Some(t) = toolchain {
        args.push(format!("+{t}"));
    }
    args.extend(
        [
            "test",
            "--offline",
            "--locked",
            "-p",
            &case.package,
            "--all-features",
            &case.target,
            &case.test,
        ]
        .into_iter()
        .map(str::to_owned),
    );
    if release {
        args.push("--release".into());
    }
    if let Some(manifest) = &case.manifest {
        args.extend([
            "--manifest-path".into(),
            manifest.to_string_lossy().into_owned(),
        ]);
    }
    if let Some(test) = &case.exact_test {
        args.extend([test.clone(), "--".into(), "--exact".into()]);
    }
    args
}
#[cfg(unix)]
fn expected_test_outcome(case: &Case, stdout: &[u8], outcome: &str) -> bool {
    case.exact_test.as_ref().is_none_or(|test| {
        let expected = format!("test {test} ... {outcome}");
        String::from_utf8_lossy(stdout)
            .lines()
            .any(|line| line == expected)
    })
}
#[cfg(unix)]
fn baseline(report: &crate::supervise::Report, log: &str) -> bool {
    report.success()
        && regex::Regex::new(r"(?m)^test result: ok\. ([1-9]\d*) passed; 0 failed;")
            .unwrap()
            .is_match(log)
        && !log.contains("running 0 tests")
}
#[cfg(unix)]
fn caught(report: &crate::supervise::Report, log: &str) -> bool {
    report.returncode == Some(101)
        && assurance::qualifies(report, log, Some("test result: FAILED"))
        && regex::Regex::new(r"(?m)^test result: FAILED\. \d+ passed; [1-9]\d* failed;")
            .unwrap()
            .is_match(log)
        && !log.contains("running 0 tests")
}
fn copy_source(root: &Path, output: &Path, names: &str) -> Result<Value> {
    let checkout = output.join("source");
    fs::create_dir(&checkout)?;
    let mut hashes = serde_json::Map::new();
    for name in names.split('\0').filter(|s| !s.is_empty()) {
        let relative = Path::new(name);
        ensure!(
            relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
            "unsafe Git source path"
        );
        let file = root.join(relative);
        if file.starts_with(output) {
            continue;
        }
        let meta = match fs::symlink_metadata(&file) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        ensure!(!meta.file_type().is_symlink(), "source symlink: {name}");
        #[cfg(windows)]
        ensure!(
            !crate::delivery::reparse(&meta),
            "source reparse point: {name}"
        );
        if !meta.is_file() {
            continue;
        }
        ensure!(
            dunce::canonicalize(&file)?.starts_with(root),
            "source escaped checkout"
        );
        let dest = checkout.join(relative);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::copy(&file, &dest)?;
        hashes.insert(name.into(), json!(sha(&dest)?));
    }
    Ok(Value::Object(hashes))
}
#[cfg(windows)]
pub fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        options.shard_index == 0 && options.shard_count == 1,
        "native Windows RNG controls are not sharded"
    );
    windows::run(run, options)
}
#[cfg(unix)]
fn shard(cases: &[Case], index: usize, count: usize) -> Result<Vec<Case>> {
    ensure!(count > 0 && index < count, "invalid mutation shard");
    ensure!(count <= cases.len(), "more shards than selected cases");
    Ok(cases
        .iter()
        .enumerate()
        .filter(|(i, _)| i % count == index)
        .map(|(_, case)| case.clone())
        .collect())
}
#[cfg(unix)]
pub fn run(run: &Run, options: Options) -> Result<()> {
    let inventory = inventory()?;
    let matching = inventory
        .cases
        .iter()
        .filter(|c| {
            options.prefixes.is_empty() || options.prefixes.iter().any(|p| c.name.starts_with(p))
        })
        .cloned()
        .collect::<Vec<_>>();
    ensure!(!matching.is_empty(), "no mutation cases selected");
    let selected = shard(&matching, options.shard_index, options.shard_count)?;
    if options.list {
        for c in selected {
            println!("{}", c.name);
        }
        return Ok(());
    }
    // Fail source drift before copying or compiling anything, not halfway through
    // an expensive campaign. Application below still validates the copied bytes.
    for case in &selected {
        apply(case, &fs::read_to_string(run.root.join(&case.file))?)?;
    }
    let output = options.output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "target/mutation-evidence/{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    });
    let mut c = Campaign::new(
        run,
        &output,
        "Safe guard mutants in an isolated source copy; successful nonempty baselines and genuine test failures required",
    )?;
    c.evidence["selection"] = json!({
        "prefixes":options.prefixes,
        "shard_index":options.shard_index,
        "shard_count":options.shard_count,
        "matching_cases":matching.iter().map(|case| &case.name).collect::<Vec<_>>(),
        "selected_cases":selected.iter().map(|case| &case.name).collect::<Vec<_>>()
    });
    let result = (|| {
        let mut compiler = cmd(["rustc"]);
        if let Some(t) = &options.toolchain {
            compiler.push(path(format!("+{t}")));
        }
        compiler.push(path("-vV"));
        let version = c.command(compiler, "rustc-version", None, 30.)?;
        c.evidence["rustc"] = json!(version);
        c.evidence["fixture_provenance"] = json!({"source_commit":inventory.source_commit,"source_sha256":inventory.source_sha256});
        c.evidence["profile"] = json!(if options.release { "release" } else { "debug" });
        let names = c.command(
            cmd([
                "git",
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ]),
            "git-source",
            None,
            30.,
        )?;
        c.evidence["copied_source_sha256"] = copy_source(&run.root, &c.output, &names)?;
        c.evidence["results"] = json!([]);
        c.save()?;
        let checkout = c.output.join("source");
        c.env = instrumentation::environment(&run.env, &c.output.join("build"), &[]);
        let mut baselines = BTreeSet::new();
        let mut failed = false;
        for case in selected {
            let args = command(&case, options.toolchain.as_deref(), options.release);
            if baselines.insert(args.clone()) {
                let name = format!("{}-baseline", case.name);
                c.command_at(cmd(&args), &name, None, 120., &checkout)?;
                let report: crate::supervise::Report = serde_json::from_value(
                    c.evidence["commands"].as_array().unwrap().last().unwrap()["supervision"]
                        .clone(),
                )?;
                let mut log = fs::read(c.output.join(format!("{name}.stdout.log")))?;
                let selected_passed = expected_test_outcome(&case, &log, "ok");
                log.extend(fs::read(c.output.join(format!("{name}.stderr.log")))?);
                ensure!(
                    selected_passed && baseline(&report, &String::from_utf8_lossy(&log)),
                    "baseline failed or matched no intended test for {}",
                    case.name
                );
            }
            let file = checkout.join(&case.file);
            let original = fs::read_to_string(&file)?;
            let changed = apply(&case, &original)?;
            fs::write(&file, changed)?;
            // Force this package to rebuild from the mutant even on a coarse
            // timestamp filesystem. The target directory is owned by this run.
            let mut clean = cmd(["cargo"]);
            if let Some(t) = &options.toolchain {
                clean.push(path(format!("+{t}")));
            }
            clean.extend(cmd(["clean", "--offline", "--locked", "-p", &case.package]));
            if let Some(manifest) = &case.manifest {
                clean.extend(cmd([path("--manifest-path"), path(manifest)]));
            }
            let outcome: Result<bool> = (|| {
                c.command_at(
                    clean.clone(),
                    &format!("{}-clean", case.name),
                    None,
                    120.,
                    &checkout,
                )?;
                let _ = c.command_at(cmd(&args), &case.name, None, 120., &checkout);
                let row = c.evidence["commands"].as_array().unwrap().last().unwrap();
                let report: crate::supervise::Report =
                    serde_json::from_value(row["supervision"].clone())
                        .context("mutant process report")?;
                let mut log = fs::read(c.output.join(format!("{}.stdout.log", case.name)))?;
                let selected_failed = expected_test_outcome(&case, &log, "FAILED");
                log.extend(fs::read(
                    c.output.join(format!("{}.stderr.log", case.name)),
                )?);
                let detected = selected_failed && caught(&report, &String::from_utf8_lossy(&log));
                let row = c.evidence["commands"]
                    .as_array_mut()
                    .unwrap()
                    .last_mut()
                    .unwrap();
                row["passed"] = json!(detected);
                row["expected_exit"] = json!(101);
                if let Some(test) = &case.exact_test {
                    row["expected_assertion"] = json!(format!("test {test} ... FAILED"));
                    row["expected_assertion_stream"] = json!("stdout");
                } else {
                    row["expected_assertion"] = json!("test result: FAILED");
                    row["expected_assertion_stream"] = json!("combined");
                }
                Ok(detected)
            })();
            fs::write(&file, &original)
                .with_context(|| format!("restore isolated source {}", file.display()))?;
            ensure!(
                fs::read_to_string(&file)? == original,
                "isolated source restoration failed"
            );
            c.command_at(
                clean,
                &format!("{}-restore-clean", case.name),
                None,
                120.,
                &checkout,
            )?;
            let detected = outcome?;
            let mut row = serde_json::to_value(&case)?;
            row["caught"] = json!(detected);
            row["command"] = json!(args);
            c.evidence["results"].as_array_mut().unwrap().push(row);
            c.save()?;
            failed |= !detected;
            eprintln!(
                "{}: {}",
                case.name,
                if detected { "caught" } else { "NOT CAUGHT" }
            );
        }
        for (name, hash) in c.evidence["copied_source_sha256"]
            .as_object()
            .context("copied source inventory")?
        {
            ensure!(
                sha(&checkout.join(name))? == hash.as_str().context("copied source hash")?,
                "isolated source changed after restoration: {name}"
            );
        }
        ensure!(
            !failed,
            "one or more mutants were not detected by a genuine test failure"
        );
        Ok(())
    })();
    c.finish(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(unix)]
    fn shards_cover_the_selection_once_without_reordering_or_case_changes() {
        let cases = inventory().unwrap().cases;
        let matching = cases
            .into_iter()
            .filter(|case| case.name.starts_with("r1-"))
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(shard(&matching, 0, 1).unwrap()).unwrap(),
            serde_json::to_value(&matching).unwrap()
        );
        for count in [2, 3, 7] {
            let mut seen = BTreeSet::new();
            let mut lengths = Vec::new();
            for index in 0..count {
                let selected = shard(&matching, index, count).unwrap();
                assert!(!selected.is_empty());
                lengths.push(selected.len());
                assert_eq!(
                    serde_json::to_value(&selected).unwrap(),
                    serde_json::to_value(shard(&matching, index, count).unwrap()).unwrap()
                );
                let mut previous = None;
                for case in selected {
                    let position = matching.iter().position(|c| c.name == case.name).unwrap();
                    assert!(previous.is_none_or(|p| p < position));
                    assert!(seen.insert(case.name.clone()));
                    assert_eq!(
                        serde_json::to_value(case).unwrap(),
                        serde_json::to_value(&matching[position]).unwrap()
                    );
                    previous = Some(position);
                }
            }
            assert_eq!(seen, matching.iter().map(|c| c.name.clone()).collect());
            assert!(lengths.iter().max().unwrap() - lengths.iter().min().unwrap() <= 1);
        }
    }
    #[test]
    #[cfg(unix)]
    fn invalid_and_empty_shards_reject_before_iteration() {
        let cases = inventory().unwrap().cases;
        for (index, count) in [
            (0, 0),
            (1, 1),
            (3, 3),
            (usize::MAX, 3),
            (0, cases.len() + 1),
        ] {
            assert!(shard(&cases, index, count).is_err());
        }
        assert!(shard(&[], 0, 1).is_err());
    }
    #[test]
    fn source_copy_skips_its_own_output_and_rejects_symlink_escape() {
        let root = crate::support::Scratch::new(&std::env::temp_dir(), "mutation-copy").unwrap();
        let outside =
            crate::support::Scratch::new(&std::env::temp_dir(), "mutation-outside").unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(root.path().join("src/input.rs"), "original").unwrap();
        let output = root.path().join("owned");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep.log"), "evidence").unwrap();
        let copied = copy_source(root.path(), &output, "src/input.rs\0owned/keep.log\0").unwrap();
        assert_eq!(copied.as_object().unwrap().len(), 1);
        assert_eq!(
            fs::read_to_string(output.join("source/src/input.rs")).unwrap(),
            "original"
        );
        assert!(!output.join("source/owned").exists());
        assert_eq!(
            fs::read_to_string(root.path().join("src/input.rs")).unwrap(),
            "original"
        );
        #[cfg(unix)]
        {
            fs::write(outside.path().join("secret.rs"), "unchanged").unwrap();
            std::os::unix::fs::symlink(
                outside.path().join("secret.rs"),
                root.path().join("escape.rs"),
            )
            .unwrap();
            let rejected = root.path().join("rejected");
            fs::create_dir(&rejected).unwrap();
            assert!(copy_source(root.path(), &rejected, "escape.rs\0").is_err());
            assert_eq!(
                fs::read_to_string(outside.path().join("secret.rs")).unwrap(),
                "unchanged"
            );
        }
        #[cfg(windows)]
        let _ = outside;
    }
    #[cfg(unix)]
    #[test]
    fn fixture_preserves_test_commands_and_selects_release_explicitly() {
        let data = inventory().unwrap();
        assert_eq!(data.cases.len(), 404);
        let c = &data.cases[0];
        assert_eq!(
            command(c, None, false),
            [
                "cargo",
                "test",
                "--offline",
                "--locked",
                "-p",
                "weekend-db",
                "--all-features",
                "--test",
                "allocation"
            ]
        );
        assert_eq!(command(c, Some("1.99.1"), false)[1], "+1.99.1");
        let mut optimized = command(c, None, false);
        optimized.push("--release".into());
        assert_eq!(command(c, None, true), optimized);
    }
    #[cfg(unix)]
    #[test]
    fn standalone_mutations_select_the_pinned_manifest_in_both_profiles() {
        let fixture = inventory().unwrap();
        let case = fixture
            .cases
            .iter()
            .find(|c| c.name == "r1-inventory-example-kind")
            .unwrap();
        for release in [false, true] {
            let args = command(case, Some("1.99.1"), release);
            assert_eq!(
                &args[args.len() - 2..],
                ["--manifest-path", "tools/decision-inventory/Cargo.toml"]
            );
            assert!(args.windows(2).any(|a| a == ["--test", "catalog"]));
            assert_eq!(
                args.iter().filter(|a| *a == "--release").count(),
                usize::from(release)
            );
        }
        let mut data: Value = serde_json::from_str(FIXTURE).unwrap();
        let standalone = data["cases"]
            .as_array()
            .unwrap()
            .iter()
            .position(|c| c["name"] == "r1-inventory-example-kind")
            .unwrap();
        for manifest in [Value::Null, json!("../Cargo.toml"), json!("Cargo.toml")] {
            data["cases"][standalone]["manifest"] = manifest;
            assert!(parse_inventory(&data.to_string()).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn exact_integration_selection_requires_the_named_test_outcome() {
        let inventory = inventory().unwrap();
        let mut case = inventory.cases[0].clone();
        case.exact_test = Some("module::expected".into());
        let args = command(&case, Some("1.99.1"), true);
        assert_eq!(
            &args[args.len() - 4..],
            ["--release", "module::expected", "--", "--exact"]
        );
        assert!(expected_test_outcome(
            &case,
            b"running 1 test\r\ntest module::expected ... ok\r\n",
            "ok"
        ));
        assert!(expected_test_outcome(
            &case,
            b"test module::expected ... FAILED\n",
            "FAILED"
        ));
        for log in [
            b"test module::expected_suffix ... FAILED\n".as_slice(),
            b"test module::other ... FAILED\n",
            b"panic: test module::expected ... FAILED\n",
            b"test module::expected ... ok\n",
        ] {
            assert!(!expected_test_outcome(&case, log, "FAILED"));
        }

        let mut data: Value = serde_json::from_str(FIXTURE).unwrap();
        for name in ["", "--exact", "a:b", "a::", "a\nb"] {
            data["cases"][0]["exact_test"] = json!(name);
            assert!(parse_inventory(&data.to_string()).is_err(), "{name:?}");
        }
        data["cases"][0]["exact_test"] = json!("module::expected");
        assert!(parse_inventory(&data.to_string()).is_ok());
        data["cases"][0]["target"] = json!("--lib");
        assert!(parse_inventory(&data.to_string()).is_err());
    }
    #[test]
    fn only_requested_production_occurrence_changes_and_source_drift_rejects() {
        let mut c = inventory().unwrap().cases[0].clone();
        c.old = "guard".into();
        c.new = "false".into();
        c.matches = Some(2);
        c.occurrence = 2;
        let source = "guard first guard last\n#[cfg(test)]\nmod tests { guard }";
        assert_eq!(
            apply(&c, source).unwrap(),
            "guard first false last\n#[cfg(test)]\nmod tests { guard }"
        );
        assert!(apply(&c, "guard").is_err());
        c.occurrence = 3;
        assert!(apply(&c, source).is_err());
    }
    #[test]
    fn external_test_module_does_not_hide_later_production_guards() {
        let mut c = inventory().unwrap().cases[0].clone();
        c.old = "guard".into();
        c.new = "false".into();
        c.matches = Some(1);
        c.occurrence = 1;
        let source = "#[cfg(test)]\nmod tests;\nfn production() { guard }\n#[cfg(test)]\nmod local { guard }";
        assert_eq!(
            apply(&c, source).unwrap(),
            "#[cfg(test)]\nmod tests;\nfn production() { false }\n#[cfg(test)]\nmod local { guard }"
        );
    }
    #[test]
    fn every_frozen_expression_still_matches_current_production_source() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for c in inventory().unwrap().cases {
            #[cfg(unix)]
            apply(&c, &fs::read_to_string(root.join(&c.file)).unwrap()).unwrap();
            #[cfg(windows)]
            windows::apply_native(&c, &fs::read_to_string(root.join(&c.file)).unwrap()).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn compiler_failure_empty_selection_timeout_and_cleanup_failure_are_not_detection() {
        const FAILED: &str = "test result: FAILED. 0 passed; 1 failed;";
        let mut r = crate::supervise::Report {
            returncode: Some(101),
            cleanup_verified: true,
            failure: Some("child exit Some(101)".into()),
            ..Default::default()
        };
        assert!(caught(&r, FAILED));
        assert!(!caught(&r, "error: failed to compile"));
        assert!(!caught(
            &r,
            "compiler help: test result: FAILED. 0 passed; 1 failed;"
        ));
        assert!(!caught(&r, "running 0 tests\ntest result: FAILED"));
        r.timed_out = true;
        assert!(!caught(&r, FAILED));
        r.timed_out = false;
        r.cleanup_verified = false;
        assert!(!caught(&r, FAILED));
        r = crate::supervise::Report {
            returncode: Some(0),
            cleanup_verified: true,
            ..Default::default()
        };
        assert!(baseline(&r, "test result: ok. 1 passed; 0 failed;"));
        assert!(!baseline(&r, "running 0 tests\ntest result: ok"));
        assert!(!caught(&r, FAILED));
    }
}
