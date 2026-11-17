//! Native Windows RNG controls using the existing Job-owned process boundary.
use super::*;
use crate::trace_process::Report;
use sha2::Digest;
const TEST: &str = "storage::windows::decision_tests::identity_rejects_every_nonzero_status_before_accepting_bytes";
fn assertion(case: &Case) -> Result<&'static str> {
    match case.name.as_str() {
        "windows-rng-status-negative" => Ok(
            "called `Result::unwrap_err()` on an `Ok` value: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7]",
        ),
        "windows-rng-diagnostic-omission" => Ok("called `Option::unwrap()` on a `None` value"),
        _ => anyhow::bail!("unknown Windows mutation contract"),
    }
}
fn qualifies(report: &Report, log: &str, case: &Case, mutant: bool) -> bool {
    #[cfg(windows)]
    if report.stopped || report.timed_out || report.output_exceeded {
        return false;
    }
    let (exit, outcome, passed, failed) = if mutant {
        (101, "FAILED", "0", "1")
    } else {
        (0, "ok", "1", "0")
    };
    if report.exit != Some(exit) || !report.qualifies(mutant) || log.contains("running 0 tests") {
        return false;
    }
    let mut results = Vec::new();
    let mut summaries = Vec::new();
    for line in log.lines() {
        if line.starts_with("test result:") {
            summaries.push(line);
        } else if line.starts_with("test ") {
            results.push(line);
        }
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
    if results != [format!("test {} ... {outcome}", case.test).as_str()] || summaries.len() != 1 {
        return false;
    }
    let footer = regex::Regex::new(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; 0 ignored; 0 measured; \d+ filtered out; finished in \d+(?:\.\d+)?s$").unwrap();
    let Some(counts) = footer.captures(summaries[0]) else {
        return false;
    };
    counts.get(1).unwrap().as_str() == outcome
        && &counts[2] == passed
        && &counts[3] == failed
        && (!mutant
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
            case.file == Path::new("src/storage/windows.rs")
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
    argv.extend(["--message-format=json", "--", "--exact", "--test-threads=1"].map(str::to_owned));
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
fn pinned_source(data: &Inventory, original: &str) -> Result<()> {
    ensure!(
        format!(
            "{:x}",
            sha2::Sha256::digest(original.replace("\r\n", "\n").as_bytes())
        ) == data.source_sha256,
        "Windows source changed from the reviewed fixture"
    );
    Ok(())
}
fn artifact(log: &str, target: &Path) -> Result<PathBuf> {
    let mut paths = BTreeSet::new();
    for line in log.lines() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if row["reason"] == "compiler-artifact"
            && row["profile"]["test"] == true
            && row["target"]["name"] == "weekend_db"
            && row["target"]["kind"] == json!(["rlib", "cdylib"])
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
    let data = windows_cases()?;
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
        pinned_source(&data, &source)?;
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
            pinned_source(&data, std::str::from_utf8(&original)?)?;
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
                        let target = output.join(format!("build-{label}"));
                        fs::create_dir(&target)?;
                        let mut env = instrumentation::environment(&run.env, &target, &[]);
                        env.insert(path("CARGO_TERM_COLOR"), path("never"));
                        env.remove(&path("CARGO_BUILD_TARGET"));
                        let argv = test_command(case, options.toolchain.as_deref(), release);
                        let (report, log) =
                            invoke(&mut proof, &output, &label, cmd(&argv), &checkout, &env)?;
                        let executable = artifact(&log, &target)?;
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
    fn windows_inventory_is_exact_and_matches_current_production() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let data = windows_cases().unwrap();
        for case in &data.cases {
            let source = fs::read_to_string(root.join(&case.file)).unwrap();
            pinned_source(&data, &source).unwrap();
            pinned_source(&data, &source.replace("\r\n", "\n").replace('\n', "\r\n")).unwrap();
            assert!(pinned_source(&data, &format!("{source}\n")).is_err());
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
                "test {} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 135 filtered out; finished in 0.00s",
                case.test
            );
            let failed = format!(
                "test {} ... FAILED\n{}\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 135 filtered out; finished in 0.00s",
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
                failed.replace(assertion(&case).unwrap(), "missing"),
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
        let root = Scratch::new(&std::env::temp_dir(), "mutation-artifact").unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let binary = target.join("test.exe");
        fs::write(&binary, b"fixture").unwrap();
        let row = json!({"reason":"compiler-artifact","profile":{"test":true},"target":{"name":"weekend_db","kind":["rlib","cdylib"]},"executable":binary});
        assert_eq!(artifact(&row.to_string(), &target).unwrap(), binary);
        assert!(artifact("compiler failure", &target).is_err());
        let second = target.join("other.exe");
        fs::write(&second, b"other").unwrap();
        let mut duplicate = row.clone();
        duplicate["executable"] = json!(second);
        assert!(artifact(&format!("{row}\n{duplicate}"), &target).is_err());
        let mut wrong = row.clone();
        wrong["profile"]["test"] = json!(false);
        assert!(artifact(&wrong.to_string(), &target).is_err());
        let outside = root.path().join("outside.exe");
        fs::write(&outside, b"fixture").unwrap();
        wrong = row;
        wrong["executable"] = json!(outside);
        assert!(artifact(&wrong.to_string(), &target).is_err());
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
