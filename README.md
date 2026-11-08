# weekend-db

Licensed under MIT OR Apache-2.0. Cargo packages remain private.
[Operations guide](docs/operations.md) · [Private release contract](docs/release-contract.md) · [Roadmap status](docs/roadmap-status.md)

A weekend learning project: an embedded store for persistent nested values,
explicitly retained snapshots, and conservative record-scoped transactions.
Implemented in Rust, with a native Rust API and a C shared library for Odin.

The Rust convenience package is [`weekend-db-typed`](crates/weekend-db-typed/README.md).
It provides `#[derive(Model)]`, named typed tables, transaction closures,
unique lookups, explicit migrations, and history. The real Rust application is
[`weekend-tasks`](apps/weekend-tasks/README.md), a task-tracker CLI with tags,
notes, priorities, checkpoints, undo/redo, and compaction.

Compare how the core API and the typed package insert a task with a unique slug
in [Raw and typed Rust](#raw-and-typed-rust).

```sh
cargo run -p weekend-tasks -- demo
# Choose a new path. On Windows, use a path on local fixed NTFS.
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb init
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb add learn "Build a Rust app"
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb list
```

The core, convenience runtime, derive implementation, and application are separate
Cargo workspace packages. The default engine build has no external crate
dependencies. The derive builds with `syn`, `quote`, `proc-macro2`, and
`unicode-ident`.
Run `cargo fetch --locked` once before the offline check scripts on a fresh
checkout. `python3 tools/check.py` checks the whole workspace in debug/release,
including the CLI lifecycle and a renamed-dependency consumer.

[DESIGN.md](DESIGN.md) contains the complete target design, implementation
sketches, decisions, competitor comparison, reading references, milestones,
and definitions of done. The [production readiness roadmap](docs/production-readiness.md)
tracks release requirements, coverage, fuzzing, and operational gaps.

The in-memory engine through milestone 5 implements immutable nested maps/lists,
scalars/bytes, historical live snapshots, final put/delete overlays, and
conservative record-scoped optimistic transactions. Independent existing records
can merge; edits within one record conflict. Missing reads/new writes and scans
observe the collection, so unrelated new inserts can conflict. Databases and
immutable handles can be shared across threads; transaction drafts have one
owner and can transfer between threads. Publication is serialized by a mutex. The byte codec/durability simulator (milestone 6) and macOS
file storage with durable snapshot retain/release (milestone 7) are implemented.
Linux and Windows file persistence use the same v2 format. macOS uses
`F_FULLFSYNC`, Linux uses `fsync`, and Windows uses exclusive handles,
write-through I/O, and `FlushFileBuffers` on local fixed NTFS. See the
[file adapter contracts](DESIGN.md#windows-file-storage).
Milestone 8 adds optional schema descriptors and a typed Rust example with stable
field IDs, unknown-field preservation, explicit migrations, and an email lookup.

```sh
cargo run --example memory
cargo run --example simulated
cargo run --example typed
cargo run --example file -- /tmp/weekend-demo.db
python3 tools/check.py
```

Rust/Cargo 1.99+ is required.
The file example creates a new path, retains an old snapshot, reopens both
versions, releases retention, and leaves the database in place. Creation never
overwrites an existing file. The check script uses temporary paths and runs
debug/release tests plus memory/file Rust/C/Odin examples on macOS and Linux;
Odin uses its installed `vendor:wgpu` prebuilt static archive. `--skip-odin`
explicitly omits that integration. See [DESIGN.md](DESIGN.md#implemented-in-memory-contract)
for the API, resource limits, allocation behavior, and tested scope.

Rust callers can use `Database::create_on` / `open_on` with `MemoryStore` or
`SimulatedDisk` to exercise the real codec and recovery protocol. The simulator
models separate volatile/durable bytes, torn and short writes, failed syncs,
and reordered/dropped unsynced writes. Native `create_file` / `open_file` and C
`wdb_file_create` / `wdb_file_open` use the native adapter. See the
[v2 format and file contract](DESIGN.md#milestone-7-files-and-byte-format-v2).
The experimental v2 reader rejects v1 images; there is no automatic conversion.

The opt-in Rust `replication` feature ships verified snapshots to one passive
follower over an application-authenticated connection. It uses `sha2` to
verify transferred images and reports follower durability separately from
local commits. See [the replication API and failure cases](DESIGN.md#snapshot-replication).

The [StudyProgress peer demo](apps/study-peer/README.md) runs two writable
files in separate processes. Each accepts local edits while disconnected
and exchanges mergeable state in the background with one configured peer.
The demo reports local commit revisions separately from peer acknowledgments.

The [write coordinator demo](apps/write-coordinator/README.md) lets two
client processes edit one record in a single file. It uses ordinary
transactions and returns a conflict when a client submits a stale revision.

`retain_snapshot`, `retained_snapshot`, `retained_ids`, and `release_retained`
manage up to 64 persisted snapshot IDs per store. Releasing retention preserves
live readers and does not immediately reclaim file space. The current image
limit remains 64 MiB. Milestone 9 adds actual concurrent reader/commit/retention
stress, ASan/TSan checks, coverage tooling and platform CI jobs. Milestone 10 adds
quiescent compact copies, offline macOS/Linux file replacement, and baseline benchmarks. C close requires external quiescence of all calls; individual
handle release requires exclusive access. See the
[concurrency contract](DESIGN.md#milestone-9-concurrency-and-platform-contract).

The [schema/application contract](DESIGN.md#milestone-8-schema-and-application-contract)
documents `weekend_db::schema` and the handwritten `Users` wrapper. The typed
example demonstrates an additive rename/default, an explicit years-to-months
conversion, and retained schema/index versions. To exercise a real file:

```sh
cargo run --example typed -- /tmp/weekend-typed.db
```

This creates a new file and leaves it in place. Schema/index guarantees belong
to validated access: raw core writes can bypass them. Example write methods
abort the entire draft on error so partial primary/index changes cannot commit.

`cargo build --release` produces `target/release/libweekend_db.dylib` on macOS.
Foreign consumers use [include/weekend_db.h](include/weekend_db.h); the examples
arrange the loader search path with rpath. Shipping a relocatable bundle remains
future work. The CI matrix runs native checks on macOS, Linux, and Windows.
`tools/check.py` loads each shared library through its C ABI. File factories
support macOS, Linux (x86_64/aarch64), and Windows on local fixed NTFS.
Native Rust consumers use this crate directly. Run `cargo clean` to remove
engine artifacts.

The [milestone-1 C-versus-Rust experiment](probe/README.md) now exercises a shared
C ABI, immutable nested values, and C/Rust/Odin consumers, including the local
prebuilt wgpu-native integration. Run `python3 probe/run.py` on macOS. Its
results led to the accepted Rust/shared-library distribution. The probe remains
separate experiment evidence; its ABI is not the engine ABI.

Compaction copies the current root and every retained root, preserving their
IDs, revisions, schema records and secondary lookups. Native byte-backed stores
use `Database::compact_to(&mut self, destination)`. On macOS or Linux, close the database
first, then use `Database::compact_file(path, limits)` or C/Odin
`wdb_file_compact`. Replacement keeps both inodes locked through durable atomic
exchange. Hard-link sources are rejected. Interrupted operations may leave
`.wdb-compact-*.tmp` recovery copies; reopen the canonical path before removing
any leftovers. See the [compaction contract](DESIGN.md#milestone-10-compaction-and-measurements).
On Windows, use `compact_to` with `storage::FileStore::create(new_path)` to
write a verified compact copy to a new NTFS file.

To record release microbenchmarks and before/after compaction results:

```sh
python3 tools/benchmark.py --output target/my-benchmark-run --compact
```

The output directory must be new. The script records raw CSV and median/range
summaries; keep other test and I/O workloads idle during measurements.

## Raw and typed Rust

Both versions store task ID `1` with the slug `learn`. The core example also
marks the task done, looks it up by slug, and restores its earlier value from a
checkpoint. [Full core example](examples/raw_tasks.rs)

```sh
cargo run --example raw_tasks
```

### Core API

The application creates a task collection keyed by numeric ID and a separate
slug index keyed by the slug bytes. This is the insert from the runnable example.

```rust
use weekend_db::{Database, Error};

let db = Database::new()?;
let tasks = db.create_collection()?;
let by_slug = db.create_collection()?;
let id = 1u64;
let key = id.to_le_bytes();
let slug = b"learn";

let task = db
    .map()?
    .map_set(b"slug", &db.bytes(slug)?)?
    .map_set(b"title", &db.bytes(b"Build something")?)?
    .map_set(b"done", &db.boolean(false)?)?;

let mut insert = db.begin();
if insert.get(tasks, &key)?.is_some() || insert.get(by_slug, slug)?.is_some() {
    return Err(Error::AlreadyExists);
}
insert.put(tasks, &key, &task)?;
insert.put(by_slug, slug, &db.unsigned(id)?)?;
insert.commit()?;
```

`put` is an upsert. The two reads check for duplicates and record dependencies
that let commit detect a concurrent claim. The single commit publishes the task
and its index entry together. A slug lookup reads the ID from `by_slug` and
fetches the task from the same snapshot.

### Typed API

The model declares the unique slug. The typed transaction handles both writes.

```rust
use weekend_db_typed::{Model, Store};

#[derive(Model)]
#[weekend(type_id = 1001, version = 1)]
struct Task {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2)]
    title: String,
    #[weekend(id = 3)]
    done: bool,
}

let mut store = Store::memory()?;
let tasks = store.table::<Task>("tasks")?;
store.write(|tx| {
    tx.insert(&tasks, 1, &Task {
        slug: "learn".into(),
        title: "Build something".into(),
        done: false,
    })
})?;
```

The derive marks field `1` as unique and generates `Task::SLUG` for calls such
as `tx.find(&tasks, Task::SLUG, "learn")`. The store initializes collections
1–3 and assigns collection 4 to the first registered table.

| Collection | Entry for this example |
| --- | --- |
| 1. Catalog | `table/tasks` → record collection `4`, type `1001`, version `1`, unique field `1` |
| 2. Schemas | `(type 1001, version 1)` → task field definitions |
| 3. Shared unique indexes | `(collection 4, field 1, bytes, "learn")` → task ID `1` |
| 4. Tasks | Task ID `1` → task record |

All unique fields share collection 3. The index key combines the record
collection ID, field ID, value kind, and encoded field value. `tx.insert` checks
the primary and index keys and stages both entries in the same engine
transaction. `Store::write` commits after the closure and its typed operations
succeed. On a slug rename,
the typed layer checks the new key, removes the old index entry, and stages the
new index entry and updated task in that transaction. A change to `done` keeps
the existing index entry. [Typed package guide and complete example](crates/weekend-db-typed/README.md)
