# weekend-db-typed

Rust convenience module for `weekend-db`: derived models, named tables,
transaction closures, exact unique lookups, explicit migration, and checkpoints.
The engine and its C shared library keep their existing format and contracts.

```rust
use weekend_db_typed::{Model, Result, Store};

#[derive(Model)]
#[weekend(type_id = 1001, version = 1)]
struct Task {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2)]
    title: String,
    #[weekend(id = 3)]
    done: bool,
    #[weekend(id = 4)]
    tags: Vec<String>,
}

fn main() -> Result<()> {
    let mut store = Store::memory()?;
    let tasks = store.table::<Task>("tasks")?;
    store.write(|tx| tx.insert(&tasks, 1, &Task {
        slug: "learn".into(), title: "Build something".into(),
        done: false, tags: vec!["rust".into()],
    }))?;
    let before = store.checkpoint()?;
    store.write(|tx| tx.update(&tasks, 1, |task| {
        task.done = true;
        Ok(())
    }))?;
    store.read(|tx| {
        let task = tx.find(&tasks, Task::SLUG, "learn")?.unwrap();
        assert!(task.done);
        Ok(())
    })?;
    store.restore_table(&tasks, before)?;
    assert!(!store.read(|r| Ok(r.get(&tasks, 1)?.unwrap().done))?);
    Ok(())
}
```

Add this unpublished local package using a path dependency. It reexports both
the `Model` trait and its derive macro, plus the engine as `engine`.

```toml
[dependencies]
weekend-db-typed = { path = "path/to/weekend-db/crates/weekend-db-typed" }
```

`Store::create_file(path)` and `open_file(path)` use the macOS, Linux, or Windows
NTFS adapter. Creation is exclusive. On macOS and Linux, close the store before
`Store::compact_file(path)`. Byte-backed
stores also support `create_on`, `open_on`, and `compact_to(&mut self, target)`.
`Store::initialize(database)` accepts a fresh revision-zero engine database with
custom limits; `from_database` opens one already initialized by this module.
Existing engine databases are never automatically adopted, repaired, or erased.

## Model declarations

Models are owned, nongeneric structs with named fields. Type/field IDs and
versions must be nonzero; field IDs are independent of names and declaration
order. A field attribute `id` identifies a schema field; the record ID is the
separate `u64` argument to insert/get/update. `#[weekend(name = "stored_label")]` lets a Rust field be renamed without
changing its descriptor label. Changing a published descriptor requires a new
version. Declare retired IDs with `#[weekend(reserved(3, 8))]`; never reuse them.

Supported field types: `String`, `bool`, `i8/i16/i32/i64`, `u8/u16/u32/u64`,
`f64`, `Bytes`, `Vec<T>`, `Option<T>`, and `Presence<T>`. `Vec<u8>` stores a
list of unsigned values; `Bytes(Vec<u8>)` stores a byte string. Narrow integers
are range checked, strings validate UTF-8, and floating-point values retain
their engine bit semantics. Custom types can implement `FieldValue`.
Custom codecs own their nested formats and evolution policies; generated
patching preserves unknown top-level fields and envelope keys.

`Option<T>` reads missing/null as `None` and writes `None` as explicit null.
`Presence<T>` distinguishes `Missing`, `Null`, and `Value(T)`, including when
the stored descriptor has a default; inspect `document.schema().read_field`
for an effective default. Nested nullable types such as `Option<Option<T>>`
are rejected. A missing list item has no representation; use null explicitly.

`#[weekend(default)]` uses `Default`; `#[weekend(default = EXPR)]` uses a native
value expression. Defaults must be deterministic, owned values. Stored schema
defaults govern declared fields. The derive's default is also an explicit
projection fallback for fields absent from an older schema. It does not silently
adopt a colliding unknown field. Model registration publishes predecessor schemas
in order: open/register the old model before the successor. It does not invent
historical descriptors from the latest Rust struct.

