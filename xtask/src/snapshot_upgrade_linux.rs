//! Optional Linux follower for the native-client upgrade sequence.
use super::{Endpoint, STREAM};
use crate::{
    artifact_trace::{docker_retire, nonce},
    assurance::Campaign,
    snapshot_proof::{Receipt, hex, receipt, selected_slot},
    support::{cmd, path, qualification_inventory as inventory, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::{
    ffi::OsString,
    fs,
    net::{SocketAddr, TcpListener},
    path::Path,
    time::Duration,
};

const PORT: u16 = 43179;
const NAMES: [&str; 2] = ["historical", "current"];

pub(super) struct Follower {
    container: String,
    volume: String,
    image: String,
    port: u16,
}
impl Follower {
    pub fn new(image: &str) -> Result<Self> {
        ensure!(
            image.strip_prefix("sha256:").is_some_and(|id| hex(id, 64)),
            "Docker follower requires an immutable local sha256 image ID"
        );
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let container = format!("weekend-snapshot-upgrade-{}", nonce()?);
        Ok(Self {
            volume: format!("{container}-data"),
            container,
            image: image.into(),
            port,
        })
    }
    fn exec(&self, args: Vec<OsString>) -> Vec<OsString> {
        let mut command = cmd(["docker", "exec", &self.container]);
        command.extend(args);
        command
    }
    fn cli(endpoint: usize) -> String {
        format!("/data/bin/{}-weekend-replicate", NAMES[endpoint])
    }
    fn fixture(endpoint: usize) -> String {
        format!("/data/bin/{}-fixture", NAMES[endpoint])
    }
    fn digest(&self, c: &mut Campaign<'_>, file: &str, label: &str) -> Result<String> {
        let line = c.command(self.exec(cmd(["sha256sum", file])), label, None, 30.)?;
        let (digest, actual) = line.split_once("  ").context("invalid Linux hash result")?;
        ensure!(hex(digest, 64) && actual == file, "invalid Linux file hash");
        Ok(digest.into())
    }
    fn copy_out(
        &self,
        c: &mut Campaign<'_>,
        remote: &str,
        local: &Path,
        label: &str,
    ) -> Result<()> {
        c.command(
            vec![
                "docker".into(),
                "cp".into(),
                format!("{}:{remote}", self.container).into(),
                path(local),
            ],
            label,
            None,
            60.,
        )?;
        Ok(())
    }
    pub fn prepare(
        &self,
        c: &mut Campaign<'_>,
        endpoints: [&Endpoint; 2],
        fixture: &Path,
    ) -> Result<()> {
        c.evidence["native_platform"] =
            json!(c.command(cmd(["uname", "-a"]), "native-platform", None, 30.)?);
        c.evidence["native_filesystem"] = json!(c.command(
            vec!["df".into(), "-P".into(), path(&c.output)],
            "native-filesystem",
            None,
            30.
        )?);
        c.evidence["topology"] = json!("native macOS client / Docker Linux ext4 follower");
        c.evidence["docker"] = json!({"image":self.image,"container":self.container,"volume":self.volume,
            "host_address":format!("127.0.0.1:{}", self.port),"cleanup_verified":false,
            "network":"Docker bridge with loopback-only published follower port; offline builds"});
        c.save()?;
        ensure!(
            c.command(
                cmd([
                    "docker",
                    "image",
                    "inspect",
                    "--format={{.Id}}",
                    &self.image
                ]),
                "linux-image",
                None,
                30.
            )? == self.image,
            "local Docker image identity mismatch"
        );
        let seed = c.output.join("linux-seed");
        fs::create_dir(&seed)?;
        for name in NAMES {
            fs::copy(
                c.output.join(format!("{name}.tar")),
                seed.join(format!("{name}.tar")),
            )?;
        }
        fs::copy(fixture, seed.join("fixture.rs"))?;
        c.evidence["linux_seed_sha256"] = json!(inventory(&seed, &[])?);
        let registry = toolchain::cargo_home(&c.run.root, &c.env)?.join("registry");
        c.command(
            cmd(["docker", "volume", "create", &self.volume]),
            "linux-create-volume",
            None,
            30.,
        )?;
        c.command(
            cmd([
                "docker",
                "run",
                "--pull=never",
                "-d",
                "--name",
                &self.container,
                "--memory=2g",
                "--cpus=2",
                "--pids-limit=256",
                "--publish",
                &format!("127.0.0.1:{}:{PORT}", self.port),
                "--mount",
                &format!("type=volume,source={},target=/data", self.volume),
                "--mount",
                &format!("type=bind,source={},target=/seed,readonly", seed.display()),
                "--mount",
                &format!(
                    "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                    registry.display()
                ),
                &self.image,
                "tail",
                "-f",
                "/dev/null",
            ]),
            "linux-start",
            None,
            60.,
        )?;
        c.evidence["linux_platform"] =
            json!(c.command(self.exec(cmd(["uname", "-a"])), "linux-platform", None, 30.)?);
        c.evidence["linux_compiler"] = json!(c.command(
            self.exec(cmd(["rustc", "-Vv"])),
            "linux-compiler",
            None,
            30.
        )?);
        c.evidence["linux_cargo"] =
            json!(c.command(self.exec(cmd(["cargo", "-V"])), "linux-cargo", None, 30.)?);
        let filesystem = c.command(
            self.exec(cmd([
                "findmnt",
                "-T",
                "/data",
                "-n",
                "-o",
                "FSTYPE,SOURCE,TARGET",
            ])),
            "linux-filesystem",
            None,
            30.,
        )?;
        ensure!(
            filesystem.split_ascii_whitespace().next() == Some("ext4"),
            "Docker follower requires ext4"
        );
        c.evidence["linux_filesystem"] = json!(filesystem);
        let local = c.output.join("linux-binaries");
        fs::create_dir(&local)?;
        c.command(
            self.exec(cmd([
                "mkdir",
                "-p",
                "/data/bin",
                "/data/source/historical",
                "/data/source/current",
                "/data/follower",
            ])),
            "linux-directories",
            None,
            30.,
        )?;
        for (i, endpoint) in endpoints.into_iter().enumerate() {
            let name = NAMES[i];
            let prefix = format!("linux-{name}");
            let archive = format!("/seed/{name}.tar");
            ensure!(
                self.digest(c, &archive, &format!("{prefix}-archive"))?
                    == sha(&seed.join(format!("{name}.tar")))?,
                "Linux archive digest mismatch"
            );
            let source = format!("/data/source/{name}");
            c.command(
                self.exec(cmd(["tar", "-xf", &archive, "-C", &source])),
                &format!("{prefix}-extract"),
                None,
                30.,
            )?;
            let revision = c.evidence["sources"][name]["commit"]
                .as_str()
                .context("source commit missing")?
                .to_owned();
            let target = format!("/data/build/{name}");
            c.command(
                cmd([
                    "docker",
                    "exec",
                    "--workdir",
                    &source,
                    "-e",
                    "CARGO_INCREMENTAL=0",
                    "-e",
                    &format!("WEEKEND_DB_SOURCE_ID={revision}"),
                    &self.container,
                    "cargo",
                    "build",
                    "--offline",
                    "--locked",
                    "--release",
                    "-p",
                    "weekend-replicate",
                    "--target-dir",
                    &target,
                ]),
                &format!("{prefix}-build"),
                None,
                240.,
            )?;
            let deps = format!("{target}/release/deps");
            let found = c.command(
                self.exec(cmd([
                    "find",
                    &deps,
                    "-maxdepth",
                    "1",
                    "-type",
                    "f",
                    "-name",
                    "libweekend_db*.rlib",
                ])),
                &format!("{prefix}-rlib-discovery"),
                None,
                30.,
            )?;
            let rlibs: Vec<_> = found.lines().collect();
            ensure!(
                rlibs.len() == 1
                    && rlibs[0].starts_with(&format!("{deps}/libweekend_db"))
                    && rlibs[0].ends_with(".rlib"),
                "one exact Linux engine rlib required"
            );
            let rlib = rlibs[0];
            let linked_hash = self.digest(c, rlib, &format!("{prefix}-rlib"))?;
            ensure!(
                self.digest(c, "/seed/fixture.rs", &format!("{prefix}-fixture-source"))?
                    == sha(fixture)?,
                "Linux fixture source mismatch"
            );
            c.command(
                self.exec(cmd([
                    "rustc",
                    "--edition=2024",
                    "-O",
                    "/seed/fixture.rs",
                    "--extern",
                    &format!("weekend_db={rlib}"),
                    "-L",
                    &format!("dependency={deps}"),
                    "-o",
                    &Self::fixture(i),
                ])),
                &format!("{prefix}-fixture-build"),
                None,
                90.,
            )?;
            ensure!(
                self.digest(c, rlib, &format!("{prefix}-rlib-after"))? == linked_hash,
                "Linux linked rlib changed"
            );
            c.command(
                self.exec(cmd([
                    "cp",
                    &format!("{target}/release/weekend-replicate"),
                    &Self::cli(i),
                ])),
                &format!("{prefix}-install-cli"),
                None,
                30.,
            )?;
            let identity = c.command(
                self.exec(cmd([&Self::fixture(i), "identity"])),
                &format!("{prefix}-identity"),
                None,
                30.,
            )?;
            ensure!(
                identity == revision,
                "Linux fixture runtime identity mismatch"
            );
            for (kind, remote) in [("cli", Self::cli(i)), ("fixture", Self::fixture(i))] {
                let file = local.join(format!("{name}-{kind}"));
                self.copy_out(c, &remote, &file, &format!("{prefix}-copy-{kind}"))?;
                let digest = sha(&file)?;
                ensure!(
                    self.digest(c, &remote, &format!("{prefix}-{kind}-hash"))? == digest,
                    "copied Linux executable differs"
                );
                c.evidence["linux_binaries"][name][kind] = json!(digest);
            }
            let lock = self.digest(
                c,
                &format!("{source}/Cargo.lock"),
                &format!("{prefix}-lock"),
            )?;
            ensure!(
                lock == sha(&endpoint.source.join("Cargo.lock"))?,
                "Linux source lock differs from immutable archive"
            );
            c.evidence["linux_binaries"][name]["runtime_source_id"] = json!(identity);
            c.evidence["linux_binaries"][name]["rlib"] = json!(linked_hash);
            c.evidence["linux_binaries"][name]["lock_sha256"] = json!(lock);
        }
        c.save()
    }
    pub fn initialize(&self, c: &mut Campaign<'_>, identity: &str) -> Result<()> {
        c.command(
            self.exec(cmd([
                &Self::cli(0),
                "follower-init",
                "/data/follower",
                STREAM,
                identity,
            ])),
            "linux-follower-init",
            None,
            45.,
        )?;
        Ok(())
    }
    pub fn credentials(&self, c: &mut Campaign<'_>, server: &Path, client: &Path) -> Result<()> {
        for (source, destination, label) in [
            (server.to_owned(), "server", "linux-server-credentials"),
            (
                client.join("cert.der"),
                "client-cert.der",
                "linux-client-certificate",
            ),
        ] {
            c.command(
                vec![
                    "docker".into(),
                    "cp".into(),
                    path(source),
                    format!("{}:/data/{destination}", self.container).into(),
                ],
                label,
                None,
                60.,
            )?;
        }
        Ok(())
    }
    pub fn serve<T>(
        &self,
        c: &mut Campaign<'_>,
        endpoint: usize,
        label: &str,
        identity: &str,
        call: impl FnOnce(&mut Campaign<'_>, SocketAddr) -> Result<T>,
    ) -> Result<T> {
        // Finite servers retire in the container itself; stopping docker exec
        // alone cannot prove that its remote child has exited.
        let exchanges = match label {
            "old-server" => 2,
            "old-server-new-client" => 3,
            "retry-new-to-old" => 1,
            "new-server-old-client" => 2,
            "both-new" => 3,
            _ => anyhow::bail!("unknown Linux upgrade stage"),
        };
        c.ready_process(
            self.exec(cmd([
                &Self::cli(endpoint),
                "serve",
                "/data/follower",
                STREAM,
                identity,
                &format!("0.0.0.0:{PORT}"),
                "/data/server",
                "/data/client-cert.der",
                "--max-exchanges",
                &exchanges.to_string(),
            ])),
            label,
            (Duration::from_secs(10), 200.),
            true,
            |line| {
                ensure!(
                    line == format!("ready 0.0.0.0:{PORT}"),
                    "unexpected Linux server readiness"
                );
                Ok(SocketAddr::from(([127, 0, 0, 1], self.port)))
            },
            call,
        )
    }
    pub fn offline(
        &self,
        c: &mut Campaign<'_>,
        endpoint: usize,
        identity: &str,
        expected: &Receipt,
        export: &Path,
        label: &str,
    ) -> Result<()> {
        let status = c.command(
            self.exec(cmd([
                &Self::cli(endpoint),
                "status",
                "/data/follower",
                STREAM,
                identity,
            ])),
            &format!("linux-{label}-status"),
            None,
            45.,
        )?;
        ensure!(
            receipt(&status, STREAM)? == *expected,
            "Linux reopened durable receipt mismatch"
        );
        let file = format!("/data/follower/replica-{}.wdb", selected_slot(&status)?);
        let before = self.digest(c, &file, &format!("linux-{label}-selected-before"))?;
        ensure!(
            before == sha(export)?,
            "Linux selected image differs from export"
        );
        for i in 0..2 {
            c.command(
                self.exec(cmd([
                    &Self::fixture(i),
                    "check",
                    &file,
                    &expected.revision.to_string(),
                ])),
                &format!("linux-{label}-oracle-{i}"),
                None,
                45.,
            )?;
        }
        ensure!(
            self.digest(c, &file, &format!("linux-{label}-selected-after"))? == before,
            "Linux oracle changed selected image"
        );
        Ok(())
    }
    pub fn copy_follower(&self, c: &mut Campaign<'_>, local: &Path, label: &str) -> Result<()> {
        // Keep a fresh byte-for-byte mirror for both native fixture oracles and
        // the existing whole-directory mutation checks.
        fs::remove_dir_all(local)?;
        self.copy_out(
            c,
            "/data/follower",
            local,
            &format!("linux-{label}-copy-follower"),
        )
    }
    pub fn verify(&self, c: &mut Campaign<'_>, endpoints: [&Endpoint; 2]) -> Result<()> {
        for (i, endpoint) in endpoints.into_iter().enumerate() {
            let name = NAMES[i];
            let copied = c.output.join(format!("linux-source-{name}"));
            self.copy_out(
                c,
                &format!("/data/source/{name}"),
                &copied,
                &format!("linux-{name}-copy-source"),
            )?;
            ensure!(
                inventory(&copied, &[])? == endpoint.inventory,
                "Linux source changed during qualification"
            );
            for (kind, remote) in [("cli", Self::cli(i)), ("fixture", Self::fixture(i))] {
                let digest = self.digest(c, &remote, &format!("linux-{name}-{kind}-final"))?;
                ensure!(
                    c.evidence["linux_binaries"][name][kind].as_str() == Some(&digest),
                    "Linux executable changed"
                );
            }
        }
        ensure!(
            json!(inventory(&c.output.join("linux-seed"), &[])?) == c.evidence["linux_seed_sha256"],
            "frozen Linux seed changed"
        );
        c.evidence["linux_sources_and_binaries_unchanged"] = json!(true);
        Ok(())
    }
    pub fn cleanup(&self, c: &mut Campaign<'_>) -> Result<()> {
        let result = docker_retire(
            c,
            &self.container,
            &self.volume,
            [
                cmd(["docker", "rm", "-f", &self.container]),
                cmd(["docker", "volume", "rm", &self.volume]),
            ],
        );
        c.evidence["docker"]["cleanup_verified"] = json!(result.is_ok());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutable_or_malformed_images_are_rejected_before_docker() {
        for image in [
            "rust:latest",
            "sha256:abc",
            &format!("sha256:{}", "G".repeat(64)),
        ] {
            assert!(Follower::new(image).is_err());
        }
    }
}
