//! Route only the next append's first ext4 data block to a kernel error target.
use super::{Calls, Guest, Scratch, cmd, fs, json, path, quota_oracle, sha};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::Read,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
};

const SECTOR: u64 = 512;
const BLOCK: u64 = 4096;
const BLOCK_SECTORS: u64 = BLOCK / SECTOR;

pub(super) fn formatter(tool: &Path, image: &Path) -> Vec<OsString> {
    let mut args = vec![path(tool)];
    args.extend(cmd([
        "-t", "ext4", "-q", "-F", "-O", "^quota", "-b", "4096",
    ]));
    args.push(path(image));
    args
}

fn exact_block(report: &str, logical: u64, capacity: u64) -> Result<u64> {
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
        if start != logical {
            continue;
        }
        let end: u64 = end.strip_suffix(':').context("extent end")?.parse()?;
        let physical: u64 = physical
            .strip_suffix("..")
            .context("physical start")?
            .parse()?;
        let physical_end: u64 = physical_end
            .strip_suffix(':')
            .context("physical end")?
            .parse()?;
        let count: u64 = count.strip_suffix(':').context("extent length")?.parse()?;
        let flags = rest.last().context("missing FIEMAP flags")?;
        ensure!(
            found.is_none()
                && count == BLOCK_SECTORS
                && logical.checked_add(count - 1) == Some(end)
                && physical.checked_add(count - 1) == Some(physical_end)
                && physical_end < capacity - 1
                && physical > 0
                && physical.is_multiple_of(BLOCK_SECTORS)
                && flags.split(',').any(|f| f == "unwritten")
                && flags
                    .split(',')
                    .all(|f| matches!(f, "unwritten" | "last" | "eof")),
            "next append lacks an exact private unwritten block"
        );
        found = Some(physical);
    }
    found.context("FIEMAP omitted the reserved append block")
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
        if output.trim().is_empty() {
            return Ok(None);
        }
        let fields: Vec<_> = output.trim().split(':').map(str::trim).collect();
        let [name, uuid, major, minor] = fields.as_slice() else {
            anyhow::bail!("invalid mapper identity");
        };
        ensure!(
            *name == self.name && *uuid == self.uuid,
            "foreign device mapper identity"
        );
        Ok(Some((major.parse()?, minor.parse()?)))
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
        let database = guest.data.join("database.wdb");
        let mut results = Vec::new();
        let mut phase = |calls: &mut Calls<'_, '_>, stage: &str, revision: u64| -> Result<()> {
            let mut args = vec![stage.into(), path(&database)];
            if stage == "inspect" {
                args.push(revision.to_string().into());
            }
            let reply = guest.fixture(calls, args, 30., &[0])?;
            let row = quota_oracle::one_json(&reply.stdout)?;
            crate::quota_append::verify(&row, source, stage, revision, reply.pid, 5)?;
            ensure!(
                !results.iter().any(|r: &Value| r["pid"] == reply.pid),
                "writeback fixture reused a process"
            );
            results.push(row);
            Ok(())
        };
        phase(calls, "seed", 3)?;
        let seed = fs::read(&database)?;
        let bytes = seed.len() as u64;
        ensure!(
            (5 * BLOCK..=super::MIB).contains(&bytes) && bytes.is_multiple_of(BLOCK),
            "unexpected seed length"
        );
        let seed_artifact = calls.c.output.join("seed.wdb");
        fs::write(&seed_artifact, &seed)?;
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
            fs::read(&database)? == seed,
            "preallocation changed acknowledged database bytes"
        );
        let extents = calls.exec(vec![
            "filefrag".into(),
            "-v".into(),
            "-b512".into(),
            path(&database),
        ])?;
        let physical = exact_block(&extents.stdout, bytes / SECTOR, self.sectors)?;
        self.fault = Some(self.error_table(physical));
        let fault = self.fault.as_deref().unwrap();
        calls.c.evidence["writeback_extent"] = json!({"logical_offset":bytes,"sector":physical,
            "length":BLOCK,"active_table":fault,"linear_table":self.linear(),"dm_name":self.name,"dm_uuid":self.uuid});
        self.switch(calls, scratch, fault, false)?;
        phase(calls, "fail-sync", 3)?;
        self.switch(calls, scratch, &self.linear(), false)?;
        self.unmount(calls, scratch, false)?;
        self.mount(calls, scratch)?;
        phase(calls, "inspect", 3)?;
        let mut prefix = Vec::with_capacity(seed.len());
        fs::File::open(&database)?
            .take(bytes)
            .read_to_end(&mut prefix)?;
        ensure!(prefix == seed, "acknowledged prefix changed after remount");
        let failed = calls.c.output.join("reopened.wdb");
        ensure!(
            fs::metadata(&database)?.len() <= 2 * super::MIB,
            "unbounded fail image"
        );
        fs::copy(&database, &failed)?;
        phase(calls, "retry", 4)?;
        self.unmount(calls, scratch, false)?;
        self.mount(calls, scratch)?;
        phase(calls, "inspect", 4)?;
        let retried = calls.c.output.join("retried.wdb");
        ensure!(
            fs::metadata(&database)?.len() <= 2 * super::MIB,
            "unbounded retry image"
        );
        fs::copy(&database, &retried)?;
        calls.c.evidence["writeback_images"] = json!({"seed":sha(&seed_artifact)?,"reopened":sha(&failed)?,"retried":sha(&retried)?,"prefix_verified":true});
        calls.c.evidence["results"] = json!(results);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_target_must_be_unwritten_exact_and_within_its_owned_device() {
        let row = "  1:  8.. 15:  53448.. 53455:  8:  49360: last,unwritten,eof";
        assert_eq!(exact_block(row, 8, 262144).unwrap(), 53448);
        for bad in [
            row.replace("unwritten", "delalloc"),
            row.replace("unwritten", "unwritten,shared"),
            row.replace("53455", "53454"),
            row.replace("15:", "16:"),
            format!("{row}\n{row}"),
        ] {
            assert!(exact_block(&bad, 8, 262144).is_err(), "{bad}");
        }
        assert!(exact_block(row, 8, 53456).is_err());
        assert!(exact_block(row, 16, 262144).is_err());
    }
}
