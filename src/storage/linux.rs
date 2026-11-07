//! Linux primitives for the shared regular-file protocol. Uses fsync, never a
//! weaker fallback, and requires renameat2(RENAME_EXCHANGE) for compaction.
use super::file::retry;
use crate::{Error, Result, memory::Memory};
use std::{
    ffi::{c_char, c_int, c_void},
    fs::{File, Permissions},
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, PermissionsExt},
    },
};

// Keep the in-memory engine buildable on other Linux ABIs; file factories
// reject them before any pathname or random-source operation.
pub(super) const SUPPORTED: bool = cfg!(any(target_arch = "x86_64", target_arch = "aarch64"));
// Verified with each ABI's fcntl.h. arm64 inherits ARM's O_NOFOLLOW value;
// using x86_64's value there would silently follow final-component symlinks.
#[cfg(target_arch = "x86_64")]
const O_NOFOLLOW: c_int = 0x20000;
#[cfg(target_arch = "x86_64")]
pub(super) const OPEN_DIRECTORY: c_int = 0x10000;
#[cfg(target_arch = "aarch64")]
const O_NOFOLLOW: c_int = 0x8000;
#[cfg(target_arch = "aarch64")]
pub(super) const OPEN_DIRECTORY: c_int = 0x4000;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const O_NOFOLLOW: c_int = 0; // unreachable behind SUPPORTED
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub(super) const OPEN_DIRECTORY: c_int = 0;
pub(super) const OPEN_REQUIRED: c_int = 0x800 | O_NOFOLLOW | 0x80000;
pub(super) const OPEN_LEASE: c_int = 0;
pub(super) const OPEN_CREATE: c_int = 0x40 | 0x80; // O_CREAT | O_EXCL
unsafe extern "C" {
    fn flock(fd: c_int, operation: c_int) -> c_int;
    fn getrandom(buffer: *mut c_void, len: usize, flags: u32) -> isize;
    fn renameat2(
        from_dir: c_int,
        from: *const c_char,
        to_dir: c_int,
        to: *const c_char,
        flags: u32,
    ) -> c_int;
    fn fchown(fd: c_int, owner: u32, group: u32) -> c_int;
    fn flistxattr(fd: c_int, list: *mut c_char, size: usize) -> isize;
    fn fgetxattr(fd: c_int, name: *const c_char, value: *mut c_void, size: usize) -> isize;
    fn fsetxattr(
        fd: c_int,
        name: *const c_char,
        value: *const c_void,
        size: usize,
        flags: c_int,
    ) -> c_int;
    fn fremovexattr(fd: c_int, name: *const c_char) -> c_int;
}
pub(super) fn os_error(error: io::Error) -> Error {
    match error.raw_os_error() {
        Some(2) => Error::NotFound,
        Some(17) => Error::AlreadyExists,
        Some(11) => Error::Busy,              // EWOULDBLOCK == EAGAIN
        Some(20 | 21 | 40) => Error::Invalid, // ENOTDIR / EISDIR / ELOOP
        _ => Error::Io,
    }
}
pub(super) fn full_sync(file: &File) -> Result<()> {
    retry(|| file.sync_all())
}
pub(super) fn acquire_open_lock(file: &File) -> Result<()> {
    retry(|| {
        // LOCK_EX | LOCK_NB. The shared protocol rechecks the pathname after
        // this call, while the lease is held, before reading any database bytes.
        if unsafe { flock(file.as_raw_fd(), 2 | 4) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    })
}
pub(crate) fn identity() -> Result<[u8; 16]> {
    if !SUPPORTED {
        return Err(Error::UnsupportedPlatform);
    }
    for _ in 0..3 {
        let mut id = [0; 16];
        let mut offset = 0;
        while offset < id.len() {
            let n = retry(|| {
                // The writable suffix is valid for the requested length. No
                // flags: wait for initialized kernel randomness; no PRNG fallback.
                let n =
                    unsafe { getrandom(id[offset..].as_mut_ptr().cast(), id.len() - offset, 0) };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            })?;
            if n == 0 || n > id.len() - offset {
                return Err(Error::Io);
            }
            offset += n;
        }
        if id != [0; 16] {
            return Ok(id);
        }
    }
    Err(Error::Io)
}
pub(super) fn exchange(from_dir: &File, from: &[u8], to_dir: &File, to: &[u8]) -> Result<()> {
    // RENAME_EXCHANGE = 2. Both NUL-terminated names are held in one directory.
    // Unsupported kernels/filesystems fail; no unlink/rename fallback or retry.
    if unsafe {
        renameat2(
            from_dir.as_raw_fd(),
            from.as_ptr().cast(),
            to_dir.as_raw_fd(),
            to.as_ptr().cast(),
            2,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(os_error(io::Error::last_os_error()))
    }
}
fn status(code: c_int) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn blob(mem: &Memory, mut read: impl FnMut(*mut c_void, usize) -> isize) -> Result<Vec<u8>> {
    let mut call = |buffer, size| {
        retry(|| {
            let n = read(buffer, size);
            if n < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(n as usize)
            }
        })
    };
    let size = call(std::ptr::null_mut(), 0)?;
    // Linux bounds xattr values and lists at 64 KiB. Fail closed if metadata
    // changes between calls or exceeds the supported bound; never truncate it.
    if size > 65536 {
        return Err(Error::Limit);
    }
    let mut bytes = Vec::new();
    mem.reserve(&mut bytes, size)?;
    bytes.resize(size, 0);
    let n = call(bytes.as_mut_ptr().cast(), size)?;
    if n != size {
        return Err(Error::Io);
    }
    Ok(bytes)
}
fn names(file: &File, mem: &Memory) -> Result<Vec<u8>> {
    let bytes = blob(mem, |buffer, size| unsafe {
        flistxattr(file.as_raw_fd(), buffer.cast(), size)
    })?;
    if !bytes.is_empty() && bytes.last() != Some(&0) {
        return Err(Error::Io);
    }
    Ok(bytes)
}
pub(super) fn copy_metadata(from: &File, to: &File, mem: &Memory) -> Result<()> {
    let metadata = retry(|| from.metadata())?;
    let target = retry(|| to.metadata())?;
    if metadata.uid() != target.uid() || metadata.gid() != target.gid() {
        retry(|| status(unsafe { fchown(to.as_raw_fd(), metadata.uid(), metadata.gid()) }))?;
    }
    // chown may clear set-id bits/capabilities. Set ownership, mode, then exact
    // accessible xattrs (including POSIX ACLs) before syncing and re-verifying.
    retry(|| to.set_permissions(Permissions::from_mode(metadata.mode() & 0o7777)))?;
    let source_names = names(from, mem)?;
    let target_names = names(to, mem)?;
    for name in target_names.split_inclusive(|b| *b == 0) {
        if !source_names
            .split_inclusive(|b| *b == 0)
            .any(|other| other == name)
        {
            retry(|| status(unsafe { fremovexattr(to.as_raw_fd(), name.as_ptr().cast()) }))?;
        }
    }
    for name in source_names.split_inclusive(|b| *b == 0) {
        if name.len() <= 1 {
            return Err(Error::Io);
        }
        let value = blob(mem, |buffer, size| unsafe {
            fgetxattr(from.as_raw_fd(), name.as_ptr().cast(), buffer, size)
        })?;
        retry(|| {
            status(unsafe {
                fsetxattr(
                    to.as_raw_fd(),
                    name.as_ptr().cast(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            })
        })?;
    }
    let copied = retry(|| to.metadata())?;
    if copied.uid() != metadata.uid()
        || copied.gid() != metadata.gid()
        || copied.mode() & 0o7777 != metadata.mode() & 0o7777
    {
        return Err(Error::Io);
    }
    Ok(())
}
