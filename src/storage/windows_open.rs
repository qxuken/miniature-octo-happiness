//! Order the Windows parent checks and exclusive leaf open.
use super::NativeIoOperation;
use super::windows_admission::{
    GENERIC_READ, OPEN_EXISTING, check_directory, check_leaf, leaf_open,
};
use super::windows_path::{append_leaf, canonical_drive, parent_path, read_path};
use crate::{AllocatorHandle, Error, Result};

const BACKUP_SEMANTICS: u32 = 0x0200_0000;
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const WRITE_THROUGH: u32 = 0x8000_0000;
const SHARE_READ_WRITE: u32 = 3;
const SHARE_DELETE: u32 = 4;

pub(super) struct Open {
    pub(super) access: u32,
    pub(super) share: u32,
    pub(super) disposition: u32,
    pub(super) flags: u32,
    pub(super) operation: NativeIoOperation,
}

pub(super) struct FileMetadata {
    pub(super) directory: bool,
    pub(super) regular: bool,
    pub(super) attributes: u32,
}

pub(super) trait FileCalls {
    type Handle;

    fn resolve(&mut self, input: &[u16], out: &mut [u16]) -> u32;
    fn canonical(&mut self, file: &Self::Handle, out: &mut [u16]) -> u32;
    fn path_error() -> Error;
    fn open(&mut self, path: &[u16], options: Open) -> Result<Self::Handle>;
    fn metadata(&mut self, file: &Self::Handle) -> Result<FileMetadata>;
    fn volume(&mut self, parent: &Self::Handle, drive: u8) -> Result<()>;
    fn file_type(&mut self, file: &Self::Handle) -> u32;
}

