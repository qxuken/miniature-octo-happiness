//! Experimental byte storage, publication protocol, and power-loss simulator.
//! Native file adapters preserve the modeled ordering; hardware assumptions
//! remain conditional and are documented in DESIGN.md.
use crate::memory::ChargedVec;
mod codec;
pub(crate) mod diagnostics;
pub use diagnostics::capture as with_native_diagnostics;
pub use diagnostics::{NativeIoCategory, NativeIoDiagnostic, NativeIoOperation, StorageFailure};
mod inspection;
mod logical;
pub use inspection::{ImageInfo, RetainedInfo, backup_on, inspect_on};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub use inspection::{backup_file, inspect_file, restore_file};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub use logical::{export_logical_file, import_logical_file};
pub use logical::{export_logical_on, import_logical_on};
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod file;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", target_os = "linux", test))]
mod replacement;
mod salvage;
mod simulator;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub use salvage::salvage_file;
pub use salvage::{SalvageReport, salvage_on};
#[cfg(windows)]
mod windows;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use file::FileStore;
#[cfg(target_os = "macos")]
pub use file::FileStore as MacFileStore;
#[cfg(target_os = "linux")]
pub use file::FileStore as LinuxFileStore;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use file::compact_file;
#[cfg(all(
    any(target_os = "macos", target_os = "linux"),
    any(test, feature = "fault-injection")
))]
pub use file::{CompactionEvent, CompactionStep, compact_file_with_hook};
#[cfg(target_os = "linux")]
pub(crate) use linux::identity as file_identity;
#[cfg(target_os = "macos")]
pub(crate) use macos::identity as file_identity;
pub use simulator::{Crash, Fault, IoEvent, IoKind, MemoryStore, SimulatedDisk};
#[cfg(windows)]
pub use windows::FileStore;
#[cfg(windows)]
pub use windows::FileStore as WindowsFileStore;
#[cfg(windows)]
pub(crate) use windows::identity as file_identity;

use crate::{
    Error, Result,
    engine::{Head, Retention},
    memory::{Memory, Shared},
};
use codec::{CacheEntry, DataEntry, Slot};

pub const REGION: usize = 4096;
pub const APPEND_START: usize = 5 * REGION;
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactionStats {
    pub before_bytes: u64,
    pub after_bytes: u64,
}

/// Cheap byte-store measurements, without a graph scan or reclaimable-size estimate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageSpace {
    pub revision: u64,
    pub physical_bytes: u64,
    pub committed_bytes: u64,
    pub next_append_offset: u64,
    pub capacity_bytes: u64,
    pub remaining_append_bytes: u64,
}

/// An exclusive writer lease covers recovery and publication. Successful sync
/// makes all preceding writes durable. Errors may have performed partial I/O.
/// Writes may be short, including zero. Adapters must protect independent 4096
/// byte regions: a torn write cannot damage another region or a safe append's
/// preceding region. A platform adapter must establish these assumptions.
/// Implementations may move between threads under the database's commit mutex.
/// Methods are serialized and must not re-enter their Database. A panic poisons
/// publication until the database is closed and recovered.
pub trait ByteStore: Send {
    fn lock(&mut self) -> Result<()>;
    fn unlock(&mut self);
    fn length(&mut self) -> Result<u64>;
    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize>;
    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize>;
    fn sync(&mut self) -> Result<()>;
}

