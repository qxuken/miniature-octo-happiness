//! An immutable view with explicit full verification and no publication authority.
use super::*;
use crate::storage::codec::{self, paging::Source};

/// Opens a paged snapshot under an exclusive read-only lease. A matching durable
/// witness allows verification of multi-region byte payloads on first access.
/// Collections, revisions, graph edges and limits are checked before return.
/// An unwitnessed revision requires full verification during open.
pub struct ReadOnlyDatabase {
    db: Database,
    source: Source,
    identity: [u8; 16],
    slot: codec::Slot,
}

impl ReadOnlyDatabase {
    pub fn open_on<B: ByteStore + 'static>(backend: B, options: DatabaseOptions) -> Result<Self> {
        let mut db = Database::with_options(options)?;
        let source = Source::acquire(backend, &db.mem)?;
        let recovered = codec::recover_read_only(source.clone(), &db.mem, true)?;
        *db.state.get_mut().unwrap() = State {
            head: recovered.head,
            next_collection: recovered.next_collection,
            retention: recovered.retention,
        };
        Ok(Self {
            db,
            source,
            identity: recovered.identity,
            slot: recovered.slot,
        })
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn open_file(path: impl AsRef<std::path::Path>, options: DatabaseOptions) -> Result<Self> {
        options.limits.validate()?;
        let file = crate::storage::FileStore::open_read_only_in(path, options.allocator.clone())?;
        Self::open_on(file, options)
    }

    /// Recheck the entire current and retained graph under the same lease.
    pub fn check(&self) -> Result<()> {
        let checked = codec::recover_read_only(self.source.clone(), &self.db.mem, false)?;
        if checked.identity != self.identity || checked.slot != self.slot {
            return Err(crate::Error::Corrupt);
        }
        Ok(())
    }

    pub fn collection_id(&self, id: u64) -> CollectionId {
        self.db.collection_id(id)
    }
    pub fn snapshot(&self) -> Snapshot<'_> {
        self.db.snapshot()
    }
    pub fn retained_snapshot(&self, id: u64) -> Result<Snapshot<'_>> {
        self.db.retained_snapshot(id)
    }
    pub fn retained_ids(&self) -> Result<Vec<u64>> {
        self.db.retained_ids()
    }
    pub fn memory_usage(&self) -> crate::MemoryUsage {
        self.db.memory_usage()
    }
}
