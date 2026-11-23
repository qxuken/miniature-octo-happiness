//! Shared macOS/Linux regular-file protocol. Protected-region/safe-append assumptions
//! remain conditional; process interruption tests are not physical power cuts.
use super::NativeIoOperation;
#[cfg(target_os = "linux")]
use super::linux as sys;
#[cfg(target_os = "macos")]
use super::macos as sys;
use super::{ByteStore, CompactionStats, Storage};
use crate::{AllocatorHandle, Error, Limits, Result, memory::Memory};
use std::{
    ffi::{c_char, c_int},
    fs::{File, Metadata},
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileExt, MetadataExt},
        },
    },
    path::Path,
};
use sys::{OPEN_CREATE, OPEN_LEASE, OPEN_REQUIRED, full_sync, identity, os_error};

unsafe extern "C" {
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
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
fn file_metadata(file: &File) -> Result<Metadata> {
    retry(NativeIoOperation::Metadata, || {
        #[cfg(test)]
        metadata_decisions::before_read()?;
        file.metadata()
    })
}
/// A regular-file writer lease on macOS or Linux. Final-component symlinks are
/// rejected and hard-link aliases share the same inode lock. The held parent
/// descriptor and a post-lock inode check reject open/replacement races.
/// No file cloning, weak sync mode or multi-process write protocol is exposed.
pub struct FileStore {
    file: File,
    location: Location,
    parent_pending: bool,
    lease: Lease,
}
#[derive(PartialEq, Eq)]
enum Lease {
    Open,
    Claimed,
    Released,
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
        Location::new(path.as_ref())?.open_mode(false, false)
    }
    pub(crate) fn open_in(
        path: impl AsRef<Path>,
        create: bool,
        allocator: AllocatorHandle,
    ) -> Result<Self> {
        Location::new_in(path.as_ref(), allocator)?.open_mode(create, true)
    }
    fn check(&self) -> Result<()> {
        if self.lease == Lease::Claimed {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
}
impl Drop for FileStore {
    fn drop(&mut self) {
        // A concurrent fork can keep the open description alive until exec.
        if self.lease != Lease::Released {
            self.unlock();
        }
    }
}
impl ByteStore for FileStore {
    fn lock(&mut self) -> Result<()> {
        if self.lease == Lease::Claimed {
            return Err(Error::Busy);
        }
        retry(NativeIoOperation::Lock, || {
            if unsafe { flock(self.file.as_raw_fd(), 2 | 4) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        })?;
        self.lease = Lease::Open;
        if let Err(error) = self.location.check_lease(&self.file) {
            self.unlock();
            return Err(error);
        }
        self.lease = Lease::Claimed;
        Ok(())
    }
    fn unlock(&mut self) {
        let result = retry(NativeIoOperation::Unlock, || {
            if unsafe { flock(self.file.as_raw_fd(), 8) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
        self.lease = if result.is_ok() {
            Lease::Released
        } else {
            // Disable I/O and leave the descriptor available for cleanup.
            Lease::Open
        };
    }
    fn length(&mut self) -> Result<u64> {
        self.check()?;
        Ok(file_metadata(&self.file)?.len())
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
        self.sync_with(full_sync, Location::sync)
    }
}

impl FileStore {
    fn sync_with(
        &mut self,
        mut sync_file: impl FnMut(&File) -> Result<()>,
        sync_parent: impl FnOnce(&Location) -> Result<()>,
    ) -> Result<()> {
        sync_file(&self.file)?;
        if self.parent_pending {
            sync_parent(&self.location)?;
            sync_file(&self.file)?;
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
    name: Vec<u8, AllocatorHandle>, // native leaf bytes, including terminating zero
}
impl Location {
    fn new(path: &Path) -> Result<Self> {
        Self::new_in(path, AllocatorHandle::default())
    }
    fn new_in(path: &Path, allocator: AllocatorHandle) -> Result<Self> {
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
        let directory = {
            let parent = Self::terminated_path(allocator.clone(), parent.as_os_str().as_bytes())?;
            retry(NativeIoOperation::OpenDirectory, || {
                // SAFETY: parent is NUL-terminated. A read-only directory open needs no mode.
                let fd =
                    unsafe { open(parent.as_ptr().cast(), sys::OPEN_DIRECTORY | sys::O_CLOEXEC) };
                if fd < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    // SAFETY: open returned a new owned descriptor.
                    Ok(unsafe { File::from_raw_fd(fd) })
                }
            })?
        };
        let name = Self::terminated_path(allocator, name)?;
        Ok(Self { directory, name })
    }
    fn terminated_path(
        allocator: AllocatorHandle,
        bytes: &[u8],
    ) -> Result<Vec<u8, AllocatorHandle>> {
        let mut path = Self::copy_path(allocator, bytes, 1)?;
        path.push(0);
        Ok(path)
    }
    fn copy_path(
        allocator: AllocatorHandle,
        bytes: &[u8],
        extra: usize,
    ) -> Result<Vec<u8, AllocatorHandle>> {
        let mut path = Vec::new_in(allocator);
        path.try_reserve_exact(bytes.len() + extra)
            .map_err(|_| Error::OutOfMemory)?;
        path.extend_from_slice(bytes);
        Ok(path)
    }
    // Offline compaction keeps a location for the later exchange and cleanup.
    fn duplicate(&self) -> Result<Self> {
        Ok(Self {
            directory: retry(NativeIoOperation::DuplicateHandle, || {
                self.directory.try_clone()
            })?,
            name: Self::copy_path(self.name.allocator().clone(), &self.name, 0)?,
        })
    }
    fn open_mode(self, create: bool, writable: bool) -> Result<FileStore> {
        let access = if writable { 2 } else { 0 };
        let file = self.open_flags(access | OPEN_LEASE | if create { OPEN_CREATE } else { 0 })?;
        if !file_metadata(&file)?.is_file() {
            return Err(Error::Invalid);
        }
        self.acquire_open_lease(&file)?;
        Ok(FileStore {
            file,
            location: self,
            parent_pending: create,
            lease: Lease::Open,
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
        let actual = file_metadata(&actual)?;
        let expected = file_metadata(file)?;
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
        let actual = file_metadata(&actual_file)?;
        let expected = file_metadata(file)?;
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
    fn scratch(&self, make_identity: impl FnOnce() -> Result<[u8; 16]>) -> Result<Self> {
        let suffix = b".tmp\0";
        let mut name = Self::copy_path(
            self.name.allocator().clone(),
            b".wdb-compact-",
            2 * 16 + suffix.len(),
        )?;
        for byte in make_identity()? {
            for n in [byte >> 4, byte & 15] {
                name.push(b"0123456789abcdef"[n as usize]);
            }
        }
        name.extend_from_slice(suffix);
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
type Hook = Box<dyn FnMut(CompactionEvent) -> Result<()> + Send, crate::AllocatorHandle>;
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
    compact_file_impl(path, &Memory::new(limits)?, Observer::default())
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
    compact_file_impl(path, &Memory::new(limits)?, observer)
}
fn compact_file_impl(path: &Path, mem: &Memory, observer: Observer) -> Result<CompactionStats> {
    let original = Location::new(path)?;
    let source = original.duplicate()?.open_mode(false, true)?;
    let old_file = retry(NativeIoOperation::DuplicateHandle, || {
        source.file.try_clone()
    })?;
    // Replacing one name cannot preserve a hard-link alias's writer authority.
    if file_metadata(&old_file)?.nlink() != 1 {
        return Err(Error::Invalid);
    }
    let (mut storage, head, next_collection, retention) = Storage::open(
        Observed {
            file: source,
            observer: observer.clone(),
            target: false,
        },
        mem,
    )?;
    storage.release_serialization_caches();
    let scratch = original.scratch(identity)?;
    let target = scratch.duplicate()?.open_mode(true, true)?;
    let new_file = retry(NativeIoOperation::DuplicateHandle, || {
        target.file.try_clone()
    })?;
    let prepared = storage.encode_compact_copy(
        Observed {
            file: target,
            observer: observer.clone(),
            target: true,
        },
        mem,
        &head,
        next_collection,
        &retention,
    );
    // The source backend still owns the lease while verification walks the
    // newly serialized graph. The offline copy no longer needs the old graph.
    drop(head);
    drop(retention);
    let (mut replacement, stats) = prepared
        .and_then(|(mut copy, stats)| {
            copy.verify(mem)?;
            Ok((copy, stats))
        })
        .map_err(|e| {
            if e == Error::OutcomeUnknown {
                Error::Io
            } else {
                e
            }
        })?;
    observer.call(CompactionStep::CopyMetadata, || {
        sys::copy_metadata(&old_file, &new_file, mem)
    })?;
    observer.call(CompactionStep::SyncMetadata, || full_sync(&new_file))?;
    // Metadata APIs can affect data interpretation (e.g. compression xattrs).
    // Re-verify after every mutation, before publishing a directory entry.
    replacement.verify(mem)?;
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
    any(
        target_os = "macos",
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    )
))]
mod tests {
    use super::*;

    #[test]
    fn dropping_open_store_releases_lease_with_an_inherited_descriptor() {
        let path =
            std::env::temp_dir().join(format!("weekend-open-inherited-{}.db", std::process::id()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let cleanup = Cleanup(path);
        let store = FileStore::create(&cleanup.0).unwrap();
        // Like a descriptor inherited during fork, this shares the open file description.
        let inherited = store.file.try_clone().unwrap();
        assert_eq!(FileStore::open(&cleanup.0).err(), Some(Error::Busy));
        drop(store);
        let reopened = FileStore::open(&cleanup.0).unwrap();
        assert!(inherited.metadata().is_ok());
        drop(reopened);
    }

    #[test]
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
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

#[cfg(test)]
mod decision_tests {
    use super::*;

    pub(super) struct PathFixture(pub(super) std::path::PathBuf);

    impl PathFixture {
        pub(super) fn new(case: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("weekend-path-{case}-{}", std::process::id()));
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for PathFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn file_compaction_verifies_without_holding_copy_graphs_or_releasing_leases() {
        use std::{
            fs,
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            },
        };

        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        const PAYLOAD: usize = 512 * 1024;
        let directory =
            std::env::temp_dir().join(format!("weekend-compact-lifetimes-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let fixture = Fixture(directory);
        let path = fixture.0.join("store.wdb");
        let limits = Limits::default();
        {
            let db = crate::Database::create_file(&path, limits).unwrap();
            let collection = db.create_collection().unwrap();
            let mut draft = db.begin();
            draft
                .put(
                    collection,
                    b"value",
                    &db.bytes(&vec![0x5a; PAYLOAD]).unwrap(),
                )
                .unwrap();
            draft.commit().unwrap();
            db.retain_snapshot(&db.snapshot()).unwrap();
        }

        let memory = Memory::new(limits).unwrap();
        let observed_memory = memory.clone();
        let observed_path = path.clone();
        let verifications = Arc::new(AtomicUsize::new(0));
        let observed_verifications = verifications.clone();
        let mut check_next_read = true;
        let hook = move |event: CompactionEvent| {
            if event.step == CompactionStep::SyncMetadata && event.after {
                check_next_read = true;
            }
            if event.step == CompactionStep::TargetRead && !event.after && check_next_read {
                check_next_read = false;
                let live = observed_memory.usage().0;
                assert!(
                    live < PAYLOAD,
                    "verification started with {live} charged bytes"
                );
                assert!(matches!(FileStore::open(&observed_path), Err(Error::Busy)));
                let scratch = fs::read_dir(observed_path.parent().unwrap())
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .starts_with(".wdb-compact-")
                    })
                    .unwrap();
                assert!(matches!(FileStore::open(scratch), Err(Error::Busy)));
                observed_verifications.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        };
        let hook: Hook = memory.boxed(hook).unwrap();
        let observer = Observer {
            hook: Some(memory.shared(std::sync::Mutex::new(hook)).unwrap()),
        };
        compact_file_impl(&path, &memory, observer).unwrap();
        assert_eq!(verifications.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn pathname_and_claimed_read_only_controls() {
        for name in ["", ".", "..", "end/", "a\0b"] {
            assert_eq!(Location::new(Path::new(name)).err(), Some(Error::Invalid));
        }
        let long = "x".repeat(1024 * 1024);
        assert!(Location::new(Path::new(&long)).is_ok());
        assert_eq!(
            Location::new(Path::new(&(long + "x"))).err(),
            Some(Error::Limit)
        );
        let path = std::env::temp_dir().join(format!(
            "weekend-native-decisions-{}.db",
            std::process::id()
        ));
        let mut file = FileStore::create(&path).unwrap();
        assert_eq!(file.length(), Err(Error::Io));
        file.lock().unwrap();
        assert_eq!(file.lock(), Err(Error::Busy));
        file.write(0, b"abc").unwrap();
        file.sync().unwrap();
        file.sync().unwrap();
        file.unlock();
        assert_eq!(file.length(), Err(Error::Io));
        drop(file);
        let mut file = FileStore::open_read_only(&path).unwrap();
        file.lock().unwrap();
        assert_eq!(file.length(), Ok(3));
        let mut bytes = [0; 3];
        assert_eq!(file.read(0, &mut bytes), Ok(3));
        assert_eq!(&bytes, b"abc");
        assert_eq!(file.write(0, b"x"), Err(Error::Io));
        file.unlock();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(file.lock(), Err(Error::Busy));
        assert_eq!(file.length(), Err(Error::Io));
    }

    #[test]
    fn invalid_names_stop_before_the_selected_allocator() {
        use crate::memory::test_allocator::TestAllocator;
        use std::sync::Arc;

        let allocator = Arc::new(TestAllocator::default());
        let handle = allocator.handle();
        let mut max = "x".repeat(1024 * 1024);
        let (location, requests) =
            allocator.count_with_refusal(2, || Location::new_in(Path::new(&max), handle.clone()));
        let location = location.unwrap();
        assert_eq!(requests, 2);
        assert_eq!(location.name.len(), max.len() + 1);
        assert_eq!(location.name.last(), Some(&0));
        assert_eq!(allocator.last_request_size(), max.len() + 1);
        let parent = location.directory.metadata().unwrap();
        let cwd = std::fs::metadata(".").unwrap();
        assert_eq!((parent.dev(), parent.ino()), (cwd.dev(), cwd.ino()));
        drop(location);
        max.push('x');

        let live = allocator.live_bytes();
        for (name, error) in [
            ("", Error::Invalid),
            ("a/", Error::Invalid),
            (".", Error::Invalid),
            ("..", Error::Invalid),
            ("a\0b", Error::Invalid),
            (max.as_str(), Error::Limit),
        ] {
            let (result, requests) = allocator
                .count_with_refusal(0, || Location::new_in(Path::new(name), handle.clone()));
            assert_eq!(result.err(), Some(error));
            assert_eq!(requests, 0);
            assert_eq!(allocator.live_bytes(), live);
        }
    }

    #[test]
    fn parent_and_leaf_refusal_leave_create_and_open_retriable() {
        use super::super::diagnostics::capture;
        use crate::memory::test_allocator::TestAllocator;
        use std::sync::Arc;

        let fixture = PathFixture::new("allocation");
        let path = fixture.0.join("store λ.db");
        let leaf = "store λ.db\0".as_bytes();
        let parent_bytes = fixture.0.as_os_str().as_bytes().len() + 1;
        let allocator = Arc::new(TestAllocator::default());
        let handle = allocator.handle();
        let live = allocator.live_bytes();
        for create in [true, false] {
            for (allowed, size) in [(0, parent_bytes), (1, leaf.len())] {
                let (result, requests) = allocator.count_with_refusal(allowed, || {
                    capture(|| FileStore::open_in(&path, create, handle.clone()))
                });
                let failure = result.err().unwrap();
                assert_eq!(failure.error, Error::OutOfMemory);
                assert_eq!(failure.native, None);
                assert_eq!(requests, allowed + 1);
                assert_eq!(allocator.last_request_size(), size);
                assert_eq!(allocator.live_bytes(), live);
                if create {
                    assert!(std::fs::read_dir(&fixture.0).unwrap().next().is_none());
                } else {
                    assert_eq!(std::fs::read(&path).unwrap(), b"kept");
                }
            }
            let (store, requests) = allocator
                .count_with_refusal(2, || FileStore::open_in(&path, create, handle.clone()));
            let mut store = store.unwrap();
            assert_eq!(requests, 2);
            assert_eq!(store.location.name.as_slice(), leaf);
            assert_eq!(allocator.live_bytes(), live + leaf.len());
            store.lock().unwrap();
            if create {
                assert_eq!(store.write(0, b"kept"), Ok(4));
                store.sync().unwrap();
            } else {
                let mut bytes = [0; 4];
                assert_eq!(store.read(0, &mut bytes), Ok(4));
                assert_eq!(&bytes, b"kept");
            }
            drop(store);
            assert_eq!(allocator.live_bytes(), live);
        }

        let location = Location::new_in(&path, handle.clone()).unwrap();
        let baseline = allocator.live_bytes();
        let (duplicate, requests) = allocator.count_with_refusal(0, || location.duplicate());
        assert_eq!(duplicate.err(), Some(Error::OutOfMemory));
        assert_eq!(requests, 1);
        assert_eq!(allocator.last_request_size(), leaf.len());
        assert_eq!(allocator.live_bytes(), baseline);
        let duplicate = allocator.with_refusal(1, || location.duplicate()).unwrap();
        assert_eq!(duplicate.name.as_slice(), leaf);
        assert_eq!(allocator.live_bytes(), baseline + leaf.len());
        assert_eq!(std::fs::read(&path).unwrap(), b"kept");
        drop(duplicate);
        drop(location);
        assert_eq!(allocator.live_bytes(), live);
    }

    #[test]
    fn scratch_uses_one_selected_reservation_and_stops_on_entropy_failure() {
        use super::super::diagnostics::capture;
        use crate::memory::test_allocator::TestAllocator;
        use std::sync::Arc;

        let fixture = PathFixture::new("scratch-allocation");
        let allocator = Arc::new(TestAllocator::default());
        let location = Location::new_in(&fixture.0.join("source"), allocator.handle()).unwrap();
        let live = allocator.live_bytes();
        let expected = b".wdb-compact-0102030405060708090a0b0c0d0e0f10.tmp\0";

        let (result, requests) = allocator.count_with_refusal(0, || {
            capture(|| location.scratch(|| panic!("entropy queried after refused allocation")))
        });
        let failure = result.err().unwrap();
        assert_eq!(failure.error, Error::OutOfMemory);
        assert_eq!(failure.native, None);
        assert_eq!(requests, 1);
        assert_eq!(allocator.last_request_size(), expected.len());
        assert_eq!(allocator.live_bytes(), live);

        let (result, requests) =
            allocator.count_with_refusal(1, || location.scratch(|| Err(Error::Io)));
        assert_eq!(result.err(), Some(Error::Io));
        assert_eq!(requests, 1);
        assert_eq!(allocator.live_bytes(), live);
        assert!(std::fs::read_dir(&fixture.0).unwrap().next().is_none());

        let (result, requests) = allocator.count_with_refusal(1, || {
            location.scratch(|| Ok(std::array::from_fn(|index| index as u8 + 1)))
        });
        let scratch = result.unwrap();
        assert_eq!(requests, 1);
        assert_eq!(allocator.last_request_size(), expected.len());
        assert_eq!(scratch.name.as_slice(), expected);
        assert_eq!(allocator.live_bytes(), live + expected.len());
        let parent = scratch.directory.metadata().unwrap();
        let actual = location.directory.metadata().unwrap();
        assert_eq!((parent.dev(), parent.ino()), (actual.dev(), actual.ino()));
        drop(scratch);
        assert_eq!(allocator.live_bytes(), live);
    }

    #[test]
    fn scratch_collision_and_unlink_preserve_native_operation_and_siblings() {
        use super::super::diagnostics::capture;
        use std::ffi::OsStr;

        let fixture = PathFixture::new("scratch-remove");
        let original = fixture.0.join("source");
        std::fs::write(&original, b"canonical").unwrap();
        let location = Location::new(&original).unwrap();
        let scratch = location.scratch(|| Ok([0xa5; 16])).unwrap();
        let scratch_path = fixture
            .0
            .join(OsStr::from_bytes(scratch.name.strip_suffix(&[0]).unwrap()));
        let mut target = scratch.duplicate().unwrap().open_mode(true, true).unwrap();
        target.lock().unwrap();
        assert_eq!(target.write(0, b"candidate"), Ok(9));
        target.sync().unwrap();
        drop(target);

        let collision = capture(|| scratch.duplicate()?.open_mode(true, true))
            .err()
            .unwrap();
        assert_eq!(collision.error, Error::AlreadyExists);
        let native = collision.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Create);
        assert_eq!(native.raw_os_error, Some(17)); // EEXIST
        assert_eq!(std::fs::read(&scratch_path).unwrap(), b"candidate");

        capture(|| scratch.remove()).unwrap();
        assert!(!scratch_path.exists());
        let missing = capture(|| scratch.remove()).unwrap_err();
        assert_eq!(missing.error, Error::NotFound);
        let native = missing.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Remove);
        assert_eq!(native.raw_os_error, Some(2)); // ENOENT
        assert_eq!(std::fs::read(&original).unwrap(), b"canonical");
    }

    #[test]
    fn native_positioned_io_distinguishes_progress_eof_and_refusal() {
        use super::super::diagnostics::capture;

        let fixture = PathFixture::new("positioned-io");
        let path = fixture.0.join("store");
        let mut writer = FileStore::create(&path).unwrap();
        writer.lock().unwrap();
        assert_eq!(writer.write(0, b"abcd"), Ok(4));
        assert_eq!(writer.write(1, b"XY"), Ok(2));
        writer.sync().unwrap();
        assert_eq!(writer.length(), Ok(4));
        drop(writer);

        let mut reader = FileStore::open_read_only(&path).unwrap();
        reader.lock().unwrap();
        let mut out = *b"????";
        assert_eq!(reader.read(1, &mut out), Ok(3));
        assert_eq!(&out, b"XYd?");
        assert_eq!(reader.read(4, &mut out), Ok(0));
        assert_eq!(&out, b"XYd?");

        let read = capture(|| reader.read(u64::MAX, &mut out)).unwrap_err();
        assert_eq!(read.error, Error::Io);
        let native = read.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Read);
        assert_eq!(native.raw_os_error, Some(22)); // EINVAL
        assert_eq!(&out, b"XYd?");

        let write = capture(|| reader.write(1, b"!")).unwrap_err();
        assert_eq!(write.error, Error::Io);
        let native = write.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Write);
        assert_eq!(native.raw_os_error, Some(9)); // EBADF
        assert_eq!(reader.length(), Ok(4));
        assert_eq!(std::fs::read(&path).unwrap(), b"aXYd");
    }

    #[test]
    fn native_path_errors_report_the_failed_operation() {
        let fixture = PathFixture::new("diagnostics");
        let file = fixture.0.join("file");
        std::fs::write(&file, b"kept").unwrap();
        for (create, path, error, operation, raw) in [
            (
                false,
                fixture.0.join("missing/leaf"),
                Error::NotFound,
                NativeIoOperation::OpenDirectory,
                2,
            ),
            (
                false,
                file.join("leaf"),
                Error::Invalid,
                NativeIoOperation::OpenDirectory,
                20,
            ),
            (
                false,
                fixture.0.join("missing"),
                Error::NotFound,
                NativeIoOperation::Open,
                2,
            ),
            (
                true,
                file.clone(),
                Error::AlreadyExists,
                NativeIoOperation::Create,
                17,
            ),
        ] {
            let result = if create {
                FileStore::create_with_diagnostics(&path)
            } else {
                FileStore::open_with_diagnostics(&path)
            };
            let failure = result.err().unwrap();
            assert_eq!(failure.error, error);
            let native = failure.native.unwrap();
            assert_eq!(native.operation, operation);
            assert_eq!(native.raw_os_error, Some(raw));
        }
        for path in [Path::new(""), fixture.0.as_path()] {
            let failure = FileStore::open_read_only_with_diagnostics(path)
                .err()
                .unwrap();
            assert_eq!(failure.error, Error::Invalid);
            assert_eq!(failure.native, None);
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"kept");
    }

    #[test]
    fn all_io_requires_a_claim_and_failed_reacquisition_releases_the_inode() {
        let dir =
            std::env::temp_dir().join(format!("weekend-claim-decisions-{}", std::process::id()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        std::fs::create_dir(&dir).unwrap();
        let dir = Cleanup(dir);
        let path = dir.0.join("data");
        let moved = dir.0.join("moved");
        let reject_io = |file: &mut FileStore| {
            let mut byte = *b"?";
            assert_eq!(file.length(), Err(Error::Io));
            assert_eq!(file.read(0, &mut byte), Err(Error::Io));
            assert_eq!(byte, *b"?");
            assert_eq!(file.write(0, b"x"), Err(Error::Io));
            assert_eq!(file.sync(), Err(Error::Io));
        };
        let mut first = FileStore::create(&path).unwrap();
        reject_io(&mut first);
        first.lock().unwrap();
        assert_eq!(first.lock(), Err(Error::Busy));
        assert_eq!(first.write(0, b"a"), Ok(1));
        first.sync().unwrap();
        assert!(!first.parent_pending);
        first.unlock();
        assert!(matches!(first.lease, Lease::Released));
        reject_io(&mut first);
        let competing = FileStore::open_read_only(&path).unwrap();
        assert_eq!(first.lock(), Err(Error::Busy));
        reject_io(&mut first);
        assert!(matches!(FileStore::open(&path), Err(Error::Busy)));
        drop(competing);
        first.lock().unwrap();
        assert_eq!(first.length(), Ok(1));
        let mut byte = [0];
        assert_eq!(first.read(0, &mut byte), Ok(1));
        assert_eq!(byte, [b'a']);
        first.unlock();

        std::fs::rename(&path, &moved).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        assert_eq!(first.lock(), Err(Error::Busy));
        reject_io(&mut first);
        assert!(matches!(first.lease, Lease::Released));
        // The stale adapter stays alive across this open of its old inode.
        let old = FileStore::open(&moved).unwrap();
        assert_eq!(std::fs::read(&moved).unwrap(), b"a");
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        drop(old);
        drop(first);
    }

    #[test]
    fn creation_sync_retries_every_barrier_after_each_possible_failure() {
        use std::cell::RefCell;

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Barrier {
            File,
            Parent,
        }

        let fixture = PathFixture::new("creation-sync");
        let sequence = [Barrier::File, Barrier::Parent, Barrier::File];
        let sync = |store: &mut FileStore, failure: Option<usize>, expected: &[Barrier]| {
            let calls = RefCell::new(Vec::new());
            let record = |barrier| {
                let mut calls = calls.borrow_mut();
                let index = calls.len();
                calls.push(barrier);
                if failure == Some(index) {
                    Err(Error::Busy)
                } else {
                    Ok(())
                }
            };
            let file_fd = store.file.as_raw_fd();
            let parent_fd = store.location.directory.as_raw_fd();
            let result = store.sync_with(
                |file| {
                    assert_eq!(file.as_raw_fd(), file_fd);
                    record(Barrier::File)
                },
                |location| {
                    assert_eq!(location.directory.as_raw_fd(), parent_fd);
                    record(Barrier::Parent)
                },
            );
            assert_eq!(calls.into_inner(), expected);
            assert_eq!(result, failure.map_or(Ok(()), |_| Err(Error::Busy)));
        };

        for failure in 0..sequence.len() {
            let path = fixture.0.join(format!("created-{failure}"));
            let mut store = FileStore::create(&path).unwrap();
            store.lock().unwrap();
            sync(&mut store, Some(failure), &sequence[..=failure]);
            assert!(store.parent_pending);
            sync(&mut store, None, &sequence);
            assert!(!store.parent_pending);
            sync(&mut store, Some(0), &[Barrier::File]);
            assert!(!store.parent_pending);
            sync(&mut store, None, &[Barrier::File]);
            drop(store);

            let mut reopened = FileStore::open(&path).unwrap();
            reopened.lock().unwrap();
            sync(&mut reopened, None, &[Barrier::File]);
            assert!(!reopened.parent_pending);
        }
    }

    #[test]
    fn namespace_rechecks_distinguish_aliases_replacement_and_removal() {
        let path =
            std::env::temp_dir().join(format!("weekend-name-decisions-{}.db", std::process::id()));
        let alias = path.with_extension("alias");
        let store = FileStore::create(&path).unwrap();
        let location = Location::new(&path).unwrap();
        assert_eq!(location.matches(&store.file), Ok(()));
        let absent = Location::new(&alias).unwrap();
        assert_eq!(location.exchange(&absent), Err(Error::NotFound));
        assert_eq!(absent.remove(), Err(Error::NotFound));
        assert_eq!(location.matches(&store.file), Ok(()));
        std::fs::hard_link(&path, &alias).unwrap();
        assert_eq!(location.check_lease(&store.file), Ok(()));
        assert_eq!(location.matches(&store.file), Err(Error::Invalid));
        std::fs::remove_file(&alias).unwrap();
        assert_eq!(location.matches(&store.file), Ok(()));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(location.check_lease(&store.file), Err(Error::Busy));
        assert_eq!(location.matches(&store.file), Err(Error::NotFound));
        std::fs::write(&path, b"replacement").unwrap();
        assert_eq!(location.check_lease(&store.file), Err(Error::Busy));
        assert_eq!(location.matches(&store.file), Err(Error::Invalid));
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(location.check_lease(&store.file), Err(Error::Busy));
        assert_eq!(location.matches(&store.file), Err(Error::Invalid));
        std::fs::remove_dir(&path).unwrap();
        std::os::unix::fs::symlink(&alias, &path).unwrap();
        assert_eq!(location.check_lease(&store.file), Err(Error::Invalid));
        assert_eq!(location.matches(&store.file), Err(Error::Invalid));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn interrupted_retry_can_finish_successfully_without_diagnostics() {
        let mut calls = 0;
        let result = super::super::diagnostics::capture(|| {
            retry(NativeIoOperation::Read, || {
                calls += 1;
                if calls < 3 {
                    Err(io::Error::from(io::ErrorKind::Interrupted))
                } else {
                    Ok(9)
                }
            })
        });
        assert_eq!(result, Ok(9));
        assert_eq!(calls, 3);
    }
}

#[cfg(test)]
mod metadata_decisions {
    use super::decision_tests::PathFixture;
    use super::*;
    use crate::{Database, storage::with_storage_diagnostics};
    use std::{cell::Cell, fs};

    #[derive(Clone, Copy)]
    struct Probe {
        fail: Option<(usize, i32)>,
        calls: usize,
    }

    thread_local! {
        static STAT: Cell<Option<Probe>> = const { Cell::new(None) };
    }

    pub(super) fn before_read() -> io::Result<()> {
        STAT.with(|slot| {
            if let Some(mut probe) = slot.get() {
                let call = probe.calls;
                probe.calls += 1;
                slot.set(Some(probe));
                if let Some((index, code)) = probe.fail
                    && call == index
                {
                    return Err(io::Error::from_raw_os_error(code));
                }
            }
            Ok(())
        })
    }

    fn probing<T>(fail: Option<(usize, i32)>, action: impl FnOnce() -> T) -> (T, usize) {
        struct Reset(Option<Probe>);
        impl Drop for Reset {
            fn drop(&mut self) {
                STAT.set(self.0);
            }
        }

        let _reset = Reset(STAT.replace(Some(Probe { fail, calls: 0 })));
        let result = action();
        (result, STAT.get().unwrap().calls)
    }

    fn require_metadata_error<T>(result: std::result::Result<T, super::super::StorageFailure>) {
        let failure = result.err().expect("metadata read must fail");
        assert_eq!(failure.error, Error::Io);
        let native = failure.native.unwrap();
        assert_eq!(native.operation, NativeIoOperation::Metadata);
        assert_eq!(native.raw_os_error, Some(5)); // EIO
    }

    fn on_test_directory(case: &str, test: impl FnOnce(&Path)) {
        let dir = PathFixture::new(case);
        test(&dir.0);
    }

    #[test]
    fn claimed_length_retries_interruption_and_preserves_terminal_metadata_error() {
        on_test_directory("length", |dir| {
            let path = dir.join("source");
            fs::write(&path, b"kept").unwrap();
            let mut store = FileStore::open_read_only(&path).unwrap();
            store.lock().unwrap();
            let (error, calls) =
                probing(Some((0, 5)), || with_storage_diagnostics(|| store.length()));
            require_metadata_error(error);
            assert_eq!(calls, 1);
            let (length, calls) = probing(Some((0, 4)), || store.length()); // EINTR
            assert_eq!(length, Ok(4));
            assert_eq!(calls, 2);
            assert_eq!(store.length(), Ok(4));
        });
    }

    #[test]
    fn metadata_failure_at_open_or_claim_does_not_leave_a_lease() {
        on_test_directory("lease", |dir| {
            let path = dir.join("source");
            fs::write(&path, b"unchanged").unwrap();

            // A regular-file check, the reopened name, then the held file.
            let (store, calls) = probing(None, || FileStore::open(&path));
            drop(store.unwrap());
            assert_eq!(calls, 3);
            for fail in 0..3 {
                let (result, calls) =
                    probing(Some((fail, 5)), || FileStore::open_with_diagnostics(&path));
                require_metadata_error(result);
                assert_eq!(calls, fail + 1);
                drop(FileStore::open(&path).unwrap());
            }

            for fail in 0..2 {
                let mut store = FileStore::open(&path).unwrap();
                let (result, calls) = probing(Some((fail, 5)), || {
                    with_storage_diagnostics(|| store.lock())
                });
                require_metadata_error(result);
                assert_eq!(calls, fail + 1);
                assert_eq!(store.length(), Err(Error::Io));
                // The failed adapter is still alive. Its lock must be gone.
                drop(FileStore::open(&path).unwrap());
            }
            assert_eq!(fs::read(&path).unwrap(), b"unchanged");
        });
    }

    #[test]
    fn compaction_metadata_read_refusals_preserve_canonical_and_retained_images() {
        on_test_directory("compaction", |dir| {
            let limits = Limits::default();
            let source = dir.join("source");
            let db = Database::create_file(&source, limits).unwrap();
            let c = db.create_collection().unwrap();
            for n in [10, 20] {
                let mut tx = db.begin();
                tx.put(c, b"key", &db.integer(n).unwrap()).unwrap();
                tx.commit().unwrap();
                if n == 10 {
                    db.retain_snapshot(&db.snapshot()).unwrap();
                }
            }
            let revision = db.snapshot().revision();
            let retained_revision = db.retained_snapshot(1).unwrap().revision();
            drop(db);
            let before = fs::read(&source).unwrap();
            let reference = dir.join("reference");
            fs::write(&reference, &before).unwrap();
            let (stats, metadata_calls) =
                probing(None, || Database::compact_file(&reference, limits));
            stats.unwrap();
            let compacted = fs::read(&reference).unwrap();
            assert_ne!(compacted, before);
            assert!(metadata_calls > 8);

            // The source link count follows the three source-open reads.
            // The final four reads check the two name/held-file pairs.
            for fail in [
                3,
                metadata_calls - 4,
                metadata_calls - 3,
                metadata_calls - 2,
                metadata_calls - 1,
            ] {
                let attempt = dir.join(format!("attempt-{fail}"));
                fs::create_dir(&attempt).unwrap();
                let path = attempt.join("canonical");
                fs::write(&path, &before).unwrap();
                let inode = fs::metadata(&path).unwrap().ino();
                let (result, calls) = probing(Some((fail, 5)), || {
                    Database::compact_file_with_diagnostics(&path, limits)
                });
                require_metadata_error(result);
                assert_eq!(calls, fail + 1);
                assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
                assert_eq!(fs::read(&path).unwrap(), before);

                let mut names = fs::read_dir(&attempt)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|name| name != &path);
                let scratch = if fail == 3 {
                    None
                } else {
                    let name = names.next().unwrap();
                    assert_eq!(fs::read(&name).unwrap(), compacted);
                    Some(name)
                };
                assert!(names.next().is_none());

                for retry in [false, true] {
                    if retry {
                        Database::compact_file(&path, limits).unwrap();
                        assert_eq!(fs::read(&path).unwrap(), compacted);
                        if let Some(name) = &scratch {
                            assert_eq!(fs::read(name).unwrap(), compacted);
                        }
                    }
                    let db = Database::open_file(&path, limits).unwrap();
                    let c = db.collection_id(1);
                    let current = db.snapshot();
                    let retained = db.retained_snapshot(1).unwrap();
                    assert_eq!(current.revision(), revision);
                    assert_eq!(retained.revision(), retained_revision);
                    assert_eq!(current.get(c, b"key").unwrap().unwrap().as_i64(), Ok(20));
                    assert_eq!(retained.get(c, b"key").unwrap().unwrap().as_i64(), Ok(10));
                }
            }
        });
    }
}
