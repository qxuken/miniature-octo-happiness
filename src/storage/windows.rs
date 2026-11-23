//! Exclusive, write-through regular files on local NTFS.
use super::ByteStore;
use super::windows_path::{
    append_leaf, canonical_drive, parent_path, read_path, validate_leaf, validate_path,
};
use super::{NativeIoOperation, diagnostics};
use crate::{AllocatorHandle, Error, Result};
use std::{
    ffi::c_void,
    fs::File,
    io,
    os::windows::{
        ffi::OsStrExt,
        fs::{FileExt, MetadataExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path, Prefix},
    ptr,
};

const BACKUP_SEMANTICS: u32 = 0x0200_0000;
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const WRITE_THROUGH: u32 = 0x8000_0000;
const REPARSE_POINT: u32 = 0x400;
const UNSUPPORTED_ATTRIBUTES: u32 = 0x200 | 0x800 | 0x4000; // sparse, compressed, encrypted
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const SHARE_READ_WRITE: u32 = 3;
const SHARE_DELETE: u32 = 4;
const CREATE_NEW: u32 = 1;
const OPEN_EXISTING: u32 = 3;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        disposition: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn GetFullPathNameW(path: *const u16, size: u32, out: *mut u16, leaf: *mut *mut u16) -> u32;
    fn GetFinalPathNameByHandleW(file: *mut c_void, out: *mut u16, size: u32, flags: u32) -> u32;
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn GetFileType(file: *mut c_void) -> u32;
    fn GetVolumeInformationByHandleW(
        file: *mut c_void,
        volume_name: *mut u16,
        volume_name_size: u32,
        serial: *mut u32,
        max_component: *mut u32,
        flags: *mut u32,
        filesystem_name: *mut u16,
        filesystem_name_size: u32,
    ) -> i32;
}

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, size: u32, flags: u32) -> i32;
}

fn os_error(operation: NativeIoOperation, error: io::Error) -> Error {
    diagnostics::record(operation, &error);
    match error.raw_os_error() {
        Some(2 | 3) => Error::NotFound,
        Some(32 | 33) => Error::Busy,
        Some(80 | 183) => Error::AlreadyExists,
        Some(123 | 267) => Error::Invalid,
        _ => Error::Io,
    }
}

pub(crate) fn identity() -> Result<[u8; 16]> {
    identity_with(|id| {
        // SAFETY: the system RNG accepts a null algorithm and a writable buffer.
        unsafe { BCryptGenRandom(ptr::null_mut(), id.as_mut_ptr(), 16, 2) }
    })
}

fn identity_with(mut fill: impl FnMut(&mut [u8; 16]) -> i32) -> Result<[u8; 16]> {
    for _ in 0..3 {
        let mut id = [0; 16];
        let status = fill(&mut id);
        if status != 0 {
            diagnostics::record(
                NativeIoOperation::Random,
                &io::Error::from_raw_os_error(status),
            );
            return Err(Error::Io);
        }
        if id != [0; 16] {
            return Ok(id);
        }
    }
    Err(Error::Io)
}

fn drive(path: &Path) -> Result<Option<u8>> {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => Ok(Some(letter)),
            _ => Err(Error::UnsupportedPlatform),
        },
        _ => Ok(None),
    }
}

fn open_native(
    path: &[u16],
    access: u32,
    share: u32,
    disposition: u32,
    flags: u32,
    operation: NativeIoOperation,
) -> Result<File> {
    // SAFETY: callers pass a NUL-terminated path. Null security and template
    // arguments select a non-inheritable handle and default file attributes.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            share,
            ptr::null(),
            disposition,
            flags,
            ptr::null_mut(),
        )
    };
    if handle == (-1isize) as *mut c_void {
        return Err(os_error(operation, io::Error::last_os_error()));
    }
    // SAFETY: CreateFileW returned a new owned handle.
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn path_error() -> Error {
    os_error(NativeIoOperation::OpenDirectory, io::Error::last_os_error())
}

