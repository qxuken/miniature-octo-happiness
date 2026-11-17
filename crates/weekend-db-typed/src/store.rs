use crate::{
    Error, FieldValue, LookupValue, Model, Record, Result, Schema,
    engine::{
        self, CollectionId, Database, Kind, Limits, Snapshot, Transaction, TransactionState, Value,
        schema::ReadView,
    },
};
use std::{marker::PhantomData, ops::Deref};
const FORMAT: &[u8] = b"\0weekend-typed";

pub struct Store {
    db: Database,
}
pub struct Table<T> {
    key: Vec<u8>,
    records: CollectionId,
    marker: PhantomData<fn() -> T>,
}
impl<T> Clone for Table<T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            records: self.records,
            marker: PhantomData,
        }
    }
}
impl<T> Table<T> {
    /// Raw writes through this collection bypass typed/index maintenance.
    pub fn collection(&self) -> CollectionId {
        self.records
    }
}
/// A read projection retaining the full stored envelope; updates load afresh in
/// their transaction. No save(detached_struct) operation can lose unknown fields.
pub struct Document<T> {
    id: u64,
    value: T,
    raw: Record,
    schema: Schema,
}
impl<T> Document<T> {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn version(&self) -> u64 {
        self.raw.version()
    }
    pub fn record(&self) -> &Record {
        &self.raw
    }
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn into_value(self) -> T {
        self.value
    }
}
impl<T> Deref for Document<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
pub struct Index<M, V> {
    id: u64,
    marker: PhantomData<fn() -> (M, V)>,
}
impl<M, V> Copy for Index<M, V> {}
impl<M, V> Clone for Index<M, V> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M, V> Index<M, V> {
    #[doc(hidden)]
    pub const fn new(id: u64) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }
    pub const fn id(self) -> u64 {
        self.id
    }
}
pub struct Read<'a> {
    store: &'a Store,
    snapshot: Snapshot<'a>,
}
pub struct Write<'a> {
    store: &'a Store,
    tx: Transaction<'a>,
    failed: bool,
}
/// Index consistency for one table at one snapshot or transaction view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndexReport {
    pub records: usize,
    pub expected_entries: usize,
    pub stored_entries: usize,
    pub missing: usize,
    pub orphaned: usize,
    pub mismatched: usize,
}
impl IndexReport {
    pub fn is_consistent(self) -> bool {
        self.missing == 0 && self.orphaned == 0 && self.mismatched == 0
    }
}
trait View: ReadView {
    fn scan_records(&mut self, c: CollectionId) -> engine::Result<engine::Rows>;
}
impl View for Snapshot<'_> {
    fn scan_records(&mut self, c: CollectionId) -> engine::Result<engine::Rows> {
        self.scan(c)
    }
}
impl View for Transaction<'_> {
    fn scan_records(&mut self, c: CollectionId) -> engine::Result<engine::Rows> {
        self.scan(c)
    }
}

