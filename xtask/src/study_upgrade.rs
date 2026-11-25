//! Exact-build StudyProgress CLI upgrade with the existing public-API fixture.
#[cfg(windows)]
use crate::windows_compatibility::Campaign;
#[cfg(unix)]
use crate::{
    artifact_trace::{docker_retire, nonce},
    assurance::{self, Campaign},
    faults, toolchain,
};
use crate::{
    delivery, source_archive,
    support::{
        Run, cargo_build_env, cmd, copy_qualification_tree as copy_tree, path,
        qualification_inventory as inventory, sha,
    },
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    env::consts::EXE_SUFFIX,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const HISTORICAL: &str = "fb80989d98bd977743168ee10418a434a67a0261";
const CURRENT: &str = "365daf6c8a6b7c85fb049fb5a0370b401dde2424";
const LABELS: [&str; 2] = ["old", "new"];
const FIXTURE: &str = "study-cli-upgrade-fixture.rs";
const MARKERS: [&str; 3] = [
    "PASS old-old",
    "PASS reachable mixed-version ALPN",
    "PASS complete causal records",
];

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = CURRENT)]
    new: String,
    #[cfg(unix)]
    #[arg(long)]
    docker_image: Option<String>,
    #[cfg(unix)]
    #[arg(long, hide = true)]
    worker: Option<PathBuf>,
}

fn replace_fixture_once(source: &str, before: &str, after: &str) -> Result<String> {
    ensure!(
        source.matches(before).count() == 1,
        "study fixture seam differs: {before}"
    );
    Ok(source.replacen(before, after, 1))
}

fn fixture(helpers: &str, scenario: &str, windows: bool) -> Result<String> {
    let helpers = helpers.replace("\r\n", "\n");
    let scenario = scenario.replace("\r\n", "\n");
    let boundary =
        "#[test]\nfn persistent_study_progress_survives_interrupted_credential_cutover()";
    ensure!(
        helpers.matches(boundary).count() == 1,
        "helper extraction boundary differs"
    );
    let (helpers, _) = helpers.split_once(boundary).unwrap();
    let old = "        log: &Log,\n    ) -> Self {";
    ensure!(
        helpers.matches(old).count() == 1 && helpers.matches("Command::new(BIN)").count() == 1,
        "helper executable seam differs"
    );
    let mut moved = helpers
        .replace(old, "        (log, binary): (&Log, &str),\n    ) -> Self {")
        .replace("Command::new(BIN)", "Command::new(binary)");
    if windows {
        moved = replace_fixture_once(
            &moved,
            "std::env::temp_dir().join",
            "PathBuf::from(std::env::var_os(\"STUDY_UPGRADE_RUNTIME\").unwrap()).join",
        )?;
    }
    if windows
        && !moved.starts_with("#![cfg(any(target_os = \"macos\", target_os = \"linux\", windows))]")
    {
        for (before, after) in [
            (
                "#![cfg(any(target_os = \"macos\", target_os = \"linux\"))]",
                "#![cfg(windows)]",
            ),
            ("    os::unix::fs::{DirBuilderExt, PermissionsExt},\n", ""),
            (
                "fs::DirBuilder::new().mode(0o700).create(&path).unwrap();",
                "fs::create_dir(&path).unwrap();",
            ),
            (
                "fs::DirBuilder::new().mode(0o700).create(&active).unwrap();",
                "fs::create_dir(&active).unwrap();",
            ),
            (
                "    fs::set_permissions(active.join(\"key.der\"), fs::Permissions::from_mode(0o600)).unwrap();\n",
                "",
            ),
        ] {
            moved = replace_fixture_once(&moved, before, after)?;
        }
    }
    Ok(format!("#![allow(dead_code)]\n{moved}{scenario}"))
}

fn generate(source: &Path, scenario: &Path) -> Result<String> {
    fixture(
        &fs::read_to_string(source.join("apps/study-peer/tests/credential_rotation.rs"))?,
        &fs::read_to_string(scenario)?,
        cfg!(windows),
    )
}

