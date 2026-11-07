use crate::{
    Error, Kind, Limits, Result, Value,
    memory::{Memory, Shared},
    storage::{ByteStore, Storage},
    tree::Map,
};
use std::{
    cell::Cell,
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Mutex, MutexGuard},
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
    pub entries: Vec<Retained>,
}
impl Retention {
    fn new() -> Self {
        Self {
            next_id: 1,
            entries: Vec::new(),
        }
    }
    fn fork(&self, mem: &Memory, extra: usize) -> Result<Self> {
        let mut entries = Vec::new();
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
#[derive(Clone)]
pub(crate) struct Record {
    pub(crate) revision: u64,
    pub(crate) value: Value,
}
#[derive(Clone)]
pub(crate) struct Collection {
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
    storage: Option<Storage>,
    retention: Retention,
}
impl State {
    fn persist(&mut self, mem: &Memory, head: &Head, next_collection: u64) -> Result<()> {
        if let Some(storage) = &mut self.storage {
            storage.persist(mem, head, next_collection, &self.retention)?;
        }
        Ok(())
    }
}

/// Thread-safe writer authority. Preparation is private to each transaction;
/// one mutex serializes validation, rebasing, persistence and head publication.
/// Acquired snapshots traverse immutable nodes without this mutex.
pub struct Database {
    pub(crate) mem: Memory,
    store: u64,
    state: Mutex<State>,
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
    key: Vec<u8>,
    revision: u64,
}
struct Write {
    collection: CollectionId,
    key: Vec<u8>,
    value: Option<Value>,
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
    points: Vec<Point>,
    collections: Vec<(CollectionId, u64)>,
    writes: Vec<Write>,
    bytes: usize,
    state: TransactionState,
    _exclusive: PhantomData<Cell<()>>,
}
impl Database {
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
        let state = self.state.get_mut().map_err(|_| Error::RecoveryRequired)?;
        let storage = state.storage.as_mut().ok_or(Error::Invalid)?;
        let (_copy, stats) = storage.compact(
            destination,
            &self.mem,
            &state.head,
            state.next_collection,
            &state.retention,
        )?;
        Ok(stats)
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
    // Poison never silently re-enables publication: a ByteStore implementation
    // may have unwound after writing a root but before returning its outcome.
    // Immutable acknowledged heads remain readable for diagnosis/recovery.
    fn read_state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn write_state(&self) -> Result<MutexGuard<'_, State>> {
        self.state.lock().map_err(|_| Error::RecoveryRequired)
    }
    pub fn new() -> Result<Self> {
        Self::with_limits(Limits::default())
    }
    pub fn with_limits(limits: Limits) -> Result<Self> {
        let mem = Memory::new(limits)?;
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
            state: Mutex::new(State {
                head,
                next_collection: 1,
                storage: None,
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
        let db = Self::with_limits(limits)?;
        let storage = {
            let state = db.read_state();
            Storage::create(
                backend,
                &db.mem,
                &state.head,
                state.next_collection,
                identity,
                &state.retention,
            )?
        };
        db.write_state().unwrap().storage = Some(storage);
        Ok(db)
    }
    /// Verify the full graph, stabilize its bytes/witness, then open. Recovery
    /// needs an exclusive writer lease and may perform sync/metadata I/O.
    pub fn open_on<B: ByteStore + 'static>(backend: B, limits: Limits) -> Result<Self> {
        let db = Self::with_limits(limits)?;
        let (storage, head, next_collection, retention) = Storage::open(backend, &db.mem)?;
        *db.write_state().unwrap() = State {
            head,
            next_collection,
            storage: Some(storage),
            retention,
        };
        Ok(db)
    }
    /// Reads still use the last acknowledged head, but publication is blocked.
    /// Reopen to determine an uncertain commit's outcome.
    pub fn requires_recovery(&self) -> bool {
        let state = self.read_state();
        self.state.is_poisoned()
            || state
                .storage
                .as_ref()
                .is_some_and(Storage::requires_recovery)
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn create_file(path: impl AsRef<std::path::Path>, limits: Limits) -> Result<Self> {
        limits.validate()?;
        let identity = crate::storage::file_identity()?;
        Self::create_on(crate::storage::FileStore::create(path)?, identity, limits)
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn open_file(path: impl AsRef<std::path::Path>, limits: Limits) -> Result<Self> {
        limits.validate()?;
        Self::open_on(crate::storage::FileStore::open(path)?, limits)
    }
    /// Publish a durable local snapshot ID. The snapshot can be historical, but
    /// must belong to this database. No record/value or old catalog is rewritten.
    pub fn retain_snapshot(&self, snapshot: &Snapshot<'_>) -> Result<u64> {
        if !std::ptr::eq(self, snapshot.db) {
            return Err(Error::ForeignStore);
        }
        let mut state = self.write_state()?;
        if let Some(storage) = &state.storage {
            storage.guard()?;
        }
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
        if let Some(storage) = &mut state.storage {
            storage.persist(&self.mem, &head, next_collection, &retention)?;
        }
        state.retention = retention;
        state.head = head;
        Ok(id)
    }
    pub fn release_retained(&self, id: u64) -> Result<()> {
        let mut state = self.write_state()?;
        if let Some(storage) = &state.storage {
            storage.guard()?;
        }
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
        if let Some(storage) = &mut state.storage {
            storage.persist(&self.mem, &head, next_collection, &retention)?;
        }
        state.retention = retention;
        state.head = head;
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
        let state = self.read_state();
        let mut ids = Vec::new();
        self.mem.reserve(&mut ids, state.retention.entries.len())?;
        ids.extend(state.retention.entries.iter().map(|r| r.id));
        Ok(ids)
    }
    pub fn limits(&self) -> Limits {
        self.mem.limits()
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
        let mut state = self.write_state()?;
        if let Some(storage) = &state.storage {
            storage.guard()?;
        }
        let next = state.head.revision.checked_add(1).ok_or(Error::Exhausted)?;
        let id = state.next_collection;
        let following = id.checked_add(1).ok_or(Error::Exhausted)?;
        let collections = state.head.collections.set(
            &self.mem,
            &id.to_le_bytes(),
            Collection {
                revision: next,
                records: Map::new(),
            },
        )?;
        let head = self.mem.shared(Head {
            revision: next,
            data_revision: next,
            collections,
        })?;
        state.persist(&self.mem, &head, following)?;
        state.head = head;
        state.next_collection = following;
        Ok(self.collection_id(id))
    }
    /// Catalog operations are immediate atomic commits, never part of a draft.
    /// IDs are never reused; transactions on a removed collection conflict.
    pub fn drop_collection(&self, id: CollectionId) -> Result<()> {
        self.check_collection(id)?;
        let mut state = self.write_state()?;
        if let Some(storage) = &state.storage {
            storage.guard()?;
        }
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
        state.persist(&self.mem, &head, next_collection)?;
        state.head = head;
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
            points: Vec::new(),
            collections: Vec::new(),
            writes: Vec::new(),
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
    /// Deterministic allocation-failure hook, available only in testing builds.
    #[cfg(any(test, feature = "fault-injection"))]
    pub fn fail_allocations_after(&self, remaining: Option<usize>) {
        self.mem.fail_after(remaining);
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
fn rows(db: &Database, records: &Map<Record>) -> Result<Rows> {
    records.materialize(&db.mem, |record| record.value.clone())
}

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
        Ok(collection(&self.head, id)?
            .records
            .get(k)
            .map(|r| r.value.clone()))
    }
    pub fn scan(&self, id: CollectionId) -> Result<Rows> {
        self.db.check_collection(id)?;
        rows(self.db, &collection(&self.head, id)?.records)
    }
    pub fn count(&self, id: CollectionId) -> Result<usize> {
        self.db.check_collection(id)?;
        Ok(collection(&self.head, id)?.records.len())
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
        let Some(record) = collection(&self.base, id)?.records.get(k) else {
            return self.observe_collection(id);
        };
        if self.points.iter().any(|p| p.collection == id && p.key == k) {
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
        let revision = record.revision;
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
            .find(|w| w.collection == id && w.key == k)
        {
            return Ok(w.value.clone());
        }
        Ok(collection(&self.base, id)?
            .records
            .get(k)
            .map(|r| r.value.clone()))
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
            .position(|w| w.collection == id && w.key == k)
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
        let extra = k
            .len()
            .checked_add(weight)
            .and_then(|n| n.checked_add(std::mem::size_of::<Write>()))
            .ok_or(Error::Limit)?;
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
        self.active()?;
        self.observe_collection(id)?;
        let mut records = collection(&self.base, id)?.records.clone();
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
            records = match &w.value {
                Some(value) => records.set(
                    &self.db.mem,
                    &w.key,
                    Record {
                        revision: 0,
                        value: value.clone(),
                    },
                )?,
                None => records.remove(&self.db.mem, &w.key)?,
            };
        }
        rows(self.db, &records)
    }
    pub fn count(&mut self, id: CollectionId) -> Result<usize> {
        Ok(self.scan(id)?.len())
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
            let actual = collection(current, p.collection)
                .ok()
                .and_then(|c| c.records.get(&p.key))
                .map(|r| r.revision);
            if actual != Some(p.revision) {
                return Err(Error::Conflict);
            }
        }
        Ok(())
    }
    fn publish(&self) -> Result<u64> {
        // The commit gate covers validation, rebasing, persistence and publication.
        // ByteStore methods must not call back into this database.
        let mut state = self.db.write_state()?;
        if let Some(storage) = &state.storage {
            storage.guard()?;
        }
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
            c.records = match &w.value {
                Some(value) => c.records.set(
                    &self.db.mem,
                    &w.key,
                    Record {
                        revision: next,
                        value: value.clone(),
                    },
                )?,
                None => c.records.remove(&self.db.mem, &w.key)?,
            };
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
        state.persist(&self.db.mem, &head, next_collection)?;
        state.head = head;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_exhaustion_is_atomic() {
        let db = Database::new().unwrap();
        let c = db.create_collection().unwrap();
        let old = db.read_state().head.clone();
        db.write_state().unwrap().head = db
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
        db.write_state().unwrap().next_collection = u64::MAX;
        assert_eq!(db.create_collection(), Err(Error::Exhausted));
        assert_eq!(db.snapshot().revision(), 0);
    }
    #[test]
    fn retained_id_exhaustion_is_atomic() {
        let db = Database::new().unwrap();
        let snapshot = db.snapshot();
        db.write_state().unwrap().retention.next_id = u64::MAX;
        assert_eq!(db.retain_snapshot(&snapshot), Err(Error::Exhausted));
        assert_eq!(db.snapshot().revision(), 0);
        assert!(db.retained_ids().unwrap().is_empty());
    }
}