pub(crate) struct Storage {
    backend: Box<dyn ByteStore>,
    identity: [u8; 16],
    revision: u64,
    end: u64,
    data_cache: ChargedVec<DataEntry>,
    cache: ChargedVec<CacheEntry>,
    poisoned: bool,
}
impl Drop for Storage {
    fn drop(&mut self) {
        self.backend.unlock();
    }
}
impl Storage {
    pub(crate) fn space(&mut self) -> Result<StorageSpace> {
        self.guard()?;
        let physical_bytes = self.backend.length()?;
        if physical_bytes < self.end || physical_bytes > MAX_IMAGE_BYTES as u64 {
            self.poisoned = true;
            return Err(Error::Corrupt);
        }
        let next_append_offset = codec::align(physical_bytes)?;
        Ok(StorageSpace {
            revision: self.revision,
            physical_bytes,
            committed_bytes: self.end,
            next_append_offset,
            capacity_bytes: MAX_IMAGE_BYTES as u64,
            remaining_append_bytes: MAX_IMAGE_BYTES as u64 - next_append_offset,
        })
    }
    #[cfg(feature = "replication")]
    pub(crate) fn identity(&self) -> [u8; 16] {
        self.identity
    }
    #[cfg(feature = "replication")]
    pub(crate) fn image_bytes(&mut self) -> &mut dyn ByteStore {
        &mut *self.backend
    }
    pub(crate) fn compact<B: ByteStore + 'static>(
        &mut self,
        destination: B,
        mem: &Memory,
        head: &Head,
        next_collection: u64,
        retention: &Retention,
    ) -> Result<(Self, CompactionStats)> {
        self.guard()?;
        let before_bytes = self.backend.length()?;
        if before_bytes < self.end || before_bytes > MAX_IMAGE_BYTES as u64 {
            return Err(Error::Corrupt);
        }
        // Empty caches are essential: copy exactly the current and explicitly
        // retained graphs, never the source's obsolete serialization cache.
        let mut copy = Self::create(
            destination,
            mem,
            head,
            next_collection,
            self.identity,
            retention,
        )?;
        copy.verify(mem)?;
        let stats = CompactionStats {
            before_bytes,
            after_bytes: copy.end,
        };
        Ok((copy, stats))
    }
    pub(crate) fn verify(&mut self, mem: &Memory) -> Result<()> {
        let verified = codec::recover(&mut *self.backend, mem)?;
        if verified.identity != self.identity
            || verified.slot.revision != self.revision
            || verified.slot.end != self.end
            || !verified.witnessed
        {
            return Err(Error::Corrupt);
        }
        Ok(())
    }
    fn locked<B: ByteStore + 'static>(backend: B, mem: &Memory) -> Result<Self> {
        let mut backend: Box<dyn ByteStore> = mem.boxed(backend)?;
        backend.lock()?;
        Ok(Self {
            backend,
            identity: [0; 16],
            revision: 0,
            end: 0,
            data_cache: ChargedVec::new(),
            cache: ChargedVec::new(),
            poisoned: false,
        })
    }
    pub(crate) fn create<B: ByteStore + 'static>(
        backend: B,
        mem: &Memory,
        head: &Head,
        next_collection: u64,
        identity: [u8; 16],
        retention: &Retention,
    ) -> Result<Self> {
        let mut storage = Self::locked(backend, mem)?;
        if identity == [0; 16] {
            return Err(Error::Invalid);
        }
        if storage.backend.length()? != 0 {
            return Err(Error::Invalid);
        }
        storage.identity = identity;
        // Prepare everything before modifying the previously empty byte store.
        let candidate = codec::encode(
            mem,
            head,
            next_collection,
            identity,
            APPEND_START as u64,
            &[],
            &[],
            retention,
        )?;
        let header = codec::header(identity);
        write_all(&mut *storage.backend, 0, &header)?;
        let zero = [0; REGION];
        for i in 1..5 {
            write_all(&mut *storage.backend, (i * REGION) as u64, &zero)?;
        }
        storage.publish(candidate, head.revision)?;
        Ok(storage)
    }
    pub(crate) fn open<B: ByteStore + 'static>(
        backend: B,
        mem: &Memory,
    ) -> Result<(Self, Shared<Head>, u64, Retention)> {
        let mut storage = Self::locked(backend, mem)?;
        let recovered = codec::recover(&mut *storage.backend, mem)?;
        // Reopening in the same process can see a complete but unsynced root
        // through volatile/page-cache bytes. Stabilize the verified generation,
        // then witness it before exposing that recovered head to the caller.
        storage.backend.sync().map_err(|_| Error::Io)?;
        let witness = codec::metadata(recovered.identity, recovered.slot, true);
        write_synced(
            &mut *storage.backend,
            ((3 + recovered.slot.index) * REGION) as u64,
            &witness,
        )
        .map_err(|_| Error::Io)?;
        storage.identity = recovered.identity;
        storage.revision = recovered.slot.revision;
        storage.end = recovered.slot.end;
        storage.data_cache = recovered.data_cache;
        storage.cache = recovered.cache;
        Ok((
            storage,
            recovered.head,
            recovered.next_collection,
            recovered.retention,
        ))
    }
    pub(crate) fn requires_recovery(&self) -> bool {
        self.poisoned
    }
    pub(crate) fn guard(&self) -> Result<()> {
        if self.poisoned {
            Err(Error::RecoveryRequired)
        } else {
            Ok(())
        }
    }
    pub(crate) fn persist(
        &mut self,
        mem: &Memory,
        head: &Head,
        next_collection: u64,
        retention: &Retention,
    ) -> Result<()> {
        self.guard()?;
        if self.revision.checked_add(1) != Some(head.revision) {
            return Err(Error::Invalid);
        }
        let length = match self.backend.length() {
            Ok(n) => n,
            Err(_) => {
                self.poisoned = true;
                return Err(Error::Io);
            }
        };
        if length < self.end || length > MAX_IMAGE_BYTES as u64 {
            self.poisoned = true;
            return Err(Error::Corrupt);
        }
        let start = codec::align(length)?;
        let candidate = codec::encode(
            mem,
            head,
            next_collection,
            self.identity,
            start,
            &self.cache,
            &self.data_cache,
            retention,
        )?;
        self.publish(candidate, head.revision)
    }
    fn publish(&mut self, candidate: codec::Candidate, revision: u64) -> Result<()> {
        let index = (revision & 1) as usize;
        let slot = Slot {
            index,
            revision,
            descriptor: candidate.descriptor,
            end: candidate.end,
        };
        let publication = codec::metadata(self.identity, slot, false);
        let witness = codec::metadata(self.identity, slot, true);
        write_synced(&mut *self.backend, candidate.start, &candidate.bytes)
            .map_err(|_| Error::Io)
            .inspect_err(|_| self.poisoned = true)?;
        // From the first slot-write attempt onward the caller cannot infer the
        // result from an error. The witness is a durable acknowledged-revision
        // lower bound, synchronized before the in-process head becomes visible.
        write_synced(
            &mut *self.backend,
            ((1 + index) * REGION) as u64,
            &publication,
        )
        .and_then(|_| write_synced(&mut *self.backend, ((3 + index) * REGION) as u64, &witness))
        .map_err(|_| Error::OutcomeUnknown)
        .inspect_err(|_| self.poisoned = true)?;
        self.revision = revision;
        self.end = candidate.end;
        self.data_cache = candidate.data_cache;
        self.cache = candidate.cache;
        Ok(())
    }
}
fn write_synced(backend: &mut dyn ByteStore, offset: u64, bytes: &[u8]) -> Result<()> {
    write_all(backend, offset, bytes)?;
    backend.sync()
}
pub(crate) fn read_all(
    backend: &mut dyn ByteStore,
    mut offset: u64,
    mut out: &mut [u8],
) -> Result<()> {
    while !out.is_empty() {
        let size = out.len().min(REGION);
        let n = backend.read(offset, &mut out[..size])?;
        if n == 0 || n > size {
            return Err(Error::Io);
        }
        offset = offset.checked_add(n as u64).ok_or(Error::Corrupt)?;
        out = &mut out[n..];
    }
    Ok(())
}
pub(crate) fn write_all(
    backend: &mut dyn ByteStore,
    mut offset: u64,
    mut bytes: &[u8],
) -> Result<()> {
    while !bytes.is_empty() {
        let size = bytes.len().min(REGION - (offset % REGION as u64) as usize);
        let n = backend.write(offset, &bytes[..size])?;
        if n == 0 || n > size {
            return Err(Error::Io);
        }
        offset = offset.checked_add(n as u64).ok_or(Error::Limit)?;
        bytes = &bytes[n..];
    }
    Ok(())
}