fn check_volume(parent: &File, drive: u8) -> Result<()> {
    let root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: root is a NUL-terminated drive root. The directory stays open.
    if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 {
        return Err(Error::UnsupportedPlatform); // DRIVE_FIXED
    }
    let mut name = [0u16; 16];
    // SAFETY: name is writable for the specified length. All other outputs
    // are optional and null, and the directory handle is valid for the call.
    if unsafe {
        GetVolumeInformationByHandleW(
            parent.as_raw_handle(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    } == 0
    {
        return Err(os_error(
            NativeIoOperation::Volume,
            io::Error::last_os_error(),
        ));
    }
    if name[..5] != [b'N', b'T', b'F', b'S', 0].map(u16::from) {
        return Err(Error::UnsupportedPlatform);
    }
    Ok(())
}

/// Windows file access holds an exclusive handle from create/open to unlock.
/// Unlock closes the handle. Use `open` to obtain another lease.
pub struct FileStore {
    file: Option<File>,
    claimed: bool,
}

impl FileStore {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_in(path, true, AllocatorHandle::default())
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_in(path, false, AllocatorHandle::default())
    }

    /// Open with read access and the same exclusive lease as a writer.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_impl(path.as_ref(), false, false, AllocatorHandle::default())
    }

    pub(crate) fn open_in(
        path: impl AsRef<Path>,
        create: bool,
        allocator: AllocatorHandle,
    ) -> Result<Self> {
        Self::open_impl(path.as_ref(), create, true, allocator)
    }

    fn open_impl(
        path: &Path,
        create: bool,
        writable: bool,
        allocator: AllocatorHandle,
    ) -> Result<Self> {
        drive(path)?;
        validate_path(path.as_os_str().encode_wide())?;
        let name = path.file_name().ok_or(Error::Invalid)?;
        validate_leaf(name.encode_wide())?;

        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut path = parent_path(
            parent.as_os_str().encode_wide(),
            allocator,
            |input, out| {
                // SAFETY: input ends in NUL and out is writable for its full length.
                unsafe {
                    GetFullPathNameW(
                        input.as_ptr(),
                        out.len() as u32,
                        out.as_mut_ptr(),
                        ptr::null_mut(),
                    )
                }
            },
            path_error,
        )?;
        let lookup = open_native(
            &path,
            0,
            SHARE_READ_WRITE | SHARE_DELETE,
            OPEN_EXISTING,
            BACKUP_SEMANTICS,
            NativeIoOperation::OpenDirectory,
        )?;
        read_path(
            &mut path,
            |out| {
                // SAFETY: lookup is open and out is writable for its full length.
                // Zero selects the normalized path with the DOS drive name.
                unsafe {
                    GetFinalPathNameByHandleW(
                        lookup.as_raw_handle(),
                        out.as_mut_ptr(),
                        out.len() as u32,
                        0,
                    )
                }
            },
            path_error,
        )?;
        drop(lookup);
        let letter = canonical_drive(&path)?;
        let directory = open_native(
            &path,
            GENERIC_READ,
            SHARE_READ_WRITE,
            OPEN_EXISTING,
            BACKUP_SEMANTICS,
            NativeIoOperation::OpenDirectory,
        )?;
        let metadata = directory
            .metadata()
            .map_err(|error| os_error(NativeIoOperation::Metadata, error))?;
        if !metadata.is_dir() {
            return Err(Error::Invalid);
        }
        if metadata.file_attributes() & UNSUPPORTED_ATTRIBUTES != 0 {
            return Err(Error::UnsupportedPlatform);
        }
        check_volume(&directory, letter)?;

        // Canonicalizing the parent gives an extended-length path. The final
        // component is checked separately and opened without following a link.
        append_leaf(&mut path, name.encode_wide())?;
        let file = open_native(
            &path,
            GENERIC_READ | if writable { GENERIC_WRITE } else { 0 },
            0,
            if create { CREATE_NEW } else { OPEN_EXISTING },
            OPEN_REPARSE_POINT | WRITE_THROUGH,
            if create {
                NativeIoOperation::Create
            } else {
                NativeIoOperation::Open
            },
        )?;
        drop(path);
        let metadata = file
            .metadata()
            .map_err(|error| os_error(NativeIoOperation::Metadata, error))?;
        // SAFETY: the handle is open for the duration of GetFileType.
        if unsafe { GetFileType(file.as_raw_handle()) } != 1
            || !metadata.is_file()
            || metadata.file_attributes() & REPARSE_POINT != 0
        {
            return Err(Error::Invalid);
        }
        if metadata.file_attributes() & UNSUPPORTED_ATTRIBUTES != 0 {
            return Err(Error::UnsupportedPlatform);
        }
        Ok(Self {
            file: Some(file),
            claimed: false,
        })
    }

    fn file(&self) -> Result<&File> {
        self.file.as_ref().filter(|_| self.claimed).ok_or(Error::Io)
    }
}

