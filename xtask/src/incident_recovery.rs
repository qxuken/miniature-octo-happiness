//! Finite operator rehearsal. Every writable path is under a newly owned directory.
#[cfg(unix)]
use crate::assurance::Campaign;
use crate::support::{Run, cmd, path, qualification_inventory, sha};
#[cfg(windows)]
use crate::windows_compatibility::Campaign;
use anyhow::{Result, ensure};
use clap::Args;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/logical-v1.bin");
fn stderr_name(label: &str) -> String {
    format!("{label}.stderr{}", if cfg!(windows) { "" } else { ".log" })
}
fn protect_source(file: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file, fs::Permissions::from_mode(0o400))?;
    }
    #[cfg(windows)]
    {
        let mut permissions = fs::metadata(file)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(file, permissions)?;
        ensure!(
            fs::OpenOptions::new()
                .write(true)
                .open(file)
                .err()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied),
            "Windows source still admits a writable open"
        );
    }
    ensure!(
        fs::metadata(file)?.permissions().readonly(),
        "source write protection missing"
    );
    Ok(())
}
#[derive(Args)]
pub struct Options {
    #[arg(long)]
    admin: PathBuf,
    #[arg(long)]
    coordinator: PathBuf,
    /// Operator-declared provenance of the selected CLI artifacts.
    #[arg(long)]
    artifact_source: String,
    #[arg(long)]
    output: PathBuf,
}
fn unchanged(path: &str, original: &[u8]) -> Result<()> {
    ensure!(
        fs::read(path)? == original,
        "preserved bytes changed: {path}"
    );
    Ok(())
}

