//! Route one ext4 data block to a kernel error target at a commit barrier.
use super::{
    Calls, CopyBarrier, Guest, Output, Scratch, Space, cmd, fs, json, path, quota_oracle, sha,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
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

pub(super) fn formatter(tool: &Path, image: &Path, fault_journal: bool) -> Vec<OsString> {
    let mut args = vec![path(tool)];
    let features = if fault_journal {
        "^quota,^fast_commit"
    } else {
        "^quota"
    };
    args.extend(cmd([
        "-t", "ext4", "-q", "-F", "-O", features, "-b", "4096",
    ]));
    if fault_journal {
        args.extend(cmd(["-E", "lazy_itable_init=0,lazy_journal_init=0"]));
    }
    args.push(path(image));
    args
}

fn journal_sectors(report: &str, capacity: u64) -> Result<(u64, u64)> {
    ensure!(
        report.starts_with("Inode: 8 "),
        "missing ext4 journal inode"
    );
    let (inode, extents) = report
        .split_once("\nEXTENTS:\n")
        .context("missing journal extents")?;
    let (logical, physical) = extents
        .trim()
        .split_once("):")
        .context("missing journal range")?;
    let last_logical: u64 = logical
        .strip_prefix("(0-")
        .context("journal must start at block zero")?
        .parse()?;
    let (start, end) = physical
        .split_once('-')
        .context("journal must be contiguous")?;
    let start: u64 = start.parse()?;
    let end: u64 = end.parse()?;
    let size = inode
        .lines()
        .find_map(|line| line.split_once(" Size: ").map(|(_, size)| size.trim()))
        .context("missing journal size")?
        .parse::<u64>()?;
    ensure!(
        start > 0
            && last_logical > 0
            && end.checked_sub(start) == Some(last_logical)
            && last_logical
                .checked_add(1)
                .and_then(|n| n.checked_mul(BLOCK))
                == Some(size),
        "journal extent and size disagree"
    );
    let edge = |block: u64| {
        block
            .checked_add(1)
            .and_then(|n| n.checked_mul(BLOCK_SECTORS))
            .context("journal sector overflow")
    };
    let after = edge(end)?;
    ensure!(after < capacity, "journal outside owned volume");
    // Leave the journal superblock addressable. Deny I/O to transaction blocks.
    Ok((edge(start)?, after))
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
        self.error_range(start, start + BLOCK_SECTORS)
    }

    fn error_range(&self, start: u64, after: u64) -> String {
        let length = after - start;
        format!(
            "0 {start} linear {} 0\n{start} {length} error\n{after} {} linear {} {after}",
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
    barrier: CopyBarrier,
}

fn stage_gate(
    calls: &mut Calls<'_, '_>,
    scratch: &Scratch,
    guest: &Guest,
) -> Result<(PathBuf, String)> {
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
    let evidence_gate = calls.c.output.join("writeback-gate.so");
    fs::copy(&shim, &evidence_gate)?;
    ensure!(
        sha(&staged)? == source_hash && sha(&evidence_gate)? == shim_hash,
        "writeback gate source or executable changed during staging"
    );
    calls.c.evidence["writeback_gate"] =
        json!({"source_sha256":source_hash,"binary_sha256":shim_hash});
    Ok((shim, shim_hash))
}

impl<'a> MaintenanceFault<'a> {
    pub(super) fn prepare(
        mapper: &'a mut Mapping,
        scratch: &'a Scratch,
        calls: &mut Calls<'_, '_>,
        guest: &Guest,
        barrier: CopyBarrier,
    ) -> Result<Self> {
        let (shim, shim_hash) = stage_gate(calls, scratch, guest)?;
        Ok(Self {
            mapper,
            scratch,
            shim,
            shim_hash,
            barrier,
        })
    }

    pub(super) fn fail(
        &mut self,
        calls: &mut Calls<'_, '_>,
        guest: &Guest,
        source: &Path,
        operation: &str,
    ) -> Result<(Output, Value, Option<PathBuf>)> {
        ensure!(
            sha(&self.shim)? == self.shim_hash,
            "staged writeback gate changed"
        );
        let label = format!("writeback-{operation}");
        let gate = self.scratch.directory.join(format!("armed-{operation}"));
        let destination = format!("failed-{operation}.wdb");
        let offset = self.barrier.offset();
        let early = self.barrier == CopyBarrier::Data;
        let cleanup_directory = self.barrier == CopyBarrier::CleanupDirectory;
        let cleanup_file = self.barrier == CopyBarrier::CleanupFile;
        let published_file = self.barrier.published_file_sync();
        let scratch_mode = if self.barrier.live_scratch().is_some() {
            0o640
        } else {
            0o600
        };
        let original_info = fs::symlink_metadata(source)?;
        let journal = if offset.is_none() {
            ensure!(
                operation == "compact",
                "journal barrier requires in-place compaction"
            );
            let file = fs::File::open(source)?;
            file.set_permissions(fs::Permissions::from_mode(0o640))?;
            file.sync_all()?;
            self.scratch.ownership()?;
            let geometry = calls.exec(vec![
                "debugfs".into(),
                "-R".into(),
                "stat <8>".into(),
                path(&self.scratch.image),
            ])?;
            Some(journal_sectors(&geometry.stdout, self.mapper.sectors)?)
        } else {
            None
        };
        let gate_len = if early {
            0
        } else if cleanup_directory {
            original_info.len()
        } else {
            63_074_304
        };
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
                format!(
                    "WEEKEND_PRIVATE_MAINTENANCE={}",
                    Space::Writeback(self.barrier).profile()
                )
                .into(),
                assignment("LD_PRELOAD", &self.shim),
                assignment("WDB_WRITEBACK_DIRECTORY", &guest.data),
                assignment("WDB_WRITEBACK_GATE", &gate),
                format!("WDB_WRITEBACK_LABEL={label}").into(),
                format!("WDB_WRITEBACK_BARRIER={}", self.barrier.name()).into(),
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
        let (reply, (target, scratch, mut proof)) = held_fixture(
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
                let scratch = guest.data.join(name);
                let target = if published_file {
                    source.to_path_buf()
                } else {
                    scratch.clone()
                };
                let info = fs::symlink_metadata(&target)?;
                ensure!(
                    info.is_file()
                        && info.len() == gate_len
                        && info.nlink() == 1
                        && info.uid() == guest.uid
                        && info.dev() == device
                        && info.ino() == inode
                        && info.dev() == fs::metadata(&guest.data)?.dev(),
                    "wrong maintenance target identity or allocation boundary"
                );
                let (fault, mut boundary) = if let Some((start, after)) = journal {
                    let canonical = fs::symlink_metadata(source)?;
                    if published_file {
                        ensure!(
                            inode != original_info.ino() && info.mode() & 0o7777 == 0o640,
                            "replacement inode or mode differs at file sync gate"
                        );
                        verify_published_scratch(&scratch, device, &original_info, cleanup_file)?;
                    } else {
                        ensure!(
                            canonical.is_file()
                                && canonical.dev() == device
                                && canonical.uid() == guest.uid
                                && canonical.mode() & 0o7777 == 0o640
                                && canonical.ino() != inode
                                && if cleanup_directory {
                                    inode == original_info.ino() && canonical.len() == 63_074_304
                                } else {
                                    canonical.ino() == original_info.ino()
                                        && canonical.len() == original_info.len()
                                }
                                && info.mode() & 0o7777 == scratch_mode,
                            "wrong namespace or mode at journal gate"
                        );
                    }
                    (
                        self.mapper.error_range(start, after),
                        json!({"journal_inode":8,"sector":start,"end_sector_exclusive":after,"length":(after-start)*SECTOR,"source_inode_before":canonical.ino()}),
                    )
                } else {
                    let offset = offset.context("missing copy barrier offset")?;
                    let geometry = active.exec(vec![
                        "filefrag".into(),
                        "-v".into(),
                        "-b512".into(),
                        path(&target),
                    ])?;
                    let sector = mapped_block(
                        &geometry.stdout,
                        offset / SECTOR,
                        self.mapper.sectors,
                        early,
                    )?;
                    (
                        self.mapper.error_table(sector),
                        json!({"sector":sector,"logical_offset":offset,"length":BLOCK}),
                    )
                };
                self.mapper.fault = Some(fault.clone());
                self.mapper.switch(active, self.scratch, &fault, false)?;
                boundary["target"] = json!(if published_file {
                    "source.wdb"
                } else {
                    name.as_str()
                });
                if published_file {
                    // The file is already clean. Dirty its inode so fsync
                    // must commit a journal transaction at this barrier.
                    let file = fs::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW)
                        .open(&target)?;
                    let opened = file.metadata()?;
                    ensure!(
                        opened.dev() == device && opened.ino() == inode,
                        "replacement changed while opening the verified inode"
                    );
                    file.set_permissions(fs::Permissions::from_mode(0o740))?;
                    ensure!(
                        file.metadata()?.mode() & 0o7777 == 0o740,
                        "pending replacement mode change missing"
                    );
                    boundary["scratch_name"] = json!(name);
                    boundary["pending_source_mode"] = json!(0o740);
                    boundary["scratch_present_at_gate"] = json!(!cleanup_file);
                } else if journal.is_some() {
                    boundary["scratch_mode_before"] = json!(scratch_mode);
                }
                boundary["barrier"] = json!(self.barrier.name());
                boundary["device"] = json!(device);
                boundary["inode"] = json!(inode);
                boundary["active_table"] = json!(fault);
                boundary["linear_table"] = json!(self.mapper.linear());
                Ok((target, scratch, boundary))
            },
        )?;
        self.mapper
            .switch(calls, self.scratch, &self.mapper.linear(), false)?;
        self.remount(calls)?;
        let persisted = fs::symlink_metadata(&target)?;
        ensure!(
            persisted.is_file()
                && persisted.len() == if early { 63_074_304 } else { gate_len }
                && proof["device"] == persisted.dev()
                && proof["inode"] == persisted.ino(),
            "failed maintenance target changed identity after remount"
        );
        proof["remounted"] = json!(true);
        if let Some(offset) = offset {
            let mut block = [0u8; BLOCK as usize];
            let mut file = fs::File::open(&target)?;
            file.seek(SeekFrom::Start(offset))?;
            file.read_exact(&mut block)?;
            ensure!(
                block == [0; BLOCK as usize],
                "failed target block persisted"
            );
            proof["failed_block_zero"] = json!(true);
            if early {
                proof["failed_header_zero"] = json!(true);
            }
        } else {
            let canonical = fs::symlink_metadata(source)?;
            ensure!(
                canonical.is_file()
                    && canonical.dev() == persisted.dev()
                    && canonical.mode() & 0o7777 == 0o640
                    && proof["source_inode_before"] == canonical.ino()
                    && persisted.mode() & 0o7777 == scratch_mode,
                "unexpected source or scratch mode after remount"
            );
            proof["source_mode_remounted"] = json!(0o640);
            proof["source_inode_remounted"] = json!(canonical.ino());
            proof["source_sha256_remounted"] = json!(sha(source)?);
            if published_file {
                let expected = format!("kernel {} fsync: rc=-1 errno=5", self.barrier.name());
                ensure!(
                    reply.stderr == format!("{expected}\n"),
                    "missing exact kernel fsync result"
                );
                proof["kernel_fsync"] = json!({"return":-1,"errno":5});
                verify_published_scratch(&scratch, persisted.dev(), &original_info, cleanup_file)?;
                proof["scratch_present_after_remount"] = json!(!cleanup_file);
                if !cleanup_file {
                    let hash = sha(&scratch)?;
                    fs::copy(&scratch, calls.c.output.join("original-scratch.wdb"))?;
                    ensure!(
                        sha(&calls.c.output.join("original-scratch.wdb"))? == hash,
                        "copied original scratch changed"
                    );
                    proof["scratch_sha256_remounted"] = json!(hash);
                }
            }
            if !cleanup_file {
                proof["scratch_mode_remounted"] = json!(scratch_mode);
            }
        }
        proof["failed_target_sha256"] = json!(sha(&target)?);
        let images = calls.c.output.join("failed-images");
        fs::create_dir_all(&images)?;
        let copy = images.join(format!("{operation}.wdb"));
        fs::copy(&target, &copy)?;
        ensure!(
            proof["failed_target_sha256"] == sha(&copy)?,
            "copied failed maintenance target changed"
        );
        Ok((reply, proof, (!cleanup_file).then_some(scratch)))
    }

    pub(super) fn remount(&self, calls: &mut Calls<'_, '_>) -> Result<()> {
        self.mapper.unmount(calls, self.scratch, false)?;
        self.mapper.mount(calls, self.scratch)
    }
}

