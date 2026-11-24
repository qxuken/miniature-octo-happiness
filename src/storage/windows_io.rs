//! Windows handle ownership and native error mapping.
use super::{NativeIoOperation, diagnostics};
use crate::{Error, Result};
use std::io;

pub(super) fn os_error(operation: NativeIoOperation, error: io::Error) -> Error {
    diagnostics::record(operation, &error);
    match error.raw_os_error() {
        Some(2 | 3) => Error::NotFound,
        Some(32 | 33) => Error::Busy,
        Some(80 | 183) => Error::AlreadyExists,
        Some(123 | 267) => Error::Invalid,
        _ => Error::Io,
    }
}

/// Owns an exclusive handle from open until release. A claim permits byte-store I/O.
pub(super) enum Lease<T> {
    Open(T),
    Claimed(T),
    Closed,
}

impl<T> Lease<T> {
    pub(super) fn claim(&mut self) -> Result<()> {
        match std::mem::replace(self, Self::Closed) {
            Self::Open(handle) => {
                *self = Self::Claimed(handle);
                Ok(())
            }
            Self::Claimed(handle) => {
                *self = Self::Claimed(handle);
                Err(Error::Busy)
            }
            Self::Closed => Err(Error::Closed),
        }
    }

    pub(super) fn release(&mut self) {
        *self = Self::Closed;
    }

    pub(super) fn execute<R>(
        &self,
        operation: NativeIoOperation,
        action: impl FnOnce(&T) -> io::Result<R>,
    ) -> Result<R> {
        match self {
            Self::Claimed(handle) => action(handle).map_err(|error| os_error(operation, error)),
            Self::Open(_) | Self::Closed => Err(Error::Io),
        }
    }
}

pub(super) fn own_open_handle<T>(
    handle: *mut std::ffi::c_void,
    operation: NativeIoOperation,
    last_error: impl FnOnce() -> io::Error,
    own: impl FnOnce(*mut std::ffi::c_void) -> T,
) -> Result<T> {
    if handle == (-1isize) as *mut std::ffi::c_void {
        return Err(os_error(operation, last_error()));
    }
    Ok(own(handle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn open_result_checks_only_the_invalid_handle_sentinel() {
        let mut value = 42u32;
        let handle = (&raw mut value).cast();
        let opened = diagnostics::capture(|| {
            own_open_handle(
                handle,
                NativeIoOperation::Open,
                || panic!("a successful open consulted the last error"),
                |opened| opened,
            )
        });
        assert_eq!(opened.unwrap(), handle);

        for (operation, code, expected) in [
            (NativeIoOperation::OpenDirectory, 3, Error::NotFound),
            (NativeIoOperation::Open, 32, Error::Busy),
            (NativeIoOperation::Create, 80, Error::AlreadyExists),
            (NativeIoOperation::Create, 5, Error::Io),
        ] {
            let reads = Cell::new(0);
            let failure = diagnostics::capture(|| {
                own_open_handle(
                    (-1isize) as *mut std::ffi::c_void,
                    operation,
                    || {
                        reads.set(reads.get() + 1);
                        io::Error::from_raw_os_error(code)
                    },
                    |_| panic!("an invalid handle was adopted"),
                )
            })
            .unwrap_err();
            assert_eq!(reads.get(), 1);
            assert_eq!(failure.error, expected);
            let native = failure.native.unwrap();
            assert_eq!(native.operation, operation);
            assert_eq!(native.raw_os_error, Some(code));
        }
    }

    #[test]
    fn error_mapping_preserves_the_raw_code_and_operation() {
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
            (112, Error::Io),
            (1816, Error::Io),
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
            Err(os_error(NativeIoOperation::Read, io::Error::other("read")))
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        let native = failure.native.unwrap();
        assert_eq!(native.raw_os_error, None);
        assert_eq!(native.operation, NativeIoOperation::Read);
    }

    #[test]
    fn claim_and_release_preserve_exclusive_handle_lifetime() {
        struct Handle<'a>(&'a Cell<u32>);
        impl Drop for Handle<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let drops = Cell::new(0);
        let mut lease = Lease::Open(Handle(&drops));
        let probe = |lease: &Lease<Handle<'_>>| {
            diagnostics::capture(|| {
                lease.execute::<()>(NativeIoOperation::Read, |_| {
                    panic!("I/O before a claim or after release")
                })
            })
            .unwrap_err()
        };
        let blocked = probe(&lease);
        assert_eq!(blocked.error, Error::Io);
        assert_eq!(blocked.native, None);

        assert_eq!(lease.claim(), Ok(()));
        assert_eq!(lease.claim(), Err(Error::Busy));
        assert_eq!(drops.get(), 0);
        assert_eq!(
            lease.execute(NativeIoOperation::Read, |handle| Ok(handle.0.get())),
            Ok(0)
        );
        lease.release();
        assert_eq!(drops.get(), 1);
        assert_eq!(lease.claim(), Err(Error::Closed));
        let blocked = probe(&lease);
        assert_eq!(blocked.error, Error::Io);
        assert_eq!(blocked.native, None);
        lease.release();
        drop(lease);
        assert_eq!(drops.get(), 1);

        let mut unused = Lease::Open(Handle(&drops));
        unused.release();
        assert_eq!(drops.get(), 2);
        assert_eq!(unused.claim(), Err(Error::Closed));

        let mut dropped = Lease::Open(Handle(&drops));
        dropped.claim().unwrap();
        drop(dropped);
        assert_eq!(drops.get(), 3);
    }

    #[test]
    fn claimed_io_preserves_return_values_and_diagnostics_for_each_operation() {
        let mut lease = Lease::Open(37);
        lease.claim().unwrap();
        for operation in [
            NativeIoOperation::Metadata,
            NativeIoOperation::Read,
            NativeIoOperation::Write,
            NativeIoOperation::Sync,
        ] {
            let called = Cell::new(0);
            let error = diagnostics::capture(|| {
                lease.execute::<u64>(operation, |handle| {
                    assert_eq!(*handle, 37);
                    called.set(called.get() + 1);
                    Err(io::Error::from_raw_os_error(5))
                })
            })
            .unwrap_err();
            assert_eq!(called.get(), 1);
            assert_eq!(error.error, Error::Io);
            let native = error.native.unwrap();
            assert_eq!(native.operation, operation);
            assert_eq!(native.raw_os_error, Some(5));
            assert_eq!(lease.execute(operation, |handle| Ok(*handle)), Ok(37));
        }
    }
}
