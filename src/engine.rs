use crate::memory::ChargedVec;
use crate::{
    AllocatorHandle, Error, Kind, Limits, Result, Value,
    memory::{Memory, Shared},
    storage::{ByteStore, Storage},
    tree::Map,
};
use std::{
    cell::Cell,
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard},
};

static NEXT_STORE: AtomicU64 = AtomicU64::new(1);
pub const MAX_RETAINED_SNAPSHOTS: usize = 64;
#[derive(Clone)]
pub(crate) struct Retained {
    pub id: u64,
    pub head: Shared<Head>,
    pub next_collection: u64,
}
pub(crate) struct Retention {
    pub next_id: u64,
    pub entries: ChargedVec<Retained>,
}
impl Retention {
    fn new() -> Self {
        Self {
            next_id: 1,
            entries: ChargedVec::new(),
        }
    }
    fn fork(&self, mem: &Memory, extra: usize) -> Result<Self> {
        let mut entries = ChargedVec::new();
        mem.reserve(&mut entries, self.entries.len() + extra)?;
        entries.extend(self.entries.iter().cloned());
        Ok(Self {
            next_id: self.next_id,
            entries,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionId {
    store: u64,
    id: u64,
}
impl CollectionId {
    pub fn as_u64(self) -> u64 {
        self.id
    }
}
mod pages;
mod read_only;
pub(crate) use pages::{Backing, Base, Row};
pub use read_only::ReadOnlyDatabase;
#[derive(Clone)]
pub(crate) struct Record {
    pub(crate) revision: u64,
    pub(crate) value: Value,
}
#[derive(Clone)]
pub(crate) struct Collection {
    pub(crate) base: Option<Base>,
    pub(crate) revision: u64,
    pub(crate) records: Map<Record>,
}
pub(crate) struct Head {
    pub(crate) revision: u64,
    pub(crate) data_revision: u64,
    pub(crate) collections: Map<Collection>,
}
struct State {
    head: Shared<Head>,
    next_collection: u64,
    retention: Retention,
}
#[cfg(feature = "replication")]
pub(crate) struct ReplicationSnapshot {
    packed: crate::storage::FileFormat,
    pub(crate) head: Shared<Head>,
    pub(crate) identity: [u8; 16],
    next_collection: u64,
    retention: Retention,
}
#[cfg(feature = "replication")]
impl ReplicationSnapshot {
    pub(crate) fn encode(
        self,
        destination: impl ByteStore + 'static,
        mem: &Memory,
    ) -> Result<(Storage, u64)> {
        Storage::create_compact(
            destination,
            mem,
            &self.head,
            self.next_collection,
            self.identity,
            &self.retention,
            self.packed,
        )
    }
}
struct Publication<'db> {
    db: &'db Database,
    storage: MutexGuard<'db, Option<Storage>>,
    state: RwLockReadGuard<'db, State>,
}
impl Publication<'_> {
    fn publish(
        self,
        head: Shared<Head>,
        next_collection: u64,
        retention: Option<Retention>,
    ) -> Result<()> {
        let Self {
            db,
            mut storage,
            state,
        } = self;
        let mut head = head;
        if let Some(backend) = storage.as_mut() {
            backend.persist(
                &db.mem,
                &head,
                next_collection,
                retention.as_ref().unwrap_or(&state.retention),
            )?;
        }
        if let Some(new) = storage.as_mut().and_then(|s| s.rebased.take()) {
            head = new;
        }
        // Keep the writer mutex held through installation. Readers may have
        // acquired the prior state while persistence was in progress.
        drop(state);
        let mut state = db.state.write().unwrap_or_else(|e| e.into_inner());
        let old_head = std::mem::replace(&mut state.head, head);
        state.next_collection = next_collection;
        let old_retention =
            retention.map(|retention| std::mem::replace(&mut state.retention, retention));
        drop(state);
        drop((old_head, old_retention));
        Ok(())
    }
}

/// Resource policy and allocator for an in-memory or durable database.
#[derive(Default)]
pub struct DatabaseOptions {
    pub(crate) paged_reads: bool,
    packed_pages: crate::storage::FileFormat,
    pub limits: Limits,
    pub(crate) allocator: AllocatorHandle,
}

impl From<Limits> for DatabaseOptions {
    fn from(limits: Limits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }
}

impl DatabaseOptions {
    /// Keep verified file-backed collection directories and materialize records on demand
    /// after open. This process-local option works with the existing v2 file format.
    pub fn with_paged_reads(mut self) -> Self {
        self.paged_reads = true;
        self
    }

    /// Create new images with packed, copy-on-write directory pages (format 3).
    /// Existing images retain the format selected at creation.
    pub fn with_packed_pages(mut self) -> Self {
        self.packed_pages = crate::storage::FileFormat::Packed;
        self.with_paged_reads()
    }

    /// Create a format-4 file with the given durable capacity. Open discovers it from disk.
    pub fn with_file_capacity(mut self, capacity: crate::storage::FileCapacity) -> Self {
        self.packed_pages = crate::storage::FileFormat::Extended(capacity);
        self.with_paged_reads()
    }

    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Select the allocator for core storage and opaque handles. Allocator
    /// methods may run on multiple threads and must avoid reentering the database.
    pub fn with_allocator(mut self, allocator: AllocatorHandle) -> Self {
        self.allocator = allocator;
        self
    }
}

/// Thread-safe writer authority. Preparation is private to each transaction;
/// a writer mutex serializes validation, rebasing, persistence and publication.
/// Snapshots and drafts can acquire the acknowledged state during storage I/O.
pub struct Database {
    pub(crate) mem: Memory,
    store: u64,
    writer: Mutex<Option<Storage>>,
    state: RwLock<State>,
}
pub struct Snapshot<'db> {
    db: &'db Database,
    head: Shared<Head>,
    next_collection: u64,
}
impl Clone for Snapshot<'_> {
    fn clone(&self) -> Self {
        Self {
            db: self.db,
            head: self.head.clone(),
            next_collection: self.next_collection,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validation {
    WholeDatabase,
    Records,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionState {
    Active,
    Committed,
    Aborted,
    Failed,
}
struct Point {
    collection: CollectionId,
    key: ChargedVec<u8>,
    revision: u64,
}
struct Write {
    collection: CollectionId,
    key: ChargedVec<u8>,
    value: Option<Value>,
}
// A saturated sum exceeds the validated transaction-byte cap. The caller
// applies that cap after including the bytes already staged in the draft.
fn write_charge(key_bytes: usize, value_weight: usize) -> usize {
    key_bytes
        .saturating_add(value_weight)
        .saturating_add(std::mem::size_of::<Write>())
}

/// An exclusively owned draft: movable between threads, but not shareable.
/// ```compile_fail
/// fn requires_sync<T: Sync>() {}
/// requires_sync::<weekend_db::Transaction<'static>>();
/// ```
pub struct Transaction<'db> {
    db: &'db Database,
    base: Shared<Head>,
    validation: Validation,
    points: ChargedVec<Point>,
    collections: ChargedVec<(CollectionId, u64)>,
    writes: ChargedVec<Write>,
    bytes: usize,
    state: TransactionState,
    _exclusive: PhantomData<Cell<()>>,
}
impl Database {
    /// Requested live core allocation bytes and observed high-water mark.
    /// Counts core layouts. Allocator overhead, backend/application/FFI buffers,
    /// and the fixed budget and allocator ownership blocks are outside this total.
    pub fn memory_usage(&self) -> crate::memory::MemoryUsage {
        self.mem.public_usage()
    }

    /// Measure append headroom with one length query and no graph reads or writes.
    /// Requires a healthy byte-backed store. Length I/O errors can be retried.
    /// Impossible physical lengths block publication until reopening.
    pub fn storage_space(&self) -> Result<crate::storage::StorageSpace> {
        self.lock_writer()?.as_mut().ok_or(Error::Invalid)?.space()
    }
    /// Return the persistent identity of a byte-backed database without I/O.
    /// Copies and restores preserve this identity. It identifies a lineage,
    /// so callers also need a revision or application receipt to detect rollback.
    /// An in-memory database without a byte store returns `Error::Invalid`.
    pub fn storage_identity(&self) -> Result<[u8; 16]> {
        self.writer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(Storage::identity)
            .ok_or(Error::Invalid)
    }
    /// Write a fully synchronized, verified compact copy to an empty byte store.
    /// The source stays open and unchanged; the destination lease is released
    /// on return. Requires a byte-backed source and exclusive access to this
    /// database: no live borrowed snapshots or transactions. Owned values may
    /// survive. File replacement uses the separate offline compact_file API.
    /// ```compile_fail
    /// use weekend_db::{Database, Limits, storage::MemoryStore};
    /// let mut db = Database::create_on(MemoryStore::new().unwrap(), [1;16], Limits::default()).unwrap();
    /// let snapshot = db.snapshot();
    /// db.compact_to(MemoryStore::new().unwrap()).unwrap();
    /// println!("{}", snapshot.revision());
    /// ```
    pub fn compact_to<B: ByteStore + 'static>(
        &mut self,
        destination: B,
    ) -> Result<crate::storage::CompactionStats> {
        self.compact_copy(destination).map(|(_, stats)| stats)
    }
    pub(crate) fn compact_copy<B: ByteStore + 'static>(
        &mut self,
        destination: B,
    ) -> Result<(Storage, crate::storage::CompactionStats)> {
        let storage = self
            .writer
            .get_mut()
            .map_err(|_| Error::RecoveryRequired)?
            .as_mut()
            .ok_or(Error::Invalid)?;
        let state = self.state.get_mut().map_err(|_| Error::RecoveryRequired)?;
        let (mut copy, stats) = storage.encode_compact_copy(
            destination,
            &self.mem,
            &state.head,
            state.next_collection,
            &state.retention,
        )?;
        copy.verify(&self.mem)?;
        Ok((copy, stats))
    }
    #[cfg(feature = "replication")]
    pub(crate) fn replication_head(&self) -> Result<([u8; 16], u64)> {
        let writer = self.lock_writer()?;
        let storage = writer.as_ref().ok_or(Error::Invalid)?;
        Ok((storage.identity(), self.read_state().head.revision))
    }
    #[cfg(feature = "replication")]
    pub(crate) fn replication_snapshot(&self, mem: &Memory) -> Result<ReplicationSnapshot> {
        let writer = self.lock_writer()?;
        let storage = writer.as_ref().ok_or(Error::Invalid)?;
        let state = self.read_state();
        Ok(ReplicationSnapshot {
            packed: storage.format,
            head: state.head.clone(),
            identity: storage.identity(),
            next_collection: state.next_collection,
            retention: state.retention.fork(mem, 0)?,
        })
    }
    #[cfg(all(
        feature = "replication",
        any(test, target_os = "macos", target_os = "linux", windows)
    ))]
    pub(crate) fn decode_replica(
        backend: &mut dyn ByteStore,
        identity: [u8; 16],
        revision: u64,
        length: u64,
        limits: Limits,
    ) -> Result<Self> {
        let mut db = Self::with_limits(limits)?;
        let recovered =
            crate::storage::recover_replica(backend, &db.mem, identity, revision, length)?;
        *db.state.get_mut().unwrap() = State {
            head: recovered.head,
            next_collection: recovered.next_collection,
            retention: recovered.retention,
        };
        Ok(db)
    }
    /// Offline compaction: close every database handle for this file first.
    /// Both old and replacement inodes remain locked through durable exchange.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub fn compact_file(
        path: impl AsRef<std::path::Path>,
        limits: Limits,
    ) -> Result<crate::storage::CompactionStats> {
        crate::storage::compact_file(path.as_ref(), limits)
    }
    fn read_state(&self) -> RwLockReadGuard<'_, State> {
        self.state.read().unwrap_or_else(|e| e.into_inner())
    }
    // A ByteStore may unwind after writing a root. Writer poison blocks all
    // later publication while leaving the acknowledged state available.
    fn lock_writer(&self) -> Result<MutexGuard<'_, Option<Storage>>> {
        let writer = self.writer.lock().map_err(|_| Error::RecoveryRequired)?;
        if let Some(storage) = writer.as_ref() {
            storage.guard()?;
        }
        Ok(writer)
    }
    fn publication_state(&self) -> Result<Publication<'_>> {
        let storage = self.lock_writer()?;
        let state = self.read_state();
        Ok(Publication {
            db: self,
            storage,
            state,
        })
    }
    pub fn new() -> Result<Self> {
        Self::with_limits(Limits::default())
    }
    pub fn with_limits(limits: Limits) -> Result<Self> {
        Self::with_options(limits.into())
    }
    pub fn with_options(options: DatabaseOptions) -> Result<Self> {
        let mem = Memory::with_allocator(options.limits, options.allocator)?;
        let store = NEXT_STORE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        let head = mem.shared(Head {
            revision: 0,
            data_revision: 0,
            collections: Map::new(),
        })?;
        Ok(Self {
            mem,
            store,
            writer: Mutex::new(None),
            state: RwLock::new(State {
                head,
                next_collection: 1,
                retention: Retention::new(),
            }),
        })
    }
    /// Create a v2 image on an empty, exclusively locked byte store. The caller
    /// supplies a nonzero identity.
    pub fn create_on<B: ByteStore + 'static>(
        backend: B,
        identity: [u8; 16],
        limits: Limits,
    ) -> Result<Self> {
        Self::create_on_with_options(backend, identity, limits.into())
    }
    pub fn create_on_with_options<B: ByteStore + 'static>(
        backend: B,
        identity: [u8; 16],
        options: DatabaseOptions,
    ) -> Result<Self> {
        let packed = options.packed_pages;
        let mut db = Self::with_options(options)?;
        let storage = {
            let state = db.read_state();
            Storage::create_format(
                backend,
                &db.mem,
                &state.head,
                state.next_collection,
                identity,
                &state.retention,
                packed,
            )?
        };
        *db.writer.get_mut().unwrap() = Some(storage);
        Ok(db)
    }
    /// Verify the full graph, stabilize its bytes/witness, then open. Recovery
    /// needs an exclusive writer lease and may perform sync/metadata I/O.
    pub fn open_on<B: ByteStore + 'static>(backend: B, limits: Limits) -> Result<Self> {
        Self::open_on_with_options(backend, limits.into())
    }
    pub fn open_on_with_options<B: ByteStore + 'static>(
        backend: B,
        options: DatabaseOptions,
    ) -> Result<Self> {
        let paged = options.paged_reads;
        let mut db = Self::with_options(options)?;
        let (storage, head, next_collection, retention) = if paged {
            Storage::open_paged(backend, &db.mem)?
        } else {
            Storage::open(backend, &db.mem)?
        };
        *db.state.get_mut().unwrap() = State {
            head,
            next_collection,
            retention,
        };
        *db.writer.get_mut().unwrap() = Some(storage);
        Ok(db)
    }
    /// Reads still use the last acknowledged head, but publication is blocked.
    /// Reopen to determine an uncertain commit's outcome.
    pub fn requires_recovery(&self) -> bool {
        match self.writer.lock() {
            Ok(writer) => writer.as_ref().is_some_and(Storage::requires_recovery),
            Err(_) => true,
        }
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn create_file(path: impl AsRef<std::path::Path>, limits: Limits) -> Result<Self> {
        Self::create_file_with_options(path, limits.into())
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn create_file_with_options(
        path: impl AsRef<std::path::Path>,
        options: DatabaseOptions,
    ) -> Result<Self> {
        Self::create_file_using(path, options, crate::storage::file_identity)
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn create_file_using(
        path: impl AsRef<std::path::Path>,
        options: impl Into<DatabaseOptions>,
        make_identity: impl FnOnce() -> Result<[u8; 16]>,
    ) -> Result<Self> {
        // Entropy failure must return before even converting the pathname.
        let options = options.into();
        options.limits.validate()?;
        let identity = make_identity()?;
        let backend = crate::storage::FileStore::open_in(path, true, options.allocator.clone())?;
        Self::create_on_with_options(backend, identity, options)
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn open_file(path: impl AsRef<std::path::Path>, limits: Limits) -> Result<Self> {
        Self::open_file_with_options(path, limits.into())
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn open_file_with_options(
        path: impl AsRef<std::path::Path>,
        options: DatabaseOptions,
    ) -> Result<Self> {
        options.limits.validate()?;
        let backend = crate::storage::FileStore::open_in(path, false, options.allocator.clone())?;
        Self::open_on_with_options(backend, options)
    }
    /// Publish a durable local snapshot ID. The snapshot can be historical, but
    /// must belong to this database. No record/value or old catalog is rewritten.
    pub fn retain_snapshot(&self, snapshot: &Snapshot<'_>) -> Result<u64> {
        if !std::ptr::eq(self, snapshot.db) {
            return Err(Error::ForeignStore);
        }
        let publication = self.publication_state()?;
        let state = &publication.state;
        if state.retention.entries.len() >= MAX_RETAINED_SNAPSHOTS {
            return Err(Error::Limit);
        }
        let next = state.head.revision.checked_add(1).ok_or(Error::Exhausted)?;
        let id = state.retention.next_id;
        let following = id.checked_add(1).ok_or(Error::Exhausted)?;
        let mut retention = state.retention.fork(&self.mem, 1)?;
        retention.next_id = following;
        retention.entries.push(Retained {
            id,
            head: snapshot.head.clone(),
            next_collection: snapshot.next_collection,
        });
        let head = self.mem.shared(Head {
            revision: next,
            data_revision: state.head.data_revision,
            collections: state.head.collections.clone(),
        })?;
        let next_collection = state.next_collection;
        publication.publish(head, next_collection, Some(retention))?;
        Ok(id)
    }
    pub fn release_retained(&self, id: u64) -> Result<()> {
        let publication = self.publication_state()?;
        let state = &publication.state;
        let index = state
            .retention
            .entries
            .iter()
            .position(|r| r.id == id)
            .ok_or(Error::NotFound)?;
        let next = state.head.revision.checked_add(1).ok_or(Error::Exhausted)?;
        let mut retention = state.retention.fork(&self.mem, 0)?;
        retention.entries.remove(index);
        let head = self.mem.shared(Head {
            revision: next,
            data_revision: state.head.data_revision,
            collections: state.head.collections.clone(),
        })?;
        let next_collection = state.next_collection;
        publication.publish(head, next_collection, Some(retention))?;
        Ok(())
    }
    pub fn retained_snapshot(&self, id: u64) -> Result<Snapshot<'_>> {
        let state = self.read_state();
        let retained = state
            .retention
            .entries
            .iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
        Ok(Snapshot {
            db: self,
            head: retained.head.clone(),
            next_collection: retained.next_collection,
        })
    }
    pub fn retained_ids(&self) -> Result<Vec<u64>> {
        self.retained_ids_owned()?.into_external()
    }
    pub(crate) fn retained_ids_owned(&self) -> Result<ChargedVec<u64>> {
        let state = self.read_state();
        let mut ids = ChargedVec::new();
        self.mem.reserve(&mut ids, state.retention.entries.len())?;
        ids.extend(state.retention.entries.iter().map(|r| r.id));
        Ok(ids)
    }
    pub fn limits(&self) -> Limits {
        self.mem.limits()
    }
    /// Share this database's allocator for caller-owned scratch. The returned
    /// handle outlives the database and its allocations are outside the live-byte cap.
    pub fn allocator(&self) -> AllocatorHandle {
        self.mem.allocator()
    }
    pub fn collection_id(&self, id: u64) -> CollectionId {
        CollectionId {
            store: self.store,
            id,
        }
    }
    fn check_collection(&self, id: CollectionId) -> Result<()> {
        if id.store == self.store {
            Ok(())
        } else {
            Err(Error::ForeignStore)
        }
    }
    pub fn create_collection(&self) -> Result<CollectionId> {
        let publication = self.publication_state()?;
        let state = &publication.state;
        let next = state.head.revision.checked_add(1).ok_or(Error::Exhausted)?;
        let id = state.next_collection;
        let following = id.checked_add(1).ok_or(Error::Exhausted)?;
        let collections = state.head.collections.set(
            &self.mem,
            &id.to_le_bytes(),
            Collection {
                base: None,
                revision: next,
                records: Map::new(),
            },
        )?;
        let head = self.mem.shared(Head {
            revision: next,
            data_revision: next,
            collections,
        })?;
        publication.publish(head, following, None)?;
        Ok(self.collection_id(id))
    }
    /// Catalog operations are immediate atomic commits, never part of a draft.
    /// IDs are never reused; transactions on a removed collection conflict.
    pub fn drop_collection(&self, id: CollectionId) -> Result<()> {
        self.check_collection(id)?;
        let publication = self.publication_state()?;
        let state = &publication.state;
        if state.head.collections.get(&id.id.to_le_bytes()).is_none() {
            return Err(Error::NotFound);
        }
        let next = state.head.revision.checked_add(1).ok_or(Error::Exhausted)?;
        let collections = state
            .head
            .collections
            .remove(&self.mem, &id.id.to_le_bytes())?;
        let head = self.mem.shared(Head {
            revision: next,
            data_revision: next,
            collections,
        })?;
        let next_collection = state.next_collection;
        publication.publish(head, next_collection, None)?;
        Ok(())
    }
    pub fn snapshot(&self) -> Snapshot<'_> {
        let state = self.read_state();
        Snapshot {
            db: self,
            head: state.head.clone(),
            next_collection: state.next_collection,
        }
    }
    pub fn begin(&self) -> Transaction<'_> {
        self.begin_with_validation(Validation::Records)
    }
    pub fn begin_with_validation(&self, validation: Validation) -> Transaction<'_> {
        Transaction {
            db: self,
            base: self.read_state().head.clone(),
            validation,
            points: ChargedVec::new(),
            collections: ChargedVec::new(),
            writes: ChargedVec::new(),
            bytes: 0,
            state: TransactionState::Active,
            _exclusive: PhantomData,
        }
    }
    pub fn none(&self) -> Result<Value> {
        Value::scalar(&self.mem, Kind::None, 0)
    }
    pub fn boolean(&self, v: bool) -> Result<Value> {
        Value::scalar(&self.mem, Kind::Boolean, u64::from(v))
    }
    pub fn integer(&self, v: i64) -> Result<Value> {
        Value::scalar(&self.mem, Kind::Integer, v as u64)
    }
    pub fn unsigned(&self, v: u64) -> Result<Value> {
        Value::scalar(&self.mem, Kind::Unsigned, v)
    }
    pub fn float(&self, v: f64) -> Result<Value> {
        Value::scalar(&self.mem, Kind::Float, v.to_bits())
    }
    pub fn bytes(&self, v: &[u8]) -> Result<Value> {
        Value::bytes_new(&self.mem, v)
    }
    pub fn map(&self) -> Result<Value> {
        Value::scalar(&self.mem, Kind::Map, 0)
    }
    pub fn list(&self) -> Result<Value> {
        Value::scalar(&self.mem, Kind::List, 0)
    }
}
fn collection(head: &Head, id: CollectionId) -> Result<&Collection> {
    head.collections
        .get(&id.id.to_le_bytes())
        .ok_or(Error::NotFound)
}
fn key(db: &Database, key: &[u8]) -> Result<()> {
    if key.len() > db.limits().max_bytes {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
pub type Rows = Vec<(Vec<u8>, Value)>;
pub(crate) type OwnedRows = ChargedVec<(ChargedVec<u8>, Value)>;

impl Snapshot<'_> {
    pub fn revision(&self) -> u64 {
        self.head.revision
    }
    pub fn data_revision(&self) -> u64 {
        self.head.data_revision
    }
    pub fn get(&self, id: CollectionId, k: &[u8]) -> Result<Option<Value>> {
        self.db.check_collection(id)?;
        key(self.db, k)?;
        collection(&self.head, id)?.lookup(&self.db.mem, k)
    }
    /// Visit rows in binary-key order. Paged directories stream through one page
    /// at a time. A callback error stops the traversal and is returned unchanged.
    pub fn visit(
        &self,
        id: CollectionId,
        visit: impl FnMut(&[u8], &Value) -> Result<()>,
    ) -> Result<()> {
        self.db.check_collection(id)?;
        collection(&self.head, id)?.visit_values(&self.db.mem, visit)
    }
    pub fn scan(&self, id: CollectionId) -> Result<Rows> {
        self.db.check_collection(id)?;
        collection(&self.head, id)?.rows(&self.db.mem)
    }
    pub(crate) fn scan_owned(&self, id: CollectionId) -> Result<OwnedRows> {
        self.db.check_collection(id)?;
        collection(&self.head, id)?.rows_owned(&self.db.mem)
    }
    pub fn count(&self, id: CollectionId) -> Result<usize> {
        self.db.check_collection(id)?;
        Ok(collection(&self.head, id)?.len())
    }
}
impl Transaction<'_> {
    pub fn state(&self) -> TransactionState {
        self.state
    }
    fn active(&self) -> Result<()> {
        if self.state == TransactionState::Active {
            Ok(())
        } else {
            Err(Error::Closed)
        }
    }
    fn charge(&self, extra: usize) -> Result<usize> {
        let next = self.bytes.checked_add(extra).ok_or(Error::Limit)?;
        if next > self.db.limits().max_transaction_bytes {
            Err(Error::Limit)
        } else {
            Ok(next)
        }
    }
    fn observe_collection(&mut self, id: CollectionId) -> Result<()> {
        self.db.check_collection(id)?;
        let base = collection(&self.base, id)?;
        if self.collections.iter().any(|(c, _)| *c == id) {
            return Ok(());
        }
        if self.collections.len() + self.points.len() >= self.db.limits().max_observations {
            return Err(Error::Limit);
        }
        let bytes = self.charge(std::mem::size_of::<(CollectionId, u64)>())?;
        self.db.mem.reserve(&mut self.collections, 1)?;
        self.collections.push((id, base.revision));
        self.bytes = bytes;
        Ok(())
    }
    // All point reads and writes capture their dependency from BASE, even when
    // reading an overlay insertion/deletion. Absence becomes a collection read.
    fn observe(&mut self, id: CollectionId, k: &[u8]) -> Result<()> {
        self.active()?;
        self.db.check_collection(id)?;
        key(self.db, k)?;
        let Some(revision) = collection(&self.base, id)?.record_revision(&self.db.mem, k)? else {
            return self.observe_collection(id);
        };
        if self
            .points
            .iter()
            .any(|p| p.collection == id && p.key.as_slice() == k)
        {
            return Ok(());
        }
        if self.collections.len() + self.points.len() >= self.db.limits().max_observations {
            return Err(Error::Limit);
        }
        let bytes = self.charge(
            k.len()
                .checked_add(std::mem::size_of::<Point>())
                .ok_or(Error::Limit)?,
        )?;
        let k = self.db.mem.copy(k)?;
        self.db.mem.reserve(&mut self.points, 1)?;
        self.points.push(Point {
            collection: id,
            key: k,
            revision,
        });
        self.bytes = bytes;
        Ok(())
    }
    pub fn get(&mut self, id: CollectionId, k: &[u8]) -> Result<Option<Value>> {
        self.observe(id, k)?;
        if let Some(w) = self
            .writes
            .iter()
            .find(|w| w.collection == id && w.key.as_slice() == k)
        {
            return Ok(w.value.clone());
        }
        collection(&self.base, id)?.lookup(&self.db.mem, k)
    }
    pub fn contains(&mut self, id: CollectionId, k: &[u8]) -> Result<bool> {
        Ok(self.get(id, k)?.is_some())
    }
    pub fn put(&mut self, id: CollectionId, k: &[u8], value: &Value) -> Result<()> {
        if !self.db.mem.same(&value.mem) {
            return Err(Error::ForeignStore);
        }
        self.assign(id, k, Some(value.clone()))
    }
    pub fn delete(&mut self, id: CollectionId, k: &[u8]) -> Result<()> {
        self.assign(id, k, None)
    }
    fn assign(&mut self, id: CollectionId, k: &[u8], value: Option<Value>) -> Result<()> {
        self.observe(id, k)?;
        let weight = value.as_ref().map_or(0, Value::weight);
        if let Some(index) = self
            .writes
            .iter()
            .position(|w| w.collection == id && w.key.as_slice() == k)
        {
            let old = self.writes[index].value.as_ref().map_or(0, Value::weight);
            let bytes = self
                .bytes
                .checked_sub(old)
                .and_then(|n| n.checked_add(weight))
                .ok_or(Error::Limit)?;
            if bytes > self.db.limits().max_transaction_bytes {
                return Err(Error::Limit);
            }
            self.writes[index].value = value;
            self.bytes = bytes;
            return Ok(());
        }
        if self.writes.len() >= self.db.limits().max_writes {
            return Err(Error::Limit);
        }
        let extra = write_charge(k.len(), weight);
        let bytes = self.charge(extra)?;
        let k = self.db.mem.copy(k)?;
        self.db.mem.reserve(&mut self.writes, 1)?;
        self.writes.push(Write {
            collection: id,
            key: k,
            value,
        });
        self.bytes = bytes;
        Ok(())
    }
    pub fn scan(&mut self, id: CollectionId) -> Result<Rows> {
        self.scan_records(id)?.rows(&self.db.mem)
    }
    /// Materialize rows in binary-key order using this database's allocator
    /// and budget. The iterator owns its rows, so the transaction can be
    /// edited while consuming the scan result.
    pub fn scan_iter(
        &mut self,
        id: CollectionId,
    ) -> Result<impl Iterator<Item = (impl AsRef<[u8]> + use<>, Value)> + use<>> {
        Ok(self.scan_owned(id)?.into_iter())
    }
    pub(crate) fn scan_owned(&mut self, id: CollectionId) -> Result<OwnedRows> {
        self.scan_records(id)?.rows_owned(&self.db.mem)
    }
    fn scan_records(&mut self, id: CollectionId) -> Result<Collection> {
        self.active()?;
        self.observe_collection(id)?;
        let mut records = collection(&self.base, id)?.clone();
        for w in self
            .writes
            .iter()
            .filter(|w| w.collection == id && w.value.is_none())
            .chain(
                self.writes
                    .iter()
                    .filter(|w| w.collection == id && w.value.is_some()),
            )
        {
            records.edit(&self.db.mem, &w.key, w.value.as_ref(), 0)?;
        }
        Ok(records)
    }
    pub fn count(&mut self, id: CollectionId) -> Result<usize> {
        Ok(self.scan_owned(id)?.len())
    }
    fn validate(&self, current: &Head) -> Result<()> {
        if self.validation == Validation::WholeDatabase && self.base.revision != current.revision {
            return Err(Error::Conflict);
        }
        for (id, expected) in &self.collections {
            if collection(current, *id).map(|c| c.revision) != Ok(*expected) {
                return Err(Error::Conflict);
            }
        }
        for p in &self.points {
            let actual = match collection(current, p.collection) {
                Ok(c) => c.record_revision(&self.db.mem, &p.key)?,
                Err(_) => None,
            };
            if actual != Some(p.revision) {
                return Err(Error::Conflict);
            }
        }
        Ok(())
    }
    fn publish(&self) -> Result<u64> {
        // The commit gate covers validation, rebasing, persistence and publication.
        // ByteStore methods must not call back into this database.
        let publication = self.db.publication_state()?;
        let state = &publication.state;
        self.validate(&state.head)?;
        let next = state.head.revision.checked_add(1).ok_or(Error::Exhausted)?;
        let mut collections = state.head.collections.clone();
        for w in self
            .writes
            .iter()
            .filter(|w| w.value.is_none())
            .chain(self.writes.iter().filter(|w| w.value.is_some()))
        {
            let mut c = collections
                .get(&w.collection.id.to_le_bytes())
                .ok_or(Error::Conflict)?
                .clone();
            c.edit(&self.db.mem, &w.key, w.value.as_ref(), next)?;
            c.revision = next; // Accepted deletes bump this even if already absent.
            collections = collections.set(&self.db.mem, &w.collection.id.to_le_bytes(), c)?;
        }
        let data_revision = if self.writes.is_empty() {
            state.head.data_revision
        } else {
            next
        };
        let head = self.db.mem.shared(Head {
            revision: next,
            data_revision,
            collections,
        })?;
        let next_collection = state.next_collection;
        publication.publish(head, next_collection, None)?;
        Ok(next)
    }
    /// Commit errors are terminal. Conflicts abort; preparation failures fail.
    /// A failed candidate never changes the published root or any old snapshot.
    pub fn commit(&mut self) -> Result<u64> {
        self.active()?;
        let result = self.publish();
        self.state = match result {
            Ok(_) => TransactionState::Committed,
            Err(Error::Conflict | Error::Io) => TransactionState::Aborted,
            Err(_) => TransactionState::Failed,
        };
        self.writes.clear();
        self.points.clear();
        self.collections.clear();
        self.bytes = 0;
        result
    }
    pub fn abort(&mut self) -> Result<()> {
        self.active()?;
        self.state = TransactionState::Aborted;
        self.writes.clear();
        self.points.clear();
        self.collections.clear();
        self.bytes = 0;
        Ok(())
    }
}

