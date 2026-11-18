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
        let output = if success {
            self.c.command(argv, name, None, 15.)
        } else {
            self.c.command_exit(argv, name, 1, 15.)
        }?;
        if success {
            ensure!(
                fs::read(self.path(&stderr_name(name)))?.is_empty(),
                "unexpected successful CLI stderr for {name}"
            );
        }
        Ok(output)
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
        // Windows' exclusive database lease also excludes raw file reads.
        // Compare the read-only phase only after its service has retired, then
        // open a separate service for the deliberately mutating next phase.
        let before = fs::read(&restored)?;
        let before_journal = fs::read(&restored_journal)?;
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
                Ok(())
            },
        )?;
        unchanged(&restored, &before)?;
        unchanged(&restored_journal, &before_journal)?;
        self.service(
            "continued-restored-service",
            &restored,
            &secret,
            |d, port| {
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

    fn journal(
        &mut self,
        label: &str,
        action: &str,
        journal: &str,
        port: &str,
        secret: &str,
        fields: &[&str],
    ) -> Result<String> {
        let mut args = vec!["journal", action, journal, port];
        args.extend_from_slice(fields);
        args.extend_from_slice(&["--secret-file", secret]);
        self.coordinator(label, &args, true)
    }

    fn reject_journal(
        &mut self,
        label: &str,
        journal: &str,
        port: &str,
        secret: &str,
        marker: &str,
    ) -> Result<()> {
        let before = fs::read(journal)?;
        ensure!(
            self.coordinator(
                label,
                &["journal", "check", journal, port, "--secret-file", secret],
                false
            )?
            .is_empty(),
            "rejected journal check printed success output"
        );
        ensure!(
            fs::read_to_string(self.path(&stderr_name(label)))?.contains(marker),
            "wrong journal rejection for {label}"
        );
        unchanged(journal, &before)
    }

    fn observe_handoff(
        &mut self,
        label: &str,
        phase: &str,
        server: &str,
        journals: &[String; 2],
    ) -> Result<()> {
        let consumer = self.binaries.consumer.clone();
        let output = self.command(
            label,
            &consumer,
            &["journals", phase, server, &journals[0], &journals[1]],
            true,
        )?;
        ensure!(
            output
                == if phase == "checkpoint" {
                    "coordinator 5 77 journals 2 slots 0 1023"
                } else {
                    "coordinator 7 111 journals 2 slots 0 1023"
                },
            "fresh handoff observer mismatch"
        );
        Ok(())
    }

    fn handoff(&mut self) -> Result<()> {
        let old_secret = self.path("handoff-old.secret");
        let new_secret = self.path("handoff-new.secret");
        for (file, token) in [(&old_secret, "6"), (&new_secret, "7")] {
            // Synthetic credentials exist only within the private output tree.
            fs::write(file, token.repeat(64))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(file, fs::Permissions::from_mode(0o600))?;
            }
        }
        let original = self.path("handoff-original.wdb");
        let journals = [
            self.path("handoff-journal-0.wdb"),
            self.path("handoff-journal-1023.wdb"),
        ];
        let stale_backup = self.path("handoff-stale-backup.wdb");
        let stale_journal_backup = self.path("handoff-stale-journal-backup.wdb");
        self.coordinator("handoff-init", &["init-sequenced", &original], true)?;
        self.admin(
            "handoff-stale-backup",
            &["backup", &original, &stale_backup],
            true,
        )?;
        let mut statuses = [String::new(), String::new()];
        self.service(
            "handoff-original-service",
            &original,
            &old_secret,
            |d, port| {
                for (index, slot) in ["0", "1023"].into_iter().enumerate() {
                    d.journal(
                        &format!("handoff-init-{slot}"),
                        "init",
                        &journals[index],
                        port,
                        &old_secret,
                        &[slot],
                    )?;
                }
                d.admin(
                    "handoff-old-journal-backup",
                    &["backup", &journals[1], &stale_journal_backup],
                    true,
                )?;
                for (index, expected, value, revision) in [(0, "3", "42", "4"), (1, "4", "77", "5")]
                {
                    ensure!(
                        d.journal(
                            &format!("handoff-write-{index}"),
                            "set",
                            &journals[index],
                            port,
                            &old_secret,
                            &[expected, value]
                        )? == format!("committed {revision}"),
                        "handoff acknowledgment mismatch"
                    );
                    statuses[index] = d.coordinator(
                        &format!("handoff-status-{index}"),
                        &["journal", "status", &journals[index]],
                        true,
                    )?;
                }
                for (index, journal) in journals.iter().enumerate() {
                    ensure!(
                        d.journal(
                            &format!("handoff-checkpoint-{index}"),
                            "check",
                            journal,
                            port,
                            &old_secret,
                            &[]
                        )? == "aligned 5 77",
                        "handoff journals did not share a checkpoint"
                    );
                }
                Ok(())
            },
        )?;
        // The owned original service has retired before any coordinator backup.
        let mut preserved = vec![(original.clone(), fs::read(&original)?)];
        preserved.extend(
            journals
                .iter()
                .map(|p| Ok((p.clone(), fs::read(p)?)))
                .collect::<Result<Vec<_>>>()?,
        );
        let restored = self.path("handoff-restored.wdb");
        let restored_journals = [
            self.path("handoff-restored-0.wdb"),
            self.path("handoff-restored-1023.wdb"),
        ];
        let mut backups = Vec::new();
        for (label, source, destination) in [
            ("server", &original, &restored),
            ("0", &journals[0], &restored_journals[0]),
            ("1023", &journals[1], &restored_journals[1]),
        ] {
            let backup = self.path(&format!("handoff-backup-{label}.wdb"));
            self.admin(
                &format!("handoff-backup-{label}"),
                &["backup", source, &backup],
                true,
            )?;
            let bytes = fs::read(&backup)?;
            self.admin(
                &format!("handoff-restore-{label}"),
                &["restore", &backup, destination],
                true,
            )?;
            ensure!(
                fs::read(destination)? == bytes,
                "restored handoff bytes differ"
            );
            self.admin(
                &format!("handoff-inspect-{label}"),
                &["check", destination],
                true,
            )?;
            backups.push(json!({"member":label,"sha256":sha(Path::new(&backup))?}));
            preserved.push((backup, bytes));
        }
        for file in [&stale_backup, &stale_journal_backup] {
            preserved.push((file.clone(), fs::read(file)?));
        }
        let stale = self.path("handoff-stale-restored.wdb");
        self.admin(
            "handoff-restore-stale",
            &["restore", &stale_backup, &stale],
            true,
        )?;
        let stale_bytes = fs::read(&stale)?;
        self.service("handoff-stale-service", &stale, &new_secret, |d, port| {
            for (index, journal) in restored_journals.iter().enumerate() {
                d.reject_journal(
                    &format!("handoff-stale-check-{index}"),
                    journal,
                    port,
                    &new_secret,
                    "behind 3 0",
                )?;
            }
            Ok(())
        })?;
        unchanged(&stale, &stale_bytes)?;
        self.observe_handoff(
            "handoff-checkpoint-observer",
            "checkpoint",
            &restored,
            &restored_journals,
        )?;
        let mut rejected = vec![path(&self.binaries.consumer)];
        rejected.extend(cmd([
            "journals",
            "checkpoint",
            &restored,
            &restored_journals[0],
            &stale_journal_backup,
        ]));
        ensure!(
            self.c
                .command_exit(rejected, "handoff-observer-mixed-set", 101, 15.)?
                .is_empty(),
            "rejected raw observer printed success output"
        );
        ensure!(
            fs::read_to_string(self.path(&stderr_name("handoff-observer-mixed-set")))?
                .contains("journal revision mismatch"),
            "raw observer did not detect the mixed set"
        );
        let before = fs::read(&restored)?;
        let before_journals = [
            fs::read(&restored_journals[0])?,
            fs::read(&restored_journals[1])?,
        ];
        self.service(
            "handoff-isolated-service",
            &restored,
            &new_secret,
            |d, port| {
                d.reject_journal(
                    "handoff-mixed-set",
                    &stale_journal_backup,
                    port,
                    &new_secret,
                    "receipt-mismatch 5 77",
                )?;
                for (index, journal) in restored_journals.iter().enumerate() {
                    d.reject_journal(
                        &format!("handoff-old-credential-{index}"),
                        journal,
                        port,
                        &old_secret,
                        "Unauthorized",
                    )?;
                    ensure!(
                        d.coordinator(
                            &format!("handoff-restored-status-{index}"),
                            &["journal", "status", journal],
                            true
                        )? == statuses[index],
                        "restored journal differs from checkpoint"
                    );
                    ensure!(
                        d.journal(
                            &format!("handoff-restored-check-{index}"),
                            "check",
                            journal,
                            port,
                            &new_secret,
                            &[]
                        )? == "aligned 5 77",
                        "restored journal not aligned"
                    );
                    ensure!(
                        d.journal(
                            &format!("handoff-local-ack-{index}"),
                            "resume",
                            journal,
                            port,
                            &new_secret,
                            &[]
                        )? == format!("committed {}", index + 4),
                        "local acknowledgment mismatch"
                    );
                }
                Ok(())
            },
        )?;
        unchanged(&restored, &before)?;
        for (journal, bytes) in restored_journals.iter().zip(&before_journals) {
            unchanged(journal, bytes)?;
        }
        self.service(
            "handoff-continued-service",
            &restored,
            &new_secret,
            |d, port| {
                ensure!(
                    d.journal(
                        "handoff-continue-0",
                        "set",
                        &restored_journals[0],
                        port,
                        &new_secret,
                        &["5", "99"]
                    )? == "committed 6",
                    "first continued write mismatch"
                );
                let conflict_journal = fs::read(&restored_journals[1])?;
                ensure!(
                    d.journal(
                        "handoff-stale-precondition",
                        "set",
                        &restored_journals[1],
                        port,
                        &new_secret,
                        &["5", "100"]
                    )? == "conflict 6 99",
                    "stale second writer did not conflict"
                );
                // This stale expectation is rejected by the journal preflight,
                // before it stages an intent or submits a server mutation.
                unchanged(&restored_journals[1], &conflict_journal)?;
                ensure!(
                    d.journal(
                        "handoff-continue-1023",
                        "set",
                        &restored_journals[1],
                        port,
                        &new_secret,
                        &["6", "111"]
                    )? == "committed 7",
                    "second continued write mismatch"
                );
                for (index, journal) in restored_journals.iter().enumerate() {
                    ensure!(
                        d.journal(
                            &format!("handoff-final-check-{index}"),
                            "check",
                            journal,
                            port,
                            &new_secret,
                            &[]
                        )? == "aligned 7 111",
                        "final journal not aligned"
                    );
                }
                Ok(())
            },
        )?;
        self.observe_handoff(
            "handoff-final-observer",
            "continued",
            &restored,
            &restored_journals,
        )?;
        let final_bytes = fs::read(&restored)?;
        for (index, journal) in restored_journals.iter().enumerate() {
            let bytes = fs::read(journal)?;
            ensure!(
                self.journal(
                    &format!("handoff-offline-ack-{index}"),
                    "resume",
                    journal,
                    "0",
                    &new_secret,
                    &[]
                )? == format!("committed {}", index + 6),
                "offline local acknowledgment mismatch"
            );
            unchanged(journal, &bytes)?;
        }
        unchanged(&restored, &final_bytes)?;
        for (file, bytes) in preserved {
            unchanged(&file, &bytes)?;
        }
        let checkpoint = json!({"revision":5,"value":77,"slots":[0,1023],"journal_statuses":statuses,"backups":backups});
        fs::write(
            self.path("handoff-checkpoint.json"),
            serde_json::to_vec_pretty(&checkpoint)?,
        )?;
        self.c.evidence["coordinator_handoff"] = json!({"checkpoint":checkpoint,"old_owned_endpoint_retired":true,"stale_server_rejected":true,"mixed_journal_set_rejected":true,"old_credential_rejected":true,"read_only_checks_preserve_images":true,"continued":{"revision":7,"value":111,"receipts":[[2,5,99,6],[2,6,111,7]]},"offline_local_acknowledgments":[6,7],"originals_and_backups_preserved":true,"authority_selection":"synthetic fixture selected before replay; external fencing and owner approval remain required"});
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    #[test]
    fn byte_observation_requires_retiring_the_exclusive_writer_lease() {
        use std::os::windows::fs::OpenOptionsExt;
        let mut dir =
            crate::support::Scratch::new(&std::env::temp_dir(), "incident-lease").unwrap();
        let file = dir.path().join("source.wdb");
        let bytes = b"preserved fixture";
        fs::write(&file, bytes).unwrap();
        let lease = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&file)
            .unwrap();
        let error = unchanged(file.to_str().unwrap(), bytes).unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(32)
        );
        drop(lease);
        unchanged(file.to_str().unwrap(), bytes).unwrap();
        dir.close().unwrap();
    }
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
    let mut c = Campaign::new_private(
        run,
        &options.output,
        "finite fresh-process administrative and coordinated-journal recovery rehearsal",
    )?;
    let result = (|| -> Result<()> {
        c.limits(1024 * 1024, None)?;
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
        drill.handoff()?;
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
    for name in [
        "synthetic.secret",
        "handoff-old.secret",
        "handoff-new.secret",
    ] {
        let secret = c.output.join(name);
        if secret.exists() {
            fs::remove_file(secret)?;
        }
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
    fn successful_cli_requires_empty_stderr() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "incident-success").unwrap();
        let run = Run::new(scratch.path().to_owned());
        for (name, script, accepted) in [
            ("expected", "printf 'aligned 5 77\\n'", true),
            (
                "unexpected-stderr",
                "printf 'aligned 5 77\\n'; printf 'unexpected\\n' >&2",
                false,
            ),
            ("failed-command", "printf 'aligned 5 77\\n'; exit 1", false),
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
            let mut c =
                Campaign::detached(&run, &output, "incident successful CLI control").unwrap();
            let result = Drill {
                c: &mut c,
                binaries: &binaries,
            }
            .coordinator("check", &[], true);
            assert_eq!(result.is_ok(), accepted, "{name}");
            if accepted {
                assert_eq!(result.as_ref().unwrap(), "aligned 5 77");
            }
            assert_eq!(c.finish(result.map(|_| ())).is_ok(), accepted, "{name}");
            let report = crate::delivery::read_json(&output.join("evidence.json")).unwrap();
            assert_eq!(report["passed"], accepted);
            assert_eq!(
                report["commands"][0]["supervision"]["cleanup_verified"],
                true
            );
        }
        scratch.close().unwrap();
    }

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
