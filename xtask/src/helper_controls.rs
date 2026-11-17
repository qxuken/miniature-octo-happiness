//! Independently account for Rust helper tests and approved skips in both profiles.
use crate::{
    artifact_trace::inventory,
    assurance::Campaign,
    delivery, serialization_scale, supervise,
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const STREAM_LIMIT: u64 = 8 * 1024 * 1024;
// The Linux quota control makes a sparse one-byte-over-limit runner fixture.
const XTASK_UNIT_FILE_LIMIT: u64 = 128 * 1024 * 1024 + 1;
const INPUTS: [&str; 5] = [
    "include/weekend_db.h",
    "bindings/odin/weekend_db.odin",
    "src/storage/codec.rs",
    "src/storage/mod.rs",
    "apps/study-peer/tests/credential_rotation.rs",
];
const OPTIONAL: [&str; 2] = ["REPLICA_PROGRESS_BIN", "REPLICA_PROGRESS_FIXTURE"];
const NATIVE_TLS: &str =
    "replica_progress::tests::authenticated_observations_restarts_dropped_reply_and_stale_capture";
#[cfg(target_os = "macos")]
const NATIVE_APFS: &str = "macos_volume::tests::native_busy_detach_releases_only_the_owned_image";

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    output: PathBuf,
}

#[derive(Clone, Copy)]
struct Suite {
    name: &'static str,
    source: &'static str,
    ignores: &'static [&'static str],
}
const XTASK: [Suite; 3] = [
    Suite {
        name: "xtask",
        source: "xtask/src/main.rs",
        ignores: &[
            NATIVE_TLS,
            #[cfg(target_os = "macos")]
            NATIVE_APFS,
        ],
    },
    Suite {
        name: "commands",
        source: "xtask/tests/commands.rs",
        ignores: &[],
    },
    Suite {
        name: "supervise",
        source: "xtask/tests/supervise.rs",
        ignores: &[],
    },
];
const SQLITE: [Suite; 1] = [Suite {
    name: "sqlite-comparator",
    source: "tools/sqlite-baseline/src/sqlite.rs",
    ignores: &[],
}];

fn regular(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = root.to_owned();
    for component in Path::new(relative).components() {
        let std::path::Component::Normal(piece) = component else {
            anyhow::bail!("invalid helper input path");
        };
        path.push(piece);
        ensure!(
            !fs::symlink_metadata(&path)?.is_symlink(),
            "symlink helper input: {relative}"
        );
    }
    ensure!(path.is_file(), "missing helper input: {relative}");
    Ok(path)
}
fn external(root: &Path, inputs: &[&str]) -> Result<BTreeMap<String, String>> {
    inputs
        .iter()
        .map(|name| Ok(((*name).to_owned(), sha(&regular(root, name)?)?)))
        .collect()
}

fn integration_registry(root: &Path) -> Result<()> {
    let directory = root.join("xtask/tests");
    ensure!(
        !fs::symlink_metadata(root.join("xtask"))?.is_symlink()
            && fs::symlink_metadata(&directory)?.is_dir(),
        "symlink or non-directory in helper integration registry"
    );
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "symlink in helper integration registry");
        if kind.is_file() && entry.path().extension() == Some(OsStr::new("rs")) {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("UTF-8 suite path"))?;
            found.insert(format!("xtask/tests/{name}"));
        }
    }
    let expected = XTASK
        .into_iter()
        .skip(1)
        .map(|case| case.source.to_owned())
        .collect::<BTreeSet<_>>();
    ensure!(
        found == expected,
        "helper integration registry must exactly cover test sources"
    );
    Ok(())
}