impl Database {
    /// Native I/O context is captured only during this synchronous call.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn create_file_with_diagnostics(
        path: impl AsRef<std::path::Path>,
        limits: Limits,
    ) -> std::result::Result<Self, crate::storage::StorageFailure> {
        crate::storage::diagnostics::capture(|| Self::create_file(path, limits))
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn open_file_with_diagnostics(
        path: impl AsRef<std::path::Path>,
        limits: Limits,
    ) -> std::result::Result<Self, crate::storage::StorageFailure> {
        crate::storage::diagnostics::capture(|| Self::open_file(path, limits))
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub fn compact_file_with_diagnostics(
        path: impl AsRef<std::path::Path>,
        limits: Limits,
    ) -> std::result::Result<crate::storage::CompactionStats, crate::storage::StorageFailure> {
        crate::storage::diagnostics::capture(|| Self::compact_file(path, limits))
    }
}
impl Transaction<'_> {
    /// Commit with operation-scoped native context. The original error and
    /// transaction terminal-state rules are preserved, including uncertain outcome.
    pub fn commit_with_diagnostics(
        &mut self,
    ) -> std::result::Result<u64, crate::storage::StorageFailure> {
        crate::storage::diagnostics::capture(|| self.commit())
    }
}

impl Snapshot<'_> {
    /// Materialize rows in binary-key order using this database's allocator
    /// and budget. The iterator owns its rows and can outlive the snapshot.
    pub fn scan_iter(
        &self,
        id: CollectionId,
    ) -> Result<impl Iterator<Item = (impl AsRef<[u8]> + use<>, Value)> + use<>> {
        Ok(self.scan_owned(id)?.into_iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[test]
    fn file_creation_orders_validation_entropy_and_path_access() {
        use std::path::{Path, PathBuf};

        enum ProbePath {
            Forbidden,
            Owned(PathBuf),
        }
        impl AsRef<Path> for ProbePath {
            fn as_ref(&self) -> &Path {
                match self {
                    Self::Forbidden => panic!("pathname touched before admission"),
                    Self::Owned(path) => path.as_path(),
                }
            }
        }
        fn forbidden_entropy() -> Result<[u8; 16]> {
            panic!("entropy touched before limits validation")
        }
        fn failed_entropy() -> Result<[u8; 16]> {
            Err(Error::Io)
        }
        fn fixed_identity() -> Result<[u8; 16]> {
            Ok([0x59; 16])
        }
        // Every control uses the same path/provider types and caller
        // instantiation. No global entropy source or filesystem hook is changed.
        type Entropy = fn() -> Result<[u8; 16]>;
        let invalid = Limits {
            max_depth: 0,
            ..Limits::default()
        };
        for (limits, provider, expected) in [
            (invalid, forbidden_entropy as Entropy, Error::Invalid),
            (Limits::default(), failed_entropy as Entropy, Error::Io),
        ] {
            assert_eq!(
                Database::create_file_using(ProbePath::Forbidden, limits, provider).err(),
                Some(expected)
            );
        }

        assert_eq!(
            Database::open_file(ProbePath::Forbidden, invalid).err(),
            Some(Error::Invalid)
        );

        struct OwnedDirectory(PathBuf);
        impl Drop for OwnedDirectory {
            fn drop(&mut self) {
                // This directory was exclusively created below. Never remove
                // arbitrary contents or follow a cleanup path outside it.
                let _ = std::fs::remove_file(self.0.join("store.wdb"));
                let _ = std::fs::remove_dir(&self.0);
            }
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "weekend-identity-factory-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let directory = OwnedDirectory(directory);
        let path = directory.0.join("store.wdb");
        let db = Database::create_file_using(
            ProbePath::Owned(path.clone()),
            Limits::default(),
            fixed_identity as Entropy,
        )
        .unwrap();
        assert_eq!(db.storage_identity(), Ok([0x59; 16]));
        assert_eq!(db.snapshot().revision(), 0);
        drop(db);
        let reopened = Database::open_file(&path, Limits::default()).unwrap();
        assert_eq!(reopened.storage_identity(), Ok([0x59; 16]));
        assert_eq!(reopened.snapshot().revision(), 0);
    }

    #[test]
    fn revision_exhaustion_is_atomic() {
        let db = Database::new().unwrap();
        let c = db.create_collection().unwrap();
        let old = db.read_state().head.clone();
        db.state.write().unwrap().head = db
            .mem
            .shared(Head {
                revision: u64::MAX,
                data_revision: old.data_revision,
                collections: old.collections.clone(),
            })
            .unwrap();
        let mut tx = db.begin();
        tx.put(c, b"x", &db.none().unwrap()).unwrap();
        assert_eq!(tx.commit(), Err(Error::Exhausted));
        assert_eq!(db.snapshot().get(c, b"x").unwrap(), None);
        assert_eq!(db.create_collection(), Err(Error::Exhausted));
        assert_eq!(db.snapshot().revision(), u64::MAX);
    }

    #[test]
    fn collection_id_exhaustion_never_reuses_identity() {
        let db = Database::new().unwrap();
        db.state.write().unwrap().next_collection = u64::MAX;
        assert_eq!(db.create_collection(), Err(Error::Exhausted));
        assert_eq!(db.snapshot().revision(), 0);
    }
    #[test]
    fn retained_id_exhaustion_is_atomic() {
        let db = Database::new().unwrap();
        let snapshot = db.snapshot();
        db.state.write().unwrap().retention.next_id = u64::MAX;
        assert_eq!(db.retain_snapshot(&snapshot), Err(Error::Exhausted));
        assert_eq!(db.snapshot().revision(), 0);
        assert!(db.retained_ids().unwrap().is_empty());
    }
    #[test]
    fn every_revision_increment_accepts_last_revision_then_rejects_wrap() {
        for operation in ["create", "drop", "retain", "release", "commit"] {
            let db = Database::new().unwrap();
            let first = db.create_collection().unwrap();
            let second = db.create_collection().unwrap();
            db.retain_snapshot(&db.snapshot()).unwrap();
            db.retain_snapshot(&db.snapshot()).unwrap();
            let old = db.read_state().head.clone();
            db.state.write().unwrap().head = db
                .mem
                .shared(Head {
                    revision: u64::MAX - 1,
                    data_revision: old.data_revision,
                    collections: old.collections.clone(),
                })
                .unwrap();
            let apply = |second_call| match operation {
                "create" => db.create_collection().map(|_| ()),
                "drop" => db.drop_collection(if second_call { second } else { first }),
                "retain" => db.retain_snapshot(&db.snapshot()).map(|_| ()),
                "release" => db.release_retained(if second_call { 2 } else { 1 }),
                "commit" => db.begin().commit().map(|_| ()),
                _ => unreachable!(),
            };
            assert_eq!(apply(false), Ok(()), "{operation}");
            let ids = db.retained_ids().unwrap();
            let before = db.memory_usage().live_bytes;
            assert_eq!(db.snapshot().revision(), u64::MAX);
            assert_eq!(apply(true), Err(Error::Exhausted), "{operation}");
            assert_eq!(db.snapshot().revision(), u64::MAX);
            assert_eq!(db.retained_ids().unwrap(), ids);
            assert_eq!(db.memory_usage().live_bytes, before);
            assert_eq!(db.snapshot().count(second), Ok(0));
        }
    }

    #[test]
    fn final_catalog_and_retention_ids_are_assigned_once() {
        let db = Database::new().unwrap();
        db.state.write().unwrap().next_collection = u64::MAX - 1;
        let last = db.create_collection().unwrap();
        assert_eq!(last.as_u64(), u64::MAX - 1);
        let revision = db.snapshot().revision();
        assert_eq!(db.create_collection(), Err(Error::Exhausted));
        assert_eq!(db.snapshot().revision(), revision);
        assert_eq!(db.snapshot().count(last), Ok(0));
        db.state.write().unwrap().retention.next_id = u64::MAX - 1;
        assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(u64::MAX - 1));
        let revision = db.snapshot().revision();
        assert_eq!(db.retain_snapshot(&db.snapshot()), Err(Error::Exhausted));
        assert_eq!(db.snapshot().revision(), revision);
        assert_eq!(db.retained_ids().unwrap(), [u64::MAX - 1]);
    }

    #[test]
    #[cfg(not(miri))]
    fn store_identity_exhaustion_is_isolated_in_a_subprocess() {
        const CHILD: &str = "WEEKEND_DB_ENGINE_STORE_EXHAUSTION";
        if std::env::var_os(CHILD).is_some() {
            NEXT_STORE.store(u64::MAX - 1, Ordering::Relaxed);
            let last = Database::new().unwrap();
            assert_eq!(last.store, u64::MAX - 1);
            assert!(matches!(Database::new(), Err(Error::Exhausted)));
            assert_eq!(NEXT_STORE.load(Ordering::Relaxed), u64::MAX);
            assert_eq!(last.snapshot().revision(), 0);
            return;
        }
        const TEST: &str = "engine::tests::store_identity_exhaustion_is_isolated_in_a_subprocess";
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--format=pretty", "--color=never"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&result.stdout);
        let passed = format!("test {TEST} ... ok");
        assert!(
            result.status.success() && stdout.lines().any(|line| line == passed),
            "child test {TEST} exited with {}\n{stdout}\n{}",
            result.status,
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[test]
    fn mutex_poison_preserves_reads_and_blocks_all_writer_gates() {
        let mut db = Database::new().unwrap();
        let c = db.create_collection().unwrap();
        let revision = db.snapshot().revision();
        #[cfg(feature = "replication")]
        assert_eq!(db.replication_head(), Err(Error::Invalid));
        assert!(!db.requires_recovery());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = db.writer.lock().unwrap();
                panic!("intentional writer-mutex poison");
            }))
            .is_err()
        );
        assert!(db.requires_recovery());
        assert_eq!(db.snapshot().revision(), revision);
        assert_eq!(db.snapshot().count(c), Ok(0));
        assert_eq!(db.begin().commit(), Err(Error::RecoveryRequired));
        assert_eq!(db.create_collection(), Err(Error::RecoveryRequired));
        assert_eq!(db.drop_collection(c), Err(Error::RecoveryRequired));
        assert_eq!(db.storage_space(), Err(Error::RecoveryRequired));
        assert_eq!(
            db.compact_to(crate::storage::MemoryStore::new().unwrap()),
            Err(Error::RecoveryRequired)
        );
        #[cfg(feature = "replication")]
        assert_eq!(db.replication_head(), Err(Error::RecoveryRequired));
    }

    #[test]
    fn compact_copy_rejects_poisoned_state_before_media_io() {
        use crate::storage::{Fault, SimulatedDisk};

        let source = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(source.clone(), [1; 16], Limits::default()).unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = db.state.write().unwrap();
                panic!("intentional state-lock poison");
            }))
            .is_err()
        );
        let target = SimulatedDisk::new().unwrap();
        source.arm(usize::MAX, Fault::Error);
        target.arm(usize::MAX, Fault::Error);
        assert_eq!(db.compact_to(target.clone()), Err(Error::RecoveryRequired));
        assert!(source.trace().unwrap().is_empty());
        assert!(target.trace().unwrap().is_empty());
        assert_eq!(db.snapshot().revision(), 0);
    }

    // Balanced shared children represent enormous logical values with O(bits)
    // distinct nodes. No enormous slice or forged Value is constructed.
    fn shared_weight(
        db: &Database,
        units: usize,
        cache: &mut std::collections::BTreeMap<usize, Value>,
    ) -> Value {
        assert!(units > 0);
        if let Some(value) = cache.get(&units) {
            return value.clone();
        }
        let value = if units == 1 {
            db.none().unwrap()
        } else if units == 2 {
            db.list().unwrap().list_push(&db.none().unwrap()).unwrap()
        } else {
            let left = (units - 1) / 2;
            let right = units - 1 - left;
            db.list()
                .unwrap()
                .list_push(&shared_weight(db, left, cache))
                .unwrap()
                .list_push(&shared_weight(db, right, cache))
                .unwrap()
        };
        assert_eq!(value.weight(), units * 16);
        cache.insert(units, value.clone());
        value
    }

    #[test]
    fn shared_dag_reaches_transaction_sum_overflow_without_large_allocations() {
        let limit = isize::MAX as usize;
        let db = Database::with_limits(Limits {
            max_value_bytes: limit,
            max_transaction_bytes: limit,
            max_live_bytes: 1024 * 1024,
            ..Limits::default()
        })
        .unwrap();
        let c = db.create_collection().unwrap();
        let mut cache = std::collections::BTreeMap::new();
        let first_overhead =
            std::mem::size_of::<(CollectionId, u64)>() + 1 + std::mem::size_of::<Write>();
        let first = shared_weight(&db, (limit - first_overhead) / 16, &mut cache);
        let largest = shared_weight(&db, limit / 16, &mut cache);
        let mut tx = db.begin();
        tx.put(c, b"a", &first).unwrap();
        let before = tx.bytes;
        assert!(limit - before < 16);
        assert_eq!(tx.charge(0), Ok(before));
        assert_eq!(tx.charge(limit - before), Ok(limit));
        assert_eq!(tx.charge(limit - before + 1), Err(Error::Limit));
        let extra = write_charge(1, largest.weight());
        assert!(before.checked_add(extra).is_none());
        assert_eq!(tx.put(c, b"b", &largest), Err(Error::Limit));
        assert_eq!(tx.bytes, before);
        assert_eq!(tx.writes.len(), 1);
        assert_eq!(tx.state(), TransactionState::Active);
        assert_eq!(db.snapshot().count(c), Ok(0));
        assert!(db.memory_usage().peak_bytes <= 1024 * 1024);
        assert!(cache.len() < 4 * usize::BITS as usize);
        tx.commit().unwrap();
        assert!(
            db.snapshot()
                .get(c, b"a")
                .unwrap()
                .unwrap()
                .same_node(&first)
        );
        assert!(db.snapshot().get(c, b"b").unwrap().is_none());
    }

    #[test]
    fn assignment_refusals_preserve_the_overlay_and_record_only_completed_observations() {
        use crate::memory::test_allocator::TestAllocator;
        use std::sync::Arc;

        for allowance in 0..=4 {
            let allocator = Arc::new(TestAllocator::default());
            let db = Database::with_options(allocator.options()).unwrap();
            let c = db.create_collection().unwrap();
            let old = db.integer(1).unwrap();
            let staged = db.integer(2).unwrap();
            let incoming = db.integer(3).unwrap();
            let changed = db.integer(4).unwrap();
            {
                let mut seed = db.begin();
                seed.put(c, b"a", &old).unwrap();
                seed.put(c, b"b", &old).unwrap();
                seed.commit().unwrap();
            }
            let base = db.snapshot();
            let mut tx = db.begin();
            tx.put(c, b"a", &staged).unwrap();
            let before = (
                db.memory_usage().live_bytes,
                allocator.live_bytes(),
                tx.bytes,
            );

            let result = allocator.with_refusal(allowance, || tx.put(c, b"b", &incoming));
            assert_eq!(tx.state(), TransactionState::Active);
            assert_eq!(db.snapshot().revision(), base.revision());
            assert_eq!(db.snapshot().get(c, b"a").unwrap(), Some(old.clone()));
            assert_eq!(db.snapshot().get(c, b"b").unwrap(), Some(old.clone()));
            assert_eq!(tx.get(c, b"a").unwrap(), Some(staged.clone()));
            if allowance == 4 {
                assert_eq!(result, Ok(()));
                tx.commit().unwrap();
                assert_eq!(db.snapshot().get(c, b"a").unwrap(), Some(staged));
                assert_eq!(db.snapshot().get(c, b"b").unwrap(), Some(incoming));
                continue;
            }

            // The first two requests record b. The last two stage its write.
            assert_eq!(result, Err(Error::OutOfMemory));
            assert_eq!(tx.writes.len(), 1);
            assert!(tx.collections.is_empty());
            let recorded = allowance >= 2;
            assert_eq!(tx.points.len(), if recorded { 2 } else { 1 });
            let charge = if recorded {
                1 + std::mem::size_of::<Point>()
            } else {
                0
            };
            assert_eq!(
                (
                    db.memory_usage().live_bytes,
                    allocator.live_bytes(),
                    tx.bytes
                ),
                (before.0 + charge, before.1 + charge, before.2 + charge)
            );

            let mut writer = db.begin();
            writer.put(c, b"b", &changed).unwrap();
            writer.commit().unwrap();
            if recorded {
                assert_eq!(tx.commit(), Err(Error::Conflict));
                assert_eq!(tx.state(), TransactionState::Aborted);
                assert_eq!(db.snapshot().get(c, b"a").unwrap(), Some(old.clone()));
            } else {
                tx.commit().unwrap();
                assert_eq!(db.snapshot().get(c, b"a").unwrap(), Some(staged));
            }
            assert_eq!(db.snapshot().get(c, b"b").unwrap(), Some(changed));
            assert_eq!(base.get(c, b"a").unwrap(), Some(old.clone()));
            assert_eq!(base.get(c, b"b").unwrap(), Some(old));
        }
    }

    #[test]
    fn new_write_charge_checks_each_sum_without_allocating_giant_keys() {
        let overhead = std::mem::size_of::<Write>();
        let half = isize::MAX as usize;
        let db = Database::with_limits(Limits {
            max_transaction_bytes: half,
            ..Limits::default()
        })
        .unwrap();
        let tx = db.begin();
        let key = half - overhead + 1;
        for (key_bytes, weight, expected) in [
            (0, 0, overhead),
            (0, half - overhead, half),
            (0, half - overhead + 1, half + 1),
            (key - 1, half, usize::MAX - 1),
            (key, half, usize::MAX),
            (key + 1, half, usize::MAX),
            // Exercise the first addition outside the public key-length domain.
            (usize::MAX, 1, usize::MAX),
        ] {
            let extra = write_charge(key_bytes, weight);
            assert_eq!(extra, expected);
            assert_eq!(
                tx.charge(extra),
                if expected <= half {
                    Ok(expected)
                } else {
                    Err(Error::Limit)
                }
            );
        }
    }
}
