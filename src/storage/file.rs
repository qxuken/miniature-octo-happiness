//! Shared macOS/Linux regular-file protocol. Protected-region/safe-append assumptions
//! remain conditional; process interruption tests are not physical power cuts.
use super::NativeIoOperation;
#[cfg(target_os = "linux")]
use super::linux as sys;
#[cfg(target_os = "macos")]
use super::macos as sys;
use super::{ByteStore, CompactionStats, Storage};
use crate::{Error, Limits, Result, memory::Memory};
use std::{
    ffi::{c_char, c_int},
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileExt, MetadataExt, OpenOptionsExt},
        },
    },
    path::Path,
};
use sys::{OPEN_CREATE, OPEN_LEASE, OPEN_REQUIRED, full_sync, identity, os_error};

unsafe extern "C" {
    fn openat(directory: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
    fn flock(fd: c_int, operation: c_int) -> c_int;
    fn unlinkat(directory: c_int, name: *const c_char, flags: c_int) -> c_int;
}
pub(super) fn retry<T>(
    operation: NativeIoOperation,
    mut action: impl FnMut() -> io::Result<T>,
) -> Result<T> {
    loop {
        match action() {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            result => return result.map_err(|error| os_error(operation, error)),
        }
    }
}
/// A regular-file writer lease on macOS or Linux. Final-component symlinks are
/// rejected and hard-link aliases share the same inode lock. The held parent
/// descriptor and a post-lock inode check reject open/replacement races.
/// No file cloning, weak sync mode or multi-process write protocol is exposed.
pub struct FileStore {
    file: File,
    location: Location,
    parent_pending: bool,
    claimed: bool,
}
impl FileStore {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_impl(path.as_ref(), true)
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_impl(path.as_ref(), false)
    }
    /// Open with read access and the same exclusive lease as a writer.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        Location::new(path.as_ref())?.open_mode(false, false)
    }
    fn open_impl(path: &Path, create: bool) -> Result<Self> {
        Location::new(path)?.open(create)
    }
    fn check(&self) -> Result<()> {
        if self.claimed { Ok(()) } else { Err(Error::Io) }
    }
}
impl ByteStore for FileStore {
    fn lock(&mut self) -> Result<()> {
        if self.claimed {
            return Err(Error::Busy);
        }
        retry(NativeIoOperation::Lock, || {
            if unsafe { flock(self.file.as_raw_fd(), 2 | 4) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        })?;
        if let Err(error) = self.location.check_lease(&self.file) {
            self.unlock();
            return Err(error);
        }
        self.claimed = true;
        Ok(())
    }
    fn unlock(&mut self) {
        let _ = retry(NativeIoOperation::Unlock, || {
            if unsafe { flock(self.file.as_raw_fd(), 8) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
        // If explicit unlock fails, closing the owned File still releases it.
        self.claimed = false;
    }
    fn length(&mut self) -> Result<u64> {
        self.check()?;
        Ok(retry(NativeIoOperation::Metadata, || self.file.metadata())?.len())
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        self.check()?;
        retry(NativeIoOperation::Read, || self.file.read_at(out, offset))
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.check()?;
        retry(NativeIoOperation::Write, || {
            self.file.write_at(bytes, offset)
        })
    }
    fn sync(&mut self) -> Result<()> {
        self.check()?;
        full_sync(&self.file)?;
        if self.parent_pending {
            self.location.sync()?;
            full_sync(&self.file)?;
            self.parent_pending = false;
        }
        Ok(())
    }
}

// All namespace operations use this one directory descriptor, including relative
// paths and parents reached through symlinks. External pathname mutation is not
// a cooperating operation; recheck both inode identities before exchange.
struct Location {
    directory: File,
    name: Vec<u8>, // native leaf bytes, including terminating zero
}
impl Location {
    fn new(path: &Path) -> Result<Self> {
        if !sys::SUPPORTED {
            return Err(Error::UnsupportedPlatform);
        }
        let bytes = path.as_os_str().as_bytes();
        if bytes.contains(&0) {
            return Err(Error::Invalid);
        }
        if bytes.len() > 1024 * 1024 {
            return Err(Error::Limit);
        }
        let name = bytes.rsplit(|b| *b == b'/').next().ok_or(Error::Invalid)?;
        if name.is_empty() || name == b"." || name == b".." {
            return Err(Error::Invalid);
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let directory = retry(NativeIoOperation::OpenDirectory, || {
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(sys::OPEN_DIRECTORY)
                .open(parent)
        })?;
        let mem = Memory::new(Limits::default())?;
        let mut name = mem.copy_external(name)?;
        mem.reserve_external(&mut name, 1)?;
        name.push(0);
        Ok(Self { directory, name })
    }
    fn open(&self, create: bool) -> Result<FileStore> {
        self.open_mode(create, true)
    }
    fn open_mode(&self, create: bool, writable: bool) -> Result<FileStore> {
        // Allocate/copy the lease location before creating any directory entry.
        let location = Self {
            directory: retry(NativeIoOperation::DuplicateHandle, || {
                self.directory.try_clone()
            })?,
            name: Memory::new(Limits::default())?.copy_external(&self.name)?,
        };
        let access = if writable { 2 } else { 0 };
        let file = self.open_flags(access | OPEN_LEASE | if create { OPEN_CREATE } else { 0 })?;
        if !retry(NativeIoOperation::Metadata, || file.metadata())?.is_file() {
            return Err(Error::Invalid);
        }
        self.acquire_open_lease(&file)?;
        Ok(FileStore {
            file,
            location,
            parent_pending: create,
            claimed: false,
        })
    }
    fn acquire_open_lease(&self, file: &File) -> Result<()> {
        sys::acquire_open_lock(file)?;
        self.check_lease(file)
    }
    fn check_lease(&self, file: &File) -> Result<()> {
        // Linux openat + flock are separate calls. A compactor may exchange the
        // pathname while an opener is paused before flock. Never recover/write
        // the old inode after its lock becomes available. Also covers reuse of
        // a manually unlocked ByteStore after a replacement.
        let actual = match self.open_flags(0) {
            Err(Error::NotFound) => return Err(Error::Busy),
            other => other?,
        };
        let actual = retry(NativeIoOperation::Metadata, || actual.metadata())?;
        let expected = retry(NativeIoOperation::Metadata, || file.metadata())?;
        if actual.is_file() && actual.dev() == expected.dev() && actual.ino() == expected.ino() {
            Ok(())
        } else {
            Err(Error::Busy)
        }
    }
    fn open_flags(&self, flags: c_int) -> Result<File> {
        let fd = retry(
            if flags & OPEN_CREATE != 0 {
                NativeIoOperation::Create
            } else {
                NativeIoOperation::Open
            },
            || {
                let fd = unsafe {
                    openat(
                        self.directory.as_raw_fd(),
                        self.name.as_ptr().cast(),
                        flags | OPEN_REQUIRED,
                        0o600 as c_int,
                    )
                };
                if fd < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(fd)
                }
            },
        )?;
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn matches(&self, file: &File) -> Result<()> {
        let actual_file = self.open_flags(0)?;
        let actual = retry(NativeIoOperation::Metadata, || actual_file.metadata())?;
        let expected = retry(NativeIoOperation::Metadata, || file.metadata())?;
        if actual.is_file()
            && actual.dev() == expected.dev()
            && actual.ino() == expected.ino()
            && actual.nlink() == 1
        {
            Ok(())
        } else {
            Err(Error::Invalid)
        }
    }
    fn scratch(&self, mem: &Memory) -> Result<Self> {
        let mut name = mem.copy_external(b".wdb-compact-")?;
        mem.reserve_external(&mut name, 37)?;
        for byte in identity()? {
            for n in [byte >> 4, byte & 15] {
                name.push(b"0123456789abcdef"[n as usize]);
            }
        }
        name.extend_from_slice(b".tmp\0");
        Ok(Self {
            directory: retry(NativeIoOperation::DuplicateHandle, || {
                self.directory.try_clone()
            })?,
            name,
        })
    }
    fn sync(&self) -> Result<()> {
        retry(NativeIoOperation::Sync, || self.directory.sync_all())
    }
    fn exchange(&self, other: &Self) -> Result<()> {
        sys::exchange(&self.directory, &self.name, &other.directory, &other.name)
    }
    fn remove(&self) -> Result<()> {
        if unsafe { unlinkat(self.directory.as_raw_fd(), self.name.as_ptr().cast(), 0) } == 0 {
            Ok(())
        } else {
            Err(os_error(
                NativeIoOperation::Remove,
                io::Error::last_os_error(),
            ))
        }
    }
}

/// Fault-injection observation points; unavailable as a production callback API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactionStep {
    SourceLength,
    SourceRead,
    SourceWrite,
    SourceSync,
    TargetLength,
    TargetRead,
    TargetWrite,
    TargetSync,
    CopyMetadata,
    SyncMetadata,
    CheckNames,
    Exchange,
    SyncDirectory,
    SyncReplacement,
    RemoveOld,
    SyncCleanupDirectory,
    SyncCleanupFile,
}
#[cfg(any(test, feature = "fault-injection"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactionEvent {
    pub step: CompactionStep,
    pub after: bool,
}
#[derive(Clone, Default)]
struct Observer {
    #[cfg(any(test, feature = "fault-injection"))]
    hook: Option<crate::memory::Shared<std::sync::Mutex<Hook>>>,
}
#[cfg(any(test, feature = "fault-injection"))]
type Hook = Box<dyn FnMut(CompactionEvent) -> Result<()> + Send>;
impl Observer {
    fn event(&self, step: CompactionStep, after: bool) -> Result<()> {
        #[cfg(any(test, feature = "fault-injection"))]
        if let Some(hook) = &self.hook {
            return hook.lock().map_err(|_| Error::Panic)?(CompactionEvent { step, after });
        }
        let _ = (step, after);
        Ok(())
    }
    fn call<T>(&self, step: CompactionStep, action: impl FnOnce() -> Result<T>) -> Result<T> {
        self.event(step, false)?;
        let result = action();
        self.event(step, true)?;
        result
    }
}
struct Observed {
    file: FileStore,
    observer: Observer,
    target: bool,
}
impl ByteStore for Observed {
    fn lock(&mut self) -> Result<()> {
        self.file.lock()
    }
    fn unlock(&mut self) {
        self.file.unlock();
    }
    fn length(&mut self) -> Result<u64> {
        self.observer.call(
            if self.target {
                CompactionStep::TargetLength
            } else {
                CompactionStep::SourceLength
            },
            || self.file.length(),
        )
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        self.observer.call(
            if self.target {
                CompactionStep::TargetRead
            } else {
                CompactionStep::SourceRead
            },
            || self.file.read(offset, out),
        )
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.observer.call(
            if self.target {
                CompactionStep::TargetWrite
            } else {
                CompactionStep::SourceWrite
            },
            || self.file.write(offset, bytes),
        )
    }
    fn sync(&mut self) -> Result<()> {
        self.observer.call(
            if self.target {
                CompactionStep::TargetSync
            } else {
                CompactionStep::SourceSync
            },
            || self.file.sync(),
        )
    }
}
pub(crate) fn compact_file(path: &Path, limits: Limits) -> Result<CompactionStats> {
    compact_file_impl(path, limits, Observer::default())
}
#[cfg(any(test, feature = "fault-injection"))]
pub fn compact_file_with_hook(
    path: &Path,
    limits: Limits,
    hook: impl FnMut(CompactionEvent) -> Result<()> + Send + 'static,
) -> Result<CompactionStats> {
    let mem = Memory::new(limits)?;
    let hook: Hook = mem.boxed(hook)?;
    let observer = Observer {
        hook: Some(mem.shared(std::sync::Mutex::new(hook))?),
    };
    compact_file_impl(path, limits, observer)
}
fn compact_file_impl(path: &Path, limits: Limits, observer: Observer) -> Result<CompactionStats> {
    let mem = Memory::new(limits)?;
    let original = Location::new(path)?;
    let source = original.open(false)?;
    let old_file = retry(NativeIoOperation::DuplicateHandle, || {
        source.file.try_clone()
    })?;
    // Replacing one name cannot preserve a hard-link alias's writer authority.
    if retry(NativeIoOperation::Metadata, || old_file.metadata())?.nlink() != 1 {
        return Err(Error::Invalid);
    }
    let (mut storage, head, next_collection, retention) = Storage::open(
        Observed {
            file: source,
            observer: observer.clone(),
            target: false,
        },
        &mem,
    )?;
    let scratch = original.scratch(&mem)?;
    let target = scratch.open(true)?;
    let new_file = retry(NativeIoOperation::DuplicateHandle, || {
        target.file.try_clone()
    })?;
    let (mut replacement, stats) = storage
        .compact(
            Observed {
                file: target,
                observer: observer.clone(),
                target: true,
            },
            &mem,
            &head,
            next_collection,
            &retention,
        )
        .map_err(|e| {
            if e == Error::OutcomeUnknown {
                Error::Io
            } else {
                e
            }
        })?;
    observer.call(CompactionStep::CopyMetadata, || {
        sys::copy_metadata(&old_file, &new_file, &mem)
    })?;
    observer.call(CompactionStep::SyncMetadata, || full_sync(&new_file))?;
    // Metadata APIs can affect data interpretation (e.g. compression xattrs).
    // Re-verify after every mutation, before publishing a directory entry.
    replacement.verify(&mem)?;
    observer.call(CompactionStep::CheckNames, || {
        original.matches(&old_file)?;
        scratch.matches(&new_file)
    })?;
    // From the exchange phase onward retain both names on errors and report an
    // uncertain replacement outcome. Never roll back or delete a recovery copy.
    use super::replacement::Step;
    super::replacement::publish(|step| match step {
        Step::Exchange => observer.call(CompactionStep::Exchange, || original.exchange(&scratch)),
        Step::SyncDirectory => observer.call(CompactionStep::SyncDirectory, || original.sync()),
        Step::SyncReplacement => {
            observer.call(CompactionStep::SyncReplacement, || full_sync(&new_file))
        }
        Step::RemoveOld => observer.call(CompactionStep::RemoveOld, || scratch.remove()),
        Step::SyncCleanupDirectory => {
            observer.call(CompactionStep::SyncCleanupDirectory, || original.sync())
        }
        Step::SyncCleanupFile => {
            observer.call(CompactionStep::SyncCleanupFile, || full_sync(&new_file))
        }
    })
    .map_err(|_| Error::OutcomeUnknown)?;
    Ok(stats)
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

#[cfg(all(
    test,
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod tests {
    use super::*;
    #[test]
    fn opener_paused_before_flock_rejects_the_replaced_inode() {
        let path =
            std::env::temp_dir().join(format!("weekend-open-race-{}.db", std::process::id()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let cleanup = Cleanup(path);
        drop(crate::Database::create_file(&cleanup.0, Limits::default()).unwrap());
        let location = Location::new(&cleanup.0).unwrap();
        // This descriptor is exactly what openat returned to an opener that was
        // descheduled before flock. Let a complete compaction pass it by.
        let old = location.open_flags(2).unwrap();
        crate::Database::compact_file(&cleanup.0, Limits::default()).unwrap();
        assert_eq!(location.acquire_open_lease(&old), Err(Error::Busy));
        drop(old);
        drop(crate::Database::open_file(&cleanup.0, Limits::default()).unwrap());
    }
}
