//! Unprivileged Linux calibrator, launched under setpriv by an owned controller.
use crate::{
    quota_oracle::{self, CEILING, CHUNK, MAX_CALIBRATION_INODES},
    resource_oracle::MIB,
    support::sha,
};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum Mode {
    Identity,
    Fill,
    InodeFill,
    InodeExhaust,
    FillInodeBallast,
    InodeRelease,
    Remove,
    WriteControl,
    Headroom,
    InstallSeed,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Fill => "fill",
            Self::InodeFill => "inode-fill",
            Self::InodeExhaust => "inode-exhaust",
            Self::FillInodeBallast => "fill-inode-ballast",
            Self::InodeRelease => "inode-release",
            Self::Remove => "remove",
            Self::WriteControl => "write-control",
            Self::Headroom => "headroom",
            Self::InstallSeed => "install-seed",
        }
    }
}
#[derive(Args)]
pub struct Options {
    pub mode: Mode,
    pub directory: PathBuf,
    pub uid: u32,
    #[arg(long,default_value_t=CHUNK)]
    pub ceiling: u64,
    #[arg(long)]
    pub seed: Option<PathBuf>,
}
fn identity(status: &str, uid: u32) -> Result<Value> {
    let fields: BTreeMap<_, _> = status.lines().filter_map(|l| l.split_once(':')).collect();
    for key in ["Uid", "Gid"] {
        let values = fields
            .get(key)
            .context("missing credentials")?
            .split_whitespace()
            .map(str::parse)
            .collect::<std::result::Result<Vec<u32>, _>>()?;
        ensure!(values == [uid; 4], "guest UID/GID mismatch");
    }
    ensure!(
        fields.get("Groups").is_some_and(|s| s.trim().is_empty())
            && fields.get("NoNewPrivs").is_some_and(|s| s.trim() == "1"),
        "guest groups/no-new-privs boundary missing"
    );
    for key in ["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"] {
        ensure!(
            u64::from_str_radix(
                fields.get(key).context("missing guest capability")?.trim(),
                16
            )? == 0,
            "guest capabilities not empty"
        );
    }
    Ok(quota_oracle::identity(uid))
}
fn output(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}
fn fill(file: &mut File, ceiling: u64) -> Result<(u64, Option<i32>)> {
    let block = [0x5a; 65536];
    let mut written = 0;
    while written < ceiling {
        let count = (ceiling - written).min(block.len() as u64) as usize;
        match file.write(&block[..count]) {
            Ok(n) => {
                ensure!(n > 0, "zero-progress ballast write");
                written += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Ok((
                    written,
                    Some(e.raw_os_error().context("ballast write errno")?),
                ));
            }
        }
        if let Err(e) = file.sync_all() {
            return Ok((
                written,
                Some(e.raw_os_error().context("ballast sync errno")?),
            ));
        }
    }
    Ok((written, None))
}
fn statvfs(path: &Path) -> Result<libc::statvfs> {
    let c = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: valid NUL-terminated path and writable statvfs output; use only on success.
    ensure!(
        unsafe { libc::statvfs(c.as_ptr(), info.as_mut_ptr()) } == 0,
        "statvfs: {}",
        io::Error::last_os_error()
    );
    Ok(unsafe { info.assume_init() })
}
pub fn space(path: &Path) -> Result<(u64, u64, u64)> {
    let info = statvfs(path)?;
    fn bytes(blocks: u64, unit: u64) -> Result<u64> {
        blocks.checked_mul(unit).context("filesystem size overflow")
    }
    Ok((
        bytes(info.f_blocks, info.f_frsize)?,
        bytes(info.f_bfree, info.f_frsize)?,
        bytes(info.f_bavail, info.f_frsize)?,
    ))
}

