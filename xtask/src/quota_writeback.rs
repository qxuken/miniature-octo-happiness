//! Route one ext4 data block to a kernel error target at a commit barrier.
use super::{Calls, Guest, Output, Scratch, cmd, fs, json, path, quota_oracle, sha};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::Read,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const SECTOR: u64 = 512;
const BLOCK: u64 = 4096;
const BLOCK_SECTORS: u64 = BLOCK / SECTOR;
pub(super) const GATE_SOURCE: &str = "tools/resource-envelope/writeback_gate.c";

fn held_fixture<T>(
    calls: &mut Calls<'_, '_>,
    mut argv: Vec<OsString>,
    label: &str,
    gate: &Path,
    limits: (Duration, f64),
    arm: impl FnOnce(&mut Calls<'_, '_>, &[String]) -> Result<T>,
) -> Result<(Output, T)> {
    argv[0] = path(&calls.tools["setpriv"]);
    let tools = calls.tools.clone();
    let mut sequence = calls.sequence;
    let ready = calls.c.ready_process(
        argv,
        label,
        limits,
        true,
        |line| {
            let mut words = line.split_ascii_whitespace();
            ensure!(
                words.next() == Some("ready") && words.next() == Some(label),
                "wrong fixture readiness"
            );
            let pid: u32 = words.next().context("missing fixture PID")?.parse()?;
            ensure!(pid > 0, "invalid fixture PID");
            let details = words.map(str::to_owned).collect::<Vec<_>>();
            Ok((pid, details))
        },
        |c, (pid, details)| {
            let mut active = Calls { c, tools, sequence };
            let armed = arm(&mut active, &details);
            sequence = active.sequence;
            let proof = armed?;
            fs::create_dir(gate)?;
            Ok((pid, proof))
        },
    );
    calls.sequence = sequence;
    let (pid, proof) = ready?;
    let log = fs::read_to_string(calls.c.output.join(format!("{label}.stdout.log")))?;
    let (ready, stdout) = log
        .split_once('\n')
        .context("missing fixture readiness line")?;
    let output = calls.c.evidence["commands"]
        .as_array()
        .unwrap()
        .last()
        .context("missing fixture supervision")?;
    ensure!(
        ready
            .split_ascii_whitespace()
            .nth(2)
            .and_then(|s| s.parse().ok())
            == Some(pid)
            && output["name"] == label
            && output["supervision"]["pid"] == pid,
        "writeback fixture PID disagreement"
    );
    Ok((
        Output {
            stdout: stdout.to_owned(),
            stderr: fs::read_to_string(calls.c.output.join(format!("{label}.stderr.log")))?,
            pid,
            exit: 0,
        },
        proof,
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    Append,
    RootSlot,
    Witness,
}

impl Boundary {
    fn name(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::RootSlot => "slot",
            Self::Witness => "witness",
        }
    }

    fn stage(self) -> &'static str {
        match self {
            Self::Append => "fail-sync",
            Self::RootSlot => "fail-slot-sync",
            Self::Witness => "fail-witness-sync",
        }
    }

    fn offset(self, end: usize) -> usize {
        // Revision 4 uses the even slot (region 1) and witness (region 3).
        match self {
            Self::Append => end,
            Self::RootSlot => BLOCK as usize,
            Self::Witness => (3 * BLOCK) as usize,
        }
    }
}

pub(super) fn formatter(tool: &Path, image: &Path) -> Vec<OsString> {
    let mut args = vec![path(tool)];
    args.extend(cmd([
        "-t", "ext4", "-q", "-F", "-O", "^quota", "-b", "4096",
    ]));
    args.push(path(image));
    args
}

