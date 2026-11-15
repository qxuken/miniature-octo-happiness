//! Native authenticated follower backup and fresh-stream reseed qualification.
use crate::{
    artifact_trace::inventory,
    assurance::{self, Campaign},
    serialization_scale,
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde::Serialize;
use serde_json::json;
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Args)]
pub struct Options {
    #[arg(long, default_value = "65d142e")]
    source: String,
    #[arg(long)]
    output: PathBuf,
}

const RECEIPT: &str = "follower durable_revision=";
fn hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Receipt {
    pub revision: u64,
    pub stream: String,
    pub digest: String,
}
pub(crate) fn receipt(output: &str, expected_stream: &str) -> Result<Receipt> {
    let mut receipts = output.lines().filter_map(|line| line.strip_prefix(RECEIPT));
    let row = receipts.next().context("durable receipt missing")?;
    ensure!(receipts.next().is_none(), "duplicate durable receipt");
    let (revision, rest) = row
        .split_once(" stream=")
        .context("missing receipt stream")?;
    let (stream, digest) = rest
        .split_once(" digest=")
        .context("missing receipt digest")?;
    ensure!(
        hex(stream, 32) && stream == expected_stream && hex(digest, 64),
        "invalid durable stream or digest"
    );
    ensure!(
        !revision.is_empty()
            && revision.len() <= 20
            && revision.bytes().all(|c| c.is_ascii_digit())
            && (revision == "0" || !revision.starts_with('0')),
        "invalid durable revision"
    );
    Ok(Receipt {
        revision: revision.parse()?,
        stream: stream.into(),
        digest: digest.into(),
    })
}

pub(crate) fn description_identity(text: &str) -> Result<String> {
    let mut values = text
        .split_ascii_whitespace()
        .filter_map(|part| part.strip_prefix("file_identity="));
    let identity = values.next().context("CLI file identity missing")?;
    ensure!(
        hex(identity, 32) && values.next().is_none(),
        "invalid or duplicate CLI file identity"
    );
    Ok(identity.to_owned())
}

pub(crate) fn selected_slot(status: &str) -> Result<&str> {
    let mut fields = status
        .split_ascii_whitespace()
        .filter_map(|part| part.strip_prefix("selected_slot="));
    let slot = match fields.next() {
        Some("Some(0)") => "0",
        Some("Some(1)") => "1",
        _ => anyhow::bail!("selected follower slot missing"),
    };
    ensure!(fields.next().is_none(), "duplicate selected follower slot");
    Ok(slot)
}

fn cli(
    c: &mut Campaign<'_>,
    args: Vec<OsString>,
    label: &str,
    expected: Option<&str>,
) -> Result<String> {
    c.command(args, label, expected, 60.)
}

struct Programs {
    cli: PathBuf,
    admin: PathBuf,
    oracle: PathBuf,
}

struct Transport<'a> {
    programs: &'a Programs,
    client: PathBuf,
    server: PathBuf,
    identity: String,
}
struct Replica<'a> {
    dir: &'a Path,
    stream: &'a str,
}
struct Shipment<'a> {
    source: &'a Path,
    export: &'a Path,
    stream: &'a str,
    receiver: Replica<'a>,
    reject: bool,
}
impl Transport<'_> {
    fn status(
        &self,
        c: &mut Campaign<'_>,
        follower: &Replica<'_>,
        label: &str,
        expected: Option<&str>,
    ) -> Result<String> {
        cli(
            c,
            vec![
                path(&self.programs.cli),
                "status".into(),
                path(follower.dir),
                follower.stream.into(),
                self.identity.clone().into(),
            ],
            label,
            expected,
        )
    }

    fn init(&self, c: &mut Campaign<'_>, follower: &Replica<'_>, label: &str) -> Result<()> {
        fs::create_dir(follower.dir)?;
        cli(
            c,
            vec![
                path(&self.programs.cli),
                "follower-init".into(),
                path(follower.dir),
                follower.stream.into(),
                self.identity.clone().into(),
            ],
            label,
            None,
        )?;
        Ok(())
    }

    fn ship(
        &self,
        c: &mut Campaign<'_>,
        shipping: &Shipment<'_>,
        label: &str,
    ) -> Result<Option<(Receipt, PathBuf)>> {
        let received = &shipping.receiver;
        let server = vec![
            path(&self.programs.cli),
            "serve".into(),
            path(received.dir),
            received.stream.into(),
            self.identity.clone().into(),
            "127.0.0.1:0".into(),
            path(&self.server),
            path(self.client.join("cert.der")),
            "--max-exchanges".into(),
            "1".into(),
        ];
        let output = c.loopback(
            server,
            &format!("{label}-server"),
            (Duration::from_secs(20), 110.),
            true,
            |campaign, address| {
                cli(
                    campaign,
                    vec![
                        path(&self.programs.cli),
                        "ship".into(),
                        path(shipping.source),
                        shipping.stream.into(),
                        path(shipping.export),
                        address.to_string().into(),
                        path(&self.client),
                        path(self.server.join("cert.der")),
                        "--attempts".into(),
                        "1".into(),
                    ],
                    &format!("{label}-ship"),
                    shipping.reject.then_some("ForeignStore"),
                )
            },
        )?;
        ensure!(
            output
                .lines()
                .any(|line| line.starts_with("local captured_revision=4 ")),
            "local snapshot capture missing"
        );
        if shipping.reject {
            ensure!(
                !output.lines().any(|line| line.starts_with(RECEIPT)),
                "failed ship reported durability"
            );
            return Ok(None);
        }
        let ack = receipt(&output, shipping.stream)?;
        ensure!(
            ack.revision == 4 && ack.digest == sha(shipping.export)?,
            "receipt differs from the exported snapshot"
        );
        let status = self.status(c, received, &format!("{label}-status"), None)?;
        ensure!(
            receipt(&status, received.stream)? == ack
                && status
                    .split_ascii_whitespace()
                    .any(|part| part == "requires_recovery=false"),
            "reopened durable receipt differs"
        );
        let selected = received
            .dir
            .join(format!("replica-{}.wdb", selected_slot(&status)?));
        ensure!(
            sha(&selected)? == ack.digest,
            "selected file differs from durable receipt"
        );
        cli(
            c,
            vec![path(&self.programs.oracle), "read".into(), path(&selected)],
            &format!("{label}-oracle"),
            None,
        )?;
        ensure!(
            sha(&selected)? == ack.digest,
            "oracle changed selected image"
        );
        Ok(Some((ack, selected)))
    }
}