fn source_locks(source: &Path) -> Result<()> {
    for name in [
        "examples/support/study.rs",
        "src/crdt.rs",
        "apps/study-peer/src/native/protocol.rs",
    ] {
        ensure!(
            sha(&source.join("old").join(name))? == sha(&source.join("new").join(name))?,
            "unqualified state/wire source change: {name}"
        );
    }
    ensure!(
        inventory(&source.join("old/src/crdt"), &["decisions.rs"])?
            == inventory(&source.join("new/src/crdt"), &["decisions.rs"])?,
        "unqualified CRDT container change"
    );
    Ok(())
}

// Cargo's binary depfile includes transitive workspace sources. Check the
// release build on both endpoints before exempting the CRDT test module.
fn verify_crdt_inputs(depfile: &str) -> Result<()> {
    let paths = depfile.replace('\\', "/");
    ensure!(
        paths.contains("/src/crdt.rs") && paths.contains("/src/crdt/multi_value_register.rs"),
        "study binary dependency report is missing CRDT runtime inputs"
    );
    ensure!(
        !paths.contains("/src/crdt/decisions.rs"),
        "study binary depends on the CRDT test module"
    );
    Ok(())
}

fn worker(run: &Run, seed: &Path, work: &Path) -> Result<()> {
    #[cfg(unix)]
    let work = assurance::fresh(&run.root, work)?;
    #[cfg(unix)]
    let mut c = Campaign::detached(
        run,
        &work.join("result"),
        "mixed-version study CLI architecture",
    )?;
    #[cfg(windows)]
    let mut c = Campaign::new(
        run,
        &work.join("result"),
        "mixed-version study CLI architecture",
    )?;
    #[cfg(unix)]
    c.limits(8 * 1024 * 1024, None)?;
    let runtime = work.join("runtime");
    fs::create_dir(&runtime)?;
    #[cfg(unix)]
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    c.env = cargo_build_env(&c.env);
    #[cfg(unix)]
    c.env.insert("TMPDIR".into(), path(&runtime));
    #[cfg(windows)]
    {
        c.env.retain(|key, _| {
            ![
                "WEEKEND_DB_SOURCE_ID",
                "STUDY_UPGRADE_RUNTIME",
                "STUDY_OLD_BINARY",
                "STUDY_NEW_BINARY",
                "STUDY_UPGRADE_EVIDENCE",
            ]
            .iter()
            .any(|name| key.eq_ignore_ascii_case(name))
        });
        c.env.insert("STUDY_UPGRADE_RUNTIME".into(), path(&runtime));
    }
    c.evidence["verified"] = json!(false);
    c.save()?;
    let result = (|| -> Result<()> {
        #[cfg(windows)]
        c.restrict_directories(&[("runtime", &runtime)])?;
        c.evidence["compiler"] = json!(c.command(cmd(["rustc", "-vV"]), "compiler", None, 30.)?);
        c.evidence["cargo"] = json!(c.command(cmd(["cargo", "-V"]), "cargo", None, 30.)?);
        #[cfg(unix)]
        let filesystem = if cfg!(target_os = "linux") {
            vec![
                "findmnt".into(),
                "-T".into(),
                path(&work),
                "-n".into(),
                "-o".into(),
                "FSTYPE,SOURCE,TARGET".into(),
            ]
        } else {
            vec!["df".into(), "-P".into(), path(&work)]
        };
        #[cfg(unix)]
        {
            c.evidence["filesystem"] = json!(c.command(filesystem, "filesystem", None, 30.)?);
        }
        let identities = delivery::read_json(&seed.join("identities.json"))?;
        let mut generated = None;
        let mut sources = BTreeMap::new();
        for label in LABELS {
            let source = work.join(label);
            copy_tree(&seed.join(label), &source)?;
            let bound = inventory(&source, &[])?;
            let lock_hash = sha(&source.join("Cargo.lock"))?;
            fs::copy(
                source.join("Cargo.lock"),
                c.output.join(format!("{label}-Cargo.lock")),
            )?;
            let target = work.join(format!("{label}-build"));
            let revision = identities[label]
                .as_str()
                .context("missing study endpoint source")?;
            ensure!(
                revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid study endpoint source"
            );
            c.env.insert("WEEKEND_DB_SOURCE_ID".into(), revision.into());
            let mut command = cmd([
                "cargo",
                "build",
                "--offline",
                "--locked",
                "--release",
                "-p",
                "study-peer",
                "--target-dir",
            ]);
            command.push(path(&target));
            c.command_at(command, &format!("{label}-build"), None, 300., &source)?;
            ensure!(
                inventory(&source, &[])? == bound,
                "locked build changed archival inputs"
            );
            let binary = target
                .join("release")
                .join(format!("study-peer{EXE_SUFFIX}"));
            let copied = c.output.join(format!("{label}-study-peer{EXE_SUFFIX}"));
            fs::copy(&binary, &copied)?;
            let depfile = fs::read_to_string(target.join("release/study-peer.d"))?;
            verify_crdt_inputs(&depfile)?;
            fs::write(c.output.join(format!("{label}-study-peer.d")), depfile)?;
            let binary_hash = sha(&binary)?;
            ensure!(
                sha(&copied)? == binary_hash,
                "copied study executable differs"
            );
            c.evidence["sources"][label] = json!({
                "files":bound,"binary_sha256":binary_hash,"lock_sha256":lock_hash,"revision":revision,
            });
            sources.insert(label, (bound, binary_hash));
            if label == "new" {
                let fixture = generate(&source, &seed.join(FIXTURE))?;
                let path = source.join("apps/study-peer/tests/cli_upgrade.rs");
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?
                    .write_all(fixture.as_bytes())?;
                fs::copy(&path, c.output.join("generated-cli-upgrade.rs"))?;
                c.evidence["fixture_sha256"] = json!(sha(&path)?);
                generated = Some(path);
            }
            c.save()?;
        }
        let source = work.join("new");
        let target = work.join("new-build");
        for (var, output) in [
            ("STUDY_OLD_BINARY", format!("old-study-peer{EXE_SUFFIX}")),
            ("STUDY_NEW_BINARY", format!("new-study-peer{EXE_SUFFIX}")),
            ("STUDY_UPGRADE_EVIDENCE", "proof".to_owned()),
        ] {
            c.env.insert(var.into(), path(c.output.join(output)));
        }
        let mut test = cmd([
            "cargo",
            "test",
            "--offline",
            "--locked",
            "--release",
            "-p",
            "study-peer",
            "--target-dir",
        ]);
        test.push(path(&target));
        test.extend(cmd(["--test", "cli_upgrade", "--", "--nocapture"]));
        c.command_at(test, "test", None, 180., &source)?;
        let mut clippy = cmd([
            "cargo",
            "clippy",
            "--offline",
            "--locked",
            "--release",
            "-p",
            "study-peer",
            "--target-dir",
        ]);
        clippy.push(path(&target));
        clippy.extend(cmd(["--test", "cli_upgrade", "--", "-D", "warnings"]));
        c.command_at(clippy, "clippy", None, 300., &source)?;
        let events = fs::read_to_string(c.output.join("proof/events.log"))?;
        ensure!(
            MARKERS.into_iter().all(|marker| events.contains(marker)),
            "missing upgrade scenario oracle"
        );
        fs::remove_file(generated.context("missing upgrade fixture")?)?;
        for (label, (bound, hash)) in sources {
            ensure!(
                inventory(&work.join(label), &[])? == bound
                    && sha(&c.output.join(format!("{label}-study-peer{EXE_SUFFIX}")))? == hash,
                "study sources or binary changed during execution"
            );
        }
        ensure!(
            fs::read_dir(&runtime)?.next().is_none(),
            "private runtime cleanup incomplete"
        );
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&runtime);
    c.evidence["runtime_removed"] = json!(cleanup.is_ok());
    let outcome = match (result, cleanup) {
        (Ok(()), Ok(())) => inventory(&c.output, &["evidence.json"])
            .map(|files| c.evidence["artifacts"] = json!(files)),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(work), Err(cleanup)) => Err(anyhow::anyhow!(
            "{work:#}\nprivate runtime cleanup failed: {cleanup}"
        )),
    };
    c.evidence["verified"] = json!(outcome.is_ok());
    c.finish(outcome)
}

