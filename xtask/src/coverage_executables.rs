//! Each executed parent's profiles and object get a separate report. Child and
//! build profiles remain visible but unqualified; there is no aggregate metric.
use crate::{
    assurance::{self, Campaign},
    coverage::{self, Mode, Scope},
    instrumentation,
    support::{Run, cmd, path, sha, text},
};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum BranchMode {
    Branch,
    Condition,
}
impl BranchMode {
    fn mode(self) -> Mode {
        match self {
            Self::Branch => Mode::Branch,
            Self::Condition => Mode::Condition,
        }
    }
}
#[derive(Debug, Args)]
pub struct Options {
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value = "nightly-2026-10-09")]
    pub toolchain: String,
    #[arg(long, value_enum, default_value = "condition")]
    pub mode: BranchMode,
    #[arg(long)]
    pub select: Vec<String>,
    #[arg(long,default_value_t=180,value_parser=clap::value_parser!(u64).range(1..=1800))]
    pub timeout: u64,
}
fn libtest_target(package: &Value, target: &Value) -> Result<bool> {
    let kinds = target["kind"]
        .as_array()
        .context("target kinds")?
        .iter()
        .map(text)
        .collect::<Result<Vec<_>>>()?;
    let section = if !kinds.is_empty()
        && kinds.iter().all(|k| {
            matches!(
                *k,
                "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro"
            )
        }) {
        "lib"
    } else if kinds == ["bin"] {
        "bin"
    } else if kinds == ["test"] {
        "test"
    } else {
        return Ok(false);
    };
    let manifest: toml::Table =
        toml::from_str(&fs::read_to_string(text(&package["manifest_path"])?)?)?;
    let custom = |entry: &toml::Value| {
        entry.get("harness").and_then(toml::Value::as_bool) == Some(false)
            && (section == "lib"
                || entry.get("name").and_then(toml::Value::as_str) == target["name"].as_str())
    };
    Ok(!manifest.get(section).is_some_and(|entries| {
        if section == "lib" {
            custom(entries)
        } else {
            entries
                .as_array()
                .is_some_and(|entries| entries.iter().any(custom))
        }
    }))
}
fn artifacts(messages: &str, packages: &BTreeMap<String, Value>) -> Result<Vec<Value>> {
    let mut result = BTreeMap::new();
    for line in messages.lines() {
        let item: Value = serde_json::from_str(line)?;
        if item["reason"] != "compiler-artifact" || !item["executable"].is_string() {
            continue;
        }
        let package = packages
            .get(text(&item["package_id"])?)
            .context("unknown artifact package")?;
        let target = &item["target"];
        let eligible =
            item["profile"]["test"].as_bool() == Some(true) && libtest_target(package, target)?;
        let object = dunce::canonicalize(text(&item["executable"])?)?;
        let cwd = Path::new(text(&package["manifest_path"])?)
            .parent()
            .context("manifest parent")?;
        result.insert(object.clone(),json!({"label":format!("{}::{}",text(&package["name"])?,text(&target["name"])?),"object":object,"kind":target["kind"],"profile_test":item["profile"]["test"],"cwd":cwd,"eligible":eligible,"object_sha256":sha(&object)?}));
    }
    let mut rows: Vec<_> = result.into_values().collect();
    rows.sort_by(|a, b| {
        a["label"]
            .as_str()
            .cmp(&b["label"].as_str())
            .then_with(|| a["object"].as_str().cmp(&b["object"].as_str()))
    });
    Ok(rows)
}
fn test_count(code: Option<i64>, log: &str) -> Result<Option<u64>> {
    ensure!(code == Some(0), "test executable failed");
    let re = regex::Regex::new(r"test result: ok\. (\d+) passed; (\d+) failed;").unwrap();
    let summaries = re
        .captures_iter(log)
        .map(|m| Ok((m[1].parse::<u64>()?, m[2].parse::<u64>()?)))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !summaries.is_empty() && summaries.iter().all(|(_, failed)| *failed == 0),
        "test executable did not produce a successful libtest result"
    );
    let count = summaries.last().unwrap().0;
    Ok((count > 0).then_some(count))
}
fn profiles(directory: &Path) -> Result<Vec<Value>> {
    let mut paths = fs::read_dir(directory)?
        .map(|e| Ok(e?.path()))
        .collect::<Result<Vec<_>>>()?;
    paths.sort();
    let re = regex::Regex::new(r"^(\d+)-(.+)\.profraw$").unwrap();
    paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|s| s == "profraw"))
        .map(|p| {
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .context("UTF-8 raw profile name")?;
            let pid = re.captures(name).and_then(|m| m[1].parse::<u32>().ok());
            let hash = if fs::symlink_metadata(&p)?.file_type().is_file() {
                Some(sha(&p)?)
            } else {
                None
            };
            Ok(json!({"path":p,"pid":pid,"name_valid":pid.is_some(),"sha256":hash}))
        })
        .collect()
}
fn split_profiles(rows: &[Value], pid: u32) -> Result<(Vec<Value>, Vec<Value>)> {
    ensure!(
        rows.iter()
            .all(|r| r["name_valid"] == true && r["sha256"].is_string()),
        "Unexpected raw profile filename or nonregular profile"
    );
    let (parent, children): (Vec<_>, Vec<_>) = rows
        .iter()
        .cloned()
        .partition(|r| r["pid"].as_u64() == Some(pid as u64));
    ensure!(
        !parent.is_empty(),
        "No raw profile from the executed parent PID"
    );
    Ok((parent, children))
}
fn execute(
    c: &mut Campaign<'_>,
    item: &Value,
    index: usize,
    llvm: &Path,
    timeout: u64,
) -> Result<Value> {
    let label = format!("object-{index:03}");
    let dest = c.output.join(&label);
    fs::create_dir(&dest)?;
    let raw = dest.join("profiles");
    fs::create_dir(&raw)?;
    let mut result = item.clone();
    result["status"] = json!("incomplete");
    result["child_profiles_qualified"] = json!(false);
    let binary = PathBuf::from(text(&item["object"])?);
    let old = c
        .env
        .insert(path("LLVM_PROFILE_FILE"), path(raw.join("%p-%m.profraw")));
    let measured = (|| {
        ensure!(
            sha(&binary)? == text(&item["object_sha256"])?,
            "Executable changed before execution"
        );
        let stage = format!("{label}-tests");
        let execution = c.command_at(
            vec![path(&binary)],
            &stage,
            None,
            timeout as f64,
            Path::new(text(&item["cwd"])?),
        );
        let command = c.evidence["commands"].as_array().unwrap().last().unwrap();
        result["test_exit"] = command["exit_code"].clone();
        result["pid"] = command["supervision"]["pid"].clone();
        execution?;
        let mut log = fs::read(c.output.join(format!("{stage}.stdout.log")))?;
        log.extend(fs::read(c.output.join(format!("{stage}.stderr.log")))?);
        let Some(count) = test_count(result["test_exit"].as_i64(), &String::from_utf8_lossy(&log))?
        else {
            result["status"] = json!("unqualified-empty-tests");
            return Ok(());
        };
        result["tests_passed"] = json!(count);
        let all = profiles(&raw)?;
        let pid = result["pid"]
            .as_u64()
            .context("executed parent PID missing")?;
        let pid = u32::try_from(pid)?;
        let (parent, children) = split_profiles(&all, pid)?;
        result["parent_profiles"] = json!(parent);
        result["child_profiles_unqualified"] = json!(children);
        let merged = dest.join("parent.profdata");
        let mut command = vec![
            path(llvm.join("llvm-profdata")),
            path("merge"),
            path("-sparse"),
        ];
        command.extend(
            parent
                .iter()
                .map(|p| Ok(path(text(&p["path"])?)))
                .collect::<Result<Vec<_>>>()?,
        );
        command.extend([path("-o"), path(&merged)]);
        coverage::checked_llvm(c, command, &format!("{label}-merge"))?;
        result["profile_sha256"] = json!(sha(&merged)?);
        let cov = llvm.join("llvm-cov");
        let args = vec![
            path(&binary),
            path(format!("-instr-profile={}", merged.display())),
            path(format!(
                "-ignore-filename-regex={}",
                text(&c.evidence["exclusions"])?
            )),
        ];
        let mut command = vec![path(&cov), path("report")];
        command.extend(args.clone());
        command.push(path("--show-branch-summary"));
        let report = coverage::checked_llvm(c, command, &format!("{label}-report"))?;
        fs::write(dest.join("report.txt"), report)?;
        let mut command = vec![path(&cov), path("export")];
        command.extend(args.clone());
        let exported = coverage::checked_llvm(c, command, &format!("{label}-export"))?;
        fs::write(dest.join("export.json"), &exported)?;
        let exported: Value = serde_json::from_str(&exported)?;
        let totals = &coverage::export_data(&exported)?["totals"];
        result["totals_for_this_object_only"] = totals.clone();
        if totals["branches"]["count"].as_u64() == Some(0)
            && totals["branches"]["covered"].as_u64() == Some(0)
        {
            result["status"] = json!("unqualified-empty-production-scope");
            return Ok(());
        }
        coverage::metric_counts(totals, "branches")?;
        result["measured_files"] = json!(coverage::coverage_files(
            &c.run.root,
            &exported,
            Scope::Workspace
        )?);
        let mut command = vec![path(cov), path("show")];
        command.extend(args);
        command.extend([
            path("--show-branches=count"),
            path("-format=html"),
            path(format!("-output-dir={}", dest.join("html").display())),
        ]);
        coverage::checked_llvm(c, command, &format!("{label}-html"))?;
        ensure!(
            sha(&binary)? == text(&item["object_sha256"])?,
            "Executable changed during measurement"
        );
        result["status"] = json!("measured-parent-object");
        Ok(())
    })();
    match old {
        Some(v) => {
            c.env.insert(path("LLVM_PROFILE_FILE"), v);
        }
        None => {
            c.env.remove(&path("LLVM_PROFILE_FILE"));
        }
    }
    if let Err(e) = measured {
        result["status"] = json!("rejected");
        result["error"] = json!(format!("{e:#}"));
    }
    match profiles(&raw) {
        Ok(rows) => result["all_raw_profiles"] = json!(rows),
        Err(e) => {
            result["all_raw_profiles_error"] = json!(e.to_string());
            result["status"] = json!("rejected");
        }
    }
    fs::write(
        dest.join("evidence.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}
pub fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        assurance::absolute(&run.root, &options.output)
            .parent()
            .is_some_and(Path::is_dir),
        "output must be new with an existing parent"
    );
    let mut c = Campaign::new(
        run,
        &options.output,
        "Separate parent executable mappings; children/build profiles and unexecuted artifacts unqualified; no union or aggregate percentage",
    )?;
    c.evidence["status"] = json!("incomplete");
    c.evidence["exclusions"] = json!(coverage::exclusions(run)?);
    c.evidence["results"] = json!([]);
    let result = (|| {
        let toolchain = format!("+{}", options.toolchain);
        let rustc = cmd(["rustc", &toolchain]);
        let version = c.command(
            cmd(["rustc", &toolchain, "-vV"]),
            "rustc-version",
            None,
            30.,
        )?;
        let host = assurance::host(&version)?.to_owned();
        c.evidence["compiler"] = json!(version);
        let sysroot = c.command(
            cmd(["rustc", &toolchain, "--print", "sysroot"]),
            "sysroot",
            None,
            30.,
        )?;
        let sysroot = PathBuf::from(sysroot);
        c.evidence["exclusions"] = json!(coverage::with_sysroot(run, &sysroot)?);
        let runtime = sysroot.join("lib/rustlib").join(&host).join("lib");
        let llvm = runtime.parent().unwrap().join("bin");
        let flags = options.mode.mode().flags();
        c.evidence["flags"] = json!(flags);
        c.env = instrumentation::environment(&run.env, &c.output.join("build"), &flags);
        let loader = if host.contains("apple-darwin") {
            "DYLD_FALLBACK_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        let mut paths = vec![runtime, c.output.join("build/debug/deps")];
        if let Some(existing) = c.env.get(&path(loader)) {
            paths.extend(std::env::split_paths(existing));
        }
        c.env.insert(path(loader), std::env::join_paths(paths)?);
        let build_profiles = c.output.join("build-profiles");
        fs::create_dir(&build_profiles)?;
        c.env.insert(
            path("LLVM_PROFILE_FILE"),
            path(build_profiles.join("%p-%m.profraw")),
        );
        let version = c.command(
            vec![path(llvm.join("llvm-cov")), path("--version")],
            "llvm-version",
            None,
            30.,
        )?;
        c.evidence["llvm_cov"] = json!(version);
        let calibration = coverage::calibrate(
            &mut c,
            &rustc,
            &flags,
            &llvm.join("llvm-cov"),
            &llvm.join("llvm-profdata"),
            options.mode.mode(),
        )?;
        c.evidence["calibration"] = calibration;
        let metadata = c.command(
            cmd([
                "cargo",
                &toolchain,
                "metadata",
                "--offline",
                "--locked",
                "--no-deps",
                "--format-version=1",
            ]),
            "metadata",
            None,
            120.,
        )?;
        let metadata: Value = serde_json::from_str(&metadata)?;
        let packages = metadata["packages"]
            .as_array()
            .context("workspace packages")?
            .iter()
            .map(|p| Ok((text(&p["id"])?.to_owned(), p.clone())))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let messages = c.command(
            cmd([
                "cargo",
                &toolchain,
                "test",
                "--offline",
                "--locked",
                "--workspace",
                "--all-features",
                "--no-run",
                "--message-format=json",
            ]),
            "build",
            None,
            600.,
        )?;
        let mut inventory = artifacts(&messages, &packages)?;
        for item in &mut inventory {
            let selected = item["eligible"] == true
                && (options.select.is_empty()
                    || options
                        .select
                        .iter()
                        .any(|s| item["label"].as_str() == Some(s)));
            item["selected"] = json!(selected);
            item["unqualified_reason"] = if selected {
                Value::Null
            } else {
                json!(if item["eligible"] == true {
                    "not selected"
                } else {
                    "not an eligible libtest artifact"
                })
            };
        }
        let selected = inventory
            .iter()
            .filter(|i| i["selected"] == true)
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            !selected.is_empty()
                && options
                    .select
                    .iter()
                    .all(|s| selected.iter().any(|i| i["label"].as_str() == Some(s))),
            "selected labels did not identify eligible libtest executables"
        );
        c.evidence["artifacts"] = json!(inventory);
        for (n, item) in selected.iter().enumerate() {
            ensure!(c.unchanged()?, "Source or qualification tools changed");
            let measured = execute(&mut c, item, n, &llvm, options.timeout)?;
            c.evidence["results"].as_array_mut().unwrap().push(measured);
            c.save()?;
        }
        c.evidence["build_profiles_unqualified"] = json!(profiles(&build_profiles)?);
        ensure!(c.unchanged()?, "Source or qualification tools changed");
        let complete = c.evidence["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "measured-parent-object");
        c.evidence["status"] = json!(if complete {
            "completed-separated-reports"
        } else {
            "degraded-separated-reports"
        });
        ensure!(
            complete,
            "one or more parent reports are rejected or unqualified"
        );
        Ok(())
    })();
    c.finish(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn script(root: &Path, name: &str, contents: &str) -> PathBuf {
        let p = root.join(name);
        fs::write(&p, contents).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        p
    }
    fn campaign(root: &Path) -> Run {
        Run::new(root.to_owned())
    }
    #[test]
    fn only_libtest_artifacts_and_package_cwd() {
        let dir =
            crate::support::Scratch::new(&std::env::temp_dir(), "coverage-artifacts").unwrap();
        let manifest = dir.path().join("Cargo.toml");
        fs::write(&manifest, "[package]\nname='pkg'\n").unwrap();
        let package = json!({"name":"pkg","manifest_path":manifest});
        let mut messages = vec![];
        for (n, (kind, test)) in [
            ("bin", false),
            ("bench", true),
            ("test", true),
            ("bin", true),
        ]
        .into_iter()
        .enumerate()
        {
            let object = dir.path().join(n.to_string());
            fs::write(&object, b"object").unwrap();
            messages.push(json!({"reason":"compiler-artifact","package_id":"p","target":{"name":n.to_string(),"kind":[kind]},"profile":{"test":test},"executable":object}).to_string());
        }
        let result = artifacts(
            &messages.join("\n"),
            &BTreeMap::from([("p".into(), package)]),
        )
        .unwrap();
        assert_eq!(
            result
                .iter()
                .map(|r| r["eligible"].as_bool().unwrap())
                .collect::<Vec<_>>(),
            [false, false, true, true]
        );
        assert!(result.iter().all(|r| r["cwd"] == json!(dir.path())));
    }
    #[test]
    fn custom_harness_rejects_and_multitype_library_is_included() {
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "coverage-harness").unwrap();
        let manifest = dir.path().join("Cargo.toml");
        fs::write(&manifest, "[[test]]\nname='custom'\nharness=false\n").unwrap();
        let package = json!({"manifest_path":manifest});
        assert!(!libtest_target(&package, &json!({"kind":["test"],"name":"custom"})).unwrap());
        for kind in [json!(["rlib", "cdylib"]), json!(["proc-macro"])] {
            assert!(libtest_target(&package, &json!({"kind":kind,"name":"core"})).unwrap());
        }
    }
    #[test]
    fn exact_parent_pid_and_children_retained() {
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "coverage-pids").unwrap();
        for name in ["12-module.profraw", "123-module.profraw"] {
            fs::write(dir.path().join(name), b"profile").unwrap();
        }
        let all = profiles(dir.path()).unwrap();
        let (parent, children) = split_profiles(&all, 12).unwrap();
        assert_eq!(parent.len(), 1);
        assert_eq!(children.len(), 1);
        assert_eq!(parent[0]["pid"], 12);
        assert_eq!(children[0]["pid"], 123);
        assert!(split_profiles(&all, 99).is_err());
        fs::write(dir.path().join("invalid.profraw"), b"").unwrap();
        assert!(split_profiles(&profiles(dir.path()).unwrap(), 12).is_err());
    }
    #[test]
    fn failed_and_empty_tests_reject() {
        let good = "test result: ok. 2 passed; 0 failed; 0 ignored;";
        assert_eq!(test_count(Some(0), good).unwrap(), Some(2));
        assert_eq!(
            test_count(
                Some(0),
                &format!("test result: ok. 99 passed; 0 failed;\n{good}")
            )
            .unwrap(),
            Some(2)
        );
        assert!(test_count(Some(1), good).is_err());
        assert!(test_count(Some(0), "").is_err());
        assert_eq!(
            test_count(Some(0), &good.replace("2 passed", "0 passed")).unwrap(),
            None
        );
    }
    #[test]
    fn object_change_rejects_before_execution() {
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "coverage-object").unwrap();
        let binary = script(dir.path(), "object", "#!/bin/sh\nexit 0\n");
        let run = campaign(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap());
        let mut c = Campaign::new(&run, &dir.path().join("evidence"), "test").unwrap();
        let before = c.evidence["commands"].as_array().unwrap().len();
        let item = json!({"object":binary,"object_sha256":"old","cwd":dir.path()});
        let result = execute(&mut c, &item, 0, dir.path(), 1).unwrap();
        assert_eq!(result["status"], "rejected");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("changed before execution")
        );
        assert_eq!(c.evidence["commands"].as_array().unwrap().len(), before);
    }
    #[test]
    fn invalid_profile_retains_rejection_and_next_target_runs() {
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "coverage-invalid-profile")
            .unwrap();
        let binary = script(
            dir.path(),
            "object",
            r#"#!/bin/sh
dir=${LLVM_PROFILE_FILE%/*}
printf profile > "$dir/not-a-pid.profraw"
printf 'test result: ok. 1 passed; 0 failed;\n'
"#,
        );
        let run = campaign(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap());
        let mut c = Campaign::new(&run, &dir.path().join("evidence"), "test").unwrap();
        let item = json!({"object":binary,"object_sha256":sha(&binary).unwrap(),"cwd":dir.path()});
        for n in [0, 1] {
            let result = execute(&mut c, &item, n, dir.path(), 5).unwrap();
            let saved: Value = serde_json::from_slice(
                &fs::read(c.output.join(format!("object-{n:03}/evidence.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(result, saved);
            assert_eq!(result["status"], "rejected");
            assert!(
                result["error"]
                    .as_str()
                    .unwrap()
                    .contains("Unexpected raw profile filename")
            );
            assert_eq!(result["all_raw_profiles"][0]["pid"], Value::Null);
            assert_eq!(result["all_raw_profiles"][0]["name_valid"], false);
        }
        assert!(c.evidence["llvm_diagnostics"].is_null());
        assert_eq!(c.evidence["commands"].as_array().unwrap().len(), 4);
    }
    #[test]
    fn actual_failed_process_never_reaches_llvm() {
        let dir =
            crate::support::Scratch::new(&std::env::temp_dir(), "coverage-failed-process").unwrap();
        let binary = script(
            dir.path(),
            "object",
            "#!/bin/sh\nprintf 'test result: ok. 2 passed; 0 failed;\\n'\nexit 1\n",
        );
        let run = campaign(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap());
        let mut c = Campaign::new(&run, &dir.path().join("evidence"), "test").unwrap();
        let item = json!({"object":binary,"object_sha256":sha(&binary).unwrap(),"cwd":dir.path()});
        let result = execute(&mut c, &item, 0, dir.path(), 5).unwrap();
        assert_eq!(result["test_exit"], 1);
        assert_eq!(result["status"], "rejected");
        assert!(c.evidence["llvm_diagnostics"].is_null());
    }
}
