//! Linux primitives for the shared regular-file protocol. Uses fsync, never a
//! weaker fallback, and requires renameat2(RENAME_EXCHANGE) for compaction.
use super::file::retry;
use super::{NativeIoOperation, diagnostics};
use crate::{AllocatorHandle, Error, Result, memory::Memory};
use std::{
    ffi::{CStr, c_char, c_int, c_void},
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
fn blob(
    mem: &Memory,
    mut read: impl FnMut(*mut c_void, usize) -> isize,
) -> Result<Vec<u8, AllocatorHandle>> {
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
    let mut bytes = Vec::new_in(mem.allocator());
    mem.reserve_external(&mut bytes, size)?;
    bytes.resize(size, 0);
    let n = call(bytes.as_mut_ptr().cast(), size)?;
    if n != size {
        return Err(Error::Io);
    }
    Ok(bytes)
}
struct XattrNames(Vec<u8, AllocatorHandle>);

impl XattrNames {
    fn new(bytes: Vec<u8, AllocatorHandle>) -> Result<Self> {
        if !bytes.is_empty() && bytes.last() != Some(&0) {
            return Err(Error::Io);
        }
        if bytes
            .split_inclusive(|b| *b == 0)
            .any(|name| name.len() == 1)
        {
            return Err(Error::Io);
        }
        Ok(Self(bytes))
    }

    fn iter(&self) -> impl Iterator<Item = &CStr> {
        self.0.split_inclusive(|b| *b == 0).map(|name| {
            // Each segment ends at the first zero. Construction checks the
            // final terminator and rejects empty names before any native call.
            unsafe { CStr::from_bytes_with_nul_unchecked(name) }
        })
    }
}

fn names(file: &File, mem: &Memory) -> Result<XattrNames> {
    let bytes = blob(mem, |buffer, size| unsafe {
        flistxattr(file.as_raw_fd(), buffer.cast(), size)
    })?;
    XattrNames::new(bytes)
}

#[derive(Clone, Copy)]
struct FileAttributes {
    uid: u32,
    gid: u32,
    mode: u32,
}

fn read_attributes(file: &File) -> io::Result<FileAttributes> {
    file.metadata().map(|metadata| FileAttributes {
        uid: metadata.uid(),
        gid: metadata.gid(),
        mode: metadata.mode() & 0o7777,
    })
}

fn change_owner(file: &File, attributes: FileAttributes) -> Result<()> {
    retry(NativeIoOperation::CopyMetadata, || {
        status(unsafe { fchown(file.as_raw_fd(), attributes.uid, attributes.gid) })
    })
}

pub(super) fn copy_metadata(from: &File, to: &File, mem: &Memory) -> Result<()> {
    copy_metadata_with(from, to, mem, read_attributes, change_owner)
}

fn copy_metadata_with(
    from: &File,
    to: &File,
    mem: &Memory,
    mut read: impl FnMut(&File) -> io::Result<FileAttributes>,
    set_owner: impl FnOnce(&File, FileAttributes) -> Result<()>,
) -> Result<()> {
    let metadata = retry(NativeIoOperation::CopyMetadata, || read(from))?;
    let target = retry(NativeIoOperation::CopyMetadata, || read(to))?;
    if metadata.uid != target.uid || metadata.gid != target.gid {
        set_owner(to, metadata)?;
    }
    // chown may clear set-id bits/capabilities. Set ownership, mode, then exact
    // accessible xattrs (including POSIX ACLs) before syncing and re-verifying.
    retry(NativeIoOperation::CopyMetadata, || {
        to.set_permissions(Permissions::from_mode(metadata.mode))
    })?;
    let source_names = names(from, mem)?;
    let target_names = names(to, mem)?;
    copy_xattrs(from, to, mem, &source_names, &target_names)?;
    let copied = retry(NativeIoOperation::CopyMetadata, || read(to))?;
    if copied.uid != metadata.uid || copied.gid != metadata.gid || copied.mode != metadata.mode {
        return Err(Error::Io);
    }
    Ok(())
}

fn copy_xattrs(
    from: &File,
    to: &File,
    mem: &Memory,
    source_names: &XattrNames,
    target_names: &XattrNames,
) -> Result<()> {
    for name in target_names.iter() {
        if !source_names.iter().any(|other| other == name) {
            retry(NativeIoOperation::CopyMetadata, || {
                status(unsafe { fremovexattr(to.as_raw_fd(), name.as_ptr()) })
            })?;
        }
    }
    for name in source_names.iter() {
        let value = blob(mem, |buffer, size| unsafe {
            fgetxattr(from.as_raw_fd(), name.as_ptr(), buffer, size)
        })?;
        retry(NativeIoOperation::CopyMetadata, || {
            status(unsafe {
                fsetxattr(
                    to.as_raw_fd(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            })
        })?;
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
                        suffix[..10].fill(0x22);
                        10
                    }
                    3 => {
                        assert_eq!(suffix, &[0]);
                        suffix[0] = 0x33;
                        1
                    }
                    _ => panic!("completed identity requested another read"),
                }
            },
            || panic!("nonnegative result consulted errno"),
        )
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(&id[..5], &[0x11; 5]);
        assert_eq!(&id[5..15], &[0x22; 10]);
        assert_eq!(id[15], 0x33);
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
    use crate::{Limits, memory::test_allocator::TestAllocator};
    use std::{
        fs,
        os::{fd::OwnedFd, unix::net::UnixStream},
        path::PathBuf,
        sync::Arc,
    };

    struct NativeDirectory(PathBuf);

    impl NativeDirectory {
        fn new(case: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "weekend-linux-primitive-{case}-{}",
                std::process::id()
            ));
            fs::create_dir(&directory).unwrap();
            Self(directory)
        }
    }

    impl Drop for NativeDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn attribute_lists_require_a_final_zero_and_nonempty_names() {
        for (input, valid) in [
            (&b""[..], true),
            (&b"user.a\0user.b\0"[..], true),
            (&b"user.a"[..], false),
            (&b"\0user.a\0"[..], false),
            (&b"user.a\0\0user.b\0"[..], false),
        ] {
            let mut bytes = Vec::new_in(AllocatorHandle::default());
            bytes.extend_from_slice(input);
            let result = XattrNames::new(bytes);
            assert_eq!(result.is_ok(), valid);
            if let Ok(names) = result {
                let original: Vec<_> = names
                    .iter()
                    .flat_map(|name| name.to_bytes_with_nul().iter().copied())
                    .collect();
                assert_eq!(original, input);
            } else {
                assert_eq!(result.err(), Some(Error::Io));
            }
        }
    }

    #[test]
    fn metadata_reads_preserve_first_failure_and_its_operation() {
        let directory = NativeDirectory::new("read-errors");
        let source = directory.0.join("source");
        fs::write(&source, b"source").unwrap();
        fs::set_permissions(&source, Permissions::from_mode(0o640)).unwrap();
        let from = File::open(&source).unwrap();
        let memory = Memory::new(Limits::default()).unwrap();
        for fail_at in 0..3 {
            let target = directory.0.join(format!("target-{fail_at}"));
            fs::write(&target, b"candidate").unwrap();
            fs::set_permissions(&target, Permissions::from_mode(0o600)).unwrap();
            let to = File::open(&target).unwrap();
            let mut reads = 0;
            let failure = diagnostics::capture(|| {
                copy_metadata_with(
                    &from,
                    &to,
                    &memory,
                    |file| {
                        let index = reads;
                        reads += 1;
                        assert_eq!(
                            file.as_raw_fd(),
                            if index == 0 {
                                from.as_raw_fd()
                            } else {
                                to.as_raw_fd()
                            }
                        );
                        if index == fail_at {
                            Err(io::Error::from_raw_os_error(5))
                        } else {
                            read_attributes(file)
                        }
                    },
                    |_, _| panic!("ownership differs in the same-owner fixture"),
                )
            })
            .unwrap_err();
            assert_eq!(reads, fail_at + 1);
            assert_eq!(failure.error, Error::Io);
            let native = failure.native.unwrap();
            assert_eq!(native.operation, NativeIoOperation::CopyMetadata);
            assert_eq!(native.raw_os_error, Some(5));
            assert_eq!(
                fs::metadata(&target).unwrap().mode() & 0o7777,
                if fail_at == 2 { 0o640 } else { 0o600 }
            );
            assert_eq!(fs::read(target).unwrap(), b"candidate");
        }
    }

    #[test]
    fn ownership_change_and_copied_attributes_have_independent_controls() {
        let directory = NativeDirectory::new("ownership");
        let source = directory.0.join("source");
        let target = directory.0.join("target");
        fs::write(&source, b"source").unwrap();
        fs::write(&target, b"candidate").unwrap();
        let from = File::open(&source).unwrap();
        let to = File::open(&target).unwrap();
        let expected = read_attributes(&from).unwrap();
        let memory = Memory::new(Limits::default()).unwrap();

        for (uid, gid) in [(false, false), (true, false), (false, true)] {
            let mut reads = 0;
            let mut changes = 0;
            let result = copy_metadata_with(
                &from,
                &to,
                &memory,
                |_| {
                    let mut reported = expected;
                    if reads == 1 {
                        reported.uid ^= u32::from(uid);
                        reported.gid ^= u32::from(gid);
                    }
                    reads += 1;
                    Ok(reported)
                },
                |file, requested| {
                    changes += 1;
                    assert_eq!(file.as_raw_fd(), to.as_raw_fd());
                    assert_eq!((requested.uid, requested.gid), (expected.uid, expected.gid));
                    Err(Error::Busy)
                },
            );
            assert_eq!(result, if uid || gid { Err(Error::Busy) } else { Ok(()) });
            assert_eq!(changes, usize::from(uid || gid));
            assert_eq!(reads, if uid || gid { 2 } else { 3 });
        }

        for (uid, gid, mode) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let mut reads = 0;
            let result = copy_metadata_with(
                &from,
                &to,
                &memory,
                |_| {
                    let mut reported = expected;
                    if reads == 2 {
                        reported.uid ^= u32::from(uid);
                        reported.gid ^= u32::from(gid);
                        reported.mode ^= u32::from(mode);
                    }
                    reads += 1;
                    Ok(reported)
                },
                |_, _| panic!("the fixture starts with matching owners"),
            );
            assert_eq!(reads, 3);
            assert_eq!(
                result,
                if uid || gid || mode {
                    Err(Error::Io)
                } else {
                    Ok(())
                }
            );
            assert_eq!(fs::read(&target).unwrap(), b"candidate");
        }
    }

    #[test]
    fn metadata_mutation_syscalls_report_rejection_of_a_path_only_descriptor() {
        use std::os::unix::fs::OpenOptionsExt;

        let directory = NativeDirectory::new("mutation-errors");
        let source = directory.0.join("source");
        let target = directory.0.join("target");
        fs::write(&source, b"source").unwrap();
        fs::write(&target, b"candidate").unwrap();
        let from = File::open(&source).unwrap();
        let to = File::options()
            .read(true)
            .custom_flags(0x200000)
            .open(&target)
            .unwrap(); // O_PATH
        let memory = Memory::new(Limits::default()).unwrap();
        let validate = |result: std::result::Result<(), super::super::StorageFailure>| {
            let failure = result.unwrap_err();
            assert_eq!(failure.error, Error::Io);
            let native = failure.native.unwrap();
            assert_eq!(native.operation, NativeIoOperation::CopyMetadata);
            assert_eq!(native.raw_os_error, Some(9));
        };
        let owner = read_attributes(&from).unwrap();
        assert_eq!(diagnostics::capture(|| change_owner(&from, owner)), Ok(()));
        validate(diagnostics::capture(|| change_owner(&to, owner)));
        validate(diagnostics::capture(|| copy_metadata(&from, &to, &memory)));

        let name = c"user.weekend-copy";
        let value = b"wanted";
        // The real source file is owned and writable by the test process.
        assert_eq!(
            unsafe {
                fsetxattr(
                    from.as_raw_fd(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            },
            0
        );
        let present = names(&from, &memory).unwrap();
        let empty = XattrNames::new(Vec::new_in(memory.allocator())).unwrap();
        validate(diagnostics::capture(|| {
            copy_xattrs(&from, &to, &memory, &empty, &present)
        }));
        validate(diagnostics::capture(|| {
            copy_xattrs(&from, &to, &memory, &present, &empty)
        }));
        assert_eq!(fs::read(target).unwrap(), b"candidate");
    }

    #[test]
    fn native_flock_and_sync_report_kernel_errors() {
        let directory = NativeDirectory::new("flock");
        let path = directory.0.join("file");
        fs::write(&path, b"kept").unwrap();
        let first = File::options().write(true).open(&path).unwrap();
        let second = File::open(&path).unwrap();
        assert_eq!(diagnostics::capture(|| acquire_open_lock(&first)), Ok(()));
        let failure = diagnostics::capture(|| acquire_open_lock(&second)).unwrap_err();
        assert_eq!(failure.error, Error::Busy);
        let native = failure.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Lock);
        assert_eq!(native.raw_os_error, Some(11));
        assert_eq!(diagnostics::capture(|| full_sync(&first)), Ok(()));
        drop(first);
        assert_eq!(diagnostics::capture(|| acquire_open_lock(&second)), Ok(()));

        let (socket, _peer) = UnixStream::pair().unwrap();
        let socket = File::from(OwnedFd::from(socket));
        let failure = diagnostics::capture(|| full_sync(&socket)).unwrap_err();
        assert_eq!(failure.error, Error::Io);
        let native = failure.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Sync);
        assert_eq!(native.raw_os_error, Some(22));
        assert_eq!(fs::read(path).unwrap(), b"kept");
    }

    #[test]
    fn native_exchange_preserves_the_original_when_the_partner_is_missing() {
        let directory = NativeDirectory::new("exchange");
        let old = directory.0.join("old");
        let new = directory.0.join("new");
        let parent = File::open(&directory.0).unwrap();
        fs::write(&old, b"original").unwrap();
        let old_inode = fs::metadata(&old).unwrap().ino();
        let swap = || exchange(&parent, b"old\0", &parent, b"new\0");
        let failure = diagnostics::capture(swap).unwrap_err();
        assert_eq!(failure.error, Error::NotFound);
        let native = failure.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Exchange);
        assert_eq!(native.raw_os_error, Some(2));
        assert_eq!(fs::metadata(&old).unwrap().ino(), old_inode);
        assert!(!new.exists());

        fs::write(&new, b"replacement").unwrap();
        let new_inode = fs::metadata(&new).unwrap().ino();
        assert_eq!(diagnostics::capture(swap), Ok(()));
        assert_eq!(fs::metadata(&new).unwrap().ino(), old_inode);
        assert_eq!(fs::metadata(&old).unwrap().ino(), new_inode);
        assert_eq!(fs::read(old).unwrap(), b"replacement");
        assert_eq!(fs::read(new).unwrap(), b"original");
    }

    #[test]
    fn metadata_copy_retries_selected_allocations_and_replaces_only_metadata() {
        let directory = NativeDirectory::new("attributes");
        let source = directory.0.join("source");
        fs::write(&source, b"source bytes").unwrap();
        fs::set_permissions(&source, Permissions::from_mode(0o640)).unwrap();
        let from = File::open(&source).unwrap();
        let shared = c"user.weekend-shared";
        let stale = c"user.weekend-stale";
        let set = |file: &File, name: &std::ffi::CStr, value: &[u8]| {
            // The held descriptor, terminated name and input bytes outlive the call.
            assert_eq!(
                unsafe {
                    fsetxattr(
                        file.as_raw_fd(),
                        name.as_ptr(),
                        value.as_ptr().cast(),
                        value.len(),
                        0,
                    )
                },
                0
            );
        };
        set(&from, shared, b"source value");
        let allocator = Arc::new(TestAllocator::default());
        let memory = allocator.memory(Limits::default());
        let baseline = allocator.live_bytes();

        for allowed in 0..3 {
            let target = directory.0.join(format!("target-{allowed}"));
            fs::write(&target, b"target bytes").unwrap();
            fs::set_permissions(&target, Permissions::from_mode(0o600)).unwrap();
            let to = File::options().write(true).open(&target).unwrap();
            set(&to, shared, b"previous value");
            set(&to, stale, b"remove");
            let (result, calls) = allocator.count_with_refusal(allowed, || {
                diagnostics::capture(|| copy_metadata(&from, &to, &memory))
            });
            let failure = result.unwrap_err();
            assert_eq!(failure.error, Error::OutOfMemory);
            assert_eq!(failure.native, None);
            assert_eq!(calls, allowed + 1);
            assert_eq!(allocator.live_bytes(), baseline);
            assert_eq!(fs::read(&source).unwrap(), b"source bytes");
            assert_eq!(fs::read(&target).unwrap(), b"target bytes");

            assert_eq!(
                diagnostics::capture(|| copy_metadata(&from, &to, &memory)),
                Ok(())
            );
            assert_eq!(allocator.live_bytes(), baseline);
            assert_eq!(fs::metadata(&target).unwrap().mode() & 0o7777, 0o640);
            let mut bytes = [0; 32];
            // Read the kernel attributes directly as an oracle for the copy.
            let length = unsafe {
                fgetxattr(
                    to.as_raw_fd(),
                    shared.as_ptr(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                )
            };
            assert_eq!(length, b"source value".len() as isize);
            assert_eq!(&bytes[..b"source value".len()], b"source value");
            assert_eq!(
                unsafe { fgetxattr(to.as_raw_fd(), stale.as_ptr(), std::ptr::null_mut(), 0) },
                -1
            );
            assert_eq!(io::Error::last_os_error().raw_os_error(), Some(61));
            assert_eq!(fs::read(target).unwrap(), b"target bytes");
        }
    }

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
    use crate::{Limits, memory::test_allocator::TestAllocator};
    use std::sync::Arc;
    #[test]
    fn metadata_blob_bounds_exact_fill_and_short_callbacks() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits {
            max_live_bytes: 0,
            ..Limits::default()
        });
        let control_bytes = allocator.live_bytes();
        for size in [0, 3, 65536] {
            let requests = allocator.requests();
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
            assert_eq!(allocator.requests() - requests, usize::from(size != 0));
            assert_eq!(allocator.live_bytes(), control_bytes + size);
            assert_eq!(mem.usage(), (0, 0));
            drop(bytes);
            assert_eq!(allocator.live_bytes(), control_bytes);
        }
        let mut calls = 0;
        assert_eq!(
            allocator.with_refusal(0, || blob(&mem, |_, _| {
                calls += 1;
                65537
            })),
            Err(Error::Limit)
        );
        assert_eq!(calls, 1);
        calls = 0;
        assert_eq!(
            allocator.with_refusal(0, || blob(&mem, |_, _| {
                calls += 1;
                65536
            })),
            Err(Error::OutOfMemory)
        );
        assert_eq!(calls, 1);
        assert_eq!(allocator.last_request_size(), 65536);
        assert_eq!(allocator.live_bytes(), control_bytes);
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
            })
            .map(|blob| blob.len()),
            Ok(0)
        );
        assert_eq!(calls, 3);
        assert!(status(0).is_ok());
        unsafe {
            *__errno_location() = 13;
        }
        assert_eq!(status(-1).unwrap_err().raw_os_error(), Some(13));
    }
}