fn mapped_block(report: &str, logical: u64, capacity: u64, unwritten: bool) -> Result<u64> {
    ensure!(capacity > BLOCK_SECTORS, "invalid mapper capacity");
    let mut found = None;
    for line in report.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let Some(index) = fields.first().and_then(|s| s.strip_suffix(':')) else {
            continue;
        };
        if index.parse::<u32>().is_err() {
            continue;
        }
        let [_, start, end, physical, physical_end, count, rest @ ..] = fields.as_slice() else {
            anyhow::bail!("incomplete FIEMAP row");
        };
        let start: u64 = start.strip_suffix("..").context("extent start")?.parse()?;
        let end: u64 = end.strip_suffix(':').context("extent end")?.parse()?;
        if !(start..=end).contains(&logical) {
            continue;
        }
        let physical: u64 = physical
            .strip_suffix("..")
            .context("physical start")?
            .parse()?;
        let physical_end: u64 = physical_end
            .strip_suffix(':')
            .context("physical end")?
            .parse()?;
        let count: u64 = count.strip_suffix(':').context("extent length")?.parse()?;
        let flags = rest
            .last()
            .filter(|s| !s.ends_with(':'))
            .copied()
            .unwrap_or("");
        ensure!(
            found.is_none()
                && (!unwritten || (count == BLOCK_SECTORS && logical == start))
                && logical
                    .checked_add(BLOCK_SECTORS - 1)
                    .is_some_and(|last| last <= end)
                && end.checked_sub(start).and_then(|n| n.checked_add(1)) == Some(count)
                && count > 0
                && physical.checked_add(count - 1) == Some(physical_end)
                && physical_end < capacity - 1
                && physical > 0
                && physical.is_multiple_of(BLOCK_SECTORS)
                && flags.split(',').any(|f| f == "unwritten") == unwritten
                && flags
                    .split(',')
                    .all(|f| matches!(f, "" | "unwritten" | "last" | "eof")),
            "target lacks an exact private data block"
        );
        let sector = physical + (logical - start);
        ensure!(
            sector.is_multiple_of(BLOCK_SECTORS),
            "target block alignment"
        );
        found = Some(sector);
    }
    found.context("FIEMAP omitted the target data block")
}

pub(super) struct Mapping {
    name: String,
    uuid: String,
    loop_device: String,
    node: PathBuf,
    backing: String,
    sectors: u64,
    fault: Option<String>,
}

fn mapper_identity(
    report: &str,
    expected_name: &str,
    expected_uuid: &str,
) -> Result<Option<(u32, u32)>> {
    let report = report.trim();
    if report.is_empty() || report == "No devices found" {
        return Ok(None);
    }
    let mut fields = report.split(':').map(str::trim);
    let name = fields.next().context("missing mapper name")?;
    let uuid = fields.next().context("missing mapper UUID")?;
    let major = fields.next().context("missing mapper major")?.parse()?;
    let minor = fields.next().context("missing mapper minor")?.parse()?;
    ensure!(
        fields.next().is_none() && name == expected_name && uuid == expected_uuid,
        "foreign device mapper identity"
    );
    Ok(Some((major, minor)))
}

fn bounded_image(path: &Path) -> Result<Vec<u8>> {
    let cap = 2 * super::MIB;
    let mut image = Vec::new();
    fs::File::open(path)?
        .take(cap + 1)
        .read_to_end(&mut image)?;
    ensure!(
        image.len() as u64 <= cap,
        "writeback image exceeds fixture bound"
    );
    Ok(image)
}

fn assert_only_block_changed(before: &[u8], after: &[u8], offset: usize) -> Result<()> {
    let end = offset + BLOCK as usize;
    ensure!(
        before.len() == after.len() && end <= before.len(),
        "invalid guarded image range"
    );
    ensure!(
        before[..offset] == after[..offset]
            && before[end..] == after[end..]
            && before[offset..end] != after[offset..end],
        "bytes outside the expected single metadata block differ or the block did not change"
    );
    Ok(())
}

fn verify_recovered_bytes(seed: &[u8], recovered: &[u8], boundary: Boundary) -> Result<()> {
    ensure!(
        recovered.len() > seed.len(),
        "candidate append did not extend the file"
    );
    let prefix = &recovered[..seed.len()];
    if boundary == Boundary::Witness {
        // Only the already synced candidate slot may change the seed prefix.
        assert_only_block_changed(seed, prefix, Boundary::RootSlot.offset(seed.len()))
    } else {
        ensure!(prefix == seed, "acknowledged prefix changed after remount");
        Ok(())
    }
}

