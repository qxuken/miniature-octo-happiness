//! Exact-build StudyProgress CLI upgrade with the existing public-API fixture.
use crate::{
    artifact_trace::{copy_tree, docker_retire, inventory, nonce},
    assurance::{self, Campaign},
    delivery, faults, serialization_scale,
    support::{Run, cmd, path, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

const HISTORICAL: &str = "fb80989d98bd977743168ee10418a434a67a0261";
const CURRENT: &str = "365daf6";
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
    #[arg(long)]
    docker_image: Option<String>,
    #[arg(long, hide = true)]
    worker: Option<PathBuf>,
}

fn fixture(helpers: &str, scenario: &str) -> Result<String> {
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
    let moved = helpers
        .replace(old, "        (log, binary): (&Log, &str),\n    ) -> Self {")
        .replace("Command::new(BIN)", "Command::new(binary)");
    Ok(format!("#![allow(dead_code)]\n{moved}{scenario}"))
}

fn generate(source: &Path, scenario: &Path) -> Result<String> {
    fixture(
        &fs::read_to_string(source.join("apps/study-peer/tests/credential_rotation.rs"))?,
        &fs::read_to_string(scenario)?,
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
        inventory(&source.join("old/src/crdt"), &[])?
            == inventory(&source.join("new/src/crdt"), &[])?,
        "unqualified CRDT container change"
    );
    Ok(())
}

fn worker(run: &Run, seed: &Path, work: &Path) -> Result<()> {
    let work = assurance::fresh(&run.root, work)?;
    let mut c = Campaign::detached(
        run,
        &work.join("result"),
        "mixed-version study CLI architecture",
    )?;
    c.limits(8 * 1024 * 1024, None)?;
    let runtime = work.join("runtime");
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    c.env = serialization_scale::clean_env(&c.env);
    c.env.insert("TMPDIR".into(), path(&runtime));
    c.evidence["verified"] = json!(false);
    c.save()?;
    let result = (|| -> Result<()> {
        c.evidence["compiler"] = json!(c.command(cmd(["rustc", "-vV"]), "compiler", None, 30.)?);
        c.evidence["cargo"] = json!(c.command(cmd(["cargo", "-V"]), "cargo", None, 30.)?);
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
        c.evidence["filesystem"] = json!(c.command(filesystem, "filesystem", None, 30.)?);
        let identities = delivery::read_json(&seed.join("identities.json"))?;
        let mut generated = None;
        let mut sources = BTreeMap::new();
        for label in LABELS {
            let source = work.join(label);
            copy_tree(&seed.join(label), &source)?;
            let bound = inventory(&source, &[])?;
            let lock_hash = sha(&source.join("Cargo.lock"))?;
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
            let binary = target.join("release/study-peer");
            let copied = c.output.join(format!("{label}-study-peer"));
            fs::copy(&binary, &copied)?;
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
                c.evidence["fixture_sha256"] = json!(sha(&path)?);
                generated = Some(path);
            }
            c.save()?;
        }
        let source = work.join("new");
        let target = work.join("new-build");
        for (var, output) in [
            ("STUDY_OLD_BINARY", "old-study-peer"),
            ("STUDY_NEW_BINARY", "new-study-peer"),
            ("STUDY_UPGRADE_EVIDENCE", "proof"),
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
                    && sha(&c.output.join(format!("{label}-study-peer")))? == hash,
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
        let observed = &proof["sources"][label];
        ensure!(
            observed["files"] == source[label]["files"]
                && observed["revision"] == source[label]["revision"],
            "copied source differs from frozen seed"
        );
        ensure!(
            observed["binary_sha256"] == sha(&result.join(format!("{label}-study-peer")))?,
            "copied study binary differs"
        );
    }
    ensure!(
        proof["fixture_sha256"] == fixture_hash,
        "copied study fixture differs"
    );
    sha(&path)
}

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
    if let Some(seed) = options.worker {
        let seed = dunce::canonicalize(seed)?;
        return worker(run, &seed, &options.output);
    }
    let output = assurance::fresh_output(&run.root, &options.output)?;
    let mut c = Campaign::new(run, &output, "coordinated native StudyProgress CLI upgrade")?;
    c.limits(8 * 1024 * 1024, None)?;
    c.evidence["verified"] = json!(false);
    c.save()?;
    let result = (|| -> Result<()> {
        let mut identities = BTreeMap::new();
        for (label, reference) in [("old", HISTORICAL), ("new", options.new.as_str())] {
            let (revision, source, archive) =
                assurance::archive_revision(&mut c, reference, Some(label))?;
            c.evidence["sources"][label] = json!({"revision":revision,"archive_sha256":archive,"files":inventory(&source,&[])?});
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
        if let Some(image) = options.docker_image {
            docker(&mut c, &seed, &image)?;
        } else {
            let work = c.output.join("work");
            worker(run, &seed, &work)?;
            copy_tree(&work.join("result"), &c.output.join("result"))?;
        }
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
        let generated = fixture(helper, scenario).unwrap();
        assert!(generated.contains("Command::new(binary)"));
        assert!(!generated.contains("Command::new(BIN)"));
        assert!(generated.contains("STUDY_OLD_BINARY"));
        assert!(generated.contains("core(root, \"a\")"));
        assert!(fixture("changed helper", scenario).is_err());
        assert!(
            fixture(
                &helper.replace("Command::new(BIN)", "Command::new(OTHER)"),
                scenario
            )
            .is_err()
        );
    }

    #[test]
    fn copied_study_evidence_rejects_mutated_artifact_source_or_fixture() {
        let tmp = Scratch::new(&std::env::temp_dir(), "study-copy").unwrap();
        let root = tmp.path();
        let mut sources = json!({});
        let mut worker = json!({"passed":true,"verified":true,"runtime_removed":true,"fixture_sha256":"fixture","sources":{}});
        for label in LABELS {
            let binary = root.join(format!("{label}-study-peer"));
            fs::write(&binary, label.as_bytes()).unwrap();
            sources[label] = json!({"revision":label,"files":{"Cargo.lock":"frozen"}});
            worker["sources"][label] = sources[label].clone();
            worker["sources"][label]["binary_sha256"] = json!(sha(&binary).unwrap());
        }
        worker["artifacts"] = json!(inventory(root, &[]).unwrap());
        let evidence = root.join("evidence.json");
        fs::write(&evidence, worker.to_string()).unwrap();
        verify_copy(root, &sources, "fixture").unwrap();
        let binary = root.join("old-study-peer");
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