fn verify_copy(result: &Path, source: &Value, fixture_hash: &str) -> Result<String> {
    let path = result.join("evidence.json");
    let proof = delivery::read_json(&path)?;
    ensure!(
        proof["passed"] == true && proof["verified"] == true && proof["runtime_removed"] == true,
        "study worker did not qualify"
    );
    ensure!(
        proof["artifacts"] == json!(inventory(result, &["evidence.json"])?),
        "copied artifact inventory differs"
    );
    for label in LABELS {
        verify_crdt_inputs(&fs::read_to_string(
            result.join(format!("{label}-study-peer.d")),
        )?)?;
        let observed = &proof["sources"][label];
        ensure!(
            observed["files"] == source[label]["files"]
                && observed["revision"] == source[label]["revision"],
            "copied source differs from frozen seed"
        );
        ensure!(
            observed["binary_sha256"]
                == sha(&result.join(format!("{label}-study-peer{EXE_SUFFIX}")))?,
            "copied study binary differs"
        );
        let lock = json!(sha(&result.join(format!("{label}-Cargo.lock")))?);
        ensure!(
            observed["lock_sha256"] == lock && observed["files"]["Cargo.lock"] == lock,
            "copied study lock differs"
        );
    }
    ensure!(
        proof["fixture_sha256"] == fixture_hash,
        "copied study fixture differs"
    );
    sha(&path)
}