struct Binaries {
    admin: PathBuf,
    coordinator: PathBuf,
    consumer: PathBuf,
}
struct Drill<'a, 'r> {
    c: &'a mut Campaign<'r>,
    binaries: &'a Binaries,
}
impl<'r> Drill<'_, 'r> {
    fn path(&self, name: &str) -> String {
        self.c.output.join(name).to_str().unwrap().to_owned()
    }
    fn command(
        &mut self,
        name: &str,
        program: &Path,
        args: &[&str],
        success: bool,
    ) -> Result<String> {
        let mut argv = vec![path(program)];
        argv.extend(cmd(args.iter().copied()));
        if success {
            self.c.command(argv, name, None, 15.)
        } else {
            self.c.command_exit(argv, name, 1, 15.)
        }
    }
    fn admin(&mut self, name: &str, args: &[&str], success: bool) -> Result<String> {
        self.command(name, &self.binaries.admin.clone(), args, success)
    }
    fn rejected_admin(&mut self, name: &str, args: &[&str], error: &str) -> Result<()> {
        ensure!(
            self.admin(name, args, false)?.is_empty(),
            "rejected operation printed success output"
        );
        ensure!(
            fs::read_to_string(self.path(&stderr_name(name)))?.starts_with(error),
            "unexpected rejection category for {name}"
        );
        Ok(())
    }
    fn coordinator(&mut self, name: &str, args: &[&str], success: bool) -> Result<String> {
        self.command(name, &self.binaries.coordinator.clone(), args, success)
    }
    fn service<F>(&mut self, label: &str, database: &str, secret: &str, f: F) -> Result<()>
    where
        F: for<'a> FnOnce(&mut Drill<'a, 'r>, &str) -> Result<()>,
    {
        let mut argv = vec![path(&self.binaries.coordinator)];
        argv.extend(cmd([
            "serve-sequenced",
            database,
            "0",
            "--secret-file",
            secret,
        ]));
        let binaries = self.binaries;
        self.c.loopback(
            argv,
            label,
            (Duration::from_secs(15), 180.),
            false,
            |c, address| f(&mut Drill { c, binaries }, &address.port().to_string()),
        )
    }
    fn physical(&mut self) -> Result<()> {
        let logical = self.path("fixture.logical");
        fs::write(&logical, FIXTURE)?;
        let original = self.path("original.wdb");
        let backup = self.path("backup.wdb");
        let restored = self.path("restored.wdb");
        self.admin("fixture-import", &["import", &logical, &original], true)?;
        let bytes = fs::read(&original)?;
        protect_source(Path::new(&original))?;
        self.c.evidence["source_write_protection"] = json!(if cfg!(windows) {
            "read-only attribute and rejected writable open"
        } else {
            "Unix mode 0400 (root may bypass permission denial)"
        });
        let inspection = self.admin("original-check", &["check", &original], true)?;
        ensure!(
            inspection.contains("format 2 revision 4 data 4 witnessed true")
                && inspection.contains("retained 1 revision 2 data 2"),
            "unexpected fixture checkpoint"
        );
        unchanged(&original, &bytes)?;
        self.admin("verified-backup", &["backup", &original, &backup], true)?;
        unchanged(&original, &bytes)?;
        let backup_bytes = fs::read(&backup)?;

        let damaged = self.path("damaged.wdb");
        let absent = self.path("rejected-physical.wdb");
        let mut damage = bytes.clone();
        damage[0] ^= 1;
        fs::write(&damaged, &damage)?;
        self.rejected_admin("damaged-check", &["check", &damaged], "UnsupportedFormat")?;
        self.rejected_admin(
            "damaged-restore",
            &["restore", &damaged, &absent],
            "UnsupportedFormat",
        )?;
        ensure!(
            !Path::new(&absent).exists(),
            "damaged source created destination"
        );
        unchanged(&damaged, &damage)?;
        unchanged(&original, &bytes)?;

        self.admin("fresh-restore", &["restore", &backup, &restored], true)?;
        let restored_bytes = fs::read(&restored)?;
        let restored_check = self.admin("restored-check", &["check", &restored], true)?;
        ensure!(
            restored_check
                .lines()
                .take(2)
                .eq(inspection.lines().take(2)),
            "restored checkpoint or identity mismatch"
        );
        self.rejected_admin(
            "existing-destination",
            &["restore", &backup, &restored],
            "AlreadyExists",
        )?;
        unchanged(&restored, &restored_bytes)?;
        let exported = self.path("restored.logical");
        self.admin("restored-export", &["export", &restored, &exported], true)?;
        ensure!(
            fs::read(&exported)? == FIXTURE,
            "logical history differs from frozen fixture"
        );
        let consumer = self.binaries.consumer.clone();
        ensure!(
            self.command("fresh-core-consumer", &consumer, &[&restored], true)?
                == "current 4 21 retained 1 2 11",
            "fresh consumer oracle mismatch"
        );
        unchanged(&restored, &restored_bytes)?;
        let damaged_logical = self.path("damaged.logical");
        let absent_logical = self.path("rejected-logical.wdb");
        let mut logical_damage = FIXTURE.to_vec();
        *logical_damage.last_mut().unwrap() ^= 1;
        fs::write(&damaged_logical, &logical_damage)?;
        self.rejected_admin(
            "damaged-logical-import",
            &["import", &damaged_logical, &absent_logical],
            "Corrupt",
        )?;
        ensure!(
            !Path::new(&absent_logical).exists(),
            "invalid logical stream created destination"
        );
        unchanged(&damaged_logical, &logical_damage)?;
        unchanged(&original, &bytes)?;
        unchanged(&backup, &backup_bytes)?;
        self.c.evidence["physical"] = json!({"original_preserved":true,"read_only_inspection":true,
            "damage_rejected_before_destination":true,"existing_destination_preserved":true,
            "logical_fixture_exact_replay":true,"current":{"revision":4,"key_0":21},
            "retained":{"id":1,"revision":2,"key_0":11}});
        Ok(())
    }
    fn journals(&mut self) -> Result<()> {
        let secret = self.path("synthetic.secret");
        // Test-only bearer credential, created solely inside this private fixture directory.
        fs::write(&secret, "4".repeat(64))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&secret, fs::Permissions::from_mode(0o600))?;
        }
        let original = self.path("coordinator.wdb");
        let stale_backup = self.path("stale-backup.wdb");
        let stale = self.path("stale-restored.wdb");
        let journal = self.path("journal.wdb");
        self.coordinator("coordinator-init", &["init-sequenced", &original], true)?;
        self.admin(
            "pre-write-backup",
            &["backup", &original, &stale_backup],
            true,
        )?;
        let mut backup_set = vec![(stale_backup.clone(), fs::read(&stale_backup)?)];
        let mut checkpoint = String::new();
        self.service("original-service", &original, &secret, |d, port| {
            d.coordinator(
                "journal-init",
                &[
                    "journal",
                    "init",
                    &journal,
                    port,
                    "0",
                    "--secret-file",
                    &secret,
                ],
                true,
            )?;
            ensure!(
                d.coordinator(
                    "journal-write",
                    &[
                        "journal",
                        "set",
                        &journal,
                        port,
                        "3",
                        "42",
                        "--secret-file",
                        &secret
                    ],
                    true
                )? == "committed 4",
                "write acknowledgment mismatch"
            );
            checkpoint =
                d.coordinator("checkpoint-status", &["journal", "status", &journal], true)?;
            ensure!(
                checkpoint.ends_with("receipt 1 3 42 4"),
                "checkpoint receipt mismatch"
            );
            ensure!(
                d.coordinator(
                    "checkpoint-check",
                    &["journal", "check", &journal, port, "--secret-file", &secret],
                    true
                )? == "aligned 4 42",
                "checkpoint not aligned"
            );
            Ok(())
        })?;
        let original_bytes = fs::read(&original)?;
        let journal_bytes = fs::read(&journal)?;
        let coordinator_backup = self.path("coordinator-backup.wdb");
        let journal_backup = self.path("journal-backup.wdb");
        let restored = self.path("coordinator-restored.wdb");
        let restored_journal = self.path("journal-restored.wdb");
        for (label, source, backup, destination) in [
            ("coordinator", &original, &coordinator_backup, &restored),
            ("journal", &journal, &journal_backup, &restored_journal),
        ] {
            self.admin(
                &format!("{label}-backup"),
                &["backup", source, backup],
                true,
            )?;
            // Snapshot before the first restore reads this backup.
            backup_set.push((backup.to_string(), fs::read(backup)?));
            self.admin(
                &format!("{label}-restore"),
                &["restore", backup, destination],
                true,
            )?;
            self.admin(
                &format!("{label}-restored-check"),
                &["check", destination],
                true,
            )?;
        }
        self.admin("stale-restore", &["restore", &stale_backup, &stale], true)?;
        let stale_bytes = fs::read(&stale)?;
        self.service("isolated-stale-service", &stale, &secret, |d, port| {
            ensure!(
                d.coordinator(
                    "stale-journal-check",
                    &["journal", "check", &journal, port, "--secret-file", &secret],
                    false,
                )?
                .is_empty(),
                "stale check printed success output"
            );
            ensure!(
                fs::read_to_string(d.path(&stderr_name("stale-journal-check")))?
                    .contains("behind 3 0"),
                "stale restore was not identified as behind"
            );
            unchanged(&journal, &journal_bytes)
        })?;
        unchanged(&stale, &stale_bytes)?;
        self.service(
            "isolated-restored-service",
            &restored,
            &secret,
            |d, port| {
                ensure!(
                    d.coordinator(
                        "restored-status",
                        &["journal", "status", &restored_journal],
                        true
                    )? == checkpoint,
                    "restored receipt mismatch"
                );
                ensure!(
                    d.coordinator(
                        "restored-journal-check",
                        &[
                            "journal",
                            "check",
                            &restored_journal,
                            port,
                            "--secret-file",
                            &secret
                        ],
                        true
                    )? == "aligned 4 42",
                    "restored checkpoint not aligned"
                );
                let before = fs::read(&restored)?;
                let before_journal = fs::read(&restored_journal)?;
                ensure!(
                    d.coordinator(
                        "restored-local-acknowledgment",
                        &[
                            "journal",
                            "resume",
                            &restored_journal,
                            port,
                            "--secret-file",
                            &secret
                        ],
                        true
                    )? == "committed 4",
                    "local acknowledgment mismatch"
                );
                unchanged(&restored, &before)?;
                unchanged(&restored_journal, &before_journal)?;
                ensure!(
                    d.coordinator(
                        "continued-write",
                        &[
                            "journal",
                            "set",
                            &restored_journal,
                            port,
                            "4",
                            "99",
                            "--secret-file",
                            &secret
                        ],
                        true
                    )? == "committed 5",
                    "continued acknowledgment mismatch"
                );
                ensure!(
                    d.coordinator(
                        "continued-check",
                        &[
                            "journal",
                            "check",
                            &restored_journal,
                            port,
                            "--secret-file",
                            &secret
                        ],
                        true
                    )? == "aligned 5 99",
                    "continued checkpoint mismatch"
                );
                Ok(())
            },
        )?;
        unchanged(&original, &original_bytes)?;
        unchanged(&journal, &journal_bytes)?;
        for (path, bytes) in backup_set {
            unchanged(&path, &bytes)?;
        }
        self.c.evidence["coordinator_reconciliation"] = json!({"checkpoint_status":checkpoint,
            "stale_restore":"behind 3 0","stale_restore_and_original_journal_unchanged":true,
            "coordinated_restore":"aligned 4 42","local_acknowledgment":"committed 4",
            "local_acknowledgment_preserved_both_images":true,"continued_write":"aligned 5 99",
            "original_pair_preserved":true,"old_endpoint_stopped":true});
        fs::remove_file(secret)?;
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    #[test]
    fn source_protection_rejects_writable_open_without_changing_bytes() {
        let mut dir =
            crate::support::Scratch::new(&std::env::temp_dir(), "incident-readonly").unwrap();
        let file = dir.path().join("source.wdb");
        fs::write(&file, b"protected fixture").unwrap();
        let original = fs::metadata(&file).unwrap().permissions();
        let result = protect_source(&file);
        let unchanged = fs::read(&file).unwrap() == b"protected fixture";
        fs::set_permissions(&file, original).unwrap();
        result.unwrap();
        assert!(unchanged);
        dir.close().unwrap();
    }
}