fn medium(
    calls: &mut Calls<'_, '_>,
    guest: &Guest,
    database: &Path,
    source: &str,
    revision: u64,
    committed: usize,
    witnessed: bool,
) -> Result<Value> {
    let physical = fs::metadata(database)?.len();
    let reply = guest.fixture(calls, vec!["media".into(), path(database)], 30., &[0])?;
    let row = quota_oracle::one_json(&reply.stdout)?;
    ensure!(
        row == json!({"stage":"media","source":source,"features":0,"pid":reply.pid,
        "revision":revision,"witnessed":witnessed,"retained_revision":2,"committed_bytes":committed,
        "physical_bytes":physical}),
        "unexpected revision or durable witness after remount"
    );
    Ok(row)
}

impl Mapping {
    pub(super) fn new(scratch: &Scratch, loop_device: &str, bytes: u64) -> Result<Self> {
        let loop_info = fs::symlink_metadata(loop_device)?;
        ensure!(
            loop_info.file_type().is_block_device() && bytes.is_multiple_of(BLOCK),
            "private loop block device and whole ext4 blocks required"
        );
        let name = scratch
            .directory
            .file_name()
            .and_then(|s| s.to_str())
            .context("scratch name")?;
        Ok(Self {
            name: name.into(),
            uuid: format!("wdb-writeback-{name}"),
            node: scratch.directory.join("block"),
            loop_device: loop_device.into(),
            backing: format!(
                "{}:{}",
                libc::major(loop_info.rdev()),
                libc::minor(loop_info.rdev())
            ),
            sectors: bytes / SECTOR,
            fault: None,
        })
    }

    fn linear(&self) -> String {
        format!("0 {} linear {} 0", self.sectors, self.backing)
    }

    fn error_table(&self, start: u64) -> String {
        let after = start + BLOCK_SECTORS;
        format!(
            "0 {start} linear {} 0\n{start} {BLOCK_SECTORS} error\n{after} {} linear {} {after}",
            self.backing,
            self.sectors - after,
            self.backing
        )
    }

    fn dm(calls: &mut Calls<'_, '_>, args: &[&str], cleanup: bool) -> Result<String> {
        let mut argv = vec!["dmsetup".into()];
        argv.extend(cmd(args.iter().copied()));
        Ok(calls.command(argv, 30., &[0], None, cleanup)?.stdout)
    }

    fn identity(&self, calls: &mut Calls<'_, '_>, cleanup: bool) -> Result<Option<(u32, u32)>> {
        let selection = format!("name={}", self.name);
        let output = Self::dm(
            calls,
            &[
                "info",
                "-c",
                "--noheadings",
                "--separator",
                ":",
                "-o",
                "name,uuid,major,minor",
                "--select",
                &selection,
            ],
            cleanup,
        )?;
        mapper_identity(&output, &self.name, &self.uuid)
    }