Unique scalar fields generate typed constants such as `Task::SLUG`; other
fields generate their numeric field IDs, such as `Task::TITLE`. Unique null or
physically missing values are unindexed, even when their schema supplies a
non-null default. Read projections can expose that default; unique lookups only
match explicitly stored values. A stale index pointing to a missing field is
reported as `IndexInconsistent`. String/byte lookups use exact bytes; floats use
bit equality. String indexes accept borrowed `&str` and Bytes indexes accept
`&[u8]` without a temporary owned query value. No normalization or ordered querying is implied. Unique index
layouts are fixed for a named table in this version; adding/removing an index
requires a future explicit reindex operation. Raw engine access can bypass all
these conventions. Index corruption is reported rather than repaired silently.

Renamed dependencies need an explicit derive path:

```ignore
use db::Model; // Cargo: db = { package = "weekend-db-typed", path = "..." }
#[derive(Model)]
#[weekend(crate = "::db", type_id = 10, version = 1)]
struct Item { #[weekend(id = 1)] name: String }
```

## Transactions, evolution, and history

`store.read` pins a current snapshot. `store.read_retained(id, ...)` reads a
fixed historical snapshot. `store.write` invokes its closure **once**, then
commits atomically. It never retries or replays side effects. Handle conflicts
explicitly. Any failed typed operation poisons the entire draft, even when the
caller ignores its error or catches a callback panic. Returning an error or
unwinding discards all staged writes.
Pass the writer to helper functions to compose one transaction. A nested call
to `store.write` starts an independent transaction; it does not join the outer
one, and raw engine catalog operations keep their immediate-commit semantics.

`Document<T>` is a read projection with `id`, version, raw record and schema.
Updates load the record inside their transaction, modify `T`, and patch stable
field IDs into the complete envelope. Unknown fields and envelope keys survive.
An older compatible model can update newer records; retired/incompatible fields
fail. Older models cannot insert new old-version rows once the table has advanced.
There is no detached-struct save operation.

Registering a new model version does not convert records. Updating an older
record through a newer model returns `MigrationRequired`. Call
`tx.migrate(&table, |old_record, new_model| { ...; Ok(()) })` explicitly: every
older row is scanned, converted once, validated, and committed with its indexes.
The callback can inspect retired raw fields; generated patching removes reserved
IDs. A collision with a prospective known ID or any conversion failure aborts
the whole transaction. Semantic conversions require application code.

`checkpoint`, `history`, `release_checkpoint`, and `read_retained` use the real
durable retention catalog. `restore_table` atomically restores **one table's
record data and unique indexes**, preserving unknown fields. It does not rewind
other tables, schema registration, or application ID allocators. Restoring an
older record version can require another explicit migration before an update.
Keep a checkpoint first when redo is wanted. Tables must exist in that checkpoint.

The runtime uses catalog/schema/index collections 1/2/3, with named record
collections registered thereafter. Table setup requires `&mut Store`; failed
registration can leave an unregistered empty collection because engine collection
creation is immediate. Failed fresh initialization can leave a partial image;
open rejects it. The runtime adds no engine-format or C ABI changes.

Engine size, observation, transaction and 64-snapshot limits still apply.
Restoration/migration must fit one transaction. All exact unique indexes share
a collection, so conservative absence tracking can conflict across tables.
Owned models and user defaults use ordinary Rust allocations; this is not a
process-memory quota. Compile-time macro dependencies are isolated from the
dependency-free engine. File persistence is available on macOS, Linux
(x86_64/aarch64), and Windows NTFS. Windows can write a compact copy via
`compact_to` and `engine::storage::FileStore::create(new_path)`.

## Compile-time contracts

Every field needs an explicit ID:

```compile_fail
use weekend_db_typed::Model;
#[derive(Model)]
#[weekend(type_id = 1, version = 1)]
struct MissingId { title: String }
```

Lists cannot be exact unique keys:

```compile_fail
use weekend_db_typed::Model;
#[derive(Model)]
#[weekend(type_id = 1, version = 1)]
struct BadIndex { #[weekend(id = 1, unique)] tags: Vec<String> }
```

Lookup values must match the indexed Rust field type:

```compile_fail
use weekend_db_typed::{Model, Result, Store};
#[derive(Model)]
#[weekend(type_id = 1, version = 1)]
struct Item { #[weekend(id = 1, unique)] slug: String }
fn main() -> Result<()> {
    let mut store = Store::memory()?;
    let items = store.table::<Item>("items")?;
    store.read(|r| r.find(&items, Item::SLUG, &42u64))?;
    Ok(())
}
```