pub(super) fn open_leaf<C: FileCalls>(
    parent: impl Iterator<Item = u16> + Clone,
    name: impl Iterator<Item = u16> + Clone,
    create: bool,
    writable: bool,
    allocator: AllocatorHandle,
    calls: &mut C,
) -> Result<C::Handle> {
    let mut path = parent_path(
        parent,
        allocator,
        |input, out| calls.resolve(input, out),
        C::path_error,
    )?;
    let lookup = calls.open(
        &path,
        Open {
            access: 0,
            share: SHARE_READ_WRITE | SHARE_DELETE,
            disposition: OPEN_EXISTING,
            flags: BACKUP_SEMANTICS,
            operation: NativeIoOperation::OpenDirectory,
        },
    )?;
    read_path(
        &mut path,
        |out| calls.canonical(&lookup, out),
        C::path_error,
    )?;
    drop(lookup);
    let letter = canonical_drive(&path)?;
    let directory = calls.open(
        &path,
        Open {
            access: GENERIC_READ,
            share: SHARE_READ_WRITE,
            disposition: OPEN_EXISTING,
            flags: BACKUP_SEMANTICS,
            operation: NativeIoOperation::OpenDirectory,
        },
    )?;
    let metadata = calls.metadata(&directory)?;
    check_directory(metadata.directory, metadata.attributes)?;
    calls.volume(&directory, letter)?;

    // The directory stays open through leaf admission. The final component
    // is opened exclusively, without following a reparse point.
    append_leaf(&mut path, name)?;
    let mode = leaf_open(create, writable);
    let file = calls.open(
        &path,
        Open {
            access: mode.access,
            share: 0,
            disposition: mode.disposition,
            flags: OPEN_REPARSE_POINT | WRITE_THROUGH,
            operation: mode.operation,
        },
    )?;
    drop(path);
    let metadata = calls.metadata(&file)?;
    let file_type = calls.file_type(&file);
    check_leaf(file_type, metadata.regular, metadata.attributes)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Call {
        Resolve,
        Lookup,
        Canonical,
        OpenDirectory,
        DirectoryMetadata,
        Volume,
        OpenLeaf,
        LeafMetadata,
        FileType,
    }

    const SUCCESS: &[Call] = &[
        Call::Resolve,
        Call::Lookup,
        Call::Canonical,
        Call::OpenDirectory,
        Call::DirectoryMetadata,
        Call::Volume,
        Call::OpenLeaf,
        Call::LeafMetadata,
        Call::FileType,
    ];
    const CANONICAL_PARENT: &str = r"\\?\D:\real";
    const MAX_TERMINATED: usize = 32_767;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Fault {
        Native(Call),
        CanonicalDrive,
        DirectoryType,
        AppendLimit,
        LeafType,
    }

    #[derive(Debug)]
    struct Handle(u8, Rc<Cell<u8>>);

    impl Drop for Handle {
        fn drop(&mut self) {
            assert_ne!(self.1.get() & self.0, 0);
            self.1.set(self.1.get() & !self.0);
        }
    }

    #[derive(Default)]
    struct Fixture {
        calls: Vec<Call>,
        live: Rc<Cell<u8>>,
        opens: u8,
        failure: Option<Fault>,
        create: bool,
        writable: bool,
    }

    fn returned_path(out: &mut [u16], value: &str) -> u32 {
        let len = value.encode_utf16().count();
        assert!(len < out.len());
        for (dest, unit) in out.iter_mut().zip(value.encode_utf16().chain([0])) {
            *dest = unit;
        }
        len as u32
    }

    impl FileCalls for Fixture {
        type Handle = Handle;

        fn resolve(&mut self, input: &[u16], out: &mut [u16]) -> u32 {
            self.calls.push(Call::Resolve);
            assert!(
                input
                    .iter()
                    .copied()
                    .eq("relative".encode_utf16().chain([0]))
            );
            if self.failure == Some(Fault::Native(Call::Resolve)) {
                return 0;
            }
            returned_path(out, r"C:\input")
        }

        fn canonical(&mut self, file: &Handle, out: &mut [u16]) -> u32 {
            self.calls.push(Call::Canonical);
            assert_eq!(file.0, 1);
            assert_eq!(self.live.get(), 1);
            match self.failure {
                Some(Fault::Native(Call::Canonical)) => 0,
                Some(Fault::CanonicalDrive) => returned_path(out, r"\\?\UNC\server\share"),
                Some(Fault::AppendLimit) => {
                    if out.len() == MAX_TERMINATED {
                        out.fill(u16::from(b'x'));
                        returned_path(out, CANONICAL_PARENT);
                        out[CANONICAL_PARENT.len()] = u16::from(b'x');
                        out[MAX_TERMINATED - 1] = 0;
                        (MAX_TERMINATED - 1) as u32
                    } else {
                        MAX_TERMINATED as u32
                    }
                }
                _ => returned_path(out, CANONICAL_PARENT),
            }
        }

        fn path_error() -> Error {
            Error::Io
        }

        fn open(&mut self, path: &[u16], options: Open) -> Result<Handle> {
            let (call, id, live) = match self.opens {
                0 => (Call::Lookup, 1, 0),
                1 => (Call::OpenDirectory, 2, 0),
                2 => (Call::OpenLeaf, 4, 2),
                _ => panic!("unexpected extra open"),
            };
            self.opens += 1;
            self.calls.push(call);
            assert_eq!(self.live.get(), live);
            assert_eq!(path.last(), Some(&0));
            match call {
                Call::Lookup => {
                    assert!(
                        path.iter()
                            .copied()
                            .eq(r"\\?\C:\input".encode_utf16().chain([0]))
                    );
                    assert_eq!(
                        (
                            options.access,
                            options.share,
                            options.disposition,
                            options.flags
                        ),
                        (0, 7, 3, 0x0200_0000)
                    );
                    assert_eq!(options.operation, NativeIoOperation::OpenDirectory);
                }
                Call::OpenDirectory => {
                    if self.failure == Some(Fault::AppendLimit) {
                        assert_eq!(path.len(), MAX_TERMINATED);
                        assert!(
                            path.iter()
                                .copied()
                                .take(CANONICAL_PARENT.len())
                                .eq(CANONICAL_PARENT.encode_utf16())
                        );
                    } else {
                        assert!(
                            path.iter()
                                .copied()
                                .eq(CANONICAL_PARENT.encode_utf16().chain([0]))
                        );
                    }
                    assert_eq!(
                        (
                            options.access,
                            options.share,
                            options.disposition,
                            options.flags
                        ),
                        (0x8000_0000, 3, 3, 0x0200_0000)
                    );
                    assert_eq!(options.operation, NativeIoOperation::OpenDirectory);
                }
                Call::OpenLeaf => {
                    assert!(
                        path.iter()
                            .copied()
                            .eq(r"\\?\D:\real\leaf.db".encode_utf16().chain([0]))
                    );
                    assert_eq!(
                        options.access,
                        if self.writable {
                            0xc000_0000
                        } else {
                            0x8000_0000
                        }
                    );
                    assert_eq!((options.share, options.flags), (0, 0x8020_0000));
                    assert_eq!(
                        (options.disposition, options.operation),
                        if self.create {
                            (1, NativeIoOperation::Create)
                        } else {
                            (3, NativeIoOperation::Open)
                        }
                    );
                }
                _ => unreachable!(),
            }
            if self.failure == Some(Fault::Native(call)) {
                return Err(Error::Busy);
            }
            self.live.set(live | id);
            Ok(Handle(id, Rc::clone(&self.live)))
        }

        fn metadata(&mut self, file: &Handle) -> Result<FileMetadata> {
            let call = match file.0 {
                2 => {
                    assert_eq!(self.live.get(), 2);
                    Call::DirectoryMetadata
                }
                4 => {
                    assert_eq!(self.live.get(), 6);
                    Call::LeafMetadata
                }
                _ => panic!("metadata requested for the lookup handle"),
            };
            self.calls.push(call);
            if self.failure == Some(Fault::Native(call)) {
                return Err(Error::Io);
            }
            Ok(FileMetadata {
                directory: file.0 == 2 && self.failure != Some(Fault::DirectoryType),
                regular: file.0 == 4,
                attributes: 0,
            })
        }

        fn volume(&mut self, parent: &Handle, drive: u8) -> Result<()> {
            self.calls.push(Call::Volume);
            assert_eq!(parent.0, 2);
            assert_eq!(self.live.get(), 2);
            assert_eq!(drive, b'D');
            if self.failure == Some(Fault::Native(Call::Volume)) {
                return Err(Error::UnsupportedPlatform);
            }
            Ok(())
        }

        fn file_type(&mut self, file: &Handle) -> u32 {
            self.calls.push(Call::FileType);
            assert_eq!(file.0, 4);
            assert_eq!(self.live.get(), 6);
            if self.failure == Some(Fault::LeafType) {
                3
            } else {
                1
            }
        }
    }

    fn attempt(calls: &mut Fixture) -> Result<Handle> {
        open_leaf(
            "relative".encode_utf16(),
            "leaf.db".encode_utf16(),
            calls.create,
            calls.writable,
            AllocatorHandle::default(),
            calls,
        )
    }

    #[test]
    fn open_stops_at_each_failure_and_releases_all_acquired_handles() {
        for (failure, expected, through) in [
            (Fault::Native(Call::Resolve), Error::Io, 1),
            (Fault::Native(Call::Lookup), Error::Busy, 2),
            (Fault::Native(Call::Canonical), Error::Io, 3),
            (Fault::CanonicalDrive, Error::UnsupportedPlatform, 3),
            (Fault::Native(Call::OpenDirectory), Error::Busy, 4),
            (Fault::Native(Call::DirectoryMetadata), Error::Io, 5),
            (Fault::DirectoryType, Error::Invalid, 5),
            (Fault::Native(Call::Volume), Error::UnsupportedPlatform, 6),
            (Fault::AppendLimit, Error::Limit, 6),
            (Fault::Native(Call::OpenLeaf), Error::Busy, 7),
            (Fault::Native(Call::LeafMetadata), Error::Io, 8),
            (Fault::LeafType, Error::Invalid, 9),
        ] {
            let mut calls = Fixture {
                failure: Some(failure),
                writable: true,
                ..Fixture::default()
            };
            let mut expected_calls = SUCCESS[..through].to_vec();
            if failure == Fault::AppendLimit {
                expected_calls.insert(3, Call::Canonical);
            }
            assert_eq!(
                attempt(&mut calls).unwrap_err(),
                expected,
                "{expected_calls:?}"
            );
            assert_eq!(calls.calls, expected_calls);
            assert_eq!(calls.live.get(), 0);
        }
    }

    #[test]
    fn success_admits_only_the_exclusive_leaf_in_all_factory_modes() {
        for (create, writable) in [(true, true), (false, true), (false, false)] {
            let mut calls = Fixture {
                create,
                writable,
                ..Fixture::default()
            };
            let leaf = attempt(&mut calls).unwrap();
            assert_eq!(calls.calls, SUCCESS);
            assert_eq!(calls.live.get(), 4);
            drop(leaf);
            assert_eq!(calls.live.get(), 0);
        }
    }
}
