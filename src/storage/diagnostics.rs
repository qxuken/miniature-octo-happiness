//! Synchronous opt-in storage error context. No paths or record bytes are stored.
use crate::{Error, Result};
use std::cell::Cell;
#[cfg(any(test, target_os = "macos", target_os = "linux", windows))]
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeIoOperation {
    Open,
    Create,
    OpenDirectory,
    DuplicateHandle,
    Metadata,
    Read,
    Write,
    Sync,
    Lock,
    Unlock,
    Exchange,
    Remove,
    CopyMetadata,
    Random,
    Volume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeIoCategory {
    Permission,
    DiskFull,
    Quota,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeIoDiagnostic {
    pub operation: NativeIoOperation,
    pub category: NativeIoCategory,
    pub raw_os_error: Option<i32>,
}

/// The verification boundary that returned an error; this is not a repair hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationStage {
    ImageLength,
    Header,
    RootSlot,
    Witness,
    RootSelection,
    CommitDescriptor,
    Directory,
    Value,
}

/// Offset is the rejected structure or attempted reference start, not necessarily
/// the exact bad byte or even an offset inside the image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerificationDiagnostic {
    pub stage: VerificationStage,
    pub offset: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageFailure {
    pub error: Error,
    pub native: Option<NativeIoDiagnostic>,
    pub verification: Option<VerificationDiagnostic>,
}
impl std::fmt::Display for StorageFailure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}", self.error)?;
        if let Some(native) = self.native {
            write!(
                out,
                " ({:?}, {:?}, OS {:?})",
                native.operation, native.category, native.raw_os_error
            )?;
        }
        if let Some(verification) = self.verification {
            write!(out, " (verification {:?}", verification.stage)?;
            if let Some(offset) = verification.offset {
                write!(out, ", structure offset {offset}")?;
            }
            write!(out, ")")?;
        }
        Ok(())
    }
}
impl std::error::Error for StorageFailure {}

#[derive(Clone, Copy, Default)]
struct Context {
    native: Option<NativeIoDiagnostic>,
    verification: Option<VerificationDiagnostic>,
}
thread_local! {
    static CAPTURE: Cell<Option<Context>> = const { Cell::new(None) };
}
struct Scope(Option<Context>);
impl Drop for Scope {
    fn drop(&mut self) {
        CAPTURE.with(|state| state.set(self.0));
    }
}

/// Capture native and verification context for one synchronous operation on this thread.
/// The returned Error remains authoritative. Context identifies the first
/// terminal native failure observed within the scope, including cleanup.
/// Nested scopes restore their caller's state, including during unwinding.
/// Custom ByteStore errors may have no native diagnostic. Verification context
/// records an observed failure within this operation. A caller that handles an
/// error and continues should use a nested capture for that separate operation:
/// Copy errors cannot carry causal identity through arbitrary caller closures.
pub fn capture<T>(action: impl FnOnce() -> Result<T>) -> std::result::Result<T, StorageFailure> {
    let _scope = Scope(CAPTURE.with(|state| state.replace(Some(Context::default()))));
    action().map_err(|error| StorageFailure {
        error,
        native: CAPTURE.with(|state| state.get().and_then(|context| context.native)),
        verification: CAPTURE.with(|state| state.get().and_then(|context| context.verification)),
    })
}

#[cfg(any(test, target_os = "macos", target_os = "linux", windows))]
pub(super) fn record(operation: NativeIoOperation, error: &io::Error) {
    CAPTURE.with(|state| {
        if let Some(mut context) = state.get()
            && context.native.is_none()
        {
            let category = match error.kind() {
                io::ErrorKind::PermissionDenied => NativeIoCategory::Permission,
                io::ErrorKind::StorageFull => NativeIoCategory::DiskFull,
                io::ErrorKind::QuotaExceeded => NativeIoCategory::Quota,
                _ => NativeIoCategory::Other,
            };
            context.native = Some(NativeIoDiagnostic {
                operation,
                category,
                raw_os_error: error.raw_os_error(),
            });
            state.set(Some(context));
        }
    });
}

/// Start a new recovery attempt, or discard a deliberately handled failed probe.
/// Native observations have their own established lifetime and are not reset.
pub(super) fn clear_verification() {
    CAPTURE.with(|state| {
        if let Some(mut context) = state.get() {
            context.verification = None;
            state.set(Some(context));
        }
    });
}