#[cfg(all(
    feature = "replication",
    any(test, target_os = "macos", target_os = "linux", windows)
))]
pub(crate) fn recover_replica(
    backend: &mut dyn ByteStore,
    mem: &Memory,
    identity: [u8; 16],
    revision: u64,
    length: u64,
) -> Result<codec::Recovered> {
    let recovered = codec::recover(backend, mem)?;
    // A shipped image has completed all barriers. Keep verification read-only
    // so it cannot modify an immutable installed image on restart.
    if recovered.identity != identity
        || recovered.slot.revision != revision
        || recovered.slot.end != length
        || !recovered.witnessed
    {
        return Err(Error::Corrupt);
    }
    Ok(recovered)
}

#[cfg(test)]
mod transfer_tests {
    use super::*;

    struct Progress {
        answer: Result<usize>,
        calls: Vec<(u64, usize)>,
    }
    impl ByteStore for Progress {
        fn lock(&mut self) -> Result<()> {
            Ok(())
        }
        fn unlock(&mut self) {}
        fn length(&mut self) -> Result<u64> {
            Ok(0)
        }
        fn sync(&mut self) -> Result<()> {
            Ok(())
        }
        fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
            self.calls.push((offset, out.len()));
            self.answer
        }
        fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
            self.calls.push((offset, bytes.len()));
            self.answer
        }
    }

    #[test]
    fn exact_transfers_reject_zero_oversized_and_failed_progress() {
        for writing in [false, true] {
            for answer in [Ok(0), Ok(1), Ok(3), Err(Error::Io)] {
                let mut store = Progress {
                    answer,
                    calls: Vec::new(),
                };
                let result = if writing {
                    write_all(&mut store, 0, &[0; 2])
                } else {
                    read_all(&mut store, 0, &mut [0; 2])
                };
                assert_eq!(
                    result,
                    if answer == Ok(1) {
                        Ok(())
                    } else {
                        Err(Error::Io)
                    }
                );
                assert_eq!(store.calls[0], (0, 2));
                assert_eq!(store.calls.len(), if answer == Ok(1) { 2 } else { 1 });
            }
        }
    }

    #[test]
    fn transfers_skip_empty_buffers_and_bound_offsets_and_regions() {
        let mut store = Progress {
            answer: Ok(1),
            calls: Vec::new(),
        };
        assert_eq!(read_all(&mut store, u64::MAX, &mut []), Ok(()));
        assert_eq!(write_all(&mut store, u64::MAX, &[]), Ok(()));
        assert!(store.calls.is_empty());
        assert_eq!(
            read_all(&mut store, u64::MAX, &mut [0]),
            Err(Error::Corrupt)
        );
        assert_eq!(write_all(&mut store, u64::MAX, &[0]), Err(Error::Limit));
        store.calls.clear();
        assert_eq!(write_all(&mut store, REGION as u64 - 1, &[1, 2, 3]), Ok(()));
        assert_eq!(
            store.calls,
            [
                (REGION as u64 - 1, 1),
                (REGION as u64, 2),
                (REGION as u64 + 1, 1)
            ]
        );
        store.calls.clear();
        store.answer = Ok(REGION);
        assert_eq!(read_all(&mut store, 0, &mut vec![0; 2 * REGION]), Ok(()));
        assert_eq!(store.calls, [(0, REGION), (REGION as u64, REGION)]);
    }
}