pub fn run(run: &Run, options: Options) -> Result<()> {
    let source = &options.artifact_source;
    ensure!(
        source.len() == 40 && source.bytes().all(|b| b.is_ascii_hexdigit()),
        "full declared source required"
    );
    let mut c = Campaign::new(
        run,
        &options.output,
        "finite fresh-process administrative and coordinated-journal recovery rehearsal",
    )?;
    let result = (|| -> Result<()> {
        c.limits(1024 * 1024, None)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&c.output, fs::Permissions::from_mode(0o700))?;
        }
        #[cfg(windows)]
        c.restrict_directories(&[("incident-output", &c.output.clone())])?;
        let fixture = run.root.join("tests/fixtures/logical-v1.bin");
        ensure!(
            fs::read(&fixture)? == FIXTURE,
            "compiled logical fixture differs from source"
        );
        c.evidence["fixture_sha256"] = json!(sha(&fixture)?);
        c.evidence["declared_artifact_source"] = json!(source);
        c.evidence["artifact_source_is_operator_supplied"] = json!(true);
        let admin = dunce::canonicalize(&options.admin)?;
        let coordinator = dunce::canonicalize(&options.coordinator)?;
        c.evidence["admin"] = json!({"path":admin,"sha256":sha(&admin)?});
        c.evidence["coordinator"] = json!({"path":coordinator,"sha256":sha(&coordinator)?});
        let build = run.root.join("target/incident-recovery");
        let manifest = run.root.join("tools/incident-recovery/Cargo.toml");
        c.evidence["consumer_lockfile_sha256"] =
            json!(sha(&manifest.with_file_name("Cargo.lock"))?);
        c.evidence["consumer_rustc"] =
            json!(c.command(cmd(["rustc", "-Vv"]), "consumer-rustc", None, 15.)?);
        c.evidence["consumer_cargo"] =
            json!(c.command(cmd(["cargo", "-V"]), "consumer-cargo", None, 15.)?);
        c.command(
            vec![
                "cargo".into(),
                "build".into(),
                "--offline".into(),
                "--locked".into(),
                "--manifest-path".into(),
                path(&manifest),
                "--target-dir".into(),
                path(&build),
            ],
            "consumer-build",
            None,
            180.,
        )?;
        let consumer = build.join(format!(
            "debug/incident-recovery-consumer{}",
            std::env::consts::EXE_SUFFIX
        ));
        c.evidence["consumer"] = json!({"path":consumer,"sha256":sha(&consumer)?});
        let binaries = Binaries {
            admin,
            coordinator,
            consumer,
        };
        let retained = c.output.join("binaries");
        fs::create_dir(&retained)?;
        let mut binary_hashes = Vec::new();
        for (label, binary) in [
            ("admin", &binaries.admin),
            ("coordinator", &binaries.coordinator),
            ("consumer", &binaries.consumer),
        ] {
            let copy = retained.join(format!("{label}{}", std::env::consts::EXE_SUFFIX));
            let digest = sha(binary)?;
            fs::copy(binary, &copy)?;
            ensure!(
                sha(&copy)? == digest,
                "retained incident executable mismatch"
            );
            binary_hashes.push((binary.clone(), copy, digest));
        }
        let mut drill = Drill {
            c: &mut c,
            binaries: &binaries,
        };
        drill.physical()?;
        drill.journals()?;
        for (original, retained, digest) in binary_hashes {
            ensure!(
                sha(&original)? == digest && sha(&retained)? == digest,
                "incident executable changed during qualification"
            );
        }
        c.evidence["selected_binaries_unchanged"] = json!(true);
        ensure!(
            fs::read(fixture)? == FIXTURE,
            "logical fixture changed during drill"
        );
        Ok(())
    })();
    // Campaign retires each command/service before returning, including failures.
    let secret = c.output.join("synthetic.secret");
    if secret.exists() {
        fs::remove_file(secret)?;
    }
    let result = result.and_then(|()| {
        c.evidence["artifact_sha256"] =
            json!(qualification_inventory(&c.output, &["evidence.json"])?);
        Ok(())
    });
    c.finish(result)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn rejection_adapter_requires_exact_exit_marker_empty_stdout_and_bounded_output() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "incident-adapter").unwrap();
        let run = Run::new(scratch.path().to_owned());
        for (name, script, accepted) in [
            (
                "expected",
                "printf 'UnsupportedFormat\\n' >&2; exit 1",
                true,
            ),
            (
                "wrong-exit",
                "printf 'UnsupportedFormat\\n' >&2; exit 2",
                false,
            ),
            ("wrong-marker", "printf 'Io\\n' >&2; exit 1", false),
            (
                "success-output",
                "printf 'verified\\n'; printf 'UnsupportedFormat\\n' >&2; exit 1",
                false,
            ),
            ("over-limit", "head -c 4096 /dev/zero >&2; exit 1", false),
        ] {
            let program = scratch.path().join(format!("{name}.sh"));
            fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
            fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
            let binaries = Binaries {
                admin: program.clone(),
                coordinator: program.clone(),
                consumer: program,
            };
            let output = scratch.path().join(name);
            let mut c = Campaign::detached(&run, &output, "incident adapter control").unwrap();
            c.limits(128, None).unwrap();
            let result = Drill {
                c: &mut c,
                binaries: &binaries,
            }
            .rejected_admin("rejection", &[], "UnsupportedFormat");
            assert_eq!(result.is_ok(), accepted, "{name}");
            assert_eq!(c.finish(result).is_ok(), accepted, "{name}");
            let report = crate::delivery::read_json(&output.join("evidence.json")).unwrap();
            let command = &report["commands"][0];
            assert_eq!(command["expected_exit"], 1, "{name}");
            assert_eq!(command["supervision"]["cleanup_verified"], true, "{name}");
            assert_eq!(report["passed"], accepted, "{name}");
        }
        scratch.close().unwrap();
    }
}
