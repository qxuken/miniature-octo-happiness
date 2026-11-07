//! Experimental byte storage, publication protocol, and power-loss simulator.
//! The macOS/Linux file adapters preserve the modeled ordering; hardware assumptions
//! remain conditional and are documented in DESIGN.md.
mod codec;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod file;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", target_os = "linux", test))]
mod replacement;
mod simulator;
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
    slot: usize,
    revision: u64,
    end: u64,
    data_cache: Vec<DataEntry>,
    cache: Vec<CacheEntry>,
    poisoned: bool,
}
impl Drop for Storage {
    fn drop(&mut self) {
        self.backend.unlock();
    }
}
impl Storage {
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
            || verified.slot.index != self.slot
            || verified.slot.end != self.end
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
            slot: 1,
            revision: 0,
            end: 0,
            data_cache: Vec::new(),
            cache: Vec::new(),
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
        // A compact image starts at an existing revision, which can be odd.
        // The v2 slot index must equal revision parity on its first publication.
        storage.slot = 1 - (head.revision & 1) as usize;
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
        write_all(
            &mut *storage.backend,
            ((3 + recovered.slot.index) * REGION) as u64,
            &witness,
        )
        .and_then(|_| storage.backend.sync())
        .map_err(|_| Error::Io)?;
        storage.identity = recovered.identity;
        storage.slot = recovered.slot.index;
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
        let index = 1 - self.slot;
        let slot = Slot {
            index,
            revision,
            descriptor: candidate.descriptor,
            end: candidate.end,
        };
        let publication = codec::metadata(self.identity, slot, false);
        let witness = codec::metadata(self.identity, slot, true);
        let before_publication = write_all(&mut *self.backend, candidate.start, &candidate.bytes)
            .and_then(|_| self.backend.sync());
        if before_publication.is_err() {
            self.poisoned = true;
            return Err(Error::Io);
        }
        // From the first slot-write attempt onward the caller cannot infer the
        // result from an error. The witness is a durable acknowledged-revision
        // lower bound, synchronized before the in-process head becomes visible.
        let publication_result = write_all(
            &mut *self.backend,
            ((1 + index) * REGION) as u64,
            &publication,
        )
        .and_then(|_| self.backend.sync())
        .and_then(|_| write_all(&mut *self.backend, ((3 + index) * REGION) as u64, &witness))
        .and_then(|_| self.backend.sync());
        if publication_result.is_err() {
            self.poisoned = true;
            return Err(Error::OutcomeUnknown);
        }
        self.slot = index;
        self.revision = revision;
        self.end = candidate.end;
        self.data_cache = candidate.data_cache;
        self.cache = candidate.cache;
        Ok(())
    }
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
fn write_all(backend: &mut dyn ByteStore, mut offset: u64, mut bytes: &[u8]) -> Result<()> {
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