    fn table(&self, calls: &mut Calls<'_, '_>, cleanup: bool) -> Result<String> {
        let table = Self::dm(calls, &["table", &self.name], cleanup)?;
        Ok(table
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn owned_table(
        &self,
        calls: &mut Calls<'_, '_>,
        scratch: &Scratch,
        cleanup: bool,
    ) -> Result<String> {
        scratch.ownership()?;
        ensure!(
            calls.loops(scratch, cleanup)? == [self.loop_device.as_str()],
            "writeback loop identity changed"
        );
        self.identity(calls, cleanup)?
            .context("writeback mapping disappeared")?;
        let table = self.table(calls, cleanup)?;
        ensure!(
            table == self.linear() || self.fault.as_ref() == Some(&table),
            "writeback mapping drifted"
        );
        Ok(table)
    }

    pub(super) fn create(&self, calls: &mut Calls<'_, '_>, scratch: &Scratch) -> Result<()> {
        ensure!(
            self.identity(calls, false)?.is_none(),
            "mapper name already exists"
        );
        Self::dm(
            calls,
            &[
                "create",
                "--noudevsync",
                &self.name,
                "--uuid",
                &self.uuid,
                "--table",
                &self.linear(),
            ],
            false,
        )?;
        ensure!(
            self.owned_table(calls, scratch, false)? == self.linear(),
            "initial mapping is not linear"
        );
        let (major, minor) = self
            .identity(calls, false)?
            .context("missing device node identity")?;
        calls.exec(vec![
            "mknod".into(),
            "--mode=0600".into(),
            path(&self.node),
            "b".into(),
            major.to_string().into(),
            minor.to_string().into(),
        ])?;
        let info = fs::symlink_metadata(&self.node)?;
        ensure!(
            info.file_type().is_block_device()
                && (libc::major(info.rdev()), libc::minor(info.rdev())) == (major, minor),
            "wrong private block node"
        );
        self.mount(calls, scratch)?;
        Ok(())
    }

    fn mount(&self, calls: &mut Calls<'_, '_>, scratch: &Scratch) -> Result<()> {
        calls.exec(vec![
            "mount".into(),
            "-t".into(),
            "ext4".into(),
            "-o".into(),
            "rw,nodiscard,nodev,nosuid,noexec".into(),
            path(&self.node),
            path(&scratch.mount),
        ])?;
        self.verify_mount(calls, scratch)?;
        Ok(())
    }

    pub(super) fn verify_mount(
        &self,
        calls: &mut Calls<'_, '_>,
        scratch: &Scratch,
    ) -> Result<Value> {
        calls.verify_mount_source(
            scratch,
            &self.loop_device,
            self.node.to_str().context("block node path")?,
        )
    }

    fn switch(
        &self,
        calls: &mut Calls<'_, '_>,
        scratch: &Scratch,
        table: &str,
        cleanup: bool,
    ) -> Result<()> {
        self.owned_table(calls, scratch, cleanup)?;
        Self::dm(calls, &["load", &self.name, "--table", table], cleanup)?;
        Self::dm(
            calls,
            &["suspend", "--nolockfs", "--noflush", &self.name],
            cleanup,
        )?;
        Self::dm(calls, &["resume", "--noudevsync", &self.name], cleanup)?;
        ensure!(
            self.owned_table(calls, scratch, cleanup)? == table,
            "requested mapper table did not become active"
        );
        Ok(())
    }

    fn unmount(&self, calls: &mut Calls<'_, '_>, scratch: &Scratch, cleanup: bool) -> Result<()> {
        if let Some(mount) = calls.mount(scratch, cleanup)? {
            quota_oracle::exact_mount(
                &mount,
                &scratch.mount,
                self.node.to_str().context("block node path")?,
            )?;
            calls.command(
                vec!["umount".into(), path(&scratch.mount)],
                30.,
                &[0],
                None,
                cleanup,
            )?;
        }
        ensure!(
            calls.mount(scratch, cleanup)?.is_none(),
            "mapped mount remains active"
        );
        Ok(())
    }

    pub(super) fn retire(&self, calls: &mut Calls<'_, '_>, scratch: &Scratch) -> Result<()> {
        if self.identity(calls, true)?.is_some() {
            // Restore normal I/O before unmount even after an interrupted switch.
            self.switch(calls, scratch, &self.linear(), true)?;
            self.unmount(calls, scratch, true)?;
            Self::dm(
                calls,
                &["remove", "--retry", "--noudevsync", &self.name],
                true,
            )?;
        }
        ensure!(
            self.identity(calls, true)?.is_none(),
            "writeback mapping remains"
        );
        calls.c.evidence["mapper_absent"] = json!(true);
        Ok(())
    }

    pub(super) fn qualify(
        &mut self,
        calls: &mut Calls<'_, '_>,
        scratch: &Scratch,
        guest: &Guest,
        source: &str,
    ) -> Result<()> {
        let mut pids = BTreeSet::new();
        let mut publication = Vec::new();
        let append =
            self.qualify_case(calls, scratch, guest, source, Boundary::Append, &mut pids)?;
        for (key, field) in [
            ("writeback_extent", "extent"),
            ("writeback_images", "images"),
            ("writeback_media", "media"),
            ("results", "results"),
        ] {
            calls.c.evidence[key] = append[field].clone();
        }
        for boundary in [Boundary::RootSlot, Boundary::Witness] {
            publication
                .push(self.qualify_case(calls, scratch, guest, source, boundary, &mut pids)?);
        }
        calls.c.evidence["publication_writeback"] = json!(publication);
        Ok(())
    }

    fn fault_process(
        &self,
        calls: &mut Calls<'_, '_>,
        scratch: &Scratch,
        guest: &Guest,
        database: &Path,
        boundary: Boundary,
    ) -> Result<Value> {
        let stage = boundary.stage();
        let gate = scratch.directory.join(format!("armed-{}", boundary.name()));
        let (reply, ()) = held_fixture(
            calls,
            guest.fixture_argv(vec![stage.into(), path(database), path(&gate)]),
            stage,
            &gate,
            (Duration::from_secs(10), 35.),
            |active, details| {
                ensure!(
                    details.is_empty(),
                    "unexpected commit fixture readiness details"
                );
                self.switch(active, scratch, self.fault.as_deref().unwrap(), false)
            },
        )?;
        let row = quota_oracle::one_json(reply.stdout.trim_end())?;
        ensure!(row["pid"] == reply.pid, "fixture changed PID after arming");
        Ok(row)
    }

    fn qualify_case(
        &mut self,
        calls: &mut Calls<'_, '_>,
        scratch: &Scratch,
        guest: &Guest,
        source: &str,
        boundary: Boundary,
        pids: &mut BTreeSet<u32>,
    ) -> Result<Value> {
        let database = guest.data.join(format!("database-{}.wdb", boundary.name()));
        let mut output = calls.c.output.clone();
        if boundary != Boundary::Append {
            output.push(boundary.name());
            fs::create_dir(&output)?;
        }
        let mut results = Vec::new();
        let mut check_pid = |row: &Value| -> Result<u32> {
            let pid: u32 = row["pid"]
                .as_u64()
                .context("missing fixture PID")?
                .try_into()?;
            ensure!(
                pid != 0 && pids.insert(pid),
                "writeback fixture reused a process"
            );
            Ok(pid)
        };
        let stage = |calls: &mut Calls<'_, '_>, stage: &str, revision: u64| -> Result<Value> {
            let mut args = vec![stage.into(), path(&database)];
            if stage == "inspect" {
                args.push(revision.to_string().into());
            }
            let reply = guest.fixture(calls, args, 30., &[0])?;
            let row = quota_oracle::one_json(&reply.stdout)?;
            crate::quota_append::verify(&row, source, stage, revision, reply.pid, 5)?;
            Ok(row)
        };
        let seeded = stage(calls, "seed", 3)?;
        check_pid(&seeded)?;
        results.push(seeded);
        let seed = bounded_image(&database)?;
        let bytes = seed.len() as u64;
        ensure!(
            (5 * BLOCK..=super::MIB).contains(&bytes) && bytes.is_multiple_of(BLOCK),
            "unexpected seed length"
        );
        let seed_artifact = output.join("seed.wdb");
        fs::write(&seed_artifact, &seed)?;
        if boundary == Boundary::Append {
            calls.exec(vec![
                "fallocate".into(),
                "--keep-size".into(),
                "--offset".into(),
                bytes.to_string().into(),
                "--length".into(),
                BLOCK.to_string().into(),
                path(&database),
            ])?;
            fs::File::open(&database)?.sync_all()?;
            ensure!(
                bounded_image(&database)? == seed,
                "preallocation changed acknowledged database bytes"
            );
        }
        let extents = calls.exec(vec![
            "filefrag".into(),
            "-v".into(),
            "-b512".into(),
            path(&database),
        ])?;
        let offset = boundary.offset(seed.len());
        let physical = mapped_block(
            &extents.stdout,
            offset as u64 / SECTOR,
            self.sectors,
            boundary == Boundary::Append,
        )?;
        self.fault = Some(self.error_table(physical));
        let extent = json!({"logical_offset":offset,"sector":physical,
            "length":BLOCK,"active_table":self.fault,"linear_table":self.linear(),"dm_name":self.name,"dm_uuid":self.uuid});
        let failure = self.fault_process(calls, scratch, guest, &database, boundary)?;
        let pid = check_pid(&failure)?;
        crate::quota_append::verify(&failure, source, boundary.stage(), 3, pid, 5)?;
        results.push(failure);
        self.switch(calls, scratch, &self.linear(), false)?;
        self.unmount(calls, scratch, false)?;
        self.mount(calls, scratch)?;
        let recovered = bounded_image(&database)?;
        verify_recovered_bytes(&seed, &recovered, boundary)?;
        let revision = if boundary == Boundary::Witness { 4 } else { 3 };
        let media_before = medium(
            calls,
            guest,
            &database,
            source,
            revision,
            if revision == 3 {
                seed.len()
            } else {
                recovered.len()
            },
            revision == 3,
        )?;
        check_pid(&media_before)?;
        ensure!(
            recovered == bounded_image(&database)?,
            "read-only recovery modified the image"
        );
        let failed = output.join("reopened.wdb");
        fs::write(&failed, &recovered)?;
        let reopened = stage(calls, "inspect", revision)?;
        check_pid(&reopened)?;
        results.push(reopened);
        if boundary != Boundary::Witness {
            let retried = stage(calls, "retry", 4)?;
            check_pid(&retried)?;
            results.push(retried);
        }
        self.unmount(calls, scratch, false)?;
        self.mount(calls, scratch)?;
        let stable = bounded_image(&database)?;
        if boundary == Boundary::Witness {
            // Opening the verified root repairs only the witness. The user
            // mutation is already present and must not be repeated.
            assert_only_block_changed(&recovered, &stable, offset)?;
        }
        let media_after = medium(calls, guest, &database, source, 4, stable.len(), true)?;
        check_pid(&media_after)?;
        ensure!(
            stable == bounded_image(&database)?,
            "read-only check modified the image"
        );
        let inspected = stage(calls, "inspect", 4)?;
        check_pid(&inspected)?;
        results.push(inspected);
        ensure!(
            stable == bounded_image(&database)?,
            "witnessed reopen changed the stable image"
        );
        let final_key = if boundary == Boundary::Witness {
            "reconciled"
        } else {
            "retried"
        };
        let final_image = output.join(format!("{final_key}.wdb"));
        fs::write(&final_image, stable)?;
        Ok(
            json!({"boundary":boundary.name(),"extent":extent,"media":[media_before,media_after],"results":results,
            "images":{"seed":sha(&seed_artifact)?,"reopened":sha(&failed)?,
                (final_key):sha(&final_image)?,
                "prefix_verified":true}}),
        )
    }
}

pub(super) struct MaintenanceFault<'a> {
    mapper: &'a mut Mapping,
    scratch: &'a Scratch,
    shim: PathBuf,
    shim_hash: String,
}

impl<'a> MaintenanceFault<'a> {
    pub(super) fn prepare(
        mapper: &'a mut Mapping,
        scratch: &'a Scratch,
        calls: &mut Calls<'_, '_>,
        guest: &Guest,
    ) -> Result<Self> {
        let archived = calls.c.output.join("frozen-source").join(GATE_SOURCE);
        let staged = scratch.directory.join("writeback_gate.c");
        fs::copy(&archived, &staged)?;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o444))?;
        let source_hash = sha(&archived)?;
        ensure!(
            sha(&staged)? == source_hash,
            "writeback gate source changed"
        );
        let build = scratch.directory.join("shim-build");
        super::private_directory(&build, guest.uid)?;
        let library = build.join("gate.so");
        let mut args = guest.drop.clone();
        args.push(path(&calls.tools["cc"]));
        args.extend(cmd([
            "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC",
        ]));
        args.extend([path(&staged), "-o".into(), path(&library), "-ldl".into()]);
        calls.guest(args, &guest.env, 30., &[0])?;
        let shim = scratch.directory.join("writeback-gate.so");
        let shim_hash = super::copy_executable(&library, &shim, 2 * super::MIB)?;
        ensure!(
            sha(&staged)? == source_hash,
            "writeback gate source changed during build"
        );
        calls.c.evidence["writeback_gate"] =
            json!({"source_sha256":source_hash,"binary_sha256":shim_hash});
        Ok(Self {
            mapper,
            scratch,
            shim,
            shim_hash,
        })
    }

    pub(super) fn fail(
        &mut self,
        calls: &mut Calls<'_, '_>,
        guest: &Guest,
        source: &Path,
        operation: &str,
    ) -> Result<(Output, Value, PathBuf)> {
        ensure!(
            sha(&self.shim)? == self.shim_hash,
            "staged writeback gate changed"
        );
        let label = format!("writeback-{operation}");
        let gate = self.scratch.directory.join(format!("armed-{operation}"));
        let destination = format!("failed-{operation}.wdb");
        let mut argv = guest.fixture_argv(vec![
            path(&guest.data),
            path(source),
            "space-fail".into(),
            operation.into(),
        ]);
        argv.splice(
            guest.drop.len()..guest.drop.len(),
            [
                path(&calls.tools["env"]),
                "WEEKEND_PRIVATE_MAINTENANCE=writeback".into(),
                assignment("LD_PRELOAD", &self.shim),
                assignment("WDB_WRITEBACK_DIRECTORY", &guest.data),
                assignment("WDB_WRITEBACK_GATE", &gate),
                format!("WDB_WRITEBACK_LABEL={label}").into(),
                format!(
                    "WDB_WRITEBACK_TARGET={}",
                    if operation == "compact" {
                        "scratch"
                    } else {
                        &destination
                    }
                )
                .into(),
            ],
        );
        let (reply, (target, mut proof)) = held_fixture(
            calls,
            argv,
            &label,
            &gate,
            (Duration::from_secs(60), 120.),
            |active, details| {
                let [device, inode, name] = details else {
                    anyhow::bail!("missing maintenance target readiness");
                };
                ensure!(
                    if operation == "compact" {
                        scratch_name(name)
                    } else {
                        *name == destination
                    },
                    "wrong maintenance target name"
                );
                let device: u64 = device.parse()?;
                let inode: u64 = inode.parse()?;
                let target = guest.data.join(name);
                let info = fs::symlink_metadata(&target)?;
                ensure!(
                    info.is_file()
                        && info.len() == 0
                        && info.nlink() == 1
                        && info.uid() == guest.uid
                        && info.dev() == device
                        && info.ino() == inode
                        && info.dev() == fs::metadata(&guest.data)?.dev(),
                    "wrong maintenance target identity or allocation boundary"
                );
                let geometry = active.exec(vec![
                    "filefrag".into(),
                    "-v".into(),
                    "-b512".into(),
                    path(&target),
                ])?;
                let sector = mapped_block(&geometry.stdout, 0, self.mapper.sectors, true)?;
                let fault = self.mapper.error_table(sector);
                self.mapper.fault = Some(fault.clone());
                self.mapper.switch(active, self.scratch, &fault, false)?;
                Ok((
                    target,
                    json!({"target":name,"sector":sector,"logical_offset":0,"length":BLOCK,
                    "device":device,"inode":inode,"active_table":fault,"linear_table":self.mapper.linear()}),
                ))
            },
        )?;
        self.mapper
            .switch(calls, self.scratch, &self.mapper.linear(), false)?;
        self.remount(calls)?;
        let persisted = fs::symlink_metadata(&target)?;
        ensure!(
            persisted.is_file()
                && persisted.len() == 63_074_304
                && proof["device"] == persisted.dev()
                && proof["inode"] == persisted.ino(),
            "failed maintenance target changed identity after remount"
        );
        let mut header = [0u8; BLOCK as usize];
        fs::File::open(&target)?.read_exact(&mut header)?;
        ensure!(
            header == [0; BLOCK as usize],
            "failed target's first block persisted"
        );
        proof["remounted"] = json!(true);
        proof["failed_header_zero"] = json!(true);
        proof["failed_target_sha256"] = json!(sha(&target)?);
        let images = calls.c.output.join("failed-images");
        fs::create_dir_all(&images)?;
        let copy = images.join(format!("{operation}.wdb"));
        fs::copy(&target, &copy)?;
        ensure!(
            proof["failed_target_sha256"] == sha(&copy)?,
            "copied failed maintenance target changed"
        );
        Ok((reply, proof, target))
    }

    pub(super) fn remount(&self, calls: &mut Calls<'_, '_>) -> Result<()> {
        self.mapper.unmount(calls, self.scratch, false)?;
        self.mapper.mount(calls, self.scratch)
    }
}