fn discover(
    cargo_output: &str,
    root: &Path,
    manifest: &Path,
    target: &Path,
    suites: &[Suite],
) -> Result<BTreeMap<&'static str, PathBuf>> {
    let mut found = BTreeMap::new();
    for line in cargo_output.lines() {
        let row = delivery::json(line.as_bytes())?;
        if row["reason"] != "compiler-artifact"
            || row["profile"]["test"] != true
            || !row["executable"].is_string()
        {
            continue;
        }
        ensure!(
            row["manifest_path"].as_str().map(Path::new) == Some(manifest),
            "unregistered helper artifact package"
        );
        let name = row["target"]["name"]
            .as_str()
            .context("missing helper artifact name")?;
        let plan = suites
            .iter()
            .find(|suite| suite.name == name)
            .context("unregistered helper artifact")?;
        let source = regular(root, plan.source)?;
        ensure!(
            row["target"]["src_path"].as_str().map(Path::new) == Some(&source),
            "helper artifact source mismatch"
        );
        let executable = PathBuf::from(row["executable"].as_str().unwrap());
        ensure!(
            executable.starts_with(target)
                && fs::symlink_metadata(&executable)?.is_file()
                && dunce::canonicalize(&executable)? == executable,
            "helper executable escaped its build target"
        );
        ensure!(
            found.insert(plan.name, executable).is_none(),
            "duplicate helper executable"
        );
    }
    ensure!(
        found.len() == suites.len(),
        "incomplete helper artifact registry"
    );
    Ok(found)
}

fn test_list(output: &str) -> Result<BTreeSet<String>> {
    let mut tests = BTreeSet::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let name = line
            .strip_suffix(": test")
            .context("invalid libtest inventory row")?;
        ensure!(
            !name.is_empty() && tests.insert(name.to_owned()),
            "unknown or duplicate test identity"
        );
    }
    ensure!(!tests.is_empty(), "zero discovered helper tests");
    Ok(tests)
}

fn validate(output: &str, expected: &BTreeSet<String>, ignored: &[&str]) -> Result<Value> {
    validate_with_filtered(output, expected, ignored, 0)
}

fn validate_with_filtered(
    output: &str,
    expected: &BTreeSet<String>,
    ignored: &[&str],
    filtered: usize,
) -> Result<Value> {
    let mut observed = BTreeMap::new();
    let mut summary = None;
    let mut size = None;
    for line in output.lines().filter(|line| !line.is_empty()) {
        ensure!(summary.is_none(), "content after libtest completion");
        if let Some(heading) = line.strip_prefix("running ") {
            let (count, description) =
                heading.split_once(' ').context("invalid libtest heading")?;
            ensure!(
                size.replace(count.parse::<usize>()?).is_none()
                    && matches!(description, "test" | "tests"),
                "invalid libtest heading"
            );
        } else if let Some(text) = line.strip_prefix("test result: ok. ") {
            summary = Some(text);
        } else if let Some(test) = line.strip_prefix("test ") {
            ensure!(size.is_some(), "helper result before libtest start");
            let (name, status) = test.split_once(" ... ").context("invalid libtest row")?;
            let skipped = status == "ignored"
                || status
                    .strip_prefix("ignored, ")
                    .is_some_and(|reason| !reason.is_empty());
            ensure!(
                status == "ok" || skipped,
                "helper test failed or returned unknown status"
            );
            ensure!(
                observed.insert(name.to_owned(), skipped).is_none(),
                "duplicate helper test result"
            );
        } else {
            anyhow::bail!("unexpected libtest output");
        }
    }
    ensure!(
        size == Some(expected.len()) && observed.keys().eq(expected),
        "incomplete or unexpected helper test execution"
    );
    let skips = observed
        .iter()
        .filter_map(|(name, skip)| skip.then_some(name.as_str()))
        .collect::<BTreeSet<_>>();
    ensure!(
        skips == ignored.iter().copied().collect(),
        "unexpected or missing registered skip"
    );
    let passed = expected.len() - skips.len();
    ensure!(passed > 0, "no helper test passed");
    let expected_summary = [
        format!("{passed} passed"),
        "0 failed".to_owned(),
        format!("{} ignored", skips.len()),
        "0 measured".to_owned(),
        format!("{filtered} filtered out"),
    ];
    let summary = summary.context("missing libtest completion")?;
    let mut counts = summary.split("; ");
    ensure!(
        counts.by_ref().take(5).eq(expected_summary)
            && counts.next().is_some_and(|end| {
                end.strip_prefix("finished in ")
                    .and_then(|time| time.strip_suffix('s'))
                    .and_then(|time| time.parse::<f64>().ok())
                    .is_some_and(|time| time.is_finite() && time >= 0.)
            })
            && counts.next().is_none(),
        "libtest completion counts differ"
    );
    Ok(
        json!({"discovered":expected,"observed":observed,"tests":expected.len(),"passed":passed,"skipped":skips}),
    )
}