#[cfg(unix)]
fn docker(c: &mut Campaign<'_>, seed: &Path, image: &str) -> Result<()> {
    let image = c.command(
        cmd(["docker", "image", "inspect", "--format={{.Id}}", image]),
        "image",
        None,
        30.,
    )?;
    ensure!(
        image.len() == 71
            && image.starts_with("sha256:")
            && image[7..].bytes().all(|b| b.is_ascii_hexdigit()),
        "immutable local Docker image required"
    );
    let runner = c.output.join("runner");
    faults::stage_runner(&c.run.root, &runner)?;
    let hashes = inventory(&runner, &[])?;
    let registry = toolchain::cargo_home(&c.run.root, &c.env)?.join("registry");
    let container = format!("study-upgrade-{}", nonce()?);
    let volume = format!("{container}-data");
    c.evidence["docker"] = json!({"image":image,"container":container,"volume":volume,"runner_sha256":hashes,"cleanup_verified":false});
    c.save()?;
    let result = (|| -> Result<()> {
        c.command(
            cmd(["docker", "volume", "create", &volume]),
            "create-volume",
            None,
            30.,
        )?;
        let script = "set -eu; cargo build --offline --locked --manifest-path /runner/Cargo.toml -p xtask --target-dir /data/runner-target; exec /data/runner-target/debug/xtask study-cli-upgrade --worker /seed --output /data/work";
        let executed = c.command(
            vec![
                "docker".into(),
                "run".into(),
                "--pull=never".into(),
                "--network=none".into(),
                "--cpus=2".into(),
                "--memory=3g".into(),
                "--pids-limit=256".into(),
                "--name".into(),
                container.clone().into(),
                "--workdir".into(),
                "/runner".into(),
                "--mount".into(),
                format!("type=volume,source={volume},target=/data").into(),
                "--mount".into(),
                format!("type=bind,source={},target=/seed,readonly", seed.display()).into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/runner,readonly",
                    runner.display()
                )
                .into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                    registry.display()
                )
                .into(),
                image.into(),
                "sh".into(),
                "-c".into(),
                script.into(),
            ],
            "docker-run",
            None,
            900.,
        );
        let copied = c.command(
            vec![
                "docker".into(),
                "cp".into(),
                format!("{container}:/data/work/result").into(),
                path(c.output.join("result")),
            ],
            "copyback",
            None,
            60.,
        );
        executed?;
        copied?;
        ensure!(
            inventory(&runner, &[])? == hashes,
            "frozen runner source changed"
        );
        Ok(())
    })();
    let retired = docker_retire(
        c,
        &container,
        &volume,
        [
            cmd(["docker", "rm", "-f", &container]),
            cmd(["docker", "volume", "rm", &volume]),
        ],
    );
    c.evidence["docker"]["cleanup_verified"] = json!(retired.is_ok());
    retired?;
    result
}