pub(crate) fn scrub_keys(directories: &[&Path]) -> Result<()> {
    let mut failures = Vec::new();
    for directory in directories {
        let path = directory.join("key.der");
        if let Err(error) = fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            failures.push(format!("{}: {error}", path.display()));
        }
    }
    ensure!(
        failures.is_empty(),
        "private key cleanup failed: {}",
        failures.join(", ")
    );
    Ok(())
}

fn qualify(c: &mut Campaign<'_>, p: &Programs, client: &Path, server: &Path) -> Result<()> {
    cli(
        c,
        vec![path(&p.cli), "keygen".into(), path(client)],
        "client-keygen",
        None,
    )?;
    cli(
        c,
        vec![path(&p.cli), "keygen".into(), path(server)],
        "server-keygen",
        None,
    )?;
    let primary = c.output.join("original-primary.wdb");
    cli(
        c,
        vec![path(&p.oracle), "write".into(), path(&primary)],
        "seed",
        None,
    )?;
    let old = c.command(
        cmd(["openssl", "rand", "-hex", "16"]),
        "old-stream",
        None,
        30.,
    )?;
    let new = c.command(
        cmd(["openssl", "rand", "-hex", "16"]),
        "new-stream",
        None,
        30.,
    )?;
    ensure!(
        old != new
            && [&old, &new]
                .into_iter()
                .all(|v| hex(v, 32) && v != &"0".repeat(32)),
        "invalid fresh stream identities"
    );
    let description = cli(
        c,
        vec![
            path(&p.cli),
            "describe".into(),
            path(&primary),
            old.clone().into(),
        ],
        "describe-original",
        None,
    )?;
    let transport = Transport {
        programs: p,
        client: client.to_owned(),
        server: server.to_owned(),
        identity: description_identity(&description)?,
    };
    let old_dir = c.output.join("old-follower");
    let old_export = c.output.join("old-export.wdb");
    transport.init(
        c,
        &Replica {
            dir: &old_dir,
            stream: &old,
        },
        "old-follower-init",
    )?;
    let (old_ack, selected) = transport
        .ship(
            c,
            &Shipment {
                source: &primary,
                export: &old_export,
                stream: &old,
                receiver: Replica {
                    dir: &old_dir,
                    stream: &old,
                },
                reject: false,
            },
            "original",
        )?
        .context("original follower receipt")?;
    let original_primary = sha(&primary)?;
    let original_follower = inventory(&old_dir, &[])?;
    let backup = c.output.join("verified-backup.wdb");
    let restored = c.output.join("restored-primary.wdb");
    cli(
        c,
        vec![
            path(&p.admin),
            "backup".into(),
            path(&selected),
            path(&backup),
        ],
        "backup",
        None,
    )?;
    cli(
        c,
        vec![
            path(&p.admin),
            "restore".into(),
            path(&backup),
            path(&restored),
        ],
        "restore",
        None,
    )?;
    let restored_hash = sha(&restored)?;
    cli(
        c,
        vec![
            path(&p.admin),
            "restore".into(),
            path(&backup),
            path(&restored),
        ],
        "reject-existing-restore",
        Some("AlreadyExists"),
    )?;
    ensure!(
        sha(&restored)? == restored_hash,
        "existing restore destination changed"
    );
    for (name, file) in [("backup", &backup), ("restored", &restored)] {
        cli(
            c,
            vec![path(&p.admin), "check".into(), path(file)],
            &format!("check-{name}"),
            None,
        )?;
        cli(
            c,
            vec![path(&p.oracle), "read".into(), path(file)],
            &format!("oracle-{name}"),
            None,
        )?;
    }
    transport.status(
        c,
        &Replica {
            dir: &old_dir,
            stream: &new,
        },
        "reject-wrong-enrollment",
        Some("ForeignStore"),
    )?;
    let rejected_export = c.output.join("rejected-export.wdb");
    transport.ship(
        c,
        &Shipment {
            source: &restored,
            export: &rejected_export,
            stream: &new,
            receiver: Replica {
                dir: &old_dir,
                stream: &old,
            },
            reject: true,
        },
        "reject-upload",
    )?;
    ensure!(
        inventory(&old_dir, &[])? == original_follower,
        "wrong enrollment changed old follower"
    );
    let old_status = transport.status(
        c,
        &Replica {
            dir: &old_dir,
            stream: &old,
        },
        "reopen-old-follower",
        None,
    )?;
    ensure!(
        receipt(&old_status, &old)? == old_ack,
        "old durable receipt changed"
    );
    let new_dir = c.output.join("new-follower");
    let description = cli(
        c,
        vec![
            path(&p.cli),
            "describe".into(),
            path(&restored),
            new.clone().into(),
        ],
        "describe-restored",
        None,
    )?;
    ensure!(
        description_identity(&description)? == transport.identity,
        "restored file identity changed"
    );
    transport.init(
        c,
        &Replica {
            dir: &new_dir,
            stream: &new,
        },
        "new-follower-init",
    )?;
    let new_export = c.output.join("new-export.wdb");
    let (new_ack, _) = transport
        .ship(
            c,
            &Shipment {
                source: &restored,
                export: &new_export,
                stream: &new,
                receiver: Replica {
                    dir: &new_dir,
                    stream: &new,
                },
                reject: false,
            },
            "reseed",
        )?
        .context("fresh follower receipt")?;
    ensure!(
        sha(&primary)? == original_primary && inventory(&old_dir, &[])? == original_follower,
        "original primary or follower changed"
    );
    c.evidence["file_identity"] = json!(transport.identity);
    c.evidence["old_receipt"] = json!(old_ack);
    c.evidence["new_receipt"] = json!(new_ack);
    c.evidence["retained"] = json!({"id":1,"revision":2});
    c.evidence["originals_unchanged"] = json!(true);
    c.evidence["wrong_enrollment_rejected_unchanged"] = json!(true);
    c.evidence["backup_sha256"] = json!(sha(&backup)?);
    c.evidence["restored_sha256"] = json!(restored_hash);
    Ok(())
}