fn bounded_text(path: &Path, limit: u64) -> Result<String> {
    use std::io::Read;
    let mut text = String::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut text)?;
    ensure!(
        text.len() as u64 <= limit,
        "helper report exceeds byte limit"
    );
    Ok(text)
}

fn remaining(deadline: Instant, limit: f64) -> Result<f64> {
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .as_secs_f64();
    ensure!(remaining > 0., "helper whole-run deadline exceeded");
    Ok(remaining.min(limit))
}

fn execute_suite(
    c: &mut Campaign<'_>,
    suite: Suite,
    binary: &Path,
    cwd: &Path,
    profile: &str,
    deadline: Instant,
) -> Result<()> {
    let identity = sha(binary)?;
    let label = format!("{profile}-{}", suite.name);
    let listed = c.command_at(
        vec![path(binary), "--list".into(), "--format=terse".into()],
        &format!("{label}-list"),
        None,
        remaining(deadline, 30.)?,
        cwd,
    )?;
    let expected = test_list(&listed)?;
    let mut request = c.request(
        vec![path(binary), "--test-threads=4".into()],
        &label,
        remaining(deadline, 90.)?,
        cwd,
    )?;
    let journal = c.output.join(format!("{label}-processes.jsonl"));
    request.env.as_mut().context("helper environment")?.insert(
        "HELPER_PROCESS_JOURNAL".into(),
        journal.to_str().context("UTF-8 helper journal")?.to_owned(),
    );
    request.journal = Some(journal);
    request.file_limit = Some(if suite.name == "xtask" {
        XTASK_UNIT_FILE_LIMIT
    } else {
        STREAM_LIMIT
    });
    c.evidence["suites"][&label] = json!({
        "suite":suite.name,"profile":profile,"argv":request.argv,"sha256":identity,"verified":false,
        "timeout_seconds":request.timeout_seconds,"allowed_skips":suite.ignores,
        "file_limit":request.file_limit, "stream_limit":request.maximum_bytes,
    });
    c.save()?;
    let report = supervise::run(&request)?;
    c.evidence["suites"][&label]["supervision"] = serde_json::to_value(&report)?;
    c.save()?;
    ensure!(
        report.success(),
        "helper suite {label} did not finish successfully: {report:?}"
    );
    let raw = bounded_text(&request.stdout, STREAM_LIMIT)?;
    c.evidence["suites"][&label]["result"] = validate(&raw, &expected, suite.ignores)?;
    ensure!(sha(binary)? == identity, "helper test executable changed");
    c.evidence["suites"][&label]["verified"] = json!(true);
    c.save()?;
    Ok(())
}

fn test_package(
    c: &mut Campaign<'_>,
    manifest_path: &str,
    suites: &[Suite],
    features: &[&str],
    profile: &str,
    deadline: Instant,
) -> Result<()> {
    let manifest = regular(&c.run.root, manifest_path)?;
    let cwd = manifest
        .parent()
        .context("helper package manifest directory")?;
    let target = c
        .output
        .join("cargo-target")
        .join(suites[0].name)
        .join(profile);
    let mut arguments = cmd([
        "cargo",
        "test",
        "--offline",
        "--locked",
        "--no-run",
        "--message-format=json",
    ]);
    if profile == "release" {
        arguments.push("--release".into());
    }
    arguments.extend([
        "--manifest-path".into(),
        path(&manifest),
        "--target-dir".into(),
        path(&target),
    ]);
    arguments.extend(cmd(features));
    let built = c.command_at(
        arguments,
        &format!("{profile}-{}-build", suites[0].name),
        None,
        remaining(deadline, 300.)?,
        &c.run.root,
    )?;
    let binaries = discover(&built, &c.run.root, &manifest, &target, suites)?;
    for suite in suites {
        execute_suite(c, *suite, &binaries[suite.name], cwd, profile, deadline)?;
    }
    Ok(())
}