impl ByteStore for FileStore {
    fn lock(&mut self) -> Result<()> {
        if self.claimed {
            return Err(Error::Busy);
        }
        if self.file.is_none() {
            return Err(Error::Closed);
        }
        self.claimed = true;
        Ok(())
    }

    fn unlock(&mut self) {
        self.file = None;
        self.claimed = false;
    }

    fn length(&mut self) -> Result<u64> {
        Ok(self
            .file()?
            .metadata()
            .map_err(|error| os_error(NativeIoOperation::Metadata, error))?
            .len())
    }

    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        self.file()?
            .seek_read(out, offset)
            .map_err(|error| os_error(NativeIoOperation::Read, error))
    }

    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.file()?
            .seek_write(bytes, offset)
            .map_err(|error| os_error(NativeIoOperation::Write, error))
    }

    fn sync(&mut self) -> Result<()> {
        self.file()?
            .sync_all()
            .map_err(|error| os_error(NativeIoOperation::Sync, error))
    }
}

impl FileStore {
    /// Synchronous native context for this exact factory invocation.
    pub fn create_with_diagnostics(
        path: impl AsRef<std::path::Path>,
    ) -> std::result::Result<Self, super::StorageFailure> {
        super::diagnostics::capture(|| Self::create(path))
    }
    pub fn open_with_diagnostics(
        path: impl AsRef<std::path::Path>,
    ) -> std::result::Result<Self, super::StorageFailure> {
        super::diagnostics::capture(|| Self::open(path))
    }
    pub fn open_read_only_with_diagnostics(
        path: impl AsRef<std::path::Path>,
    ) -> std::result::Result<Self, super::StorageFailure> {
        super::diagnostics::capture(|| Self::open_read_only(path))
    }
}

#[cfg(test)]
mod decision_tests {
    use super::*;
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
                    0
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
                0
            })
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        assert!(failure.native.is_none());
        assert_eq!(calls, 3);
    }

    #[test]
    fn identity_rejects_every_nonzero_status_before_accepting_bytes() {
        for status in [0, 1, 0xc000_0001u32 as i32] {
            for preceding_zeros in [0, 2] {
                let mut calls = 0;
                let result = diagnostics::capture(|| {
                    identity_with(|id| {
                        calls += 1;
                        assert_eq!(*id, [0; 16]);
                        if calls <= preceding_zeros {
                            0
                        } else {
                            id[15] = 7;
                            status
                        }
                    })
                });
                assert_eq!(calls, preceding_zeros + 1);
                if status == 0 {
                    let mut expected = [0; 16];
                    expected[15] = 7;
                    assert_eq!(result.unwrap(), expected);
                } else {
                    let failure = result.unwrap_err();
                    assert_eq!(failure.error, Error::Io);
                    let native = failure.native.unwrap();
                    assert_eq!(native.operation, NativeIoOperation::Random);
                    assert_eq!(native.raw_os_error, Some(status));
                }
            }
        }
    }

    #[test]
    fn native_error_mapping_preserves_raw_code_and_operation() {
        for (code, expected) in [
            (2, Error::NotFound),
            (3, Error::NotFound),
            (32, Error::Busy),
            (33, Error::Busy),
            (80, Error::AlreadyExists),
            (183, Error::AlreadyExists),
            (123, Error::Invalid),
            (267, Error::Invalid),
            (5, Error::Io),
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
mod pathname_decisions {
    use super::*;
    #[test]
    fn drive_prefix_and_early_path_guards() {
        assert_eq!(drive(Path::new("relative.db")), Ok(None));
        for path in [r"C:\file.db", r"\\?\C:\file.db"] {
            assert_eq!(drive(Path::new(path)), Ok(Some(b'C')));
        }
        for path in [r"\\server\share\file.db", r"\\.\device"] {
            assert_eq!(drive(Path::new(path)), Err(Error::UnsupportedPlatform));
        }
        for path in [
            "",
            "x ",
            "x.",
            "x/",
            "x\\",
            "x\0y",
            "name:stream",
            "x*y",
            "x?y",
            "x\u{1}y",
        ] {
            assert_eq!(
                FileStore::open(path).err(),
                Some(Error::Invalid),
                "{path:?}"
            );
        }
        assert_eq!(FileStore::open("x".repeat(32767)).err(), Some(Error::Limit));
    }
}
