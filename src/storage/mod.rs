//! Experimental byte storage, publication protocol, and power-loss simulator.
//! Native file adapters preserve the modeled ordering; hardware assumptions
//! remain conditional and are documented in DESIGN.md.
mod codec;
pub(crate) mod diagnostics;
pub use diagnostics::capture as with_native_diagnostics;
pub use diagnostics::capture as with_storage_diagnostics;
pub use diagnostics::{
    NativeIoCategory, NativeIoDiagnostic, NativeIoOperation, StorageFailure,
    VerificationDiagnostic, VerificationStage,
};
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
#[cfg(any(windows, test))]
mod windows_admission;
#[cfg(any(windows, test))]
mod windows_path;
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
use codec::{DataCache, Slot, ValueCache};

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
    backend: Box<dyn ByteStore, crate::AllocatorHandle>,
    identity: [u8; 16],
    revision: u64,
    end: u64,
    data_cache: Option<Shared<DataCache>>,
    cache: Option<Shared<ValueCache>>,
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
    pub(crate) fn identity(&self) -> [u8; 16] {
        self.identity
    }
    #[cfg(feature = "replication")]
    pub(crate) fn image_bytes(&mut self) -> &mut dyn ByteStore {
        &mut *self.backend
    }
    pub(crate) fn encode_compact_copy<B: ByteStore + 'static>(
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
        let (copy, after_bytes) = Self::create_compact(
            destination,
            mem,
            head,
            next_collection,
            self.identity,
            retention,
        )?;
        let stats = CompactionStats {
            before_bytes,
            after_bytes,
        };
        Ok((copy, stats))
    }
    pub(crate) fn create_compact<B: ByteStore + 'static>(
        destination: B,
        mem: &Memory,
        head: &Head,
        next_collection: u64,
        identity: [u8; 16],
        retention: &Retention,
    ) -> Result<(Self, u64)> {
        // Encode only the supplied graphs. Release the encoder caches before
        // independent verification builds its own graph.
        let mut copy = Self::create(destination, mem, head, next_collection, identity, retention)?;
        copy.release_serialization_caches();
        let bytes = copy.end;
        Ok((copy, bytes))
    }
    fn release_serialization_caches(&mut self) {
        self.data_cache = None;
        self.cache = None;
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
    pub(crate) fn locked<B: ByteStore + 'static>(backend: B, mem: &Memory) -> Result<Self> {
        let mut backend: Box<dyn ByteStore, crate::AllocatorHandle> = mem.boxed(backend)?;
        backend.lock()?;
        Ok(Self {
            backend,
            identity: [0; 16],
            revision: 0,
            end: 0,
            data_cache: None,
            cache: None,
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
        let candidate = codec::encode_fresh(mem, head, next_collection, identity, retention)?;
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
        // Build indexes once, before stabilization I/O; recovered entry order
        // remains authoritative. Subsequent candidates reuse or privately fork.
        let cache = mem.shared(ValueCache::from_entries(mem, recovered.cache)?)?;
        let data_cache = mem.shared(DataCache::from_entries(mem, recovered.data_cache)?)?;
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
        storage.data_cache = Some(data_cache);
        storage.cache = Some(cache);
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
            self.cache.as_ref(),
            self.data_cache.as_ref(),
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
        candidate
            .write_append(&mut *self.backend)
            .and_then(|_| self.backend.sync())
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
        self.data_cache = Some(candidate.data_cache);
        self.cache = Some(candidate.cache);
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

#[cfg(feature = "replication")]
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
        written: Vec<u8>,
        fail_at: Option<usize>,
        sync_answer: Result<()>,
        syncs: usize,
    }

    impl Progress {
        fn new(answer: Result<usize>) -> Self {
            Self {
                answer,
                calls: Vec::new(),
                written: Vec::new(),
                fail_at: None,
                sync_answer: Ok(()),
                syncs: 0,
            }
        }

        fn attempt(&mut self, offset: u64, size: usize) -> Result<usize> {
            let index = self.calls.len();
            self.calls.push((offset, size));
            if self.fail_at == Some(index) {
                Err(Error::Busy)
            } else {
                self.answer
            }
        }
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
            self.syncs += 1;
            self.sync_answer
        }
        fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
            let n = self.attempt(offset, out.len())?;
            for (index, byte) in out.iter_mut().take(n).enumerate() {
                *byte = (offset as u8).wrapping_add(index as u8);
            }
            Ok(n)
        }
        fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
            let n = self.attempt(offset, bytes.len())?;
            self.written.extend_from_slice(&bytes[..n.min(bytes.len())]);
            Ok(n)
        }
    }

    #[test]
    fn exact_transfers_reject_zero_oversized_and_failed_progress() {
        for writing in [false, true] {
            for answer in [Ok(0), Ok(1), Ok(3), Err(Error::Io)] {
                let mut store = Progress::new(answer);
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
    fn short_reads_fill_only_the_progressed_prefix_and_preserve_the_error() {
        let start = REGION as u64 - 1;
        let expected = [start as u8, (start + 1) as u8, (start + 2) as u8];
        let calls = [(start, 3), (start + 1, 2), (start + 2, 1)];
        let mut complete = Progress::new(Ok(1));
        let mut output = [9; 3];
        assert_eq!(read_all(&mut complete, start, &mut output), Ok(()));
        assert_eq!(output, expected);
        assert_eq!(complete.calls, calls);

        let mut refused = Progress::new(Ok(1));
        refused.fail_at = Some(1);
        let mut output = [9; 3];
        assert_eq!(read_all(&mut refused, start, &mut output), Err(Error::Busy));
        assert_eq!(output, [expected[0], 9, 9]);
        assert_eq!(refused.calls, calls[..2]);
    }

    #[test]
    fn write_synced_flushes_only_a_complete_transfer() {
        let start = REGION as u64 - 1;
        let input = [1, 7, 9];
        let calls = [(start, 1), (start + 1, 2), (start + 2, 1)];
        for sync_answer in [Ok(()), Err(Error::Busy)] {
            let mut store = Progress::new(Ok(1));
            store.sync_answer = sync_answer;
            assert_eq!(write_synced(&mut store, start, &input), sync_answer);
            assert_eq!(store.calls, calls);
            assert_eq!(store.written, input);
            assert_eq!(store.syncs, 1);
        }
        let mut refused = Progress::new(Ok(1));
        refused.fail_at = Some(1);
        assert_eq!(write_synced(&mut refused, start, &input), Err(Error::Busy));
        assert_eq!(refused.calls, calls[..2]);
        assert_eq!(refused.written, input[..1]);
        assert_eq!(refused.syncs, 0);
    }

    #[test]
    fn transfers_skip_empty_buffers_and_bound_offsets_and_regions() {
        let mut store = Progress::new(Ok(1));
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

#[cfg(test)]
mod decision_tests {
    use super::*;
    use crate::{
        Limits,
        memory::{ChargedVec, test_allocator::TestAllocator},
        tree::Map,
    };
    use std::sync::Arc;

    #[test]
    fn cold_open_cache_allocation_failures_precede_stabilization_and_release_the_lease() {
        fn graph(head: &Head, retention: &Retention) {
            let value = |head: &Head| {
                head.collections
                    .get(&1u64.to_le_bytes())
                    .unwrap()
                    .records
                    .get(b"k")
                    .unwrap()
                    .value
                    .as_bytes()
                    .unwrap()
                    .to_vec()
            };
            assert_eq!(head.revision, 4);
            assert_eq!(value(head), b"current");
            assert_eq!(retention.entries.len(), 1);
            assert_eq!(retention.entries[0].id, 1);
            assert_eq!(retention.entries[0].head.revision, 2);
            assert_eq!(value(&retention.entries[0].head), b"retained");
        }
        fn medium(disk: &SimulatedDisk, expected: &[u8]) {
            assert_eq!(disk.durable_image().unwrap(), expected);
            let mut probe = disk.clone();
            probe.lock().expect("failed open released its lease");
            let mut visible = vec![0; expected.len()];
            assert_eq!(probe.length().unwrap(), expected.len() as u64);
            read_all(&mut probe, 0, &mut visible).unwrap();
            probe.unlock();
            assert_eq!(visible, expected);
        }

        let original = {
            let disk = SimulatedDisk::new().unwrap();
            let db = crate::Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
            let collection = db.create_collection().unwrap();
            let mut tx = db.begin();
            tx.put(collection, b"k", &db.bytes(b"retained").unwrap())
                .unwrap();
            assert_eq!(tx.commit(), Ok(2));
            drop(tx);
            assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(1));
            let mut tx = db.begin();
            tx.put(collection, b"k", &db.bytes(b"current").unwrap())
                .unwrap();
            assert_eq!(tx.commit(), Ok(4));
            disk.durable_image().unwrap()
        };
        // Revision 4 uses parity 0. Preserve its already-published witness as
        // the byte oracle; do not reconstruct it with the code under test.
        let witness = 3 * REGION;
        assert_eq!(&original[witness..witness + 8], b"WDBACK01");
        assert_eq!(
            u64::from_le_bytes(original[witness + 32..witness + 40].try_into().unwrap()),
            4
        );
        let mut unwitnessed = original.clone();
        unwitnessed[witness..witness + REGION].fill(0);

        let allocator = Arc::new(TestAllocator::default());
        let box_requests = (0..4)
            .find(|&allowance| {
                let disk = SimulatedDisk::from_image(&unwitnessed).unwrap();
                let mem = allocator.memory(Limits::default());
                let result =
                    allocator.with_refusal(allowance, || Storage::locked(disk.clone(), &mem));
                let success = result.is_ok();
                if !success {
                    assert_eq!(result.as_ref().err(), Some(&Error::OutOfMemory));
                }
                drop(result);
                assert!(disk.trace().unwrap().is_empty());
                assert_eq!(mem.usage().0, 0);
                medium(&disk, &unwitnessed);
                success
            })
            .expect("backend box calibration completes");
        assert_eq!(box_requests, 1);

        let mut decode_trace = Vec::new();
        let decode_requests = (0..256)
            .find(|&allowance| {
                let mut disk = SimulatedDisk::from_image(&unwitnessed).unwrap();
                disk.lock().unwrap();
                let mem = allocator.memory(Limits::default());
                let result = allocator.with_refusal(allowance, || codec::recover(&mut disk, &mem));
                let success = result.is_ok();
                if let Ok(recovered) = &result {
                    assert!(!recovered.witnessed);
                    assert_eq!(recovered.next_collection, 2);
                    assert!(!recovered.cache.is_empty() && !recovered.data_cache.is_empty());
                    graph(&recovered.head, &recovered.retention);
                    decode_trace = disk.trace().unwrap();
                } else {
                    assert_eq!(result.as_ref().err(), Some(&Error::OutOfMemory));
                }
                drop(result);
                disk.unlock();
                assert_eq!(mem.usage().0, 0);
                medium(&disk, &unwitnessed);
                success
            })
            .expect("raw decoder calibration completes");

        let phases = ["VALUE index", "VALUE header", "DATA index", "DATA header"];
        // Identify each request at the constructors before applying its
        // calibrated offset to Storage::open; do not assume four failures are
        // later than the last decoder allocation merely from their read trace.
        for allowance in 0..=phases.len() {
            let mut disk = SimulatedDisk::from_image(&unwitnessed).unwrap();
            disk.lock().unwrap();
            let mem = allocator.memory(Limits::default());
            let recovered = codec::recover(&mut disk, &mem).unwrap();
            let mut phase = phases[0];
            let prepared = allocator.with_refusal(allowance, || {
                let values = ValueCache::from_entries(&mem, recovered.cache)?;
                phase = phases[1];
                let values = mem.shared(values)?;
                phase = phases[2];
                let data = DataCache::from_entries(&mem, recovered.data_cache)?;
                phase = phases[3];
                Ok((values, mem.shared(data)?))
            });
            if allowance < phases.len() {
                assert_eq!(phase, phases[allowance], "constructor request calibration");
                assert_eq!(prepared.as_ref().err(), Some(&Error::OutOfMemory));
            } else {
                assert!(prepared.is_ok());
            }
            drop(prepared);
            drop(recovered.head);
            drop(recovered.retention);
            disk.unlock();
            assert_eq!(mem.usage().0, 0);
            medium(&disk, &unwitnessed);
        }

        for allowance in 0..=phases.len() {
            let disk = SimulatedDisk::from_image(&unwitnessed).unwrap();
            let mem = allocator.memory(Limits::default());
            let result = allocator.with_refusal(box_requests + decode_requests + allowance, || {
                Storage::open(disk.clone(), &mem)
            });
            let trace = disk.trace().unwrap();
            if allowance < phases.len() {
                assert_eq!(
                    result.as_ref().err(),
                    Some(&Error::OutOfMemory),
                    "open {} preserves the core allocation error",
                    phases[allowance]
                );
                assert!(
                    trace
                        .iter()
                        .all(|e| matches!(e.kind, IoKind::Length | IoKind::Read)),
                    "open {} issued stabilization I/O",
                    phases[allowance]
                );
                assert_eq!(trace, decode_trace, "complete decode before cache failure");
            } else {
                let (storage, head, next_collection, retention) = result.as_ref().unwrap();
                assert_eq!(storage.revision, 4);
                assert_eq!(*next_collection, 2);
                graph(head, retention);
                assert_eq!(&trace[..decode_trace.len()], &decode_trace);
                assert_eq!(
                    &trace[decode_trace.len()..],
                    &[
                        IoEvent {
                            kind: IoKind::Sync,
                            offset: 0,
                            len: 0
                        },
                        IoEvent {
                            kind: IoKind::Write,
                            offset: witness as u64,
                            len: REGION
                        },
                        IoEvent {
                            kind: IoKind::Sync,
                            offset: 0,
                            len: 0
                        },
                    ]
                );
            }
            drop(result);
            assert_eq!(
                mem.usage().0,
                0,
                "failed/successful open reclaimed core charge"
            );
            medium(
                &disk,
                if allowance < phases.len() {
                    &unwitnessed
                } else {
                    &original
                },
            );
        }
        eprintln!(
            "cold-open calibration: box={box_requests}, decoder={decode_requests}, phases={phases:?}"
        );
    }

    fn fixture() -> (Storage, Memory, Shared<Head>, Retention, SimulatedDisk) {
        let mem = Memory::new(Limits::default()).unwrap();
        let head = mem
            .shared(Head {
                revision: 0,
                data_revision: 0,
                collections: Map::new(),
            })
            .unwrap();
        let retention = Retention {
            next_id: 1,
            entries: ChargedVec::new(),
        };
        let disk = SimulatedDisk::new().unwrap();
        let storage = Storage::create(disk.clone(), &mem, &head, 1, [1; 16], &retention).unwrap();
        (storage, mem, head, retention, disk)
    }

    #[test]
    fn compact_copy_releases_serialization_caches_before_verification() {
        let (source, mem, head, retention, original) = fixture();
        assert!(source.cache.is_some() && source.data_cache.is_some());
        let disk = SimulatedDisk::new().unwrap();
        let (mut copy, size) =
            Storage::create_compact(disk.clone(), &mem, &head, 1, source.identity, &retention)
                .unwrap();
        assert!(copy.cache.is_none() && copy.data_cache.is_none());
        assert_eq!(size as usize, disk.durable_image().unwrap().len());
        assert_eq!(
            disk.durable_image().unwrap(),
            original.durable_image().unwrap()
        );
        assert_eq!(copy.verify(&mem), Ok(()));
    }

    #[test]
    fn compact_copy_headroom_guard_rejects_poison_before_length() {
        let (mut storage, _, _, _, disk) = fixture();
        disk.arm(usize::MAX, Fault::Error);
        assert_eq!(
            storage.space().unwrap().physical_bytes,
            disk.durable_image().unwrap().len() as u64
        );
        assert_eq!(disk.trace().unwrap()[0].kind, IoKind::Length);

        storage.poisoned = true;
        disk.arm(usize::MAX, Fault::Error);
        assert_eq!(storage.space(), Err(Error::RecoveryRequired));
        assert!(disk.trace().unwrap().is_empty());
    }

    #[test]
    fn verification_envelope_predicates_have_independent_controls() {
        for changed in ["none", "identity", "revision", "end", "witness"] {
            let (mut storage, mem, _, _, disk) = fixture();
            match changed {
                "identity" => storage.identity = [2; 16],
                "revision" => storage.revision = 1,
                "end" => storage.end += REGION as u64,
                "witness" => {
                    storage
                        .backend
                        .write(3 * REGION as u64, &[0; REGION])
                        .unwrap();
                    storage.backend.sync().unwrap();
                }
                _ => {}
            }
            let before = disk.durable_image().unwrap();
            disk.arm(usize::MAX, Fault::Error);
            let expected = if changed == "none" {
                Ok(())
            } else {
                Err(Error::Corrupt)
            };
            assert_eq!(storage.verify(&mem), expected, "{changed}");
            #[cfg(feature = "replication")]
            assert_eq!(
                recover_replica(
                    &mut *storage.backend,
                    &mem,
                    storage.identity,
                    storage.revision,
                    storage.end
                )
                .map(|_| ()),
                expected,
                "replica {changed}"
            );
            assert_eq!(disk.durable_image().unwrap(), before);
            assert!(
                disk.trace()
                    .unwrap()
                    .iter()
                    .all(|event| matches!(event.kind, IoKind::Length | IoKind::Read))
            );
        }
    }

    #[test]
    fn persist_revision_mismatch_and_exhaustion_reject_before_io() {
        for (poisoned, current, proposed, expected) in [
            (false, 0, 0, Err(Error::Invalid)),
            (false, 0, 2, Err(Error::Invalid)),
            (false, u64::MAX, 0, Err(Error::Invalid)),
            (true, 0, 1, Err(Error::RecoveryRequired)),
            (false, 0, 1, Ok(())),
        ] {
            let (mut storage, mem, _, retention, disk) = fixture();
            // Model the private persist preconditions independently.
            storage.revision = current;
            storage.poisoned = poisoned;
            let head = mem
                .shared(Head {
                    revision: proposed,
                    data_revision: proposed,
                    collections: Map::new(),
                })
                .unwrap();
            let before = disk.durable_image().unwrap();
            disk.arm(usize::MAX, Fault::Error);
            assert_eq!(storage.persist(&mem, &head, 1, &retention), expected);
            if expected.is_err() {
                assert!(disk.trace().unwrap().is_empty());
                assert_eq!(disk.durable_image().unwrap(), before);
                assert_eq!(storage.requires_recovery(), poisoned);
            } else {
                assert_eq!(storage.revision, 1);
                assert!(storage.verify(&mem).is_ok());
            }
        }
    }
}