fn verify_published_scratch(
    scratch: &Path,
    device: u64,
    original: &fs::Metadata,
    removed: bool,
) -> Result<()> {
    if removed {
        ensure!(
            fs::symlink_metadata(scratch)
                .is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound),
            "unlinked original scratch returned"
        );
    } else {
        let info = fs::symlink_metadata(scratch)?;
        ensure!(
            info.is_file()
                && info.dev() == device
                && info.ino() == original.ino()
                && info.uid() == original.uid()
                && info.nlink() == 1
                && info.len() == original.len()
                && info.mode() & 0o7777 == 0o640,
            "scratch no longer names the original inode"
        );
    }
    Ok(())
}

pub(super) fn publish_full(
    calls: &mut Calls<'_, '_>,
    scratch: &Scratch,
    guest: &Guest,
    device: &str,
) -> Result<(Output, Value)> {
    let (shim, shim_hash) = stage_gate(calls, scratch, guest)?;
    let source = guest.data.join("source.wdb");
    let backup = guest.data.join("backup.wdb");
    let original_hash = sha(&source)?;
    let backup_hash = sha(&backup)?;
    let original = fs::File::open(&source)?;
    original.set_permissions(fs::Permissions::from_mode(0o640))?;
    original.sync_all()?;
    let original_info = original.metadata()?;
    drop(original);
    let gate = scratch.directory.join("armed-namespace-enospc");
    let label = "namespace-enospc";
    let mut argv = guest.fixture_argv(vec![
        path(&guest.data),
        path(&source),
        "space-publish".into(),
        "compact".into(),
    ]);
    argv.splice(
        guest.drop.len()..guest.drop.len(),
        [
            path(&calls.tools["env"]),
            "WEEKEND_PRIVATE_MAINTENANCE=inode-enospc".into(),
            assignment("LD_PRELOAD", &shim),
            assignment("WDB_WRITEBACK_DIRECTORY", &guest.data),
            assignment("WDB_WRITEBACK_GATE", &gate),
            "WDB_WRITEBACK_LABEL=namespace-enospc".into(),
            "WDB_WRITEBACK_BARRIER=namespace-enospc".into(),
            "WDB_WRITEBACK_TARGET=scratch".into(),
        ],
    );
    let (out, (target, mut proof)) = held_fixture(
        calls,
        argv,
        label,
        &gate,
        (Duration::from_secs(60), 180.),
        |active, details| {
            let [dev, inode, name] = details else {
                anyhow::bail!("missing compaction scratch readiness");
            };
            ensure!(scratch_name(name), "invalid compaction scratch name");
            let target = guest.data.join(name);
            let meta = fs::symlink_metadata(&target)?;
            ensure!(
                meta.is_file()
                    && meta.dev() == dev.parse::<u64>()?
                    && meta.dev() == original_info.dev()
                    && meta.ino() == inode.parse::<u64>()?
                    && meta.ino() != original_info.ino()
                    && meta.nlink() == 1
                    && meta.uid() == guest.uid
                    && meta.mode() & 0o7777 == 0o640
                    && meta.len() == 63_074_304
                    && sha(&target)? == backup_hash
                    && sha(&source)? == original_hash,
                "wrong file at compaction publication gate"
            );
            active.verify_mount(scratch, device)?;
            let inodes = guest.probe(
                active,
                "inode-exhaust",
                super::CEILING,
                Space::InodeEnospc,
                None,
            )?;
            let blocks = guest.probe(
                active,
                "fill-inode-ballast",
                super::CEILING,
                Space::Enospc,
                None,
            )?;
            Ok((
                target,
                json!({"original_inode":original_info.ino(),"replacement_inode":meta.ino(),"original_sha256":original_hash,"replacement_sha256":backup_hash,"inode_exhaustion":inodes,"full":blocks}),
            ))
        },
    )?;
    ensure!(
        out.stderr
            == "exchange available_blocks=0 available_inodes=0\nunlink available_blocks=0 available_inodes=0\n",
        "missing zero-headroom checks immediately before exchange and unlink"
    );
    let canonical = fs::symlink_metadata(&source)?;
    ensure!(
        canonical.dev() == original_info.dev()
            && proof["replacement_inode"] == canonical.ino()
            && canonical.mode() & 0o7777 == 0o640
            && sha(&source)? == backup_hash
            && sha(&backup)? == backup_hash
            && !target.exists()
            && sha(&shim)? == shim_hash,
        "publication changed exact copy bytes or left the old image"
    );
    proof["namespace_checks"] = json!(out.stderr.lines().collect::<Vec<_>>());
    Ok((out, proof))
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
    fn journal_fault_skips_the_superblock_and_requires_one_bounded_extent() {
        let stat =
            "Inode: 8   Type: regular\nUser: 0 Size: 16777216\nEXTENTS:\n(0-4095):32801-36896\n";
        assert_eq!(journal_sectors(stat, 524288).unwrap(), (262416, 295176));
        for changed in [
            stat.replace("Inode: 8", "Inode: 7"),
            stat.replace("(0-4095)", "(1-4096)"),
            stat.replace("16777216", "16777215"),
            stat.replace("36896", "36897"),
            stat.replace("32801-36896", "32801-34848, (2048-4095):34850-36897"),
        ] {
            assert!(journal_sectors(&changed, 524288).is_err(), "{changed}");
        }
        assert!(journal_sectors(stat, 295176).is_err());
    }

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
