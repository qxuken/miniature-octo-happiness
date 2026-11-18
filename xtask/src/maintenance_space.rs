//! Qualify near-cap copies on an owned native volume using the compound fixture.

use crate::resource_oracle as oracle;
use anyhow::{Result, ensure};
use serde_json::{Value, json};

#[cfg(any(windows, target_os = "macos"))]
const SOURCE_BYTES: u64 = 66_154_496;
const COPY_BYTES: u64 = 63_074_304;
const OPERATIONS: [&str; 3] = ["backup", "restore", "compact"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Profile {
    #[cfg(any(windows, test))]
    Ntfs,
    #[cfg(any(windows, test))]
    NtfsQuota(i32),
    #[cfg(any(target_os = "macos", test))]
    Apfs,
}

impl Profile {
    const fn errno(self) -> i32 {
        match self {
            #[cfg(any(windows, test))]
            Self::Ntfs => 112,
            #[cfg(any(windows, test))]
            Self::NtfsQuota(code) => code,
            #[cfg(any(target_os = "macos", test))]
            Self::Apfs => 28,
        }
    }

    fn replaces_source(self, operation: &str) -> bool {
        match self {
            #[cfg(any(windows, test))]
            Self::Ntfs | Self::NtfsQuota(_) => {
                let _ = operation;
                false
            }
            #[cfg(any(target_os = "macos", test))]
            Self::Apfs => operation == "compact",
        }
    }
}

fn verify_phase(
    rows: &[Value],
    source: &str,
    mode: &str,
    label: &str,
    pid: u32,
    profile: Profile,
) -> Result<()> {
    ensure!(
        rows.len() == 2 && rows[1] == oracle::completion(mode, source),
        "incomplete/wrong fixture identity"
    );
    let row = &rows[0];
    if matches!(mode, "space-check" | "space-advance") {
        let advanced = matches!(label, "advanced" | "advanced-source");
        oracle::state(
            row,
            &format!("space_{label}"),
            212 + u64::from(advanced),
            advanced,
            None,
        )?;
        ensure!(
            row["detail"] == json!({"fresh_process":true}),
            "missing fresh process verification"
        );
        return Ok(());
    }
    ensure!(
        OPERATIONS.contains(&label) && pid != 0,
        "invalid maintenance phase"
    );
    let expected = match mode {
        #[cfg(any(target_os = "macos", test))]
        "space-exchange"
            if profile == Profile::Apfs
                && label == "compact"
                && row["stage"] == "exchange-completed" =>
        {
            json!({"stage":"exchange-completed","operation":label,"pid":pid,"before_bytes":66_154_496,"after_bytes":63_074_304})
        }
        #[cfg(any(target_os = "macos", test))]
        "space-exchange" if profile == Profile::Apfs && label == "compact" => {
            json!({"stage":"exchange","operation":label,"pid":pid,"error":15,"native_operation":"Exchange","category":"DiskFull","raw":28})
        }
        #[cfg(any(target_os = "macos", test))]
        "space-cleanup" if profile == Profile::Apfs && label == "compact" => {
            json!({"stage":"cleanup","operation":label,"pid":pid,"before_bytes":66_154_496,"after_bytes":63_074_304})
        }
        "space-fail" => {
            let partial = oracle::number(row, "partial_bytes")?;
            ensure!(
                (oracle::MIB..COPY_BYTES).contains(&partial),
                "partial copy length"
            );
            let category = if profile.errno() == 1295 {
                "Quota"
            } else {
                "DiskFull"
            };
            json!({"stage":"fail","operation":label,"pid":pid,"error":14,"category":category,"raw":profile.errno(),
                "native_operation":"Write","partial_bytes":partial,"source_unchanged":true,"backup_unchanged":true})
        }
        "space-retry" => {
            json!({"stage":"retry","operation":label,"pid":pid,"partial_rejected":true,
            "existing_rejected":!profile.replaces_source(label),"partial_preserved":true,"matches_backup":true})
        }
        "space-retire" => {
            json!({"stage":"retire","operation":label,"pid":pid,"owned_outputs_removed":true})
        }
        _ => anyhow::bail!("unknown maintenance phase"),
    };
    ensure!(*row == expected, "wrong {mode} observation for {label}");
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
mod native {
    use super::*;
    #[cfg(target_os = "macos")]
    use crate::macos_volume;
    use crate::{
        delivery,
        support::{Run, path, sha},
        trace_process::{admission, call, write_proof},
    };
    #[cfg(windows)]
    use crate::{
        windows_quota,
        windows_volume::{self, Capacity},
    };
    use anyhow::Context;
    use clap::Args;
    use std::{
        collections::{BTreeMap, BTreeSet},
        ffi::OsString,
        fs::{self, OpenOptions},
        io::{Seek, SeekFrom, Write},
        path::{Path, PathBuf},
    };

    #[cfg(windows)]
    const PROFILE: Profile = Profile::Ntfs;
    #[cfg(target_os = "macos")]
    const PROFILE: Profile = Profile::Apfs;
    const CLUSTER: u64 = 4096;
    const HEADROOM: u64 = 8 * oracle::MIB;

    fn free_space(data: &Path) -> Result<u64> {
        #[cfg(windows)]
        {
            windows_volume::space(data, Capacity::Maintenance)
        }
        #[cfg(target_os = "macos")]
        {
            macos_volume::space(data)
        }
    }

    #[derive(Args)]
    pub struct Options {
        #[arg(long)]
        fixture: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        output: PathBuf,
        #[cfg(windows)]
        /// Apply an enforced quota to the copy fixture's own user SID.
        #[arg(long)]
        owner_quota: bool,
        #[cfg(target_os = "macos")]
        /// Exhaust space immediately before compaction's native exchange.
        #[arg(long)]
        namespace_enospc: bool,
        #[cfg(target_os = "macos")]
        /// Exhaust space after durable exchange and before removing the old image.
        #[arg(long, conflicts_with = "namespace_enospc")]
        cleanup_enospc: bool,
    }

    struct Fixture<'a> {
        binary: &'a Path,
        runs: &'a Path,
        output: &'a Path,
        source: &'a str,
        env: BTreeMap<OsString, OsString>,
        #[cfg(windows)]
        processes: BTreeSet<(u32, u64)>,
        #[cfg(target_os = "macos")]
        processes: BTreeSet<u32>,
        profile: Profile,
        #[cfg(windows)]
        owner_quota: bool,
    }

    impl Fixture<'_> {
        fn command(
            &mut self,
            proof: &mut Value,
            args: Vec<OsString>,
            label: &str,
            expect_panic: bool,
        ) -> Result<(PathBuf, u32)> {
            let mut command = vec![path(self.binary)];
            command.extend(args);
            let report = call(
                proof,
                self.output,
                command,
                self.runs,
                label,
                &self.env,
                expect_panic,
            )?;
            let pid = report.pid.context("fixture process ID")?;
            #[cfg(windows)]
            let process = (
                pid,
                report
                    .creation_time_100ns
                    .context("fixture process creation time")?,
            );
            #[cfg(target_os = "macos")]
            let process = pid;
            ensure!(
                self.processes.insert(process),
                "fixture process identity reused for {label}"
            );
            if expect_panic {
                let stderr = fs::read_to_string(self.runs.join(format!("{label}.stderr")))?;
                ensure!(
                    report.exit == Some(101)
                        && stderr.contains("Result::unwrap_err()")
                        && stderr.contains("CompactionStats"),
                    "negative control failed before the required assertion"
                );
            }
            Ok((self.runs.join(format!("{label}.stdout")), pid))
        }

        fn phase(
            &mut self,
            proof: &mut Value,
            data: &Path,
            image: &Path,
            mode: &str,
            label: &str,
        ) -> Result<Value> {
            let mut args = vec![path(data), path(image), mode.into(), label.into()];
            if label == "advanced-source" {
                args.push("changed".into());
            }
            let (stdout, pid) = self.command(proof, args, &format!("{mode}-{label}"), false)?;
            let rows = rows(&stdout)?;
            verify_phase(&rows, self.source, mode, label, pid, self.profile)?;
            let result = json!({"mode":mode,"label":label,"pid":pid,"observation":rows[0]});
            proof["maintenance_records"]
                .as_array_mut()
                .unwrap()
                .push(result.clone());
            write_proof(self.output, proof)?;
            Ok(result)
        }
    }

    fn rows(path: &Path) -> Result<Vec<Value>> {
        fs::read_to_string(path)?
            .lines()
            .map(|line| delivery::json(line.as_bytes()))
            .collect()
    }

    #[cfg(windows)]
    struct OwnerQuota {
        drive: String,
        owner: windows_quota::Owner,
        code: i32,
    }

    #[cfg(windows)]
    impl OwnerQuota {
        const RAISED: u64 = 80 * oracle::MIB;

        fn tool(
            fixture: &Fixture<'_>,
            proof: &mut Value,
            tool: &str,
            label: &str,
            args: &[OsString],
        ) -> Result<()> {
            let mut command = vec![path(windows_volume::system_command(tool)?)];
            command.extend_from_slice(args);
            call(
                proof,
                fixture.output,
                command,
                fixture.runs,
                label,
                &fixture.env,
                false,
            )?;
            Ok(())
        }

        fn quota(
            &self,
            fixture: &Fixture<'_>,
            proof: &mut Value,
            label: &str,
            action: &str,
            tail: &[OsString],
        ) -> Result<()> {
            let mut args = vec!["quota".into(), action.into(), self.drive.as_str().into()];
            args.extend_from_slice(tail);
            Self::tool(fixture, proof, "fsutil.exe", label, &args)
        }

        fn limit(
            &self,
            fixture: &Fixture<'_>,
            proof: &mut Value,
            label: &str,
            hard: u64,
        ) -> Result<()> {
            self.quota(
                fixture,
                proof,
                label,
                "modify",
                &[
                    (hard / 2).to_string().into(),
                    hard.to_string().into(),
                    self.owner.account.as_str().into(),
                ],
            )?;
            self.quota(fixture, proof, &format!("{label}-query"), "query", &[])
        }

        fn create_probe(
            &self,
            fixture: &mut Fixture<'_>,
            proof: &mut Value,
            data: &Path,
            label: &str,
        ) -> Result<PathBuf> {
            let probe = data.join("quota-probe");
            let (log, _) = fixture.command(
                proof,
                vec![
                    path(data),
                    path(&probe),
                    "space-quota-probe".into(),
                    "probe".into(),
                ],
                label,
                false,
            )?;
            ensure!(
                rows(&log)? == [oracle::completion("space-quota-probe", fixture.source)],
                "wrong quota probe identity"
            );
            self.owner.verify(&probe)?;
            Ok(probe)
        }

        fn setup(
            fixture: &mut Fixture<'_>,
            proof: &mut Value,
            data: &Path,
            source: &Path,
            backup: &Path,
        ) -> Result<Self> {
            let root = data
                .parent()
                .and_then(Path::to_str)
                .context("owned drive root")?;
            let drive = root.strip_suffix('\\').context("owned drive letter")?;
            ensure!(
                drive.len() == 2
                    && drive.as_bytes()[0].is_ascii_uppercase()
                    && drive.ends_with(':')
                    && proof["private_volume"]["root"] == root,
                "quota administration requires the bound private drive"
            );
            let mut quota = Self {
                drive: drive.into(),
                owner: windows_quota::Owner::current_user()?,
                code: 0,
            };
            let preserved = windows_quota::Owner::local_service()?;
            for (file, label) in [(source, "own-source"), (backup, "own-backup")] {
                Self::tool(
                    fixture,
                    proof,
                    "icacls.exe",
                    label,
                    &[
                        path(file),
                        "/setowner".into(),
                        preserved.account.as_str().into(),
                    ],
                )?;
                preserved.verify(file)?;
            }
            fixture
                .env
                .insert("WEEKEND_PRIVATE_MAINTENANCE".into(), "ntfs-quota".into());
            let probe = quota.create_probe(fixture, proof, data, "quota-create-calibration")?;
            quota.quota(fixture, proof, "quota-track", "track", &[])?;
            quota.limit(fixture, proof, "quota-initial-limit", HEADROOM)?;
            quota.quota(fixture, proof, "quota-enforce", "enforce", &[])?;
            quota.quota(fixture, proof, "quota-enforced-query", "query", &[])?;
            let (code, bytes) = windows_quota::exhaust(&probe, HEADROOM + oracle::MIB)?;
            let physical = Self::free(data)?;
            ensure!(
                bytes == HEADROOM,
                "owner quota must allow exactly 8 MiB before refusing writes"
            );
            fs::remove_file(probe)?;
            quota.code = code;
            fixture.profile = Profile::NtfsQuota(code);
            fixture.env.insert(
                "WEEKEND_PRIVATE_QUOTA_ERRNO".into(),
                code.to_string().into(),
            );
            proof["owner_quota"] = json!({"copy_owner":quota.owner.account,"source_owner":preserved.account,
                "calibrated_code":code,"calibrated_bytes":bytes,"physical_free_at_calibration":physical,
                "initial_limit":HEADROOM,"raised_limit":Self::RAISED});
            Ok(quota)
        }

        fn free(data: &Path) -> Result<u64> {
            let free = windows_volume::physical_space(data, Capacity::Maintenance)?;
            ensure!(
                free > COPY_BYTES,
                "physical volume must have headroom for a full copy"
            );
            Ok(free)
        }

        fn probe_full(
            &self,
            fixture: &mut Fixture<'_>,
            proof: &mut Value,
            data: &Path,
            op: &str,
        ) -> Result<()> {
            let probe = self.create_probe(fixture, proof, data, &format!("quota-full-{op}"))?;
            ensure!(
                windows_quota::exhaust(&probe, oracle::MIB)? == (self.code, 0),
                "fresh opens changed the quota boundary"
            );
            fs::remove_file(probe)?;
            Ok(())
        }

        fn raise(
            &self,
            fixture: &mut Fixture<'_>,
            proof: &mut Value,
            data: &Path,
            op: &str,
        ) -> Result<()> {
            self.limit(fixture, proof, &format!("quota-raise-{op}"), Self::RAISED)?;
            let probe = self.create_probe(fixture, proof, data, &format!("quota-write-{op}"))?;
            let mut file = OpenOptions::new().append(true).open(&probe)?;
            file.write_all(&[0x6a; CLUSTER as usize])?;
            file.sync_all()?;
            drop(file);
            ensure!(
                fs::metadata(&probe)?.len() == CLUSTER,
                "raised quota did not permit fsynced write"
            );
            fs::remove_file(probe)?;
            Ok(())
        }
    }

    pub fn bounded_fill(dir: &Path, ballast: &Path, new: bool) -> Result<(u64, u64)> {
        bounded_fill_with_sync(dir, ballast, new, fs::File::sync_all)
    }

    fn bounded_fill_with_sync(
        dir: &Path,
        ballast: &Path,
        new: bool,
        sync: impl FnOnce(&fs::File) -> std::io::Result<()>,
    ) -> Result<(u64, u64)> {
        let free = free_space(dir).context("measure space before ballast fill")?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(new)
            .open(ballast)
            .context("open owned ballast")?;
        let previous = file.seek(SeekFrom::End(0)).context("seek to ballast end")?;
        let mut filled = previous;
        // Bound both the expected capacity and the individual requests, including
        // the last rejected request. Only extend the controller's own ballast.
        let block = [0x5a; 65536];
        let mut request = block.len();
        loop {
            ensure!(
                filled + request as u64 <= 256 * oracle::MIB,
                "ballast write ceiling"
            );
            match file.write(&block[..request]) {
                Ok(n) if n > 0 => filled += n as u64,
                Err(e) if e.raw_os_error() == Some(PROFILE.errno()) => {
                    if request == CLUSTER as usize {
                        break;
                    }
                    request = CLUSTER as usize;
                }
                other => anyhow::bail!("unexpected ballast write: {other:?}"),
            }
        }
        let sync_result = sync(&file);
        drop(file);
        match sync_result {
            // The ballast is temporary and the exercise needs live allocation
            // refusal. APFS can run out of space while syncing its metadata.
            #[cfg(target_os = "macos")]
            Err(error) if error.raw_os_error() == Some(libc::ENOSPC) => {
                let mut probe = OpenOptions::new()
                    .append(true)
                    .open(ballast)
                    .context("reopen ballast after sync ENOSPC")?;
                ensure!(probe.metadata()?.len() == filled, "ballast length changed");
                let refusal = probe.write(&block[..CLUSTER as usize]);
                ensure!(
                    matches!(&refusal, Err(error) if error.raw_os_error() == Some(libc::ENOSPC)),
                    "fresh 4 KiB ballast write after sync ENOSPC did not confirm exhaustion: {refusal:?}"
                );
                ensure!(probe.metadata()?.len() == filled, "ballast length changed");
                eprintln!(
                    "APFS ballast sync returned ENOSPC and a fresh 4 KiB write confirmed exhaustion"
                );
            }
            result => result.context("sync exhausted ballast")?,
        }
        let available = free_space(dir).context("measure space after ballast fill")?;
        // APFS can report available blocks after the kernel refuses a 4 KiB write.
        ensure!(
            (cfg!(target_os = "macos") || available == 0)
                && (filled - previous).abs_diff(free.saturating_sub(available)) < oracle::MIB,
            "bounded kernel exhaustion required: free before {free}, free after {available}, bytes written {}",
            filled - previous
        );
        Ok((filled, available))
    }

    #[cfg(all(test, target_os = "macos"))]
    #[test]
    #[ignore = "requires a native APFS disk image"]
    fn native_ballast_sync_enospc_requires_a_fresh_refusal() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "ballast-sync").unwrap();
        let env = BTreeMap::from([("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into())]);
        let mut proof = json!({"commands": []});
        let outcome = macos_volume::with_volume(&mut proof, scratch.path(), &env, |_, data| {
            let ballast = data.join("ballast");
            let (filled, _) = bounded_fill_with_sync(data, &ballast, true, |_| {
                Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
            })?;
            ensure!(filled > 0 && bounded_fill(data, &ballast, false)?.0 == filled);
            let error = bounded_fill_with_sync(data, &ballast, false, |_| {
                Err(std::io::Error::from_raw_os_error(libc::EIO))
            })
            .err()
            .context("unexpected sync success after EIO")?;
            ensure!(
                error
                    .downcast_ref::<std::io::Error>()
                    .and_then(std::io::Error::raw_os_error)
                    == Some(libc::EIO)
            );
            Ok(())
        });
        assert_eq!(proof["volume_cleanup"]["verified"], true);
        outcome.unwrap();
        scratch.close().unwrap();
    }

    fn headroom(data: &Path, ballast: &Path) -> Result<(u64, u64, u64)> {
        #[cfg(target_os = "macos")]
        let reserve = data.join("copy-reserve");
        #[cfg(target_os = "macos")]
        {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&reserve)?;
            for _ in 0..HEADROOM / (64 * 1024) {
                file.write_all(&[0x4b; 64 * 1024])?;
            }
            file.sync_all()?;
        }
        let (filled, saturated) = bounded_fill(data, ballast, true)?;
        #[cfg(windows)]
        {
            let file = OpenOptions::new().write(true).open(ballast)?;
            file.set_len(
                filled
                    .checked_sub(HEADROOM)
                    .context("insufficient ballast")?,
            )?;
            file.sync_all()?;
        }
        #[cfg(target_os = "macos")]
        fs::remove_file(reserve)?;
        let available = free_space(data)?;
        #[cfg(windows)]
        ensure!(
            available == HEADROOM,
            "exact 8 MiB of copy headroom required"
        );
        #[cfg(target_os = "macos")]
        ensure!(
            available.saturating_sub(saturated).abs_diff(HEADROOM) < oracle::MIB,
            "APFS reserve release produced unexpected headroom"
        );
        Ok((filled, saturated, available))
    }

    fn partial_path(data: &Path, operation: &str) -> Result<PathBuf> {
        if !PROFILE.replaces_source(operation) {
            return Ok(data.join(format!("failed-{operation}.wdb")));
        }
        let mut partial = None;
        for entry in fs::read_dir(data)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".wdb-compact-")
            {
                ensure!(
                    partial.replace(entry.path()).is_none(),
                    "duplicate compaction scratch"
                );
            }
        }
        partial.context("in-place compaction scratch missing")
    }

    fn verified_copy(source: &Path, target: &Path, length: u64) -> Result<String> {
        ensure!(
            fs::metadata(source)?.len() == length,
            "wrong near-cap seed length"
        );
        let hash = sha(source)?;
        ensure!(fs::copy(source, target)? == length, "short seed install");
        OpenOptions::new().write(true).open(target)?.sync_all()?;
        ensure!(sha(target)? == hash, "copy hash mismatch");
        Ok(hash)
    }

    #[cfg(target_os = "macos")]
    struct NamespaceImages {
        source: PathBuf,
        backup: PathBuf,
        source_hash: String,
        backup_hash: String,
        original_meta: fs::Metadata,
        ballast: PathBuf,
    }

    #[cfg(target_os = "macos")]
    impl NamespaceImages {
        fn install(
            fixture: &mut Fixture<'_>,
            proof: &mut Value,
            seed: &Path,
            data: &Path,
        ) -> Result<Self> {
            use std::os::unix::fs::PermissionsExt;

            let source = data.join("source.wdb");
            let backup = data.join("backup.wdb");
            let source_hash = verified_copy(&seed.join("source.wdb"), &source, SOURCE_BYTES)?;
            let backup_hash = verified_copy(&seed.join("backup.wdb"), &backup, COPY_BYTES)?;
            let original = fs::File::open(&source)?;
            original.set_permissions(fs::Permissions::from_mode(0o640))?;
            original.sync_all()?;
            let original_meta = original.metadata()?;
            drop(original);
            fixture.phase(proof, data, &source, "space-check", "initial-source")?;
            fixture.phase(proof, data, &backup, "space-check", "initial-backup")?;
            Ok(Self {
                source,
                backup,
                source_hash,
                backup_hash,
                original_meta,
                ballast: data.join("ballast"),
            })
        }
    }

    #[cfg(target_os = "macos")]
    #[derive(Clone, Copy)]
    enum NamespacePhase {
        Exchange,
        Cleanup,
    }

    #[cfg(target_os = "macos")]
    struct AtCapacity {
        scratch: PathBuf,
        scratch_inode: u64,
        canonical_inode: u64,
        filled: u64,
        available: u64,
        observation: Value,
    }

    #[cfg(target_os = "macos")]
    fn namespace_pressure(
        run: &Run,
        fixture: &mut Fixture<'_>,
        proof: &mut Value,
        data: &Path,
        images: &NamespaceImages,
        phase: NamespacePhase,
    ) -> Result<AtCapacity> {
        use crate::assurance::Campaign;
        use std::os::unix::fs::MetadataExt;
        use std::time::Duration;

        let (step, mode, live_source_hash, live_scratch_hash, source_len, scratch_len) = match phase
        {
            NamespacePhase::Exchange => (
                "exchange",
                "space-exchange",
                &images.source_hash,
                &images.backup_hash,
                SOURCE_BYTES,
                COPY_BYTES,
            ),
            NamespacePhase::Cleanup => (
                "cleanup",
                "space-cleanup",
                &images.backup_hash,
                &images.source_hash,
                COPY_BYTES,
                SOURCE_BYTES,
            ),
        };
        let gate_root = fixture.output.join(format!("{step}-child"));
        let ready = gate_root.join("release");
        let mut child = Campaign::detached(
            run,
            &gate_root,
            "native APFS compaction namespace at ENOSPC",
        )?;
        child.env = fixture.env.clone();
        child.limits(1024 * 1024, None)?;
        let result = child.ready_process(
            vec![
                path(fixture.binary),
                path(data),
                path(&images.source),
                mode.into(),
                "compact".into(),
                path(&ready),
            ],
            step,
            (Duration::from_secs(60), 150.),
            true,
            |line| {
                let pid: u32 = line
                    .strip_prefix(&format!("ready {step} "))
                    .context("wrong APFS namespace boundary")?
                    .parse()?;
                ensure!(pid > 0, "invalid APFS namespace PID");
                Ok(pid)
            },
            |_, pid| {
                let scratch = partial_path(data, "compact")?;
                let temporary = fs::symlink_metadata(&scratch)?;
                let canonical = fs::symlink_metadata(&images.source)?;
                let old = match phase {
                    NamespacePhase::Exchange => &canonical,
                    NamespacePhase::Cleanup => &temporary,
                };
                ensure!(
                    old.ino() == images.original_meta.ino()
                        && canonical.ino() != temporary.ino()
                        && [&canonical, &temporary]
                            .into_iter()
                            .all(|meta| meta.is_file()
                                && meta.dev() == images.original_meta.dev()
                                && meta.nlink() == 1
                                && meta.mode() & 0o7777 == 0o640)
                        && canonical.len() == source_len
                        && temporary.len() == scratch_len
                        && sha(&scratch)? == *live_scratch_hash
                        && sha(&images.source)? == *live_source_hash,
                    "wrong source or scratch image at APFS {step} gate"
                );
                let (fill, available) = bounded_fill(data, &images.ballast, true)?;
                let at_capacity = json!({
                    "phase": step,
                    "pid": pid,
                    "ballast_bytes": fill,
                    "available_bytes": available,
                    "canonical_inode": canonical.ino(),
                    "scratch_inode": temporary.ino(),
                    "canonical_sha256": live_source_hash,
                    "scratch_sha256": live_scratch_hash,
                });
                fs::write(
                    gate_root.join("before-release.json"),
                    serde_json::to_vec_pretty(&at_capacity)?,
                )?;
                fs::create_dir(&ready)?;
                Ok((
                    pid,
                    scratch,
                    temporary.ino(),
                    canonical.ino(),
                    fill,
                    available,
                ))
            },
        );
        let (pid, scratch, scratch_inode, canonical_inode, filled, available) = match result {
            Ok(gate) => {
                child.finish(Ok(()))?;
                gate
            }
            Err(error) => {
                return Err(child.finish(Err(error)).unwrap_err());
            }
        };
        let gate_record = delivery::read_json(&gate_root.join("evidence.json"))?;
        ensure!(
            gate_record["passed"] == true
                && gate_record["commands"][0]["supervision"]["pid"] == pid
                && fixture.processes.insert(pid),
            "APFS namespace process identity mismatch"
        );
        let rows = fs::read_to_string(gate_root.join(format!("{step}.stdout.log")))?
            .lines()
            .skip(1)
            .map(|line| delivery::json(line.as_bytes()))
            .collect::<Result<Vec<_>>>()?;
        verify_phase(&rows, fixture.source, mode, "compact", pid, Profile::Apfs)?;
        proof["maintenance_records"]
            .as_array_mut()
            .unwrap()
            .push(json!({"mode":mode,"label":"compact","pid":pid,"observation":rows[0]}));
        Ok(AtCapacity {
            scratch,
            scratch_inode,
            canonical_inode,
            filled,
            available,
            observation: rows.into_iter().next().unwrap(),
        })
    }

    #[cfg(target_os = "macos")]
    fn qualify_exchange(
        run: &Run,
        fixture: &mut Fixture<'_>,
        proof: &mut Value,
        seed: &Path,
        data: &Path,
    ) -> Result<()> {
        use std::os::unix::fs::MetadataExt;
        let images = NamespaceImages::install(fixture, proof, seed, data)?;
        let capacity =
            namespace_pressure(run, fixture, proof, data, &images, NamespacePhase::Exchange)?;
        if capacity.observation["stage"] == "exchange-completed" {
            let free = verify_published_at_capacity(
                fixture,
                proof,
                data,
                &images,
                &capacity,
                capacity.scratch_inode,
            )?;
            proof["exchange_completed_at_capacity"] = json!({
                "observation":capacity.observation,"original_inode":images.original_meta.ino(),
                "canonical_inode":capacity.scratch_inode,"ballast_bytes":capacity.filled,
                "free_at_exchange":capacity.available,"free_after_remount":free,
                "scratch_absent_after_remount":true,"canonical_sha256":images.backup_hash,
                "advanced_with_ballast":true,
            });
            return Ok(());
        }
        let AtCapacity {
            scratch,
            scratch_inode,
            filled,
            available,
            observation,
            ..
        } = capacity;
        let NamespaceImages {
            source,
            backup,
            source_hash,
            backup_hash,
            original_meta,
            ballast,
        } = images;
        ensure!(
            fs::metadata(&source)?.ino() == original_meta.ino()
                && fs::metadata(&scratch)?.ino() == scratch_inode
                && sha(&source)? == source_hash
                && sha(&scratch)? == backup_hash
                && sha(&backup)? == backup_hash,
            "kernel exchange refusal changed an image or pathname"
        );
        fixture.phase(proof, data, &source, "space-check", "full-source")?;
        fixture.phase(proof, data, &scratch, "space-check", "full-scratch")?;
        fixture.phase(proof, data, &backup, "space-check", "full-backup")?;
        let (after_ballast, _) = bounded_fill(data, &ballast, false)?;
        ensure!(
            after_ballast == filled,
            "space released before full reopens"
        );
        fs::remove_file(&ballast)?;
        fs::File::open(data)?.sync_all()?;
        let freed = free_space(data)?;
        ensure!(
            freed > COPY_BYTES,
            "insufficient space for a fresh replacement"
        );

        let (stdout, _) = fixture.command(
            proof,
            vec![
                path(data),
                path(&source),
                "namespace-failure-child".into(),
                "0".into(),
            ],
            "retry-exchange",
            false,
        )?;
        ensure!(
            fs::read_to_string(stdout)? == "namespace compaction completed\n",
            "missing successful exchange retry"
        );
        proof["retry_mount"] = macos_volume::remount(proof, fixture.output, &fixture.env)?;
        let retried = fs::symlink_metadata(&source)?;
        ensure!(
            retried.is_file()
                && retried.ino() != original_meta.ino()
                && retried.ino() != scratch_inode
                && retried.mode() & 0o7777 == 0o640
                && partial_path(data, "compact")? == scratch
                && fs::metadata(&scratch)?.ino() == scratch_inode
                && sha(&source)? == backup_hash
                && sha(&scratch)? == backup_hash
                && sha(&backup)? == backup_hash,
            "retry or remount changed the compacted bytes, scratch or source mode"
        );
        fixture.phase(proof, data, &source, "space-check", "retry-source")?;
        fixture.phase(
            proof,
            data,
            &scratch,
            "space-check",
            "retry-preserved-scratch",
        )?;
        fixture.phase(proof, data, &source, "space-advance", "advanced")?;
        fixture.phase(proof, data, &source, "space-check", "advanced-source")?;
        fixture.phase(proof, data, &backup, "space-check", "final-backup")?;
        verified_copy(&scratch, &fixture.output.join("scratch.wdb"), COPY_BYTES)?;
        verified_copy(
            &source,
            &fixture.output.join("advanced.wdb"),
            fs::metadata(&source)?.len(),
        )?;
        verified_copy(&backup, &fixture.output.join("backup.wdb"), COPY_BYTES)?;
        proof["maintenance_cases"].as_array_mut().unwrap().push(json!({"operation":"compact","exchange":observation,"source_sha256":source_hash,"scratch_sha256":backup_hash,"original_inode":original_meta.ino(),"scratch_inode":scratch_inode,"retried_inode":retried.ino(),"ballast_bytes":filled,"free_at_exchange":available,"free_after_release":freed,"retry_matches_backup":true,"scratch_preserved_after_remount":true}));
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn verify_published_at_capacity(
        fixture: &mut Fixture<'_>,
        proof: &mut Value,
        data: &Path,
        images: &NamespaceImages,
        full: &AtCapacity,
        new_inode: u64,
    ) -> Result<u64> {
        use std::os::unix::fs::MetadataExt;
        proof["cleanup_mount"] = macos_volume::remount(proof, fixture.output, &fixture.env)?;
        let canonical = fs::symlink_metadata(&images.source)?;
        ensure!(
            canonical.is_file()
                && canonical.ino() == new_inode
                && canonical.mode() & 0o7777 == 0o640
                && canonical.nlink() == 1
                && !full.scratch.try_exists()?
                && fs::read_dir(data)?.count() == 3
                && sha(&images.source)? == images.backup_hash
                && sha(&images.backup)? == images.backup_hash
                && fs::metadata(&images.ballast)?.len() == full.filled,
            "APFS cleanup or remount changed the canonical bytes or left the old inode"
        );
        let free = free_space(data)?;
        ensure!(
            free > full.available,
            "cleanup freed no APFS space with the ballast still present"
        );
        fixture.phase(proof, data, &images.source, "space-check", "cleaned-source")?;
        fixture.phase(proof, data, &images.backup, "space-check", "cleaned-backup")?;
        fixture.phase(proof, data, &images.source, "space-advance", "advanced")?;
        fixture.phase(
            proof,
            data,
            &images.source,
            "space-check",
            "advanced-source",
        )?;
        fixture.phase(proof, data, &images.backup, "space-check", "final-backup")?;
        ensure!(
            fs::metadata(&images.ballast)?.len() == full.filled,
            "ballast changed before the next commit"
        );
        verified_copy(
            &images.source,
            &fixture.output.join("advanced.wdb"),
            fs::metadata(&images.source)?.len(),
        )?;
        verified_copy(
            &images.backup,
            &fixture.output.join("backup.wdb"),
            COPY_BYTES,
        )?;
        Ok(free)
    }

    #[cfg(target_os = "macos")]
    fn qualify_cleanup(
        run: &Run,
        fixture: &mut Fixture<'_>,
        proof: &mut Value,
        seed: &Path,
        data: &Path,
    ) -> Result<()> {
        let images = NamespaceImages::install(fixture, proof, seed, data)?;
        let full = namespace_pressure(run, fixture, proof, data, &images, NamespacePhase::Cleanup)?;
        let free = verify_published_at_capacity(
            fixture,
            proof,
            data,
            &images,
            &full,
            full.canonical_inode,
        )?;
        proof["maintenance_cases"].as_array_mut().unwrap().push(json!({"operation":"compact","cleanup":full.observation,"old_inode":full.scratch_inode,"canonical_inode":full.canonical_inode,"ballast_bytes":full.filled,"free_before_unlink":full.available,"free_after_remount":free,"scratch_absent_after_remount":true,"canonical_sha256":images.backup_hash,"advanced_with_ballast":true}));
        Ok(())
    }

    fn qualify(
        fixture: &mut Fixture<'_>,
        proof: &mut Value,
        seed: &Path,
        data: &Path,
    ) -> Result<()> {
        let source = data.join("source.wdb");
        let backup = data.join("backup.wdb");
        let mut source_hash = verified_copy(&seed.join("source.wdb"), &source, SOURCE_BYTES)?;
        let backup_hash = verified_copy(&seed.join("backup.wdb"), &backup, COPY_BYTES)?;
        proof["installed_seed"] = json!({"source_sha256":source_hash,"source_bytes":SOURCE_BYTES,
            "backup_sha256":backup_hash,"backup_bytes":COPY_BYTES});
        fixture.phase(proof, data, &source, "space-check", "initial-source")?;
        fixture.phase(proof, data, &backup, "space-check", "initial-backup")?;
        let (_, pid) = fixture.command(
            proof,
            vec![
                path(data),
                path(&source),
                "space-fail".into(),
                "backup".into(),
            ],
            "ample-space-control",
            true,
        )?;
        let control = data.join("failed-backup.wdb");
        ensure!(
            sha(&control)? == backup_hash,
            "negative control did not produce working backup bytes"
        );
        fixture.phase(proof, data, &control, "space-check", "ample-space-backup")?;
        fs::remove_file(control)?;
        proof["negative_control"] = json!({"pid":pid,"exit":101,"copy_verified":true});

        #[cfg(windows)]
        let quota = if fixture.owner_quota {
            Some(OwnerQuota::setup(fixture, proof, data, &source, &backup)?)
        } else {
            None
        };
        #[cfg(windows)]
        let quota_active = quota.is_some();
        #[cfg(target_os = "macos")]
        let quota_active = false;
        let available_space = |data: &Path| -> Result<u64> {
            #[cfg(windows)]
            if quota_active {
                return OwnerQuota::free(data);
            }
            free_space(data)
        };
        let ballast = data.join("ballast");
        for op in OPERATIONS {
            let (filled, saturated, available) = if quota_active {
                #[cfg(windows)]
                quota.as_ref().unwrap().limit(
                    fixture,
                    proof,
                    &format!("quota-cap-{op}"),
                    HEADROOM,
                )?;
                (0, 0, available_space(data)?)
            } else {
                headroom(data, &ballast)?
            };
            let failure = fixture.phase(proof, data, &source, "space-fail", op)?;
            ensure!(
                sha(&source)? == source_hash && sha(&backup)? == backup_hash,
                "failed operation modified a good image"
            );
            let partial = partial_path(data, op)?;
            let partial_hash = sha(&partial)?;
            let remaining = available_space(data)?;
            ensure!(remaining < available, "failed operation consumed no space");
            #[cfg(windows)]
            if let Some(quota) = &quota {
                quota.owner.verify(&partial)?;
                ensure!(
                    fs::metadata(&partial)?.len() == HEADROOM,
                    "copy did not exhaust its owner allowance"
                );
            }
            let (before_ballast, before_reopen) = if quota_active {
                (0, remaining)
            } else {
                bounded_fill(data, &ballast, false)?
            };
            let full_source = fixture.phase(
                proof,
                data,
                &source,
                "space-check",
                &format!("full-source-{op}"),
            )?;
            let full_backup = fixture.phase(
                proof,
                data,
                &backup,
                "space-check",
                &format!("full-backup-{op}"),
            )?;
            #[cfg(windows)]
            let after_reopen = {
                let after_reopen = available_space(data)?;
                let expected = if quota_active { before_reopen } else { 0 };
                ensure!(
                    after_reopen == expected,
                    "fresh opens changed the pressure boundary"
                );
                after_reopen
            };
            #[cfg(target_os = "macos")]
            let after_reopen = {
                let (after_ballast, after_reopen) = bounded_fill(data, &ballast, false)?;
                ensure!(
                    before_ballast == after_ballast,
                    "fresh opens made additional space writable"
                );
                after_reopen
            };
            if quota_active {
                #[cfg(windows)]
                {
                    let quota = quota.as_ref().unwrap();
                    quota.probe_full(fixture, proof, data, op)?;
                    quota.raise(fixture, proof, data, op)?;
                }
            } else {
                fs::remove_file(&ballast)?;
                let control = data.join("write-control");
                {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&control)?;
                    file.write_all(&[0x6a; CLUSTER as usize])?;
                    file.sync_all()?;
                }
                fs::remove_file(control)?;
            }
            let freed = available_space(data)?;
            ensure!(freed >= COPY_BYTES, "retry lacks physical copy headroom");
            let retry = fixture.phase(proof, data, &source, "space-retry", op)?;
            let retried = if PROFILE.replaces_source(op) {
                source_hash = backup_hash.clone();
                source.clone()
            } else {
                data.join(format!("retry-{op}.wdb"))
            };
            #[cfg(windows)]
            if let Some(quota) = &quota {
                quota.owner.verify(&retried)?;
            }
            ensure!(
                sha(&retried)? == backup_hash
                    && sha(&backup)? == backup_hash
                    && sha(&source)? == source_hash
                    && sha(&partial)? == partial_hash,
                "retry changed a protected image or differed from the good backup"
            );
            fixture.phase(
                proof,
                data,
                &retried,
                "space-check",
                &format!("retried-{op}"),
            )?;
            fixture.phase(proof, data, &source, "space-retire", op)?;
            let mut record = json!({"operation":op,"initial_ballast_bytes":filled,
                "free_at_calibration":saturated,"copy_headroom":available,"free_after_copy":remaining,"free_at_fresh_opens":before_reopen,
                "free_after_fresh_opens":after_reopen,"ballast_at_reopen_bytes":before_ballast,"free_after_removal":freed,
                "failure":failure,"full_source":full_source,"full_backup":full_backup,"retry":retry,"partial_sha256":partial_hash});
            if quota_active {
                for key in [
                    "initial_ballast_bytes",
                    "free_at_calibration",
                    "copy_headroom",
                    "free_after_removal",
                    "ballast_at_reopen_bytes",
                ] {
                    record.as_object_mut().unwrap().remove(key);
                }
                record["quota_headroom"] = json!(HEADROOM);
                record["physical_free_before"] = json!(available);
                record["physical_free_after_raise"] = json!(freed);
                record["quota_still_full_after_reopen"] = json!(true);
                record["same_owner_fsynced_write"] = json!(CLUSTER);
            }
            proof["maintenance_cases"]
                .as_array_mut()
                .unwrap()
                .push(record);
            write_proof(fixture.output, proof)?;
        }
        fixture.phase(proof, data, &source, "space-advance", "advanced")?;
        fixture.phase(proof, data, &source, "space-check", "advanced-source")?;
        fixture.phase(proof, data, &backup, "space-check", "final-backup")?;
        ensure!(
            sha(&source)? != source_hash && sha(&backup)? == backup_hash,
            "final acknowledged commit or preserved backup"
        );
        verified_copy(
            &source,
            &fixture.output.join("advanced.wdb"),
            fs::metadata(&source)?.len(),
        )?;
        verified_copy(&backup, &fixture.output.join("backup.wdb"), COPY_BYTES)?;
        Ok(())
    }

    pub fn run(run: &Run, opts: Options) -> Result<()> {
        ensure!(
            opts.source.len() == 40
                && opts
                    .source
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "exact source SHA-1 required"
        );
        let output = admission(&run.root, &opts.output)?;
        let binary = dunce::canonicalize(run.root.join(opts.fixture))?;
        ensure!(
            binary.is_file() && !binary.starts_with(&output),
            "fixture must be an existing binary outside output"
        );
        let original_hash = sha(&binary)?;
        fs::create_dir_all(output.parent().context("output parent")?)?;
        fs::create_dir(&output)?;
        let runs = output.join("runs");
        fs::create_dir(&runs)?;
        let copy = output.join(format!("fixture{}", std::env::consts::EXE_SUFFIX));
        verified_copy(&binary, &copy, fs::metadata(&binary)?.len())?;
        #[cfg(windows)]
        let env: BTreeMap<_, _> = run
            .env
            .iter()
            .filter(|(k, _)| {
                matches!(
                    k.to_string_lossy().to_ascii_uppercase().as_str(),
                    "SYSTEMROOT" | "WINDIR" | "TEMP" | "TMP" | "COMSPEC" | "PATH"
                )
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        #[cfg(target_os = "macos")]
        let env = BTreeMap::from([("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into())]);
        let mut fixture = Fixture {
            binary: &copy,
            output: &output,
            runs: &runs,
            source: &opts.source,
            env: env.clone(),
            processes: BTreeSet::new(),
            profile: PROFILE,
            #[cfg(windows)]
            owner_quota: opts.owner_quota,
        };
        let (mode, scope) = match PROFILE {
            #[cfg(any(windows, test))]
            Profile::Ntfs => (
                "ntfs-enospc",
                "near-cap copy exhaustion on private fixed NTFS",
            ),
            #[cfg(any(windows, test))]
            Profile::NtfsQuota(_) => unreachable!(),
            #[cfg(any(target_os = "macos", test))]
            Profile::Apfs => (
                "enospc",
                "near-cap copy exhaustion on private APFS disk image",
            ),
        };
        #[cfg(windows)]
        let scope = if opts.owner_quota {
            "near-cap copy exhaustion under an owner quota on private fixed NTFS"
        } else {
            scope
        };
        #[cfg(target_os = "macos")]
        let scope = if opts.namespace_enospc {
            "kernel ENOSPC at APFS compaction exchange"
        } else if opts.cleanup_enospc {
            "APFS compaction cleanup after allocation reaches ENOSPC"
        } else {
            scope
        };
        fixture
            .env
            .insert("WEEKEND_PRIVATE_MAINTENANCE".into(), mode.into());
        let mut proof = json!({"verified":false,"scope":scope,
            "source_id":opts.source,"fixture_sha256":original_hash,"runner_sha256":sha(&std::env::current_exe()?)?,
            "commands":[],"maintenance_records":[],"maintenance_cases":[]});
        let seed = output.join("seed");
        let result = (|| -> Result<()> {
            let fixture_sources = run.root.join("tools/resource-envelope");
            let inputs = [
                "src/compound.rs",
                "src/space.rs",
                #[cfg(windows)]
                "src/quota_token.rs",
                #[cfg(windows)]
                "../quota/windows_identity.rs",
                "Cargo.toml",
                "Cargo.lock",
            ]
            .into_iter()
            .map(|name| Ok((name, sha(&fixture_sources.join(name))?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
            proof["fixture_inputs_sha256"] = json!(inputs);
            #[cfg(windows)]
            {
                proof["host_volume"] = crate::windows_process::volume(&output)?;
            }
            #[cfg(target_os = "macos")]
            {
                proof["host_volume"] = macos_volume::host_volume(&output)?;
            }
            let (stdout, pid) =
                fixture.command(&mut proof, vec![path(&seed)], "near-cap-seed", false)?;
            let rows = rows(&stdout)?;
            oracle::compound(&rows, "native", &opts.source)?;
            proof["seed"] = json!({"pid":pid,"rows":rows});
            #[cfg(windows)]
            windows_volume::with_volume(
                &mut proof,
                &output,
                &env,
                Capacity::Maintenance,
                |proof, data| qualify(&mut fixture, proof, &seed, data),
            )?;
            #[cfg(target_os = "macos")]
            macos_volume::with_volume(&mut proof, &output, &env, |proof, data| {
                if opts.namespace_enospc {
                    qualify_exchange(run, &mut fixture, proof, &seed, data)
                } else if opts.cleanup_enospc {
                    qualify_cleanup(run, &mut fixture, proof, &seed, data)
                } else {
                    qualify(&mut fixture, proof, &seed, data)
                }
            })?;
            ensure!(
                sha(&binary)? == original_hash && sha(&copy)? == original_hash,
                "fixture binary changed"
            );
            for (name, digest) in inputs {
                ensure!(
                    sha(&fixture_sources.join(name))? == digest,
                    "fixture source changed"
                );
            }
            for name in ["rejected-backup.wdb", "restored.wdb", "compact.wdb"] {
                fs::remove_file(seed.join(name))?;
            }
            #[cfg(target_os = "macos")]
            ensure!(
                proof["exchange_completed_at_capacity"].is_null(),
                "APFS exchange completed at capacity without the required kernel ENOSPC"
            );
            Ok(())
        })();
        proof["verified"] = json!(result.is_ok());
        if let Err(error) = &result {
            proof["failure"] = json!(format!("{error:#}"));
        }
        let inventory = delivery::inventory(&output);
        match &inventory {
            Ok(hashes) => {
                proof["artifact_sha256"] = json!(
                    hashes
                        .iter()
                        .filter(|(name, _)| *name != "evidence.json")
                        .collect::<BTreeMap<_, _>>()
                )
            }
            Err(error) => {
                proof["verified"] = json!(false);
                proof["inventory_failure"] = json!(format!("{error:#}"));
            }
        }
        write_proof(&output, &proof)?;
        result?;
        inventory?;
        println!("{}", output.join("evidence.json").display());
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use native::bounded_fill;
#[cfg(any(windows, target_os = "macos"))]
pub use native::{Options, run};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apfs_cleanup_requires_successful_compaction_sizes() {
        let source = "1111111111111111111111111111111111111111";
        let done = oracle::completion("space-cleanup", source);
        let mut row = json!({"stage":"cleanup","operation":"compact","pid":41,"before_bytes":66_154_496,"after_bytes":63_074_304});
        let verify = |row, profile| {
            verify_phase(
                &[row, done.clone()],
                source,
                "space-cleanup",
                "compact",
                41,
                profile,
            )
        };
        verify(row.clone(), Profile::Apfs).unwrap();
        assert!(verify(row.clone(), Profile::Ntfs).is_err());
        row["after_bytes"] = json!(66_154_496);
        assert!(verify(row, Profile::Apfs).is_err());
    }

    #[test]
    fn apfs_exchange_requires_the_native_enospc_and_uncertain_outcome() {
        let source = "1111111111111111111111111111111111111111";
        let done = oracle::completion("space-exchange", source);
        let row = json!({"stage":"exchange","operation":"compact","pid":41,"error":15,"native_operation":"Exchange","category":"DiskFull","raw":28});
        let verify = |row, profile| {
            verify_phase(
                &[row, done.clone()],
                source,
                "space-exchange",
                "compact",
                41,
                profile,
            )
        };
        verify(row.clone(), Profile::Apfs).unwrap();
        assert!(verify(row.clone(), Profile::Ntfs).is_err());
        for (field, value) in [
            ("raw", json!(5)),
            ("native_operation", json!("Write")),
            ("error", json!(14)),
        ] {
            let mut invalid = row.clone();
            invalid[field] = value;
            assert!(verify(invalid, Profile::Apfs).is_err(), "{field}");
        }
    }

    #[test]
    fn apfs_completed_exchange_requires_the_success_shape() {
        let source = "1111111111111111111111111111111111111111";
        let done = oracle::completion("space-exchange", source);
        let row = json!({
            "stage":"exchange-completed","operation":"compact","pid":41,
            "before_bytes":66_154_496,"after_bytes":63_074_304
        });
        let verify = |row, profile| {
            verify_phase(
                &[row, done.clone()],
                source,
                "space-exchange",
                "compact",
                41,
                profile,
            )
        };
        verify(row.clone(), Profile::Apfs).unwrap();
        assert!(verify(row.clone(), Profile::Ntfs).is_err());
        for (field, value) in [
            ("after_bytes", json!(66_154_496)),
            ("raw", json!(28)),
            ("pid", json!(42)),
        ] {
            let mut invalid = row.clone();
            invalid[field] = value;
            assert!(verify(invalid, Profile::Apfs).is_err(), "{field}");
        }
    }

    #[test]
    fn ntfs_owner_quota_requires_the_calibrated_native_code_and_category() {
        for (code, category) in [(112, "DiskFull"), (1295, "Quota")] {
            let mut rows = [
                json!({"stage":"fail","operation":"backup","pid":10,"error":14,"category":category,
                    "raw":code,"native_operation":"Write","partial_bytes":8*oracle::MIB,
                    "source_unchanged":true,"backup_unchanged":true}),
                oracle::completion("space-fail", "source"),
            ];
            let profile = Profile::NtfsQuota(code);
            assert!(verify_phase(&rows, "source", "space-fail", "backup", 10, profile).is_ok());
            rows[0]["category"] = json!(if code == 112 { "Quota" } else { "DiskFull" });
            assert!(verify_phase(&rows, "source", "space-fail", "backup", 10, profile).is_err());
        }
    }

    #[test]
    fn ntfs_exhaustion_requires_real_partial_write_and_new_compact_destination() {
        let source = "source";
        let mut failure = [
            json!({"stage":"fail","operation":"compact","pid":10,"error":14,"category":"DiskFull",
            "raw":112,"native_operation":"Write","partial_bytes":8*oracle::MIB,"source_unchanged":true,"backup_unchanged":true}),
            oracle::completion("space-fail", source),
        ];
        assert!(verify_phase(&failure, source, "space-fail", "compact", 10, Profile::Ntfs).is_ok());
        for (field, wrong) in [
            ("raw", json!(28)),
            ("native_operation", json!("Open")),
            ("partial_bytes", json!(0)),
            ("pid", json!(11)),
        ] {
            let original = std::mem::replace(&mut failure[0][field], wrong);
            assert!(
                verify_phase(&failure, source, "space-fail", "compact", 10, Profile::Ntfs).is_err()
            );
            failure[0][field] = original;
        }
        let mut retry = [
            json!({"stage":"retry","operation":"compact","pid":12,"partial_rejected":true,
            "existing_rejected":true,"partial_preserved":true,"matches_backup":true}),
            oracle::completion("space-retry", source),
        ];
        assert!(verify_phase(&retry, source, "space-retry", "compact", 12, Profile::Ntfs).is_ok());
        assert!(verify_phase(&retry, source, "space-retry", "compact", 12, Profile::Apfs).is_err());
        retry[0]["existing_rejected"] = json!(false);
        assert!(verify_phase(&retry, source, "space-retry", "compact", 12, Profile::Ntfs).is_err());
        assert!(verify_phase(&retry, source, "space-retry", "compact", 12, Profile::Apfs).is_ok());
    }
}