pub fn run(run: &Run, options: Options) -> Result<()> {
    let mut c = Campaign::new(run, &options.output, "bounded native helper controls")?;
    c.evidence["verified"] = json!(false);
    c.evidence["suites"] = json!({});
    c.evidence["disabled_optional_env"] = json!(OPTIONAL);
    c.limits(STREAM_LIMIT, None)?;
    c.env = serialization_scale::clean_env(&c.env);
    c.env.insert("CARGO_TERM_COLOR".into(), "never".into());
    c.env.remove(OsStr::new("HELPER_PROCESS_JOURNAL"));
    for key in OPTIONAL {
        c.env.remove(OsStr::new(key));
    }
    c.save()?;
    let result = qualify(&mut c);
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

fn qualify(c: &mut Campaign<'_>) -> Result<()> {
    let run = c.run;
    integration_registry(&run.root)?;
    let before = external(&run.root, &INPUTS)?;
    c.evidence["external_inputs"] = json!(before);
    let deadline = Instant::now() + Duration::from_secs(600);
    for profile in ["debug", "release"] {
        test_package(c, "xtask/Cargo.toml", &XTASK, &[], profile, deadline)?;
        test_package(
            c,
            "tools/sqlite-baseline/Cargo.toml",
            &SQLITE,
            &[
                "--no-default-features",
                "--features",
                "sqlite-comparator",
                "--bin",
                "sqlite-comparator",
            ],
            profile,
            deadline,
        )?;
    }
    ensure!(
        external(&run.root, &INPUTS)? == before,
        "external helper input changed during run"
    );
    c.evidence["artifacts"] = json!(inventory(&c.output, &["evidence.json", "cargo-target"])?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    use std::{os::unix::fs::symlink, process::Command};

    #[test]
    fn reports_cannot_hide_failures_or_skips() {
        let expected = test_list("test_a: test\ntest_b: test\n").unwrap();
        let good = "running 2 tests\ntest test_a ... ok\ntest test_b ... ok\n\
                    test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s";
        assert_eq!(validate(good, &expected, &[]).unwrap()["passed"], 2);
        for bad in [
            good.replace("running 2 tests", "running 0 tests"),
            good.replace("test test_a ... ok\n", ""),
            good.replace("test test_a ... ok", "test test_other ... ok"),
            good.replace("test test_a ... ok", "test test_b ... ok"),
            good.replace("test test_a ... ok", "test test_a ... FAILED"),
            good.replace("test test_a ... ok", "test test_a ... ignored, skip"),
            good.replace("0 filtered out", "1 filtered out"),
            good.replace("finished in 0.01s", "finished in NaNs"),
            good.split_once("test result").unwrap().0.to_owned(),
            format!("{good}\ntest test_c ... ok"),
        ] {
            assert!(validate(&bad, &expected, &[]).is_err(), "{bad}");
        }
        let skipped = good
            .replace("... ok", "... ignored, registered")
            .replace("2 passed", "0 passed")
            .replace("0 ignored", "2 ignored");
        assert!(validate(&skipped, &expected, &["test_a", "test_b"]).is_err());
        assert!(test_list("").is_err());
        assert!(test_list("test_a: test\ntest_a: test").is_err());
    }

    #[test]
    fn registry_requires_every_integration_target_and_regular_bound_source() {
        let temp = Scratch::new(&std::env::temp_dir(), "helper-registry").unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("xtask/tests")).unwrap();
        for suite in &XTASK[1..] {
            fs::write(root.join(suite.source), b"fixture").unwrap();
        }
        integration_registry(root).unwrap();
        let extra = root.join("xtask/tests/unregistered.rs");
        fs::write(&extra, b"").unwrap();
        assert!(
            integration_registry(root)
                .unwrap_err()
                .to_string()
                .contains("exactly")
        );
        fs::remove_file(&extra).unwrap();
        symlink(root.join(XTASK[1].source), extra).unwrap();
        assert!(
            integration_registry(root)
                .unwrap_err()
                .to_string()
                .contains("symlink")
        );

        let bound = XTASK[1].source;
        let before = external(root, &[bound]).unwrap();
        fs::write(root.join(bound), b"updated fixture").unwrap();
        assert_ne!(before, external(root, &[bound]).unwrap());
        fs::remove_file(root.join(bound)).unwrap();
        assert!(external(root, &[bound]).is_err());
        symlink(root.join(XTASK[2].source), root.join(bound)).unwrap();
        assert!(external(root, &[bound]).is_err());
    }

    #[test]
    fn real_libtest_accounts_for_assertions_and_rejects_empty_or_skipped_execution() {
        fn invoke(args: &[&str]) -> String {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            String::from_utf8(output.stdout).unwrap()
        }

        let control =
            "supervise::tests::all_paths_admitted_before_launch_and_existing_data_untouched";
        let expected =
            test_list(&invoke(&["--list", "--format=terse", "--exact", control])).unwrap();
        let output = invoke(&["--exact", control]);
        let expected_filtered = test_list(&invoke(&["--list", "--format=terse"]))
            .unwrap()
            .len()
            - 1;
        assert_eq!(
            validate_with_filtered(&output, &expected, &[], expected_filtered).unwrap()["passed"],
            1
        );
        assert!(validate(&output, &expected, &[]).is_err());
        let absent = "no_such_libtest_control";
        assert!(test_list(&invoke(&["--list", "--format=terse", absent])).is_err());
        assert!(validate(&invoke(&[absent]), &expected, &[]).is_err());
        let expected = test_list(&invoke(&[
            "--list",
            "--format=terse",
            "--exact",
            NATIVE_TLS,
        ]))
        .unwrap();
        let skipped = invoke(&["--exact", NATIVE_TLS]);
        assert!(skipped.contains("ignored"));
        assert!(validate_with_filtered(&skipped, &expected, &[], expected_filtered).is_err());
        assert!(
            validate_with_filtered(&skipped, &expected, &[NATIVE_TLS], expected_filtered).is_err()
        );
        #[cfg(target_os = "macos")]
        {
            let expected = test_list(&invoke(&[
                "--list",
                "--format=terse",
                "--exact",
                control,
                NATIVE_APFS,
            ]))
            .unwrap();
            assert_eq!(expected.len(), 2);
            let output = invoke(&["--exact", control, NATIVE_APFS]);
            let registered = XTASK[0]
                .ignores
                .iter()
                .copied()
                .filter(|name| expected.contains(*name))
                .collect::<Vec<_>>();
            assert_eq!(
                validate_with_filtered(&output, &expected, &registered, expected_filtered - 1)
                    .unwrap()["passed"],
                1
            );
            assert!(
                validate_with_filtered(&output, &expected, &[], expected_filtered - 1).is_err()
            );
            assert!(
                validate_with_filtered(
                    &output,
                    &expected,
                    &[NATIVE_APFS, NATIVE_TLS],
                    expected_filtered - 1,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn deadline_and_report_bounds_reject_before_acceptance() {
        let now = Instant::now();
        assert!(remaining(now, 90.).is_err());
        assert_eq!(remaining(now + Duration::from_secs(120), 90.).unwrap(), 90.);
        let temp = Scratch::new(&std::env::temp_dir(), "helper-bounds").unwrap();
        let file = temp.path().join("report");
        fs::write(&file, b"bounded").unwrap();
        assert_eq!(bounded_text(&file, 7).unwrap(), "bounded");
        assert!(bounded_text(&file, 6).is_err());
    }

    #[test]
    fn registry_failure_preserves_negative_evidence_before_building() {
        let temp = Scratch::new(&std::env::temp_dir(), "helper-failure").unwrap();
        fs::create_dir_all(temp.path().join("xtask/tests")).unwrap();
        let run = Run::new(temp.path().to_owned());
        let mut campaign = Campaign::detached(&run, Path::new("proof"), "helper control").unwrap();
        campaign.evidence["verified"] = json!(false);
        let result = qualify(&mut campaign);
        assert!(campaign.finish(result).is_err());
        let evidence = delivery::read_json(&temp.path().join("proof/evidence.json")).unwrap();
        assert_eq!(evidence["verified"], false);
        assert!(
            evidence["error"]
                .as_str()
                .unwrap()
                .contains("exactly cover")
        );
        assert_eq!(evidence["commands"], json!([]));
    }
}