fn reserve<T>(vec: &mut Vec<T>, extra: usize) -> Result<()> {
    vec.try_reserve_exact(extra)
        .map_err(|_| Error::Engine(engine::Error::OutOfMemory))
}
fn field(raw: &Value, key: &[u8]) -> Result<Value> {
    raw.map_get(key)?.ok_or(Error::InvalidCatalog)
}
fn uint(raw: &Value, key: &[u8]) -> Result<u64> {
    field(raw, key)?.as_u64().map_err(|_| Error::InvalidCatalog)
}
fn id_key(id: u64) -> Result<[u8; 8]> {
    if id == 0 {
        Err(Error::InvalidId)
    } else {
        Ok(id.to_le_bytes())
    }
}
fn row_id(key: &[u8]) -> Result<u64> {
    let id = u64::from_le_bytes(key.try_into().map_err(|_| Error::InvalidId)?);
    id_key(id)?;
    Ok(id)
}
struct Entry {
    records: u64,
    type_id: u64,
    version: u64,
    unique: Vec<u64>,
}
impl Entry {
    fn parse(value: Value) -> Result<Self> {
        let mut unique = Vec::new();
        let ids = field(&value, b"unique")?;
        if ids.kind() != Kind::List {
            return Err(Error::InvalidCatalog);
        }
        reserve(&mut unique, ids.len()?)?;
        for i in 0..ids.len()? {
            let id = ids
                .list_get(i)?
                .as_u64()
                .map_err(|_| Error::InvalidCatalog)?;
            if id == 0 || unique.contains(&id) {
                return Err(Error::InvalidCatalog);
            }
            unique.push(id);
        }
        let entry = Self {
            records: uint(&value, b"records")?,
            type_id: uint(&value, b"type")?,
            version: uint(&value, b"version")?,
            unique,
        };
        if entry.records <= 3 || entry.type_id == 0 || entry.version == 0 {
            return Err(Error::InvalidCatalog);
        }
        Ok(entry)
    }
    fn encode(&self, db: &Database) -> Result<Value> {
        let mut unique = db.list()?;
        for id in &self.unique {
            unique = unique.list_push(&db.unsigned(*id)?)?;
        }
        Ok(db
            .map()?
            .map_set(b"records", &db.unsigned(self.records)?)?
            .map_set(b"type", &db.unsigned(self.type_id)?)?
            .map_set(b"version", &db.unsigned(self.version)?)?
            .map_set(b"unique", &unique)?)
    }
}
impl Store {
    pub fn memory() -> Result<Self> {
        Self::initialize(Database::new()?)
    }
    pub fn create_on<B: engine::storage::ByteStore + 'static>(
        backend: B,
        identity: [u8; 16],
        limits: Limits,
    ) -> Result<Self> {
        Self::initialize(Database::create_on(backend, identity, limits)?)
    }
    pub fn open_on<B: engine::storage::ByteStore + 'static>(
        backend: B,
        limits: Limits,
    ) -> Result<Self> {
        Self::from_database(Database::open_on(backend, limits)?)
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn create_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::initialize(Database::create_file(path, Limits::default())?)
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn open_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::from_database(Database::open_file(path, Limits::default())?)
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub fn compact_file(
        path: impl AsRef<std::path::Path>,
    ) -> Result<engine::storage::CompactionStats> {
        Ok(Database::compact_file(path, Limits::default())?)
    }
    /// Initialize metadata on a fresh revision-zero database. Never repairs or
    /// overwrites an existing database; failed initialization can leave a partial
    /// fresh store, just as failed engine file creation can.
    pub fn initialize(db: Database) -> Result<Self> {
        if db.snapshot().revision() != 0 {
            return Err(Error::InvalidCatalog);
        }
        for expected in 1..=3 {
            if db.create_collection()?.as_u64() != expected {
                return Err(Error::InvalidCatalog);
            }
        }
        let mut tx = db.begin();
        tx.put(db.collection_id(1), FORMAT, &db.unsigned(1)?)?;
        tx.commit()?;
        drop(tx);
        Ok(Self { db })
    }
    pub fn from_database(db: Database) -> Result<Self> {
        {
            let s = db.snapshot();
            if s.get(db.collection_id(1), FORMAT)
                .ok()
                .flatten()
                .and_then(|v| v.as_u64().ok())
                != Some(1)
            {
                return Err(Error::NotInitialized);
            }
            for id in 1..=3 {
                s.count(db.collection_id(id))?;
            }
        }
        Ok(Self { db })
    }
    /// Advanced raw access can bypass this crate's schema and index guarantees.
    pub fn database(&self) -> &Database {
        &self.db
    }
    pub fn compact_to<B: engine::storage::ByteStore + 'static>(
        &mut self,
        destination: B,
    ) -> Result<engine::storage::CompactionStats> {
        Ok(self.db.compact_to(destination)?)
    }
    pub fn checkpoint(&self) -> Result<u64> {
        Ok(self.db.retain_snapshot(&self.db.snapshot())?)
    }
    pub fn history(&self) -> Result<Vec<u64>> {
        Ok(self.db.retained_ids()?)
    }
    pub fn release_checkpoint(&self, id: u64) -> Result<()> {
        Ok(self.db.release_retained(id)?)
    }
    pub fn read<R>(&self, action: impl FnOnce(&mut Read<'_>) -> Result<R>) -> Result<R> {
        action(&mut Read {
            store: self,
            snapshot: self.db.snapshot(),
        })
    }
    pub fn read_retained<R>(
        &self,
        id: u64,
        action: impl FnOnce(&mut Read<'_>) -> Result<R>,
    ) -> Result<R> {
        action(&mut Read {
            store: self,
            snapshot: self.db.retained_snapshot(id)?,
        })
    }
    /// Executes once. No retry/replay: errors or unwinding discard the draft.
    pub fn write<R>(&self, action: impl FnOnce(&mut Write<'_>) -> Result<R>) -> Result<R> {
        let mut write = Write {
            store: self,
            tx: self.db.begin(),
            failed: false,
        };
        let value = action(&mut write)?;
        if write.failed {
            return Err(Error::Engine(engine::Error::Closed));
        }
        write.tx.commit()?;
        Ok(value)
    }
    /// Register a name/schema during exclusive setup. Schema upgrades never
    /// migrate records implicitly. Unique index layouts are immutable in v1.
    pub fn table<T: Model>(&mut self, name: &str) -> Result<Table<T>> {
        if name.is_empty() || name.len() > 128 {
            return Err(Error::InvalidCatalog);
        }
        let mut key = Vec::new();
        reserve(&mut key, 6 + name.len())?;
        key.extend_from_slice(b"table/");
        key.extend_from_slice(name.as_bytes());
        let schema = T::schema(&self.db)?;
        let mut unique = T::UNIQUE_FIELDS.to_vec();
        unique.sort_unstable();
        for (i, id) in unique.iter().enumerate() {
            let field = schema.field(*id)?.ok_or(Error::NotIndexed(*id))?;
            if i > 0 && unique[i - 1] == *id || matches!(field.kind, Kind::Map | Kind::List) {
                return Err(Error::NotIndexed(*id));
            }
        }
        let existing = self
            .db
            .snapshot()
            .get(self.db.collection_id(1), &key)?
            .map(Entry::parse)
            .transpose()?;
        if let Some(entry) = &existing {
            if entry.type_id != T::TYPE_ID {
                return Err(Error::TypeMismatch);
            }
            if entry.unique != unique {
                return Err(Error::IndexLayoutChanged);
            }
            let mut s = self.db.snapshot();
            match Schema::load(&mut s, self.db.collection_id(2), T::TYPE_ID, T::VERSION) {
                Ok(stored) => {
                    if stored.as_value() != schema.as_value() {
                        return Err(Error::Schema(engine::schema::Error::ImmutableDescriptor));
                    }
                    if entry.version >= T::VERSION {
                        s.count(self.db.collection_id(entry.records))?;
                        return Ok(Table {
                            key,
                            records: self.db.collection_id(entry.records),
                            marker: PhantomData,
                        });
                    }
                }
                Err(engine::schema::Error::Engine(engine::Error::NotFound))
                    if entry.version < T::VERSION => {}
                Err(error) => return Err(error.into()),
            }
        }
        let mut tx = self.db.begin();
        schema.publish(&mut tx, self.db.collection_id(2))?;
        let records = match &existing {
            Some(entry) => entry.records,
            None => self.db.create_collection()?.as_u64(),
        };
        // Collection creation is immediate; failed registration may leave an
        // unregistered empty collection, never a partially registered table.
        let value = Entry {
            records,
            type_id: T::TYPE_ID,
            version: T::VERSION,
            unique,
        }
        .encode(&self.db)?;
        tx.put(self.db.collection_id(1), &key, &value)?;
        tx.commit()?;
        Ok(Table {
            key,
            records: self.db.collection_id(records),
            marker: PhantomData,
        })
    }
    pub fn restore_table<T: Model>(&self, table: &Table<T>, checkpoint: u64) -> Result<()> {
        let mut historical = self.db.retained_snapshot(checkpoint)?;
        info(self, &mut historical, table)?;
        let rows = historical.scan(table.records)?;
        self.write(|w| {
            w.checked(|w| {
                let entry = info(w.store, &mut w.tx, table)?;
                for (key, _) in w.tx.scan(table.records)? {
                    w.remove_raw(table, &entry, row_id(&key)?)?;
                }
                for (key, value) in rows {
                    w.put_raw(table, &entry, row_id(&key)?, Record::from_value(value)?)?;
                }
                Ok(())
            })
        })
    }
}
fn info<T: Model>(store: &Store, view: &mut impl View, table: &Table<T>) -> Result<Entry> {
    if table.records != store.db.collection_id(table.records.as_u64()) {
        return Err(Error::Engine(engine::Error::ForeignStore));
    }
    let entry = Entry::parse(
        view.get_record(store.db.collection_id(1), &table.key)?
            .ok_or(Error::Engine(engine::Error::NotFound))?,
    )?;
    if entry.type_id != T::TYPE_ID || entry.records != table.records.as_u64() {
        return Err(Error::TypeMismatch);
    }
    Ok(entry)
}
fn decode<T: Model>(
    store: &Store,
    view: &mut impl View,
    id: u64,
    value: Value,
) -> Result<Document<T>> {
    let raw = Record::from_value(value)?;
    if raw.type_id() != T::TYPE_ID {
        return Err(Error::TypeMismatch);
    }
    let schema = Schema::load(view, store.db.collection_id(2), T::TYPE_ID, raw.version())?;
    let value = T::decode(&raw, &schema)?;
    Ok(Document {
        id,
        value,
        raw,
        schema,
    })
}
fn get<T: Model>(
    store: &Store,
    view: &mut impl View,
    table: &Table<T>,
    id: u64,
) -> Result<Option<Document<T>>> {
    info(store, view, table)?;
    view.get_record(table.records, &id_key(id)?)?
        .map(|v| decode(store, view, id, v))
        .transpose()
}
fn list<T: Model>(
    store: &Store,
    view: &mut impl View,
    table: &Table<T>,
) -> Result<Vec<Document<T>>> {
    info(store, view, table)?;
    let rows = view.scan_records(table.records)?;
    let mut out = Vec::new();
    reserve(&mut out, rows.len())?;
    for (key, value) in rows {
        out.push(decode(store, view, row_id(&key)?, value)?);
    }
    out.sort_by_key(Document::id);
    Ok(out)
}
fn index_key(records: u64, field: u64, value: &Value) -> Result<Option<Vec<u8>>> {
    let mut key = Vec::new();
    let bytes: Vec<u8> = match value.kind() {
        Kind::None => return Ok(None),
        Kind::Boolean => vec![u8::from(value.as_bool()?)],
        Kind::Integer => value.as_i64()?.to_le_bytes().to_vec(),
        Kind::Unsigned => value.as_u64()?.to_le_bytes().to_vec(),
        Kind::Float => value.as_f64()?.to_bits().to_le_bytes().to_vec(),
        Kind::Bytes => value.as_bytes()?.to_vec(),
        _ => return Err(Error::NotIndexed(field)),
    };
    reserve(&mut key, 17 + bytes.len())?;
    key.extend_from_slice(&records.to_le_bytes());
    key.extend_from_slice(&field.to_le_bytes());
    key.push(value.kind() as u8);
    key.extend_from_slice(&bytes);
    Ok(Some(key))
}
fn record_keys<T: Model>(
    store: &Store,
    view: &mut impl View,
    entry: &Entry,
    raw: &Record,
) -> Result<Vec<(u64, Vec<u8>)>> {
    if raw.type_id() != T::TYPE_ID {
        return Err(Error::TypeMismatch);
    }
    let schema = Schema::load(view, store.db.collection_id(2), T::TYPE_ID, raw.version())?;
    schema.validate(raw)?;
    let mut keys = Vec::new();
    reserve(&mut keys, entry.unique.len())?;
    for id in &entry.unique {
        if let Some(value) = raw.field(*id)?
            && let Some(key) = index_key(entry.records, *id, &value)?
        {
            keys.push((*id, key));
        }
    }
    Ok(keys)
}
struct IndexState {
    report: IndexReport,
    expected: Vec<(Vec<u8>, u64)>,
    stored: engine::Rows,
}
fn index_state<T: Model>(
    store: &Store,
    view: &mut impl View,
    table: &Table<T>,
) -> Result<IndexState> {
    let entry = info(store, view, table)?;
    let records = view.scan_records(table.records)?;
    let mut expected = Vec::new();
    for (key, value) in &records {
        let id = row_id(key)?;
        let raw = Record::from_value(value.clone())?;
        for (_, key) in record_keys::<T>(store, view, &entry, &raw)? {
            expected
                .try_reserve(1)
                .map_err(|_| Error::Engine(engine::Error::OutOfMemory))?;
            expected.push((key, id));
        }
    }
    expected.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for pair in expected.windows(2) {
        if pair[0].0 == pair[1].0 {
            // Keys were produced by index_key above, including the field ID.
            let field = u64::from_le_bytes(pair[0].0[8..16].try_into().unwrap());
            return Err(Error::UniqueViolation(field));
        }
    }
    let mut stored = view.scan_records(store.db.collection_id(3))?;
    stored.retain(|(key, _)| key.starts_with(&entry.records.to_le_bytes()));
    stored.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut report = IndexReport {
        records: records.len(),
        expected_entries: expected.len(),
        stored_entries: stored.len(),
        ..IndexReport::default()
    };
    for (key, id) in &expected {
        match stored.binary_search_by(|(candidate, _)| candidate.cmp(key)) {
            Err(_) => report.missing += 1,
            Ok(index) if stored[index].1.as_u64().ok() != Some(*id) => report.mismatched += 1,
            Ok(_) => {}
        }
    }
    report.orphaned = stored
        .iter()
        .filter(|(key, _)| {
            expected
                .binary_search_by(|(candidate, _)| candidate.cmp(key))
                .is_err()
        })
        .count();
    Ok(IndexState {
        report,
        expected,
        stored,
    })
}
fn find<T: Model, V: FieldValue, Q: LookupValue<V> + ?Sized>(
    store: &Store,
    view: &mut impl View,
    table: &Table<T>,
    index: Index<T, V>,
    value: &Q,
) -> Result<Option<Document<T>>> {
    let entry = info(store, view, table)?;
    if !entry.unique.contains(&index.id) {
        return Err(Error::NotIndexed(index.id));
    }
    let descriptor = Schema::load(view, store.db.collection_id(2), T::TYPE_ID, entry.version)?;
    let field = descriptor
        .field(index.id)?
        .ok_or(Error::NotIndexed(index.id))?;
    let encoded = value.encode_lookup(&store.db)?;
    if field.kind != V::KIND || (encoded.kind() != Kind::None && encoded.kind() != field.kind) {
        return Err(Error::InvalidField {
            id: index.id,
            reason: "lookup type differs from stored schema",
        });
    }
    let Some(key) = index_key(entry.records, index.id, &encoded)? else {
        return Ok(None);
    };
    let Some(id) = view.get_record(store.db.collection_id(3), &key)? else {
        return Ok(None);
    };
    let id = id.as_u64().map_err(|_| Error::IndexInconsistent)?;
    let doc = get(store, view, table, id)?.ok_or(Error::IndexInconsistent)?;
    let schema = Schema::load(view, store.db.collection_id(2), T::TYPE_ID, doc.version())?;
    // Defaults affect read projections, never physical index membership.
    // A legacy/default-derived or corrupt entry must not resolve an absent field.
    if doc.record().field(index.id)?.is_none() {
        return Err(Error::IndexInconsistent);
    }
    let actual = schema
        .read_field(doc.record(), index.id)?
        .ok_or(Error::IndexInconsistent)?;
    if index_key(entry.records, index.id, &actual)? != Some(key) {
        return Err(Error::IndexInconsistent);
    }
    Ok(Some(doc))
}
impl Read<'_> {
    /// Compare this table's primary records with its stored unique indexes.
    pub fn check_indexes<T: Model>(&mut self, table: &Table<T>) -> Result<IndexReport> {
        Ok(index_state(self.store, &mut self.snapshot, table)?.report)
    }
    pub fn get<T: Model>(&mut self, table: &Table<T>, id: u64) -> Result<Option<Document<T>>> {
        get(self.store, &mut self.snapshot, table, id)
    }
    pub fn list<T: Model>(&mut self, table: &Table<T>) -> Result<Vec<Document<T>>> {
        list(self.store, &mut self.snapshot, table)
    }
    pub fn find<T: Model, V: FieldValue, Q: LookupValue<V> + ?Sized>(
        &mut self,
        table: &Table<T>,
        index: Index<T, V>,
        value: &Q,
    ) -> Result<Option<Document<T>>> {
        find(self.store, &mut self.snapshot, table, index, value)
    }
}
impl Write<'_> {
    fn checked<R>(&mut self, action: impl FnOnce(&mut Self) -> Result<R>) -> Result<R> {
        if self.failed {
            return Err(Error::Engine(engine::Error::Closed));
        }
        self.failed = true; // Stays poisoned even when a caller catches a panic.
        match action(self) {
            Ok(v) => {
                self.failed = false;
                Ok(v)
            }
            Err(e) => {
                if self.tx.state() == TransactionState::Active {
                    let _ = self.tx.abort();
                }
                Err(e)
            }
        }
    }
    pub fn check_indexes<T: Model>(&mut self, table: &Table<T>) -> Result<IndexReport> {
        self.checked(|w| Ok(index_state(w.store, &mut w.tx, table)?.report))
    }
    /// Repair this table's index rows atomically. Returns the pre-repair report.
    /// Primary records are validated and never rewritten.
    pub fn rebuild_indexes<T: Model>(&mut self, table: &Table<T>) -> Result<IndexReport> {
        self.checked(|w| {
            let state = index_state(w.store, &mut w.tx, table)?;
            let indexes = w.store.db.collection_id(3);
            for (key, _) in &state.stored {
                if state
                    .expected
                    .binary_search_by(|(candidate, _)| candidate.cmp(key))
                    .is_err()
                {
                    w.tx.delete(indexes, key)?;
                }
            }
            for (key, id) in &state.expected {
                let existing = state
                    .stored
                    .binary_search_by(|(candidate, _)| candidate.cmp(key))
                    .ok()
                    .and_then(|index| state.stored[index].1.as_u64().ok());
                if existing != Some(*id) {
                    w.tx.put(indexes, key, &w.store.db.unsigned(*id)?)?;
                }
            }
            Ok(state.report)
        })
    }
    pub fn get<T: Model>(&mut self, table: &Table<T>, id: u64) -> Result<Option<Document<T>>> {
        self.checked(|w| get(w.store, &mut w.tx, table, id))
    }
    pub fn list<T: Model>(&mut self, table: &Table<T>) -> Result<Vec<Document<T>>> {
        self.checked(|w| list(w.store, &mut w.tx, table))
    }
    pub fn find<T: Model, V: FieldValue, Q: LookupValue<V> + ?Sized>(
        &mut self,
        table: &Table<T>,
        index: Index<T, V>,
        value: &Q,
    ) -> Result<Option<Document<T>>> {
        self.checked(|w| find(w.store, &mut w.tx, table, index, value))
    }
    pub fn insert<T: Model>(&mut self, table: &Table<T>, id: u64, value: &T) -> Result<()> {
        self.checked(|w| {
            let entry = info(w.store, &mut w.tx, table)?;
            if entry.version != T::VERSION {
                return Err(Error::UnsupportedVersion);
            }
            if w.tx.get(table.records, &id_key(id)?)?.is_some() {
                return Err(Error::Engine(engine::Error::AlreadyExists));
            }
            let schema = Schema::load(
                &mut w.tx,
                w.store.db.collection_id(2),
                T::TYPE_ID,
                T::VERSION,
            )?;
            let raw = value.patch(&w.store.db, &Record::new(&w.store.db, &schema)?)?;
            w.put_raw(table, &entry, id, raw)
        })
    }
    pub fn update<T: Model>(
        &mut self,
        table: &Table<T>,
        id: u64,
        action: impl FnOnce(&mut T) -> Result<()>,
    ) -> Result<()> {
        self.checked(|w| {
            let entry = info(w.store, &mut w.tx, table)?;
            let mut doc = get(w.store, &mut w.tx, table, id)?
                .ok_or(Error::Engine(engine::Error::NotFound))?;
            if doc.version() < T::VERSION {
                return Err(Error::MigrationRequired);
            }
            action(&mut doc.value)?;
            w.put_raw(table, &entry, id, doc.value.patch(&w.store.db, &doc.raw)?)
        })
    }
    pub fn delete<T: Model>(&mut self, table: &Table<T>, id: u64) -> Result<bool> {
        self.checked(|w| {
            let entry = info(w.store, &mut w.tx, table)?;
            w.remove_raw(table, &entry, id)
        })
    }
    /// Explicit all-row migration. Callback executes once per older record and
    /// may inspect retired raw fields before generated patching removes them.
    pub fn migrate<T: Model>(
        &mut self,
        table: &Table<T>,
        mut convert: impl FnMut(&Record, &mut T) -> Result<()>,
    ) -> Result<usize> {
        self.checked(|w| {
            let entry = info(w.store, &mut w.tx, table)?;
            if entry.version != T::VERSION {
                return Err(Error::UnsupportedVersion);
            }
            let target = Schema::load(
                &mut w.tx,
                w.store.db.collection_id(2),
                T::TYPE_ID,
                T::VERSION,
            )?;
            let mut count = 0;
            for (key, value) in w.tx.scan(table.records)? {
                count += usize::from(w.migrate_record(
                    table,
                    &entry,
                    &target,
                    (row_id(&key)?, value),
                    &mut convert,
                )?);
            }
            Ok(count)
        })
    }
    /// Migrate explicit primary IDs in input order, returning the number changed.
    /// Zero, duplicate, missing, or oversized inputs abort the whole closure.
    /// Already-current records skip conversion. Progress rows may be updated
    /// through this same Write so they commit atomically with the batch.
    pub fn migrate_ids<T: Model>(
        &mut self,
        table: &Table<T>,
        ids: &[u64],
        mut convert: impl FnMut(u64, &Record, &mut T) -> Result<()>,
    ) -> Result<usize> {
        self.checked(|w| {
            let limits = w.store.db.limits();
            if ids.len() > limits.max_writes || ids.len() > limits.max_observations {
                return Err(Error::Engine(engine::Error::Limit));
            }
            let mut sorted = Vec::new();
            reserve(&mut sorted, ids.len())?;
            sorted.extend_from_slice(ids);
            sorted.sort_unstable();
            if sorted.first() == Some(&0) || sorted.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(Error::InvalidId);
            }
            let entry = info(w.store, &mut w.tx, table)?;
            if entry.version != T::VERSION {
                return Err(Error::UnsupportedVersion);
            }
            let target = Schema::load(
                &mut w.tx,
                w.store.db.collection_id(2),
                T::TYPE_ID,
                T::VERSION,
            )?;
            let mut count = 0;
            for &id in ids {
                let value =
                    w.tx.get(table.records, &id_key(id)?)?
                        .ok_or(Error::Engine(engine::Error::NotFound))?;
                count += usize::from(w.migrate_record(
                    table,
                    &entry,
                    &target,
                    (id, value),
                    &mut |raw, model| convert(id, raw, model),
                )?);
            }
            Ok(count)
        })
    }
    fn migrate_record<T: Model>(
        &mut self,
        table: &Table<T>,
        entry: &Entry,
        target: &Schema,
        (id, value): (u64, Value),
        convert: &mut impl FnMut(&Record, &mut T) -> Result<()>,
    ) -> Result<bool> {
        let mut doc = decode::<T>(self.store, &mut self.tx, id, value)?;
        if doc.version() > T::VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if doc.version() == T::VERSION {
            return Ok(false);
        }
        convert(&doc.raw, &mut doc.value)?;
        let raw = doc.value.patch(
            &self.store.db,
            &doc.raw.with_schema(&self.store.db, target)?,
        )?;
        self.put_raw(table, entry, doc.id, raw)?;
        Ok(true)
    }
    fn keys<T: Model>(&mut self, entry: &Entry, raw: &Record) -> Result<Vec<(u64, Vec<u8>)>> {
        record_keys::<T>(self.store, &mut self.tx, entry, raw)
    }
    fn put_raw<T: Model>(
        &mut self,
        table: &Table<T>,
        entry: &Entry,
        id: u64,
        raw: Record,
    ) -> Result<()> {
        let key = id_key(id)?;
        let old = self
            .tx
            .get(table.records, &key)?
            .map(Record::from_value)
            .transpose()?;
        let old_keys = old
            .as_ref()
            .map(|r| self.keys::<T>(entry, r))
            .transpose()?
            .unwrap_or_default();
        let new_keys = self.keys::<T>(entry, &raw)?;
        let indexes = self.store.db.collection_id(3);
        for (_, key) in &old_keys {
            if self.tx.get(indexes, key)?.and_then(|v| v.as_u64().ok()) != Some(id) {
                return Err(Error::IndexInconsistent);
            }
        }
        for (field, key) in &new_keys {
            if let Some(existing) = self.tx.get(indexes, key)? {
                let existing = existing.as_u64().map_err(|_| Error::IndexInconsistent)?;
                if existing != id {
                    return Err(Error::UniqueViolation(*field));
                }
                if !old_keys.iter().any(|(_, k)| k == key) {
                    return Err(Error::IndexInconsistent);
                }
            }
        }
        for (_, key) in &old_keys {
            if !new_keys.iter().any(|(_, k)| k == key) {
                self.tx.delete(indexes, key)?;
            }
        }
        for (_, key) in &new_keys {
            if !old_keys.iter().any(|(_, k)| k == key) {
                self.tx.put(indexes, key, &self.store.db.unsigned(id)?)?;
            }
        }
        self.tx.put(table.records, &key, raw.as_value())?;
        Ok(())
    }
    fn remove_raw<T: Model>(&mut self, table: &Table<T>, entry: &Entry, id: u64) -> Result<bool> {
        let key = id_key(id)?;
        let Some(raw) = self.tx.get(table.records, &key)? else {
            return Ok(false);
        };
        for (_, key) in self.keys::<T>(entry, &Record::from_value(raw)?)? {
            let indexes = self.store.db.collection_id(3);
            if self.tx.get(indexes, &key)?.and_then(|v| v.as_u64().ok()) != Some(id) {
                return Err(Error::IndexInconsistent);
            }
            self.tx.delete(indexes, &key)?;
        }
        self.tx.delete(table.records, &key)?;
        Ok(true)
    }
}
