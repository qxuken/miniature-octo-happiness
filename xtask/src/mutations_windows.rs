//! Native Windows mutation controls using the existing Job-owned process boundary.
use super::*;
use crate::trace_process::Report;
use sha2::Digest;
const TEST: &str =
    "storage::windows_rng::tests::identity_rejects_every_nonzero_status_before_accepting_bytes";
const COORDINATOR_CASES: &str = include_str!("../tests/mutations.json");
const COORDINATOR_ORACLE: &str = include_str!("../tests/coordinator-windows-oracle.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CoordinatorOracle {
    source_commit: String,
    source_sha256: std::collections::BTreeMap<PathBuf, String>,
    assertions: std::collections::BTreeMap<String, String>,
}

fn coordinator_oracle() -> Result<CoordinatorOracle> {
    Ok(serde_json::from_str(COORDINATOR_ORACLE)?)
}

fn coordinator_cases() -> Result<Inventory> {
    let oracle = coordinator_oracle()?;
    ensure!(
        oracle.source_commit.len() == 40
            && oracle.source_commit.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid coordinator oracle source commit"
    );
    let mut data = parse_inventory(COORDINATOR_CASES)?;
    data.cases
        .retain(|case| case.name.starts_with("r1-coordinator-"));
    ensure!(
        data.cases.len() == 32 && data.cases.len() == oracle.assertions.len(),
        "incomplete coordinator oracle"
    );
    for case in &data.cases {
        ensure!(
            case.package == "write-coordinator"
                && case.target == "--bin"
                && case.test == case.package
                && case.exact_test.is_some()
                && case.manifest.is_none()
                && case.os.is_none()
                && oracle.source_sha256.contains_key(&case.file)
                && oracle
                    .assertions
                    .get(&case.name)
                    .is_some_and(|s| !s.is_empty()),
            "coordinator mutation scope changed: {}",
            case.name
        );
    }
    Ok(data)
}