fn assignment(name: &str, value: &Path) -> OsString {
    let mut var = OsString::from(format!("{name}="));
    var.push(value);
    var
}

fn scratch_name(name: &str) -> bool {
    name.strip_prefix(".wdb-compact-")
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .is_some_and(|id| {
            id.len() == 32
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_target_must_be_unwritten_exact_and_within_its_owned_device() {
        let row = "  1:  8.. 15:  53448.. 53455:  8:  49360: last,unwritten,eof";
        assert_eq!(mapped_block(row, 8, 262144, true).unwrap(), 53448);
        for bad in [
            row.replace("unwritten", "delalloc"),
            row.replace("unwritten", "unwritten,shared"),
            row.replace("53455", "53454"),
            row.replace("15:", "16:"),
            format!("{row}\n{row}"),
        ] {
            assert!(mapped_block(&bad, 8, 262144, true).is_err(), "{bad}");
        }
        assert!(mapped_block(row, 8, 53456, true).is_err());
        assert!(mapped_block(row, 16, 262144, true).is_err());
    }

    #[test]
    fn metadata_targets_can_be_inside_a_written_extent() {
        let row = "  0:  0.. 111:  53448.. 53559:  112:  last,eof";
        assert_eq!(mapped_block(row, 8, 262144, false).unwrap(), 53456);
        assert_eq!(mapped_block(row, 24, 262144, false).unwrap(), 53472);
        assert!(mapped_block(row, 24, 262144, true).is_err());
        assert!(mapped_block(row, 25, 262144, false).is_err());
        assert!(
            mapped_block(
                &row.replace("last,eof", "last,unwritten,eof"),
                8,
                262144,
                false
            )
            .is_err()
        );
        assert!(
            mapped_block(
                &row.replace("last,eof", "last,shared,eof"),
                8,
                262144,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn recovery_requires_only_the_preceding_metadata_block_to_have_changed() {
        let seed = vec![0; 6 * BLOCK as usize];
        let mut reopened = seed.clone();
        reopened.extend(std::iter::repeat_n(7, BLOCK as usize));
        verify_recovered_bytes(&seed, &reopened, Boundary::RootSlot).unwrap();
        let slot = Boundary::RootSlot.offset(seed.len());
        reopened[slot] = 1;
        assert!(verify_recovered_bytes(&seed, &reopened, Boundary::RootSlot).is_err());
        verify_recovered_bytes(&seed, &reopened, Boundary::Witness).unwrap();
        reopened[Boundary::Witness.offset(seed.len())] = 1;
        assert!(verify_recovered_bytes(&seed, &reopened, Boundary::Witness).is_err());
    }

    #[test]
    fn empty_mapper_reports_are_distinct_from_invalid_or_foreign_devices() {
        for output in ["", "No devices found\n"] {
            assert_eq!(mapper_identity(output, "map", "uuid").unwrap(), None);
        }
        assert_eq!(
            mapper_identity("map:uuid:253:0\n", "map", "uuid").unwrap(),
            Some((253, 0))
        );
        for output in [
            "No devices found\nmap:uuid:253:0",
            "map:other:253:0",
            "map:uuid:253:0\nforeign:uuid:253:1",
        ] {
            assert!(mapper_identity(output, "map", "uuid").is_err(), "{output}");
        }
    }
}