pub fn run(run: &Run, options: Options) -> Result<()> {
    #[cfg(unix)]
    if let Some(seed) = options.worker {
        let seed = dunce::canonicalize(seed)?;
        return worker(run, &seed, &options.output);
    }
    #[cfg(unix)]
    let output = assurance::fresh_output(&run.root, &options.output)?;
    #[cfg(windows)]
    let output = options.output;
    let mut c = Campaign::new(run, &output, "coordinated native StudyProgress CLI upgrade")?;
    #[cfg(unix)]
    c.limits(8 * 1024 * 1024, None)?;
    c.evidence["verified"] = json!(false);
    c.save()?;
    let result = (|| -> Result<()> {
        let mut identities = BTreeMap::new();
        for (label, reference) in [("old", HISTORICAL), ("new", options.new.as_str())] {
            let output = c.output.clone();
            let (revision, source, archive) = source_archive::revision(
                &output,
                reference,
                Some(label),
                |args, name, timeout| c.command(args, name, None, timeout),
            )?;
            let lock_object = source_archive::lock_object(
                &revision,
                &source.join("Cargo.lock"),
                label,
                |args, name, timeout| c.command(args, name, None, timeout),
            )?;
            c.evidence["sources"][label] = json!({"revision":revision,"archive_sha256":archive,"lock_object":lock_object,"files":inventory(&source,&[])?});
            identities.insert(label, revision);
        }
        let seed = c.output.join("source");
        source_locks(&seed)?;
        fs::write(
            seed.join("identities.json"),
            serde_json::to_vec(&identities)?,
        )?;
        fs::copy(run.root.join("tools").join(FIXTURE), seed.join(FIXTURE))?;
        c.evidence["fixture_source_sha256"] = json!(sha(&seed.join(FIXTURE))?);
        let fixture = generate(&seed.join("new"), &seed.join(FIXTURE))?;
        let hash = format!("{:x}", Sha256::digest(fixture.as_bytes()));
        let seed_hashes = inventory(&seed, &[])?;
        let native = |c: &Campaign<'_>| {
            let work = c.output.join("work");
            worker(run, &seed, &work)?;
            copy_tree(&work.join("result"), &c.output.join("result"))
        };
        #[cfg(unix)]
        match options.docker_image {
            Some(image) => docker(&mut c, &seed, &image)?,
            None => native(&c)?,
        };
        #[cfg(windows)]
        native(&c)?;
        ensure!(
            inventory(&seed, &[])? == seed_hashes,
            "frozen study seed changed"
        );
        let result = c.output.join("result");
        c.evidence["worker_sha256"] = json!(verify_copy(&result, &c.evidence["sources"], &hash)?);
        Ok(())
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;

    #[test]
    fn fixture_requires_one_native_binary_seam_and_preserves_public_oracles() {
        let helper = include_str!("../../apps/study-peer/tests/credential_rotation.rs");
        let scenario = include_str!("../../tools/study-cli-upgrade-fixture.rs");
        let generated = fixture(helper, scenario, false).unwrap();
        let helper_crlf = helper.replace("\r\n", "\n").replace('\n', "\r\n");
        let scenario_crlf = scenario.replace("\r\n", "\n").replace('\n', "\r\n");
        assert_eq!(
            fixture(&helper_crlf, &scenario_crlf, false).unwrap(),
            generated
        );
        assert!(generated.contains("Command::new(binary)"));
        assert!(!generated.contains("Command::new(BIN)"));
        let native_windows = fixture(helper, scenario, true).unwrap();
        assert!(native_windows.contains("STUDY_UPGRADE_RUNTIME"));
        assert!(!native_windows.contains("std::env::temp_dir().join"));
        assert!(generated.contains("STUDY_OLD_BINARY"));
        assert!(generated.contains("core(root, \"a\")"));
        assert!(fixture("changed helper", scenario, false).is_err());
        assert!(
            fixture(
                &helper.replace("Command::new(BIN)", "Command::new(OTHER)"),
                scenario,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn release_dependencies_must_include_runtime_and_exclude_the_test_module() {
        let runtime = r"C:\repo\src\crdt.rs C:\repo\src\crdt\multi_value_register.rs";
        assert!(verify_crdt_inputs(runtime).is_ok());
        assert!(verify_crdt_inputs("").is_err());
        assert!(verify_crdt_inputs(&format!("{runtime} /repo/src/crdt/decisions.rs")).is_err());
    }

    #[test]
    fn copied_study_evidence_rejects_mutated_artifact_source_or_fixture() {
        let tmp = Scratch::new(&std::env::temp_dir(), "study-copy").unwrap();
        let root = tmp.path();
        let mut sources = json!({});
        let mut worker = json!({"passed":true,"verified":true,"runtime_removed":true,"fixture_sha256":"fixture","sources":{}});
        for label in LABELS {
            let binary = root.join(format!("{label}-study-peer{EXE_SUFFIX}"));
            fs::write(&binary, label.as_bytes()).unwrap();
            fs::write(
                root.join(format!("{label}-study-peer.d")),
                "/repo/src/crdt.rs /repo/src/crdt/multi_value_register.rs",
            )
            .unwrap();
            let lock = root.join(format!("{label}-Cargo.lock"));
            fs::write(&lock, b"frozen lock").unwrap();
            let lock_hash = sha(&lock).unwrap();
            sources[label] = json!({"revision":label,"files":{"Cargo.lock":lock_hash}});
            worker["sources"][label] = sources[label].clone();
            worker["sources"][label]["binary_sha256"] = json!(sha(&binary).unwrap());
            worker["sources"][label]["lock_sha256"] = json!(lock_hash);
        }
        worker["artifacts"] = json!(inventory(root, &[]).unwrap());
        let evidence = root.join("evidence.json");
        fs::write(&evidence, worker.to_string()).unwrap();
        verify_copy(root, &sources, "fixture").unwrap();
        let binary = root.join(format!("old-study-peer{EXE_SUFFIX}"));
        fs::write(&binary, b"changed").unwrap();
        assert!(
            verify_copy(root, &sources, "fixture")
                .unwrap_err()
                .to_string()
                .contains("artifact")
        );
        fs::write(&binary, b"old").unwrap();
        assert!(
            verify_copy(root, &sources, "changed fixture")
                .unwrap_err()
                .to_string()
                .contains("fixture")
        );
        worker["sources"]["old"]["files"]["Cargo.lock"] = json!("changed");
        fs::write(&evidence, worker.to_string()).unwrap();
        assert!(
            verify_copy(root, &sources, "fixture")
                .unwrap_err()
                .to_string()
                .contains("source")
        );
    }
}