pub fn run(run: &Run, opts: Options) -> Result<()> {
    let mut c = Campaign::new(
        run,
        &opts.output,
        "authenticated backup/restore and explicit fresh-stream reseed",
    )?;
    c.limits(8 * 1024 * 1024, None)?;
    c.evidence["verified"] = json!(false);
    c.save()?;
    let client = c.output.join("primary-credentials");
    let server = c.output.join("follower-credentials");
    let result = (|| -> Result<()> {
        let (revision, source) = assurance::archive_source(&mut c, &opts.source)?;
        let fixture = run.root.join("tools/compatibility-driver.rs");
        let fixture_hash = sha(&fixture)?;
        let oracle_path = source.join("tools/backup-oracle.rs");
        let mut out = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&oracle_path)?;
        out.write_all(&fs::read(&fixture)?)?;
        c.evidence["fixture_sha256"] = json!(fixture_hash);
        let manifest = source.join("Cargo.toml");
        let original: toml::Value = toml::from_str(&fs::read_to_string(&manifest)?)?;
        ensure!(
            original["package"]["publish"].as_bool() == Some(false),
            "engine manifest must be private"
        );
        writeln!(
            OpenOptions::new().append(true).open(&manifest)?,
            "\n[[example]]\nname=\"backup_oracle\"\npath=\"tools/backup-oracle.rs\""
        )?;
        let bound = inventory(&source, &[])?;
        c.evidence["engine_source_sha256"] = json!(bound);
        let lock = source.join("Cargo.lock");
        c.evidence["lock_sha256"] = json!(sha(&lock)?);
        fs::copy(&lock, c.output.join("Cargo.lock"))?;
        c.env = serialization_scale::clean_env(&c.env);
        c.env.insert("WEEKEND_DB_SOURCE_ID".into(), revision.into());
        c.evidence["rustc"] = json!(c.command(cmd(["rustc", "-Vv"]), "rustc", None, 30.)?);
        let target = c.output.join("build");
        let mut build = cmd([
            "cargo",
            "build",
            "--offline",
            "--locked",
            "-p",
            "weekend-db",
            "--bin",
            "weekend-admin",
            "--example",
            "backup_oracle",
            "-p",
            "weekend-replicate",
            "--bin",
            "weekend-replicate",
        ]);
        build.extend([
            "--manifest-path".into(),
            path(manifest),
            "--target-dir".into(),
            path(&target),
        ]);
        c.command_at(build, "build", None, 300., &source)?;
        ensure!(
            inventory(&source, &[])? == bound,
            "build changed frozen source"
        );
        let copy = |name: &str, file: &str| -> Result<PathBuf> {
            let out = c.output.join(name);
            fs::copy(target.join("debug").join(file), &out)?;
            fs::set_permissions(&out, fs::Permissions::from_mode(0o700))?;
            Ok(out)
        };
        let programs = Programs {
            cli: copy("cli", "weekend-replicate")?,
            admin: copy("admin", "weekend-admin")?,
            oracle: copy("oracle", "examples/backup_oracle")?,
        };
        let artifacts = || -> Result<serde_json::Value> {
            Ok(
                json!({"cli":sha(&programs.cli)?,"admin":sha(&programs.admin)?,"oracle":sha(&programs.oracle)?}),
            )
        };
        let binary_hashes = artifacts()?;
        c.evidence["binary_sha256"] = binary_hashes.clone();
        qualify(&mut c, &programs, &client, &server)?;
        ensure!(
            inventory(&source, &[])? == bound
                && artifacts()? == binary_hashes
                && sha(&fixture)? == fixture_hash,
            "frozen source, fixture or executable changed"
        );
        Ok(())
    })();
    finish_with_key_cleanup(c, result, &[&client, &server])
}

