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
pub(super) const O_CLOEXEC: c_int = 0x01000000;
pub(super) const OPEN_REQUIRED: c_int = 0x4 | 0x100 | O_CLOEXEC;
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
    identity_with(|id| unsafe {
        arc4random_buf(id.as_mut_ptr().cast(), id.len());
    })
}

fn identity_with(mut fill: impl FnMut(&mut [u8; 16])) -> Result<[u8; 16]> {
    for _ in 0..3 {
        let mut id = [0; 16];
        fill(&mut id);
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

#[cfg(test)]
mod decision_tests {
    use super::*;
    use crate::Limits;
    use std::{
        fs,
        os::{
            fd::OwnedFd,
            unix::{
                fs::{MetadataExt, PermissionsExt},
                net::UnixStream,
            },
        },
        path::PathBuf,
    };

    struct NativeDirectory(PathBuf);

    impl NativeDirectory {
        fn new(case: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "weekend-macos-primitive-{case}-{}",
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
    fn full_sync_reports_real_success_and_rejection() {
        let directory = NativeDirectory::new("sync");
        let path = directory.0.join("file");
        fs::write(&path, b"synced").unwrap();
        let regular = File::options().write(true).open(&path).unwrap();
        assert_eq!(diagnostics::capture(|| full_sync(&regular)), Ok(()));

        let (socket, _peer) = UnixStream::pair().unwrap();
        let socket = File::from(OwnedFd::from(socket));
        let failure = diagnostics::capture(|| full_sync(&socket)).unwrap_err();
        assert_eq!(failure.error, Error::Io);
        let native = failure.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Sync);
        assert!(native.raw_os_error.is_some());
        assert_eq!(fs::read(path).unwrap(), b"synced");
    }

    #[test]
    fn exchange_swaps_only_the_named_inodes_and_reports_a_missing_name() {
        let directory = NativeDirectory::new("exchange");
        let left = directory.0.join("left");
        let right = directory.0.join("right");
        let sibling = directory.0.join("untouched");
        fs::write(&left, b"old").unwrap();
        fs::write(&sibling, b"keep").unwrap();
        let parent = File::open(&directory.0).unwrap();
        let swap = || exchange(&parent, b"left\0", &parent, b"right\0");
        let old_inode = fs::metadata(&left).unwrap().ino();
        let failure = diagnostics::capture(swap).unwrap_err();
        assert_eq!(failure.error, Error::NotFound);
        let native = failure.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Exchange);
        assert_eq!(native.raw_os_error, Some(2));
        assert_eq!(fs::metadata(&left).unwrap().ino(), old_inode);
        assert!(!right.exists());

        fs::write(&right, b"replacement").unwrap();
        let new_inode = fs::metadata(&right).unwrap().ino();
        assert_eq!(diagnostics::capture(swap), Ok(()));
        assert_eq!(fs::metadata(&left).unwrap().ino(), new_inode);
        assert_eq!(fs::metadata(&right).unwrap().ino(), old_inode);
        assert_eq!(fs::read(left).unwrap(), b"replacement");
        assert_eq!(fs::read(right).unwrap(), b"old");
        assert_eq!(fs::read(sibling).unwrap(), b"keep");
    }

    #[test]
    fn metadata_copy_preserves_data_and_reports_native_rejection() {
        let directory = NativeDirectory::new("metadata");
        let source = directory.0.join("source");
        let target = directory.0.join("target");
        fs::write(&source, b"source contents").unwrap();
        fs::write(&target, b"target contents").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let from = File::open(&source).unwrap();
        let to = File::options().write(true).open(&target).unwrap();
        let memory = Memory::new(Limits::default()).unwrap();
        assert_eq!(
            diagnostics::capture(|| copy_metadata(&from, &to, &memory)),
            Ok(())
        );
        assert_eq!(fs::metadata(&target).unwrap().mode() & 0o777, 0o640);
        assert_eq!(fs::read(&target).unwrap(), b"target contents");

        let (socket, _peer) = UnixStream::pair().unwrap();
        let socket = File::from(OwnedFd::from(socket));
        let failure = diagnostics::capture(|| copy_metadata(&from, &socket, &memory)).unwrap_err();
        assert_eq!(failure.error, Error::Io);
        let native = failure.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::CopyMetadata);
        assert!(native.raw_os_error.is_some());
        assert_eq!(fs::read(source).unwrap(), b"source contents");
    }

    #[test]
    fn identity_accepts_first_or_third_complete_nonzero_attempt() {
        for success in [1, 3] {
            let mut calls = 0;
            let mut expected = [0; 16];
            expected[15] = 7;
            assert_eq!(
                identity_with(|id| {
                    calls += 1;
                    assert!(calls <= success, "continued after success");
                    assert_eq!(*id, [0; 16]);
                    if calls == success {
                        id[15] = 7;
                    }
                }),
                Ok(expected)
            );
            assert_eq!(calls, success);
        }
    }

    #[test]
    fn identity_rejects_exactly_three_complete_zero_attempts() {
        let mut calls = 0;
        let failure = diagnostics::capture(|| {
            identity_with(|id| {
                calls += 1;
                assert_eq!(*id, [0; 16]);
            })
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        assert!(failure.native.is_none());
        assert_eq!(calls, 3);
    }

    #[test]
    fn native_error_mapping_preserves_raw_code_and_operation() {
        for (code, expected) in [
            (2, Error::NotFound),
            (17, Error::AlreadyExists),
            (35, Error::Busy),
            (20, Error::Invalid),
            (21, Error::Invalid),
            (62, Error::Invalid),
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
        let failure = diagnostics::capture::<()>(|| {
            Err(os_error(
                NativeIoOperation::Read,
                io::Error::from(io::ErrorKind::Other),
            ))
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        let native = failure.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Read);
        assert_eq!(native.raw_os_error, None);
    }
}