pub(super) fn verify<T>(
    stage: VerificationStage,
    offset: Option<u64>,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    action().inspect_err(|_| {
        CAPTURE.with(|state| {
            if let Some(mut context) = state.get()
                && context.verification.is_none()
            {
                context.verification = Some(VerificationDiagnostic { stage, offset });
                state.set(Some(context));
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_diagnostics_keep_first_error_and_clear_between_operations() {
        for (kind, category) in [
            (
                io::ErrorKind::PermissionDenied,
                NativeIoCategory::Permission,
            ),
            (io::ErrorKind::StorageFull, NativeIoCategory::DiskFull),
            (io::ErrorKind::QuotaExceeded, NativeIoCategory::Quota),
        ] {
            let failure = capture::<()>(|| {
                record(NativeIoOperation::Write, &io::Error::from(kind));
                record(
                    NativeIoOperation::Unlock,
                    &io::Error::from(io::ErrorKind::Other),
                );
                Err(Error::OutcomeUnknown)
            })
            .unwrap_err();
            assert_eq!(failure.error, Error::OutcomeUnknown);
            assert_eq!(failure.native.unwrap().category, category);
            assert_eq!(failure.native.unwrap().operation, NativeIoOperation::Write);
            assert_eq!(
                capture::<()>(|| Err(Error::RecoveryRequired))
                    .unwrap_err()
                    .native,
                None
            );
        }
        let db = crate::Database::new().unwrap();
        let mut tx = db.begin();
        tx.abort().unwrap();
        assert_eq!(
            tx.commit_with_diagnostics().unwrap_err(),
            StorageFailure {
                error: Error::Closed,
                native: None,
                verification: None,
            }
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn retried_interruptions_never_replace_the_terminal_native_error() {
        let mut calls = 0;
        let failure = capture::<()>(|| {
            super::super::file::retry(NativeIoOperation::Read, || {
                calls += 1;
                Err(io::Error::from(if calls == 1 {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::PermissionDenied
                }))
            })
        })
        .unwrap_err();
        assert_eq!(calls, 2);
        assert_eq!(
            failure.native.unwrap().category,
            NativeIoCategory::Permission
        );
        assert_eq!(failure.native.unwrap().operation, NativeIoOperation::Read);
    }

    #[test]
    fn scoped_diagnostics_restore_on_unwind_and_isolate_threads() {
        let outer = capture::<()>(|| {
            record(
                NativeIoOperation::Read,
                &io::Error::from(io::ErrorKind::PermissionDenied),
            );
            let result = std::panic::catch_unwind(|| {
                capture::<()>(|| {
                    record(
                        NativeIoOperation::Sync,
                        &io::Error::from(io::ErrorKind::StorageFull),
                    );
                    panic!("diagnostic scope unwind");
                })
            });
            assert!(result.is_err());
            let other = std::thread::spawn(|| {
                assert_eq!(capture::<()>(|| Err(Error::Io)).unwrap_err().native, None);
                capture::<()>(|| {
                    record(
                        NativeIoOperation::Write,
                        &io::Error::from(io::ErrorKind::QuotaExceeded),
                    );
                    Err(Error::Io)
                })
                .unwrap_err()
            })
            .join()
            .unwrap();
            assert_eq!(other.native.unwrap().category, NativeIoCategory::Quota);
            Err(Error::Io)
        })
        .unwrap_err();
        assert_eq!(outer.native.unwrap().operation, NativeIoOperation::Read);
        assert_eq!(capture::<()>(|| Err(Error::Io)).unwrap_err().native, None);
    }

    #[test]
    fn commit_diagnostics_preserve_uncertain_and_recovery_required_results() {
        use crate::{
            Database, Limits,
            storage::{ByteStore, SimulatedDisk},
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct FailingSync {
            disk: SimulatedDisk,
            remaining: Arc<AtomicUsize>,
        }
        impl ByteStore for FailingSync {
            fn lock(&mut self) -> Result<()> {
                self.disk.lock()
            }
            fn unlock(&mut self) {
                self.disk.unlock();
            }
            fn length(&mut self) -> Result<u64> {
                self.disk.length()
            }
            fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
                self.disk.read(offset, out)
            }
            fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
                self.disk.write(offset, bytes)
            }
            fn sync(&mut self) -> Result<()> {
                let remaining = self.remaining.load(Ordering::Relaxed);
                if remaining == 0 {
                    record(
                        NativeIoOperation::Sync,
                        &io::Error::from(io::ErrorKind::StorageFull),
                    );
                    return Err(Error::Io);
                }
                if remaining != usize::MAX {
                    self.remaining.store(remaining - 1, Ordering::Relaxed);
                }
                self.disk.sync()
            }
        }
        for (syncs, expected) in [
            (0, Error::Io),
            (1, Error::OutcomeUnknown),
            (2, Error::OutcomeUnknown),
        ] {
            let remaining = Arc::new(AtomicUsize::new(usize::MAX));
            let db = Database::create_on(
                FailingSync {
                    disk: SimulatedDisk::new().unwrap(),
                    remaining: remaining.clone(),
                },
                [89; 16],
                Limits::default(),
            )
            .unwrap();
            remaining.store(syncs, Ordering::Relaxed);
            let failure = db.begin().commit_with_diagnostics().unwrap_err();
            assert_eq!(failure.error, expected);
            assert_eq!(failure.native.unwrap().operation, NativeIoOperation::Sync);
            assert_eq!(failure.native.unwrap().category, NativeIoCategory::DiskFull);
            let next = db.begin().commit_with_diagnostics().unwrap_err();
            assert_eq!(next.error, Error::RecoveryRequired);
            assert_eq!(next.native, None);
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[test]
    fn native_factory_reports_origin_without_paths_and_resets_on_validation_error() {
        let path = std::env::temp_dir()
            .join(format!(
                "weekend-diagnostics-missing-{}",
                std::process::id()
            ))
            .join("secret-record-name");
        let failure =
            match crate::Database::open_file_with_diagnostics(&path, crate::Limits::default()) {
                Err(failure) => failure,
                Ok(_) => panic!("missing path opened"),
            };
        assert_eq!(failure.error, Error::NotFound);
        assert_eq!(
            failure.native.unwrap().operation,
            NativeIoOperation::OpenDirectory
        );
        assert!(failure.native.unwrap().raw_os_error.is_some());
        assert!(!format!("{failure:?}").contains("secret-record-name"));
        let invalid = crate::Limits {
            max_depth: 0,
            ..Default::default()
        };
        let failure = match crate::Database::create_file_with_diagnostics(&path, invalid) {
            Err(failure) => failure,
            Ok(_) => panic!("invalid limits accepted"),
        };
        assert_eq!(failure.error, Error::Invalid);
        assert_eq!(failure.native, None);
    }
    #[test]
    fn verification_inner_failure_wins_and_scopes_restore_on_unwind() {
        let outer = capture(|| {
            let _ = verify::<()>(VerificationStage::Header, Some(0), || Err(Error::Corrupt));
            let failure = capture(|| {
                verify(VerificationStage::CommitDescriptor, Some(4096), || {
                    verify::<()>(VerificationStage::Value, Some(8192), || Err(Error::Corrupt))
                })
            })
            .unwrap_err();
            assert_eq!(
                failure.verification.unwrap().stage,
                VerificationStage::Value
            );
            assert_eq!(failure.verification.unwrap().offset, Some(8192));
            assert!(
                std::panic::catch_unwind(|| capture::<()>(|| {
                    let _ =
                        verify::<()>(VerificationStage::Header, Some(0), || Err(Error::Corrupt));
                    panic!("intentional capture unwind");
                }))
                .is_err()
            );
            Err::<(), _>(Error::Corrupt)
        })
        .unwrap_err();
        assert_eq!(
            outer.verification,
            Some(VerificationDiagnostic {
                stage: VerificationStage::Header,
                offset: Some(0)
            })
        );
        assert_eq!(outer.native, None);
        assert_eq!(
            capture::<()>(|| Err(Error::Limit))
                .unwrap_err()
                .verification,
            None
        );
    }

    #[test]
    fn verification_context_is_thread_local_and_independent_from_native_context() {
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for offset in [4096, 8192] {
                let barrier = &barrier;
                scope.spawn(move || {
                    let failure = capture(|| {
                        record(
                            NativeIoOperation::Read,
                            &io::Error::from(io::ErrorKind::PermissionDenied),
                        );
                        let result =
                            verify::<()>(VerificationStage::Value, Some(offset), || Err(Error::Io));
                        barrier.wait();
                        result
                    })
                    .unwrap_err();
                    assert_eq!(failure.error, Error::Io);
                    assert_eq!(failure.native.unwrap().operation, NativeIoOperation::Read);
                    assert_eq!(failure.verification.unwrap().offset, Some(offset));
                });
            }
        });
    }
}
