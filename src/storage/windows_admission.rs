//! File and volume admission shared by the Win32 adapter and host tests.
use super::NativeIoOperation;
use crate::{Error, Result};

const REPARSE_POINT: u32 = 0x400;
const UNSUPPORTED_ATTRIBUTES: u32 = 0x200 | 0x800 | 0x4000; // sparse, compressed, encrypted
pub(super) const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const CREATE_NEW: u32 = 1;
pub(super) const OPEN_EXISTING: u32 = 3;

pub(super) fn check_volume(
    drive_type: u32,
    query: impl FnOnce(&mut [u16]) -> i32,
    on_error: impl FnOnce() -> Error,
) -> Result<()> {
    if drive_type != 3 {
        return Err(Error::UnsupportedPlatform); // DRIVE_FIXED
    }
    let mut name = [0u16; 16];
    if query(&mut name) == 0 {
        return Err(on_error());
    }
    if name[..5] != [b'N', b'T', b'F', b'S', 0].map(u16::from) {
        return Err(Error::UnsupportedPlatform);
    }
    Ok(())
}

pub(super) fn check_directory(is_dir: bool, attributes: u32) -> Result<()> {
    if !is_dir {
        return Err(Error::Invalid);
    }
    if attributes & UNSUPPORTED_ATTRIBUTES != 0 {
        return Err(Error::UnsupportedPlatform);
    }
    Ok(())
}

pub(super) fn check_leaf(file_type: u32, is_file: bool, attributes: u32) -> Result<()> {
    if file_type != 1 || !is_file || attributes & REPARSE_POINT != 0 {
        return Err(Error::Invalid);
    }
    if attributes & UNSUPPORTED_ATTRIBUTES != 0 {
        return Err(Error::UnsupportedPlatform);
    }
    Ok(())
}

pub(super) struct LeafOpen {
    pub(super) access: u32,
    pub(super) disposition: u32,
    pub(super) operation: NativeIoOperation,
}

pub(super) fn leaf_open(create: bool, writable: bool) -> LeafOpen {
    let (disposition, operation) = if create {
        (CREATE_NEW, NativeIoOperation::Create)
    } else {
        (OPEN_EXISTING, NativeIoOperation::Open)
    };
    LeafOpen {
        access: GENERIC_READ | if writable { GENERIC_WRITE } else { 0 },
        disposition,
        operation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_requires_fixed_drive_and_successful_exact_ntfs_name() {
        for drive in [0, 1, 2, 4, 5, 6, u32::MAX] {
            assert_eq!(
                check_volume(
                    drive,
                    |_| panic!("queried an unsupported drive"),
                    || panic!("drive rejection consulted the native error"),
                ),
                Err(Error::UnsupportedPlatform)
            );
        }
        for (status, name, expected) in [
            (1, "NTFS\0", Ok(())),
            (1, "NTFSx\0", Err(Error::UnsupportedPlatform)),
            (1, "NTF\0", Err(Error::UnsupportedPlatform)),
            (1, "ReFS\0", Err(Error::UnsupportedPlatform)),
            (1, "ntfs\0", Err(Error::UnsupportedPlatform)),
            (-1, "NTFS\0", Ok(())),
        ] {
            assert_eq!(
                check_volume(
                    3,
                    |out| {
                        assert_eq!(out.len(), 16);
                        for (unit, dest) in name.encode_utf16().zip(out) {
                            *dest = unit;
                        }
                        status
                    },
                    || panic!("successful query consulted the native error"),
                ),
                expected
            );
        }
        assert_eq!(
            check_volume(
                3,
                |out| {
                    out[..5].copy_from_slice(&[b'N', b'T', b'F', b'S', 0].map(u16::from));
                    0
                },
                || Error::Busy
            ),
            Err(Error::Busy)
        );
    }

    #[test]
    fn directory_type_and_attributes_have_ordered_errors() {
        assert_eq!(check_directory(true, 0), Ok(()));
        assert_eq!(check_directory(true, 0x10 | 0x20), Ok(()));
        assert_eq!(check_directory(false, 0), Err(Error::Invalid));
        for bit in [0x200, 0x800, 0x4000] {
            assert_eq!(check_directory(true, bit), Err(Error::UnsupportedPlatform));
            assert_eq!(check_directory(false, bit), Err(Error::Invalid));
        }
    }

    #[test]
    fn leaf_file_type_kind_and_reparse_flag_independently_reject() {
        assert_eq!(check_leaf(1, true, 0), Ok(()));
        assert_eq!(check_leaf(1, true, 0x20), Ok(()));
        assert_eq!(check_leaf(2, true, 0), Err(Error::Invalid));
        assert_eq!(check_leaf(1, false, 0), Err(Error::Invalid));
        assert_eq!(check_leaf(1, true, 0x400), Err(Error::Invalid));
        for bit in [0x200, 0x800, 0x4000] {
            assert_eq!(check_leaf(1, true, bit), Err(Error::UnsupportedPlatform));
            assert_eq!(check_leaf(1, true, bit | 0x400), Err(Error::Invalid));
        }
    }

    #[test]
    fn create_disposition_and_write_access_are_independent() {
        for (create, disposition, operation) in [
            (false, 3, NativeIoOperation::Open),
            (true, 1, NativeIoOperation::Create),
        ] {
            for (writable, access) in [(false, 0x8000_0000), (true, 0xc000_0000)] {
                let open = leaf_open(create, writable);
                assert_eq!(open.access, access);
                assert_eq!(open.disposition, disposition);
                assert_eq!(open.operation, operation);
            }
        }
    }
}
