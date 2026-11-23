//! Linux primitives for the shared regular-file protocol. Uses fsync, never a
//! weaker fallback, and requires renameat2(RENAME_EXCHANGE) for compaction.
use super::file::retry;
use super::{NativeIoOperation, diagnostics};
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
pub(super) const O_CLOEXEC: c_int = 0x80000;
pub(super) const OPEN_REQUIRED: c_int = 0x800 | O_NOFOLLOW | O_CLOEXEC;
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
pub(super) fn os_error(operation: NativeIoOperation, error: io::Error) -> Error {
    diagnostics::record(operation, &error);
    match error.raw_os_error() {
        Some(2) => Error::NotFound,
        Some(17) => Error::AlreadyExists,
        Some(11) => Error::Busy,              // EWOULDBLOCK == EAGAIN
        Some(20 | 21 | 40) => Error::Invalid, // ENOTDIR / EISDIR / ELOOP
        _ => Error::Io,
    }
}
pub(super) fn full_sync(file: &File) -> Result<()> {
    retry(NativeIoOperation::Sync, || file.sync_all())
}
pub(super) fn acquire_open_lock(file: &File) -> Result<()> {
    retry(NativeIoOperation::Lock, || {
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
    identity_with(
        |suffix| {
            // The exclusive writable suffix is valid for this exact length.
            // Flags 0 wait for initialized kernel randomness; no PRNG fallback.
            unsafe { getrandom(suffix.as_mut_ptr().cast(), suffix.len(), 0) }
        },
        io::Error::last_os_error,
    )
}
fn identity_with(
    mut read: impl FnMut(&mut [u8]) -> isize,
    mut last_error: impl FnMut() -> io::Error,
) -> Result<[u8; 16]> {
    for _ in 0..3 {
        let mut id = [0; 16];
        let mut offset = 0;
        while offset < id.len() {
            let n = retry(NativeIoOperation::Random, || {
                let n = read(&mut id[offset..]);
                if n < 0 {
                    // Capture errno immediately after the negative result.
                    Err(last_error())
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
        Err(os_error(
            NativeIoOperation::Exchange,
            io::Error::last_os_error(),
        ))
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
        retry(NativeIoOperation::CopyMetadata, || {
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
    mem.reserve_external(&mut bytes, size)?;
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
    let metadata = retry(NativeIoOperation::CopyMetadata, || from.metadata())?;
    let target = retry(NativeIoOperation::CopyMetadata, || to.metadata())?;
    if metadata.uid() != target.uid() || metadata.gid() != target.gid() {
        retry(NativeIoOperation::CopyMetadata, || {
            status(unsafe { fchown(to.as_raw_fd(), metadata.uid(), metadata.gid()) })
        })?;
    }
    // chown may clear set-id bits/capabilities. Set ownership, mode, then exact
    // accessible xattrs (including POSIX ACLs) before syncing and re-verifying.
    retry(NativeIoOperation::CopyMetadata, || {
        to.set_permissions(Permissions::from_mode(metadata.mode() & 0o7777))
    })?;
    let source_names = names(from, mem)?;
    let target_names = names(to, mem)?;
    for name in target_names.split_inclusive(|b| *b == 0) {
        if !source_names
            .split_inclusive(|b| *b == 0)
            .any(|other| other == name)
        {
            retry(NativeIoOperation::CopyMetadata, || {
                status(unsafe { fremovexattr(to.as_raw_fd(), name.as_ptr().cast()) })
            })?;
        }
    }
    for name in source_names.split_inclusive(|b| *b == 0) {
        if name.len() <= 1 {
            return Err(Error::Io);
        }
        let value = blob(mem, |buffer, size| unsafe {
            fgetxattr(from.as_raw_fd(), name.as_ptr().cast(), buffer, size)
        })?;
        retry(NativeIoOperation::CopyMetadata, || {
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
    let copied = retry(NativeIoOperation::CopyMetadata, || to.metadata())?;
    if copied.uid() != metadata.uid()
        || copied.gid() != metadata.gid()
        || copied.mode() & 0o7777 != metadata.mode() & 0o7777
    {
        return Err(Error::Io);
    }
    Ok(())
}

#[cfg(test)]
mod identity_decisions {
    use super::*;
    use crate::storage::NativeIoCategory;
    use std::cell::{Cell, RefCell};

    #[test]
    fn identity_fills_only_the_remaining_suffix_without_errno() {
        let mut calls = 0;
        let id = identity_with(
            |suffix| {
                calls += 1;
                match calls {
                    1 => {
                        assert_eq!(suffix.len(), 16);
                        suffix[..5].fill(0x11);
                        5
                    }
                    2 => {
                        assert_eq!(
                            suffix.len(),
                            11,
                            "short read advances only its filled prefix"
                        );
                        assert!(suffix.iter().all(|byte| *byte == 0));
                        suffix.fill(0x22);
                        11
                    }
                    _ => panic!("completed identity requested another read"),
                }
            },
            || panic!("nonnegative result consulted errno"),
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(&id[..5], &[0x11; 5]);
        assert_eq!(&id[5..], &[0x22; 11]);
    }

    #[test]
    fn identity_retries_eintr_at_the_same_suffix_without_terminal_context() {
        let events = RefCell::new(Vec::new());
        let calls = Cell::new(0);
        let failure = diagnostics::capture::<()>(|| {
            let id = identity_with(
                |suffix| {
                    let call = calls.get();
                    calls.set(call + 1);
                    events.borrow_mut().push(suffix.len());
                    match call {
                        0 => {
                            assert_eq!(suffix.len(), 16);
                            suffix[..5].fill(0x33);
                            5
                        }
                        1 => {
                            assert_eq!(suffix.len(), 11);
                            -1
                        }
                        2 => {
                            assert_eq!(suffix.len(), 11, "EINTR preserves the unfilled suffix");
                            assert!(suffix.iter().all(|byte| *byte == 0));
                            suffix.fill(0x44);
                            11
                        }
                        _ => panic!("interrupted identity exceeded its script"),
                    }
                },
                || {
                    assert_eq!(
                        calls.get(),
                        2,
                        "errno follows the negative read immediately"
                    );
                    events.borrow_mut().push(0);
                    io::Error::from_raw_os_error(4)
                },
            )?;
            assert_eq!(&id[..5], &[0x33; 5]);
            assert_eq!(&id[5..], &[0x44; 11]);
            // A later unrelated error exposes any incorrectly recorded EINTR.
            Err(Error::Invalid)
        })
        .unwrap_err();
        assert_eq!(calls.get(), 3);
        assert_eq!(&*events.borrow(), &[16, 11, 0, 11]);
        assert_eq!(failure.error, Error::Invalid);
        assert_eq!(failure.native, None);
        assert_eq!(failure.verification, None);
    }

    #[test]
    fn identity_terminal_errors_preserve_random_operation_errno_and_mapping() {
        for (progress, code, expected, category) in [
            (false, 13, Error::Io, NativeIoCategory::Permission),
            (true, 11, Error::Busy, NativeIoCategory::Other),
        ] {
            let calls = Cell::new(0);
            let events = RefCell::new(Vec::new());
            let failure = diagnostics::capture(|| {
                identity_with(
                    |suffix| {
                        let call = calls.get();
                        calls.set(call + 1);
                        if progress && call == 0 {
                            assert_eq!(suffix.len(), 16);
                            suffix[..5].fill(7);
                            events.borrow_mut().push("progress");
                            5
                        } else {
                            assert_eq!(call, usize::from(progress));
                            assert_eq!(suffix.len(), if progress { 11 } else { 16 });
                            events.borrow_mut().push("negative");
                            -1
                        }
                    },
                    || {
                        assert_eq!(events.borrow().last().copied(), Some("negative"));
                        events.borrow_mut().push("errno");
                        io::Error::from_raw_os_error(code)
                    },
                )
            })
            .unwrap_err();
            assert_eq!(calls.get(), 1 + usize::from(progress));
            assert_eq!(failure.error, expected, "terminal errno mapping");
            let native = failure
                .native
                .expect("terminal random error has native context");
            assert_eq!(
                native.operation,
                NativeIoOperation::Random,
                "random operation attribution"
            );
            assert_eq!(native.raw_os_error, Some(code));
            assert_eq!(native.category, category);
            assert_eq!(events.borrow().last().copied(), Some("errno"));
        }
    }

    #[test]
    fn identity_rejects_zero_and_oversized_progress_without_errno() {
        for (progress, answer) in [(false, 0), (false, 17), (true, 0), (true, 12)] {
            let mut calls = 0;
            let failure = diagnostics::capture(|| {
                identity_with(
                    |suffix| {
                        calls += 1;
                        assert!(
                            calls <= 1 + usize::from(progress),
                            "invalid progress did not terminate"
                        );
                        if progress && calls == 1 {
                            assert_eq!(suffix.len(), 16);
                            suffix[..5].fill(9);
                            5
                        } else {
                            assert_eq!(suffix.len(), if progress { 11 } else { 16 });
                            // Report impossible progress without unsafe or out-of-slice writes.
                            answer
                        }
                    },
                    || panic!("invalid nonnegative progress consulted errno"),
                )
            })
            .unwrap_err();
            assert_eq!(calls, 1 + usize::from(progress));
            assert_eq!(failure.error, Error::Io);
            assert_eq!(failure.native, None);
        }
    }

    #[test]
    fn identity_accepts_first_or_third_final_byte_and_rejects_exactly_three_zeros() {
        for success_at in [0, 2] {
            let mut calls = 0;
            let id = identity_with(
                |suffix| {
                    assert_eq!(suffix.len(), 16);
                    assert!(calls <= success_at);
                    suffix.fill(0);
                    if calls == success_at {
                        suffix[15] = 0x7d;
                    }
                    calls += 1;
                    16
                },
                || panic!("complete nonnegative read consulted errno"),
            )
            .expect("first/third final-byte-only identity succeeds");
            let mut expected = [0; 16];
            expected[15] = 0x7d;
            assert_eq!(id, expected);
            assert_eq!(calls, success_at + 1);
        }
        let mut calls = 0;
        let failure = diagnostics::capture(|| {
            identity_with(
                |suffix| {
                    calls += 1;
                    assert!(
                        calls <= 3,
                        "only three complete zero identities are attempted"
                    );
                    assert_eq!(suffix.len(), 16);
                    suffix.fill(0);
                    16
                },
                || panic!("zero identity consulted errno"),
            )
        })
        .unwrap_err();
        assert_eq!(calls, 3);
        assert_eq!(failure.error, Error::Io);
        assert_eq!(failure.native, None);
    }
}

#[cfg(test)]
mod decision_tests {
    use super::*;
    #[test]
    fn native_error_mapping_preserves_raw_code_and_operation() {
        for (code, expected) in [
            (2, Error::NotFound),
            (17, Error::AlreadyExists),
            (11, Error::Busy),
            (20, Error::Invalid),
            (21, Error::Invalid),
            (40, Error::Invalid),
            (13, Error::Io),
        ] {
            let failure = diagnostics::capture::<()>(|| {
                Err(os_error(
                    NativeIoOperation::Open,
                    io::Error::from_raw_os_error(code),
                ))
            })
            .unwrap_err();
            assert_eq!(failure.error, expected);
            let native = failure.native.unwrap();
            assert_eq!(native.raw_os_error, Some(code));
            assert_eq!(native.operation, NativeIoOperation::Open);
        }
        assert_eq!(
            os_error(
                NativeIoOperation::Read,
                io::Error::from(io::ErrorKind::Other)
            ),
            Error::Io
        );
    }
}

#[cfg(test)]
mod blob_decisions {
    use super::*;
    #[test]
    fn metadata_blob_bounds_exact_fill_and_short_callbacks() {
        for size in [0, 3, 65536] {
            let mem = Memory::new(crate::Limits::default()).unwrap();
            let mut calls = 0;
            let bytes = blob(&mem, |buffer, length| {
                calls += 1;
                if calls == 1 {
                    assert!(buffer.is_null());
                    assert_eq!(length, 0);
                } else {
                    assert_eq!(length, size);
                    if size != 0 {
                        unsafe {
                            std::ptr::write_bytes(buffer.cast::<u8>(), 7, length);
                        }
                    }
                }
                size as isize
            })
            .unwrap();
            assert_eq!(calls, 2);
            assert_eq!(bytes, vec![7; size]);
        }
        let mem = Memory::new(crate::Limits::default()).unwrap();
        mem.fail_after(Some(0));
        let mut calls = 0;
        assert_eq!(
            blob(&mem, |_, _| {
                calls += 1;
                65537
            }),
            Err(Error::Limit)
        );
        assert_eq!(calls, 1);
        assert_eq!(blob(&mem, |_, _| 65536), Err(Error::OutOfMemory));
        mem.fail_after(None);
        for result in [2, 4] {
            let mut calls = 0;
            assert_eq!(
                blob(&mem, |_, _| {
                    calls += 1;
                    if calls == 1 { 3 } else { result }
                }),
                Err(Error::Io)
            );
            assert_eq!(calls, 2);
        }
    }

    #[test]
    fn metadata_blob_retries_interruption_and_reports_terminal_callback_error() {
        unsafe extern "C" {
            fn __errno_location() -> *mut c_int;
        }
        let mem = Memory::new(crate::Limits::default()).unwrap();
        for fail_at in [0, 1] {
            let mut calls = 0;
            let failure = diagnostics::capture(|| {
                blob(&mem, |_, _| {
                    let at = calls;
                    calls += 1;
                    if at == fail_at {
                        unsafe {
                            *__errno_location() = 13;
                        }
                        -1
                    } else {
                        3
                    }
                })
            })
            .unwrap_err();
            assert_eq!(failure.error, Error::Io);
            assert_eq!(failure.native.unwrap().raw_os_error, Some(13));
            assert_eq!(calls, fail_at + 1);
        }
        let mut calls = 0;
        assert_eq!(
            blob(&mem, |_, _| {
                calls += 1;
                if calls == 1 {
                    unsafe {
                        *__errno_location() = 4;
                    }
                    -1
                } else {
                    0
                }
            }),
            Ok(vec![])
        );
        assert_eq!(calls, 3);
        assert!(status(0).is_ok());
        unsafe {
            *__errno_location() = 13;
        }
        assert_eq!(status(-1).unwrap_err().raw_os_error(), Some(13));
    }
}