fn assertion(case: &Case) -> Result<String> {
    match case.name.as_str() {
        "windows-rng-status-negative" => Ok(
            "called `Result::unwrap_err()` on an `Ok` value: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7]".into(),
        ),
        "windows-rng-diagnostic-omission" => Ok("called `Option::unwrap()` on a `None` value".into()),
        name if name.starts_with("r1-coordinator-") => coordinator_oracle()?.assertions
            .remove(name).context("missing coordinator assertion"),
        _ => anyhow::bail!("unknown Windows mutation contract"),
    }
}
fn qualifies(report: &Report, log: &str, case: &Case, mutant: bool) -> bool {
    #[cfg(windows)]
    if report.stopped || report.timed_out || report.output_exceeded {
        return false;
    }
    let (exit, outcome) = if mutant { (101, "FAILED") } else { (0, "ok") };
    if report.exit != Some(exit) || !report.qualifies(mutant) || log.contains("running 0 tests") {
        return false;
    }
    for line in log.lines() {
        if (line.starts_with("error:") && !line.starts_with("error: test failed, to rerun pass "))
            || line.starts_with("error[")
        {
            return false;
        }
        if let Ok(row) = serde_json::from_str::<Value>(line)
            && ((row["reason"] == "build-finished" && row["success"] == false)
                || (row["reason"] == "compiler-message" && row["message"]["level"] == "error"))
        {
            return false;
        }
    }
    exact_test_result(
        log,
        case.exact_test.as_deref().unwrap_or(&case.test),
        outcome,
    ) && (!mutant
        || assertion(case)
            .is_ok_and(|needle| log.lines().filter(|line| *line == needle).count() == 1))
}
fn windows_cases() -> Result<Inventory> {
    #[cfg(windows)]
    let data = inventory()?;
    #[cfg(not(windows))]
    let data = parse_inventory(include_str!("../tests/mutations-windows.json"))?;
    ensure!(
        data.cases.len() == 2,
        "exactly two Windows RNG mutations required"
    );
    for case in &data.cases {
        assertion(case)?;
        ensure!(
            case.file == Path::new("src/storage/windows_rng.rs")
                && case.package == "weekend-db"
                && case.target == "--lib"
                && case.test == TEST
                && case.matches == Some(1)
                && case.occurrence == 1,
            "Windows RNG mutation scope changed"
        );
    }
    Ok(data)
}
fn test_command(case: &Case, toolchain: Option<&str>, release: bool) -> Vec<String> {
    let mut argv = command(case, toolchain, release);
    if let Some(separator) = argv.iter().position(|arg| arg == "--") {
        argv.insert(separator, "--message-format=json".into());
    } else {
        argv.extend(["--message-format=json", "--", "--exact"].map(str::to_owned));
    }
    argv.push("--test-threads=1".into());
    argv
}
pub(super) fn apply_native(case: &Case, original: &str) -> Result<String> {
    if original.contains("\r\n") {
        ensure!(
            !original.replace("\r\n", "").contains('\n'),
            "mixed source line endings"
        );
        Ok(apply(case, &original.replace("\r\n", "\n"))?.replace("\n", "\r\n"))
    } else {
        apply(case, original)
    }
}
fn pinned_source(data: &Inventory, case: &Case, original: &str) -> Result<()> {
    let expected = if case.package == "write-coordinator" {
        coordinator_oracle()?
            .source_sha256
            .remove(&case.file)
            .context("missing coordinator source pin")?
    } else {
        data.source_sha256.clone()
    };
    ensure!(
        format!(
            "{:x}",
            sha2::Sha256::digest(original.replace("\r\n", "\n").as_bytes())
        ) == expected,
        "Windows source changed from the reviewed fixture"
    );
    Ok(())
}
fn artifact(log: &str, target: &Path, case: &Case) -> Result<PathBuf> {
    let (name, kind) = match case.target.as_str() {
        "--bin" => (case.test.as_str(), json!(["bin"])),
        "--lib" => ("weekend_db", json!(["rlib", "cdylib"])),
        _ => anyhow::bail!("unsupported Windows test target"),
    };
    let mut paths = BTreeSet::new();
    for line in log.lines() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if row["reason"] == "compiler-artifact"
            && row["profile"]["test"] == true
            && row["target"]["name"] == name
            && row["target"]["kind"] == kind
            && let Some(executable) = row["executable"].as_str()
        {
            paths.insert(PathBuf::from(executable));
        }
    }
    ensure!(paths.len() == 1, "one actual libtest executable required");
    let file = paths.into_iter().next().unwrap();
    let meta = fs::symlink_metadata(&file)?;
    ensure!(
        meta.is_file()
            && !meta.file_type().is_symlink()
            && !crate::delivery::reparse(&meta)
            && dunce::canonicalize(&file)?.starts_with(dunce::canonicalize(target)?),
        "libtest executable escaped owned target"
    );
    Ok(file)
}
struct Restore {
    file: PathBuf,
    bytes: Vec<u8>,
}
fn input_names(root: &Path, output: &Path, names: &str) -> BTreeSet<String> {
    names
        .split('\0')
        .filter(|name| !name.is_empty() && !root.join(name).starts_with(output))
        .map(str::to_owned)
        .collect()
}
impl Restore {
    fn finish(&self) -> Result<()> {
        fs::write(&self.file, &self.bytes)?;
        ensure!(
            fs::read(&self.file)? == self.bytes,
            "isolated source restoration failed"
        );
        Ok(())
    }
}
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = fs::write(&self.file, &self.bytes);
    }
}
#[cfg(windows)]
fn invoke(
    proof: &mut Value,
    output: &Path,
    label: &str,
    argv: Vec<std::ffi::OsString>,
    cwd: &Path,
    env: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> Result<(Report, String)> {
    let stdout = output.join(format!("{label}.stdout"));
    let stderr = output.join(format!("{label}.stderr"));
    let result = crate::trace_process::run(&crate::trace_process::Request {
        argv: &argv,
        cwd,
        env,
        stdout: &stdout,
        stderr: &stderr,
        timeout: 120.,
        maximum: 4 * 1024 * 1024,
    });
    let row = match &result {
        Ok(report) => {
            json!({"argv":argv,"cwd":cwd,"report":report,"stdout_sha256":sha(&stdout).ok(),"stderr_sha256":sha(&stderr).ok()})
        }
        Err(error) => json!({"argv":argv,"cwd":cwd,"error":format!("{error:#}")}),
    };
    proof["commands"].as_array_mut().unwrap().push(row);
    crate::trace_process::write_proof(output, proof)?;
    let report = result?;
    let mut log = fs::read(stdout)?;
    log.extend(fs::read(stderr)?);
    Ok((report, String::from_utf8(log)?))
}
#[cfg(windows)]
pub(super) fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        !options.release,
        "Windows already checks debug and release; omit --release"
    );
    use crate::{
        instrumentation,
        support::{cmd, path},
    };
    let coordinator = options
        .prefixes
        .iter()
        .any(|p| p.starts_with("r1-coordinator-"));
    let data = if coordinator {
        ensure!(
            options
                .prefixes
                .iter()
                .all(|p| p.starts_with("r1-coordinator-")),
            "select a single Windows mutation registry"
        );
        coordinator_cases()?
    } else {
        windows_cases()?
    };
    let selected = data
        .cases
        .iter()
        .filter(|case| {
            options.prefixes.is_empty()
                || options
                    .prefixes
                    .iter()
                    .any(|prefix| case.name.starts_with(prefix))
        })
        .collect::<Vec<_>>();
    ensure!(!selected.is_empty(), "no mutation cases selected");
    if options.list {
        for case in selected {
            println!("{}", case.name);
        }
        return Ok(());
    }
    for case in &selected {
        let source = fs::read_to_string(run.root.join(&case.file))?;
        pinned_source(&data, case, &source)?;
        apply_native(case, &source)?;
    }
    let output = options.output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "target/mutation-evidence/windows-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    });
    let output = crate::trace_process::admission(&run.root, &output)?;
    fs::create_dir_all(output.parent().context("output parent")?)?;
    fs::create_dir(&output)?;
    let mut proof = json!({"passed":false,"native_windows_verified":false,"scope":"Selected native Windows RNG guard mutations; exact nonempty baseline and genuine test failure required in debug/release","selected":selected.iter().map(|case|&case.name).collect::<Vec<_>>(),"commands":[],"results":[],"fixture_sha256":format!("{:x}",sha2::Sha256::digest(include_bytes!("../tests/mutations-windows.json"))),"fixture_provenance":{"source_commit":data.source_commit,"source_sha256":data.source_sha256},"runner_sha256":sha(&std::env::current_exe()?)?,"controlled_environment":{"CARGO_INCREMENTAL":"0","CARGO_ENCODED_RUSTFLAGS":"","CARGO_ENCODED_RUSTDOCFLAGS":"","CARGO_TERM_COLOR":"never","CARGO_BUILD_TARGET":"removed for test commands"}});
    if coordinator {
        proof["scope"] = json!(
            "Native Windows coordinator controls with exact tests and assertion diagnostics in debug and release"
        );
        proof["fixture_sha256"] = json!(format!(
            "{:x}",
            sha2::Sha256::digest(COORDINATOR_CASES.as_bytes())
        ));
        proof["coordinator_oracle_sha256"] = json!(format!(
            "{:x}",
            sha2::Sha256::digest(COORDINATOR_ORACLE.as_bytes())
        ));
        proof["oracle_source_commit"] = json!(coordinator_oracle()?.source_commit);
    }
    crate::trace_process::write_proof(&output, &proof)?;
    let result: Result<()> = (|| {
        let mut env = instrumentation::environment(&run.env, &output.join("metadata-target"), &[]);
        env.insert(path("CARGO_TERM_COLOR"), path("never"));
        let capture = |proof: &mut Value, label, args: Vec<std::ffi::OsString>| -> Result<String> {
            let (report, log) = invoke(proof, &output, label, args, &run.root, &env)?;
            ensure!(report.qualifies(false), "metadata command failed: {label}");
            Ok(log)
        };
        proof["source"] =
            json!(capture(&mut proof, "git-head", cmd(["git", "rev-parse", "HEAD"]))?.trim());
        proof["tree"] = json!(
            capture(
                &mut proof,
                "git-tree",
                cmd(["git", "rev-parse", "HEAD^{tree}"])
            )?
            .trim()
        );
        proof["parents"] = json!(
            capture(
                &mut proof,
                "git-parents",
                cmd(["git", "show", "-s", "--format=%P", "HEAD"])
            )?
            .trim()
        );
        proof["dirty_status"] = json!(capture(
            &mut proof,
            "git-dirty",
            cmd(["git", "status", "--porcelain"])
        )?);
        let mut compiler = cmd(["rustc"]);
        if let Some(toolchain) = &options.toolchain {
            compiler.push(path(format!("+{toolchain}")));
        }
        compiler.push(path("-vV"));
        let version = capture(&mut proof, "rustc", compiler)?;
        ensure!(
            version
                .lines()
                .any(|line| line.starts_with("host: ") && line.ends_with("-pc-windows-msvc")),
            "native Windows MSVC toolchain required"
        );
        proof["rustc"] = json!(version);
        let mut cargo = cmd(["cargo"]);
        if let Some(toolchain) = &options.toolchain {
            cargo.push(path(format!("+{toolchain}")));
        }
        cargo.push(path("-vV"));
        proof["cargo"] = json!(capture(&mut proof, "cargo", cargo)?);
        let names = capture(
            &mut proof,
            "git-files",
            cmd([
                "git",
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ]),
        )?;
        let before_names = input_names(&run.root, &output, &names);
        let copied = copy_source(&run.root, &output, &names)?;
        proof["copied_source_sha256"] = copied.clone();
        let checkout = output.join("source");
        fs::create_dir(output.join("binaries"))?;
        fs::create_dir(output.join("patches"))?;
        for case in selected {
            let file = checkout.join(&case.file);
            let original = fs::read(&file)?;
            pinned_source(&data, case, std::str::from_utf8(&original)?)?;
            let restore = Restore {
                file: file.clone(),
                bytes: original.clone(),
            };
            let changed = apply_native(case, std::str::from_utf8(&original)?)?;
            fs::write(
                output
                    .join("patches")
                    .join(format!("{}.original.rs", case.name)),
                &original,
            )?;
            fs::write(
                output
                    .join("patches")
                    .join(format!("{}.mutant.rs", case.name)),
                &changed,
            )?;
            let outcome: Result<()> = (|| {
                for release in [false, true] {
                    let profile = if release { "release" } else { "debug" };
                    for mutant in [false, true] {
                        let label = format!(
                            "{}-{profile}-{}",
                            case.name,
                            if mutant { "mutant" } else { "baseline" }
                        );
                        restore.finish()?;
                        if mutant {
                            fs::write(&file, &changed)?;
                        }
                        let target = if coordinator {
                            output.join("build").join(profile)
                        } else {
                            output.join(format!("build-{label}"))
                        };
                        fs::create_dir_all(&target)?;
                        let mut env = instrumentation::environment(&run.env, &target, &[]);
                        env.insert(path("CARGO_TERM_COLOR"), path("never"));
                        env.remove(&path("CARGO_BUILD_TARGET"));
                        if coordinator {
                            let mut clean = cmd(["cargo"]);
                            if let Some(toolchain) = &options.toolchain {
                                clean.push(path(format!("+{toolchain}")));
                            }
                            clean.extend(cmd([
                                "clean",
                                "--offline",
                                "--locked",
                                "-p",
                                &case.package,
                            ]));
                            let (cleaned, _) = invoke(
                                &mut proof,
                                &output,
                                &format!("{label}-clean"),
                                clean,
                                &checkout,
                                &env,
                            )?;
                            ensure!(
                                cleaned.qualifies(false),
                                "coordinator cleanup failed: {label}"
                            );
                        }
                        let argv = test_command(case, options.toolchain.as_deref(), release);
                        let (report, log) =
                            invoke(&mut proof, &output, &label, cmd(&argv), &checkout, &env)?;
                        let executable = artifact(&log, &target, case)?;
                        let binary = output.join("binaries").join(format!("{label}.exe"));
                        fs::copy(&executable, &binary)?;
                        let passed = qualifies(&report, &log, case, mutant);
                        proof["results"].as_array_mut().unwrap().push(json!({"case":case,"profile":profile,"mutant":mutant,"passed":passed,"executable":executable,"binary":binary,"binary_sha256":sha(&binary)?,"source_sha256":sha(&file)?,"expected_assertion":if mutant {Some(assertion(case)?)} else {None},"target":target}));
                        crate::trace_process::write_proof(&output, &proof)?;
                        ensure!(passed, "baseline or mutant contract failed: {label}");
                    }
                }
                Ok(())
            })();
            let restored = restore.finish();
            proof["restored"] = json!(restored.is_ok());
            crate::trace_process::write_proof(&output, &proof)?;
            restored?;
            outcome?;
        }
        for (name, hash) in copied.as_object().context("source inventory")? {
            ensure!(
                sha(&checkout.join(name))? == hash.as_str().context("source hash")?
                    && sha(&run.root.join(name))? == hash.as_str().context("source hash")?,
                "source inventory changed: {name}"
            );
        }
        ensure!(
            input_names(
                &run.root,
                &output,
                &capture(
                    &mut proof,
                    "git-files-after",
                    cmd([
                        "git",
                        "ls-files",
                        "--cached",
                        "--others",
                        "--exclude-standard",
                        "-z"
                    ])
                )?
            ) == before_names,
            "source file list changed"
        );
        ensure!(
            capture(
                &mut proof,
                "git-head-after",
                cmd(["git", "rev-parse", "HEAD"])
            )?
            .trim()
                == proof["source"].as_str().context("source commit")?,
            "source HEAD changed"
        );
        proof["source_unchanged"] = json!(true);
        Ok(())
    })();
    proof["passed"] = json!(result.is_ok());
    proof["native_windows_verified"] = json!(result.is_ok());
    if let Err(error) = &result {
        proof["failure"] = json!(format!("{error:#}"));
    }
    crate::trace_process::write_proof(&output, &proof)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    #[test]
    fn windows_commands_select_each_profile_once_before_exact_test_flags() {
        for case in windows_cases().unwrap().cases {
            for toolchain in [None, Some("1.99.1")] {
                for release in [false, true] {
                    let mut expected = vec!["cargo".to_owned()];
                    if let Some(toolchain) = toolchain {
                        expected.push(format!("+{toolchain}"));
                    }
                    expected.extend(
                        [
                            "test",
                            "--offline",
                            "--locked",
                            "-p",
                            "weekend-db",
                            "--all-features",
                            "--lib",
                            TEST,
                        ]
                        .map(str::to_owned),
                    );
                    if release {
                        expected.push("--release".into());
                    }
                    expected.extend(
                        ["--message-format=json", "--", "--exact", "--test-threads=1"]
                            .map(str::to_owned),
                    );
                    assert_eq!(test_command(&case, toolchain, release), expected);
                }
            }
        }
    }
    #[test]
    fn coordinator_controls_bind_exact_binary_source_and_assertion() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let data = coordinator_cases().unwrap();
        let dir = Scratch::new(&std::env::temp_dir(), "coordinator-artifact").unwrap();
        let binary = dir.path().join("write-coordinator.exe");
        fs::write(&binary, b"fixture").unwrap();
        for case in &data.cases {
            let text = fs::read_to_string(root.join(&case.file)).unwrap();
            pinned_source(&data, case, &text).unwrap();
            assert!(pinned_source(&data, case, &format!("{text} ")).is_err());
            apply_native(case, &text).unwrap();
            let test = case.exact_test.as_deref().unwrap();
            let args = test_command(case, Some("1.100.0"), true);
            assert_eq!(
                &args[8..],
                [
                    "--bin",
                    "write-coordinator",
                    "--release",
                    test,
                    "--message-format=json",
                    "--",
                    "--exact",
                    "--test-threads=1"
                ]
            );
            let row = json!({"reason":"compiler-artifact", "profile":{"test":true},
                "target":{"name":"write-coordinator","kind":["bin"]}, "executable":binary});
            assert_eq!(
                artifact(&row.to_string(), dir.path(), case).unwrap(),
                binary
            );
            let mut wrong = row;
            wrong["target"]["kind"] = json!(["test"]);
            assert!(artifact(&wrong.to_string(), dir.path(), case).is_err());
            let message = assertion(case).unwrap();
            let log = format!(
                "running 1 test\ntest {test} ... FAILED\n{message}\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 21 filtered out; finished in 0.00s"
            );
            let report = Report {
                exit: Some(101),
                cleanup_verified: true,
                ..Default::default()
            };
            assert!(qualifies(&report, &log, case, true));
            assert!(!qualifies(
                &report,
                &log.replace(&message, "unrelated failure"),
                case,
                true
            ));
        }
    }

    #[test]
    fn windows_inventory_is_exact_and_matches_current_production() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let data = windows_cases().unwrap();
        for case in &data.cases {
            let source = fs::read_to_string(root.join(&case.file)).unwrap();
            pinned_source(&data, case, &source).unwrap();
            pinned_source(
                &data,
                case,
                &source.replace("\r\n", "\n").replace('\n', "\r\n"),
            )
            .unwrap();
            assert!(pinned_source(&data, case, &format!("{source}\n")).is_err());
            apply_native(case, &source).unwrap();
        }
    }
    #[test]
    fn source_file_list_excludes_only_owned_output() {
        let root = Path::new("root");
        let output = root.join("owned");
        let names = input_names(
            root,
            &output,
            "src/input.rs\0owned/a.log\0owned-extra/keep.rs\0",
        );
        assert_eq!(
            names,
            BTreeSet::from(["src/input.rs".into(), "owned-extra/keep.rs".into()])
        );
    }
    #[test]
    fn windows_source_replacement_preserves_crlf_and_test_bytes() {
        let mut case = windows_cases().unwrap().cases.remove(0);
        case.old = "guard\nnext".into();
        case.new = "false\nnext".into();
        let original = "guard\r\nnext\r\n#[cfg(test)]\r\nmod tests { guard }\r\n";
        assert_eq!(
            apply_native(&case, original).unwrap(),
            "false\r\nnext\r\n#[cfg(test)]\r\nmod tests { guard }\r\n"
        );
        assert!(apply_native(&case, "guard\r\nnext\n").is_err());
        assert!(apply_native(&case, "changed\r\n").is_err());
    }
    #[test]
    fn windows_outcomes_require_exact_nonempty_test_failure_and_retirement() {
        for case in windows_cases().unwrap().cases {
            let good = format!(
                "running 1 test\ntest {} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 135 filtered out; finished in 0.00s",
                case.test
            );
            let failed = format!(
                "running 1 test\ntest {} ... FAILED\n{}\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 135 filtered out; finished in 0.00s",
                case.test,
                assertion(&case).unwrap()
            );
            let mut report = Report {
                exit: Some(0),
                cleanup_verified: true,
                ..Default::default()
            };
            assert!(qualifies(&report, &good, &case, false));
            assert!(!qualifies(
                &report,
                "running 0 tests\ntest result: ok. 0 passed; 0 failed;",
                &case,
                false
            ));
            report.exit = Some(101);
            assert!(qualifies(&report, &failed, &case, true));
            for bad in [
                "error: compilation failed".to_owned(),
                format!("compiler help: {failed}"),
                failed.replace(&case.test, "other_test"),
                failed.replace(assertion(&case).unwrap().as_str(), "missing"),
                format!("running 0 tests\n{failed}"),
                format!("{failed} GARBAGE"),
                format!("{failed}\n{good}"),
                format!("{failed}\n{failed}"),
                failed.replace("0 passed; 1 failed;", "1 passed; 1 failed;"),
                failed.replace("test result: FAILED.", "test result: ok."),
                failed.replace(
                    "test result: FAILED.",
                    "compiler help: test result: FAILED.",
                ),
                format!("error: compilation failed\n{failed}"),
                format!(
                    "{}\n{failed}",
                    json!({"reason":"build-finished","success":false})
                ),
                format!(
                    "{}\n{failed}",
                    json!({"reason":"compiler-message","message":{"level":"error"}})
                ),
            ] {
                assert!(!qualifies(&report, &bad, &case, true));
            }
            report.cleanup_verified = false;
            assert!(!qualifies(&report, &failed, &case, true));
            report.cleanup_verified = true;
            report.failure = Some("timeout/output/ownership failure".into());
            assert!(!qualifies(&report, &failed, &case, true));
            report.failure = None;
            report.exit = Some(0);
            assert!(!qualifies(&report, &failed, &case, true));
            #[cfg(windows)]
            {
                report.exit = Some(101);
                report.timed_out = true;
                assert!(!qualifies(&report, &failed, &case, true));
                report.timed_out = false;
                report.output_exceeded = true;
                assert!(!qualifies(&report, &failed, &case, true));
                report.output_exceeded = false;
                report.stopped = true;
                assert!(!qualifies(&report, &failed, &case, true));
            }
        }
    }
    #[test]
    fn windows_artifact_requires_one_real_owned_test_executable() {
        let case = windows_cases().unwrap().cases.remove(0);
        let root = Scratch::new(&std::env::temp_dir(), "mutation-artifact").unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let binary = target.join("test.exe");
        fs::write(&binary, b"fixture").unwrap();
        let row = json!({"reason":"compiler-artifact","profile":{"test":true},"target":{"name":"weekend_db","kind":["rlib","cdylib"]},"executable":binary});
        assert_eq!(artifact(&row.to_string(), &target, &case).unwrap(), binary);
        assert!(artifact("compiler failure", &target, &case).is_err());
        let second = target.join("other.exe");
        fs::write(&second, b"other").unwrap();
        let mut duplicate = row.clone();
        duplicate["executable"] = json!(second);
        assert!(artifact(&format!("{row}\n{duplicate}"), &target, &case).is_err());
        let mut wrong = row.clone();
        wrong["profile"]["test"] = json!(false);
        assert!(artifact(&wrong.to_string(), &target, &case).is_err());
        let outside = root.path().join("outside.exe");
        fs::write(&outside, b"fixture").unwrap();
        wrong = row;
        wrong["executable"] = json!(outside);
        assert!(artifact(&wrong.to_string(), &target, &case).is_err());
    }
    #[test]
    fn isolated_source_restores_on_error_and_unwind() {
        let root = Scratch::new(&std::env::temp_dir(), "mutation-restore").unwrap();
        let file = root.path().join("input.rs");
        fs::write(&file, b"original").unwrap();
        let error: Result<()> = (|| {
            let restore = Restore {
                file: file.clone(),
                bytes: fs::read(&file)?,
            };
            fs::write(&file, b"mutant")?;
            restore.finish()?;
            anyhow::bail!("command failed")
        })();
        assert!(error.is_err());
        assert_eq!(fs::read(&file).unwrap(), b"original");
        let panic = std::panic::catch_unwind(|| {
            let _restore = Restore {
                file: file.clone(),
                bytes: fs::read(&file).unwrap(),
            };
            fs::write(&file, b"mutant").unwrap();
            panic!("interrupted control")
        });
        assert!(panic.is_err());
        assert_eq!(fs::read(&file).unwrap(), b"original");
    }
}
