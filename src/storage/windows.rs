//! Exclusive, write-through regular files on local NTFS.
use super::ByteStore;
use super::NativeIoOperation;
use super::windows_admission::check_volume;
use super::windows_io::{Lease, os_error, own_open_handle};
use super::windows_open::{FileCalls, FileMetadata, Open, open_leaf};
use super::windows_path::{validate_leaf, validate_path};
use super::windows_rng::identity_with;
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

pub(crate) fn identity() -> Result<[u8; 16]> {
    identity_with(|id| {
        // SAFETY: the system RNG accepts a null algorithm and a writable buffer.
        unsafe { BCryptGenRandom(ptr::null_mut(), id.as_mut_ptr(), 16, 2) }
    })
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

struct NativeFileCalls;

impl FileCalls for NativeFileCalls {
    type Handle = File;

    fn resolve(&mut self, input: &[u16], out: &mut [u16]) -> u32 {
        // SAFETY: input ends in NUL and out is writable for its full length.
        unsafe {
            GetFullPathNameW(
                input.as_ptr(),
                out.len() as u32,
                out.as_mut_ptr(),
                ptr::null_mut(),
            )
        }
    }

    fn canonical(&mut self, file: &File, out: &mut [u16]) -> u32 {
        // SAFETY: file stays open and out is writable for its full length.
        // Zero selects the normalized path with the DOS drive name.
        unsafe {
            GetFinalPathNameByHandleW(file.as_raw_handle(), out.as_mut_ptr(), out.len() as u32, 0)
        }
    }

    fn path_error() -> Error {
        os_error(NativeIoOperation::OpenDirectory, io::Error::last_os_error())
    }

    fn open(&mut self, path: &[u16], options: Open) -> Result<File> {
        // SAFETY: The path is NUL terminated. Null security and template
        // select a non-inheritable handle and default file attributes.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                options.access,
                options.share,
                ptr::null(),
                options.disposition,
                options.flags,
                ptr::null_mut(),
            )
        };
        own_open_handle(
            handle,
            options.operation,
            io::Error::last_os_error,
            |handle| {
                // SAFETY: CreateFileW returned a new owned handle.
                unsafe { File::from_raw_handle(handle) }
            },
        )
    }

    fn metadata(&mut self, file: &File) -> Result<FileMetadata> {
        file.metadata()
            .map_err(|error| os_error(NativeIoOperation::Metadata, error))
            .map(|metadata| FileMetadata {
                directory: metadata.is_dir(),
                regular: metadata.is_file(),
                attributes: metadata.file_attributes(),
            })
    }

    fn volume(&mut self, parent: &File, drive: u8) -> Result<()> {
        check_native_volume(parent, drive)
    }

    fn file_type(&mut self, file: &File) -> u32 {
        // SAFETY: file stays open for GetFileType.
        unsafe { GetFileType(file.as_raw_handle()) }
    }
}

fn check_native_volume(parent: &File, drive: u8) -> Result<()> {
    let root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: root is a NUL-terminated drive root.
    let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
    check_volume(
        drive_type,
        |name| {
            // SAFETY: name is writable for its length and parent stays open.
            // All other output parameters are optional.
            unsafe {
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
            }
        },
        || os_error(NativeIoOperation::Volume, io::Error::last_os_error()),
    )
}

/// Windows file access holds an exclusive handle from create/open to unlock.
/// Unlock closes the handle. Use `open` to obtain another lease.
pub struct FileStore {
    lease: Lease<File>,
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
        open_leaf(
            parent.as_os_str().encode_wide(),
            name.encode_wide(),
            create,
            writable,
            allocator,
            &mut NativeFileCalls,
        )
        .map(|file| Self {
            lease: Lease::Open(file),
        })
    }
}

impl ByteStore for FileStore {
    fn lock(&mut self) -> Result<()> {
        self.lease.claim()
    }

    fn unlock(&mut self) {
        self.lease.release();
    }

    fn length(&mut self) -> Result<u64> {
        self.lease.execute(NativeIoOperation::Metadata, |file| {
            file.metadata().map(|metadata| metadata.len())
        })
    }

    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        self.lease
            .execute(NativeIoOperation::Read, |file| file.seek_read(out, offset))
    }

    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.lease.execute(NativeIoOperation::Write, |file| {
            file.seek_write(bytes, offset)
        })
    }

    fn sync(&mut self) -> Result<()> {
        self.lease.execute(NativeIoOperation::Sync, File::sync_all)
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
            assert_eq!(
                FileStore::open(path).err(),
                Some(Error::UnsupportedPlatform)
            );
        }
        for path in [
            "",
            "C:",
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