pub(crate) fn finish_with_key_cleanup(
    mut c: Campaign<'_>,
    result: Result<()>,
    directories: &[&Path],
) -> Result<()> {
    let scrubbed = scrub_keys(directories);
    c.evidence["private_credentials_retained"] = json!(scrubbed.is_err());
    let result = match (result, scrubbed) {
        (Ok(()), Ok(())) => {
            let artifacts = inventory(&c.output, &["source", "build", "evidence.json"]);
            artifacts.map(|files| c.evidence["artifacts"] = json!(files))
        }
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(work), Err(cleanup)) => Err(anyhow::anyhow!("{work:#}\n{cleanup:#}")),
    };
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;

    #[test]
    fn receipt_parser_separates_local_capture_and_durable_acknowledgment() {
        let stream = "11".repeat(16);
        let digest = "aa".repeat(32);
        let good = format!("follower durable_revision=4 stream={stream} digest={digest}");
        let local = "local captured_revision=4 snapshot=prepared";
        assert!(receipt(local, &stream).is_err());
        let observed = receipt(&format!("{local}\n{good}\n"), &stream).unwrap();
        assert_eq!(observed.revision, 4);
        assert_eq!(observed.stream, stream);
        assert_eq!(observed.digest, digest);
        assert!(receipt(&format!("{good}\n{good}"), &stream).is_err());
        assert!(receipt(&good, &"22".repeat(16)).is_err());
        assert!(receipt(&format!("{good} extra"), &stream).is_err());
        for revision in ["04", "-4", "18446744073709551616"] {
            let bad = good.replacen("revision=4", &format!("revision={revision}"), 1);
            assert!(receipt(&bad, &stream).is_err());
        }
        let oversized = good.replacen("revision=4", &format!("revision={}", "1".repeat(5000)), 1);
        assert!(receipt(&oversized, &stream).is_err());
        assert_eq!(selected_slot("selected_slot=Some(1)").unwrap(), "1");
        assert!(selected_slot("selected_slot=Some(2)").is_err());
    }

    #[test]
    fn key_cleanup_attempts_all_owned_credential_directories() {
        let tmp = Scratch::new(&std::env::temp_dir(), "backup-key-control").unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir_all(a.join("key.der")).unwrap();
        fs::create_dir(&b).unwrap();
        fs::write(b.join("key.der"), b"owned-private").unwrap();
        fs::write(b.join("cert.der"), b"public").unwrap();
        assert!(scrub_keys(&[&a, &b]).is_err());
        assert!(!b.join("key.der").exists());
        assert_eq!(fs::read(b.join("cert.der")).unwrap(), b"public");
        scrub_keys(&[&b]).unwrap();
    }
}
