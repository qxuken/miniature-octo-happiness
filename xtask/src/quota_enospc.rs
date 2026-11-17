//! Host controller for isolated Linux tmpfs maintenance exhaustion.
use crate::{
    artifact_trace::{docker_retire, inventory, nonce},
    assurance::{self, Campaign},
    delivery, faults,
    support::{Run, cmd, path, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf};
const DEFAULT_IMAGE: &str =
    "sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08";
const DRIVER: [&str; 5] = [
    "Cargo.toml",
    "Cargo.lock",
    "src/compound.rs",
    "src/resilience.rs",
    "src/space.rs",
];
#[derive(Args)]
pub struct Options {
    #[arg(long)]
    pub revision: String,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long,default_value=DEFAULT_IMAGE)]
    pub docker_image: String,
    #[arg(long, hide = true)]
    pub worker: Option<PathBuf>,
}
pub fn run(run: &Run, opts: Options) -> Result<()> {
    if let Some(source) = opts.worker {
        #[cfg(target_os = "linux")]
        {
            return crate::quota_host::enospc_worker(run, &source, &opts.revision, &opts.output);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = source;
            anyhow::bail!("native Linux worker required");
        }
    }
    let output = assurance::absolute(&run.root, &opts.output);
    ensure!(
        fs::symlink_metadata(&output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            && output.parent().is_some_and(std::path::Path::is_dir),
        "new evidence output with existing parent required"
    );
    let mut c = Campaign::new(run, &output, "Linux maintenance ENOSPC on disposable tmpfs")?;
    c.evidence["verified"] = json!(false);
    c.save()?;
    let result = (|| -> Result<()> {
        let rev = c.command(
            vec![
                "git".into(),
                "rev-parse".into(),
                "--verify".into(),
                "--end-of-options".into(),
                format!("{}^{{commit}}", opts.revision).into(),
            ],
            "engine-revision",
            None,
            30.,
        )?;
        ensure!(
            rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid engine SHA"
        );
        let files = c.command(
            cmd(["git", "ls-tree", "-r", &rev]),
            "engine-files",
            None,
            30.,
        )?;
        ensure!(
            !files.is_empty()
                && files
                    .lines()
                    .all(|l| l.starts_with("100644 blob ") || l.starts_with("100755 blob ")),
            "nonregular archive input"
        );
        let archive = output.join("source.tar");
        let source = output.join("source");
        fs::create_dir(&source)?;
        c.command(
            vec![
                "git".into(),
                "archive".into(),
                "--format=tar".into(),
                "--output".into(),
                path(&archive),
                rev.clone().into(),
            ],
            "archive",
            None,
            30.,
        )?;
        c.command(
            vec![
                "tar".into(),
                "-xf".into(),
                path(&archive),
                "-C".into(),
                path(&source),
            ],
            "extract",
            None,
            30.,
        )?;
        let mut overlay = BTreeMap::new();
        for file in DRIVER {
            let relative = PathBuf::from("tools/resource-envelope").join(file);
            let dest = source.join(&relative);
            fs::create_dir_all(dest.parent().unwrap())?;
            fs::copy(run.root.join(&relative), &dest)?;
            overlay.insert(relative, sha(&dest)?);
        }
        let frozen = inventory(&source, &[])?;
        c.evidence["engine_source"] = json!(rev);
        c.evidence["engine_source_hashes"] = json!(frozen);
        c.evidence["archive_sha256"] = json!(sha(&archive)?);
        c.evidence["driver_overlay"] = json!(overlay);
        let image = c.command(
            cmd([
                "docker",
                "image",
                "inspect",
                "--format={{.Id}}",
                &opts.docker_image,
            ]),
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
        let runner = output.join("runner");
        faults::stage_runner(&run.root, &runner)?;
        let frozen_runner = inventory(&runner, &[])?;
        let registry = toolchain::cargo_home(&run.root, &run.env)?.join("registry");
        let name = format!("maintenance-space-{}", nonce()?);
        let volume = format!("{name}-data");
        c.evidence["docker"] = json!({"image":image,"container":name,"volume":volume,"cleanup_verified":false,"runner_frozen_sha256":frozen_runner});
        c.save()?;
        let execution = (|| -> Result<()> {
            c.command(
                cmd(["docker", "volume", "create", &volume]),
                "volume-create",
                None,
                30.,
            )?;
            let script = "set -eu; cargo build --offline --locked --manifest-path /runner/Cargo.toml -p xtask --target-dir /data/runner-target; exec /data/runner-target/debug/xtask quota-enospc --worker /source --revision \"$1\" --output /data/result";
            let execute = c.command(
                vec![
                    "docker".into(),
                    "run".into(),
                    "--pull=never".into(),
                    "--network=none".into(),
                    "--memory=2g".into(),
                    "--cpus=2".into(),
                    "--pids-limit=256".into(),
                    "--ulimit=core=0".into(),
                    "--name".into(),
                    name.clone().into(),
                    "--tmpfs".into(),
                    "/faults:rw,size=256m,mode=755,nodev,nosuid,noexec".into(),
                    "--mount".into(),
                    format!("type=volume,source={volume},target=/data").into(),
                    "--mount".into(),
                    format!(
                        "type=bind,source={},target=/runner,readonly",
                        runner.display()
                    )
                    .into(),
                    "--mount".into(),
                    format!(
                        "type=bind,source={},target=/source,readonly",
                        source.display()
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
                    "quota-enospc".into(),
                    rev.into(),
                ],
                "linux",
                None,
                600.,
            );
            let copied = c.command(
                vec![
                    "docker".into(),
                    "cp".into(),
                    format!("{name}:/data/result").into(),
                    path(output.join("result")),
                ],
                "copyback",
                None,
                120.,
            );
            execute?;
            copied?;
            let worker = output.join("result");
            let file = worker.join("evidence.json");
            let proof = delivery::read_json(&file)?;
            ensure!(
                proof["passed"] == true
                    && proof["verified"] == true
                    && proof["maintenance_cases"]
                        .as_array()
                        .context("missing maintenance cases")?
                        .len()
                        == 3,
                "worker did not fully qualify"
            );
            ensure!(
                proof["engine_source_hashes"] == json!(frozen)
                    && inventory(&source, &[])? == frozen
                    && inventory(&runner, &[])? == frozen_runner,
                "frozen inputs changed"
            );
            ensure!(
                proof["artifact_sha256"]
                    == json!(inventory(&worker, &["evidence.json", "cargo-target"])?),
                "copied worker proof mismatch"
            );
            c.evidence["worker_evidence_sha256"] = json!(sha(&file)?);
            Ok(())
        })();
        // Only these generated names are used to construct destructive commands.
        let retired = docker_retire(
            &mut c,
            &name,
            &volume,
            [
                cmd(["docker", "rm", "-f", &name]),
                cmd(["docker", "volume", "rm", &volume]),
            ],
        );
        c.evidence["docker"]["cleanup_verified"] = json!(retired.is_ok());
        retired?;
        execution
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}