fn inode_ballast(data: &Path, uid: u32) -> Result<PathBuf> {
    let dir = data
        .parent()
        .context("missing private mount")?
        .join("inode-ballast");
    let info = fs::symlink_metadata(&dir)?;
    ensure!(
        info.is_dir() && info.uid() == uid && info.dev() == fs::metadata(data)?.dev(),
        "unowned or off-volume inode ballast directory"
    );
    Ok(dir)
}

pub fn run(opts: Options) -> Result<()> {
    ensure!(
        (CHUNK..=CEILING).contains(&opts.ceiling),
        "ballast ceiling out of bounds"
    );
    ensure!(opts.uid != 0, "guest cannot run as root");
    let ids = identity(&fs::read_to_string("/proc/self/status")?, opts.uid)?;
    let data = &opts.directory;
    let meta = fs::symlink_metadata(data)?;
    ensure!(
        meta.is_dir() && meta.uid() == opts.uid,
        "owned private data directory required"
    );
    let ballast = data.join("ballast");
    let mut row = json!({"mode":opts.mode.name(),"pid":std::process::id(),"identity":ids});
    match opts.mode {
        Mode::Identity => (),
        Mode::Fill => {
            let mut file = output(&ballast)?;
            let (written, errno) = fill(&mut file, opts.ceiling)?;
            row["errno"] = json!(errno);
            row["written"] = json!(written);
            row["ceiling"] = json!(opts.ceiling);
        }
        Mode::InodeFill => {
            let mut file = output(&ballast)?;
            let candidate = data.join("calibration-inode");
            let err = match output(&candidate) {
                Err(err) => err,
                Ok(extra) => {
                    drop(extra);
                    fs::remove_file(candidate)?;
                    anyhow::bail!("inode limit failed to reject file creation");
                }
            };
            let bytes = [0x5a; 4096];
            file.write_all(&bytes)?;
            file.sync_all()?;
            ensure!(fs::read(&ballast)? == bytes, "existing inode write changed");
            row["errno"] = json!(err.raw_os_error());
            row["fsynced_bytes"] = json!(bytes.len());
            row["global_free_inodes"] = json!(statvfs(data)?.f_ffree);
        }
        Mode::InodeExhaust => {
            let dir = inode_ballast(data, opts.uid)?;
            let created = statvfs(data)?.f_ffree;
            ensure!(
                (1..=MAX_CALIBRATION_INODES).contains(&created),
                "private filesystem inode inventory is outside the calibration bound"
            );
            let slot = dir.join("slot");
            let mut file = output(&slot)?;
            for n in 1..created {
                output(&dir.join(format!("ballast-{n}")))?;
            }
            let denied = dir.join("calibration-inode");
            let err = match output(&denied) {
                Err(err) => err,
                Ok(extra) => {
                    drop(extra);
                    fs::remove_file(denied)?;
                    anyhow::bail!("global inode limit failed to reject file creation");
                }
            };
            let bytes = [0x5a; 4096];
            file.write_all(&bytes)?;
            file.sync_all()?;
            File::open(&dir)?.sync_all()?;
            ensure!(fs::read(&slot)? == bytes, "existing inode write changed");
            row["created"] = json!(created);
            row["errno"] = json!(err.raw_os_error());
            row["fsynced_bytes"] = json!(bytes.len());
            row["global_free_inodes"] = json!(statvfs(data)?.f_ffree);
        }
        Mode::FillInodeBallast => {
            let dir = inode_ballast(data, opts.uid)?;
            let mut file = OpenOptions::new()
                .append(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(dir.join("slot"))?;
            let info = file.metadata()?;
            ensure!(
                info.is_file()
                    && info.uid() == opts.uid
                    && info.len() == 4096
                    && statvfs(data)?.f_ffree == 0,
                "owned inode ballast and a full inode table required"
            );
            let (written, errno) = fill(&mut file, opts.ceiling)?;
            let (_, _, tail) = space(data)?;
            if tail != 0 {
                // The large file can need extent-tree space even when a small
                // allocation in a separate pre-existing inode still fits.
                ensure!(
                    tail <= 65536 && written + tail <= opts.ceiling,
                    "unexpected ballast headroom"
                );
                let mut tail_file = OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(dir.join("ballast-1"))?;
                let info = tail_file.metadata()?;
                ensure!(
                    info.is_file() && info.uid() == opts.uid && info.len() == 0,
                    "owned empty ballast inode required"
                );
                tail_file.write_all(&[0xa7; 65536][..tail as usize])?;
                tail_file.sync_all()?;
            }
            let create_errno = match output(&dir.join("calibration-inode")) {
                Err(err) => err.raw_os_error(),
                Ok(_) => anyhow::bail!("exclusive create unexpectedly succeeded"),
            };
            row["errno"] = json!(errno);
            row["create_errno"] = json!(create_errno);
            row["written"] = json!(written + tail);
            row["tail_bytes"] = json!(tail);
            row["ceiling"] = json!(opts.ceiling);
            row["global_free_inodes"] = json!(statvfs(data)?.f_ffree);
        }
        Mode::InodeRelease => {
            let dir = inode_ballast(data, opts.uid)?;
            let slot = dir.join("slot");
            let info = fs::symlink_metadata(&slot)?;
            ensure!(
                info.is_file() && info.uid() == opts.uid && info.len() == 4096,
                "owned inode ballast slot required"
            );
            let before = statvfs(data)?.f_ffree;
            ensure!(before == 0, "filesystem gained a free inode before release");
            fs::remove_file(slot)?;
            File::open(&dir)?.sync_all()?;
            row["removed"] = json!(true);
            row["before_free_inodes"] = json!(before);
            row["global_free_inodes"] = json!(statvfs(data)?.f_ffree);
        }
        Mode::Headroom => {
            let file = OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&ballast)?;
            let info = file.metadata()?;
            ensure!(
                info.is_file() && info.uid() == opts.uid && info.len() >= CHUNK,
                "owned ballast too small"
            );
            file.set_len(info.len() - CHUNK)?;
            file.sync_all()?;
            row["released_bytes"] = json!(CHUNK);
        }
        Mode::InstallSeed => {
            let seed = opts.seed.context("seed required")?;
            let mut installed = BTreeMap::new();
            for name in ["source.wdb", "backup.wdb"] {
                let mut source = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(seed.join(name))?;
                let info = source.metadata()?;
                ensure!(
                    info.is_file()
                        && info.uid() == opts.uid
                        && info.len() > 60 * MIB
                        && info.len() <= 64 * MIB,
                    "unowned/bad seed"
                );
                let dest = data.join(name);
                let mut file = output(&dest)?;
                use std::io::Read;
                let count = io::copy(&mut Read::by_ref(&mut source).take(64 * MIB + 1), &mut file)?;
                ensure!(count == info.len(), "seed changed during copy");
                file.sync_all()?;
                drop(file);
                installed.insert(name, json!({"bytes":count,"sha256":sha(&dest)?}));
            }
            row["installed"] = json!(installed);
        }
        Mode::Remove => {
            let info = fs::symlink_metadata(&ballast)?;
            ensure!(
                info.is_file() && info.uid() == opts.uid,
                "ballast ownership/type mismatch"
            );
            fs::remove_file(ballast)?;
            File::open(data)?.sync_all()?;
            row["removed"] = json!(true);
        }
        Mode::WriteControl => {
            let dest = data.join("calibration-control");
            let mut file = output(&dest)?;
            let bytes = [0xa7; 4096];
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            ensure!(fs::read(&dest)? == bytes, "control contents changed");
            fs::remove_file(dest)?;
            row["fsynced_bytes"] = json!(4096);
        }
    }
    let (_, free, available) = space(data)?;
    row["global_free_bytes"] = json!(free);
    row["available_bytes"] = json!(available);
    println!("{row}");
    Ok(())
}
