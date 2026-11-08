//! Exclusive, write-through regular files on local NTFS.
use super::ByteStore;
use crate::{Error, Result};
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io,
    os::windows::{
        ffi::OsStrExt,
        fs::{FileExt, MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, Prefix},
    ptr,
};

const BACKUP_SEMANTICS: u32 = 0x0200_0000;
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const WRITE_THROUGH: u32 = 0x8000_0000;
const REPARSE_POINT: u32 = 0x400;
const UNSUPPORTED_ATTRIBUTES: u32 = 0x200 | 0x800 | 0x4000; // sparse, compressed, encrypted

#[link(name = "kernel32")]
unsafe extern "system" {
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

fn os_error(error: io::Error) -> Error {
    match error.raw_os_error() {
        Some(2 | 3) => Error::NotFound,
        Some(32 | 33) => Error::Busy,
        Some(80 | 183) => Error::AlreadyExists,
        Some(123 | 267) => Error::Invalid,
        _ => Error::Io,
    }
}

pub(crate) fn identity() -> Result<[u8; 16]> {
    for _ in 0..3 {
        let mut id = [0; 16];
        // SAFETY: the system RNG accepts a null algorithm and a writable buffer.
        if unsafe { BCryptGenRandom(ptr::null_mut(), id.as_mut_ptr(), 16, 2) } != 0 {
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
        return Err(os_error(io::Error::last_os_error()));
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
        Self::open_impl(path.as_ref(), true, true)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_impl(path.as_ref(), false, true)
    }

    /// Open with read access and the same exclusive lease as a writer.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_impl(path.as_ref(), false, false)
    }

    fn open_impl(path: &Path, create: bool, writable: bool) -> Result<Self> {
        drive(path)?;
        let units = path.as_os_str().encode_wide();
        if units.clone().count() >= 32_767 {
            return Err(Error::Limit);
        }
        if units.clone().any(|unit| unit == 0)
            || matches!(units.last(), Some(32 | 46 | 47 | 92) | None)
        {
            return Err(Error::Invalid);
        }
        let name = path.file_name().ok_or(Error::Invalid)?;
        if name.encode_wide().any(|unit| {
            u8::try_from(unit).is_ok_and(|byte| byte < 32 || b"\"*:<>?|".contains(&byte))
        }) {
            return Err(Error::Invalid);
        }

        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map_err(os_error)?;
        let letter = drive(&parent)?.ok_or(Error::UnsupportedPlatform)?;
        let directory = OpenOptions::new()
            .read(true)
            .share_mode(3) // FILE_SHARE_READ | FILE_SHARE_WRITE
            .custom_flags(BACKUP_SEMANTICS)
            .open(&parent)
            .map_err(os_error)?;
        let metadata = directory.metadata().map_err(os_error)?;
        if !metadata.is_dir() {
            return Err(Error::Invalid);
        }
        if metadata.file_attributes() & UNSUPPORTED_ATTRIBUTES != 0 {
            return Err(Error::UnsupportedPlatform);
        }
        check_volume(&directory, letter)?;

        // Canonicalizing the parent gives an extended-length path. The final
        // component is checked separately and opened without following a link.
        let file = OpenOptions::new()
            .read(true)
            .write(writable)
            .create_new(create)
            .share_mode(0)
            .custom_flags(OPEN_REPARSE_POINT | WRITE_THROUGH)
            .open(parent.join(name))
            .map_err(os_error)?;
        let metadata = file.metadata().map_err(os_error)?;
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
        Ok(self.file()?.metadata().map_err(os_error)?.len())
    }

    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        self.file()?.seek_read(out, offset).map_err(os_error)
    }

    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.file()?.seek_write(bytes, offset).map_err(os_error)
    }

    fn sync(&mut self) -> Result<()> {
        self.file()?.sync_all().map_err(os_error)
    }
}
