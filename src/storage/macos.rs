//! macOS primitives for the shared regular-file protocol.
use super::file::retry;
use super::{NativeIoOperation, diagnostics};
use crate::{Error, Result, memory::Memory};
use std::{
    ffi::{c_char, c_int, c_void},
    fs::File,
    io,
    os::fd::AsRawFd,
};
pub(super) const SUPPORTED: bool = true;
pub(super) const OPEN_DIRECTORY: c_int = 0x100000;
// Verified against the installed macOS SDK sys/fcntl.h and sys/errno.h.
pub(super) const OPEN_REQUIRED: c_int = 0x4 | 0x100 | 0x01000000;
pub(super) const OPEN_LEASE: c_int = 0x20; // O_EXLOCK, atomic with openat
pub(super) const OPEN_CREATE: c_int = 0x200 | 0x800; // O_CREAT | O_EXCL
const F_FULLFSYNC: c_int = 51;
unsafe extern "C" {
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn arc4random_buf(buffer: *mut c_void, len: usize);
    fn renameatx_np(
        from_dir: c_int,
        from: *const c_char,
        to_dir: c_int,
        to: *const c_char,
        flags: u32,
    ) -> c_int;
    fn fcopyfile(from: c_int, to: c_int, state: *mut c_void, flags: u32) -> c_int;
}
pub(super) fn os_error(operation: NativeIoOperation, error: io::Error) -> Error {
    diagnostics::record(operation, &error);
    match error.raw_os_error() {
        Some(2) => Error::NotFound,
        Some(17) => Error::AlreadyExists,
        Some(35) => Error::Busy,
        Some(20 | 21 | 62) => Error::Invalid,
        _ => Error::Io,
    }
}
pub(super) fn full_sync(file: &File) -> Result<()> {
    retry(NativeIoOperation::Sync, || {
        if unsafe { fcntl(file.as_raw_fd(), F_FULLFSYNC) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    })
}
pub(crate) fn identity() -> Result<[u8; 16]> {
    for _ in 0..3 {
        let mut id = [0; 16];
        unsafe {
            arc4random_buf(id.as_mut_ptr().cast(), id.len());
        }
        if id != [0; 16] {
            return Ok(id);
        }
    }
    Err(Error::Io)
}

pub(super) fn acquire_open_lock(_file: &File) -> Result<()> {
    Ok(())
}
pub(super) fn exchange(from_dir: &File, from: &[u8], to_dir: &File, to: &[u8]) -> Result<()> {
    // RENAME_SWAP = 2. Never retry an ambiguous namespace operation.
    if unsafe {
        renameatx_np(
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
        Err(os_error(
            NativeIoOperation::Exchange,
            io::Error::last_os_error(),
        ))
    }
}
pub(super) fn copy_metadata(from: &File, to: &File, _mem: &Memory) -> Result<()> {
    // COPYFILE_METADATA = ACL | STAT | XATTR. No copying/truncating file data.
    retry(NativeIoOperation::CopyMetadata, || {
        if unsafe { fcopyfile(from.as_raw_fd(), to.as_raw_fd(), std::ptr::null_mut(), 7) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    })
}
