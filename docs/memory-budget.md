# Live core allocation admission

`Limits::max_live_bytes` bounds the sum of requested heap layouts owned by one
core `Memory` domain, including in-flight allocation reservations. The default
is `usize::MAX`, preserving unlimited aggregate admission. Exhausting the cap
returns `Error::Limit`; actual allocator failures remain `Error::OutOfMemory`.

```rust
use weekend_db::{Database, Limits};
let db = Database::with_limits(Limits {
    max_live_bytes: 8 * 1024 * 1024,
    ..Limits::default()
})?;
let usage = db.memory_usage();
assert!(usage.live_bytes <= usage.limit_bytes);
# Ok::<(), weekend_db::Error>(())
```

`MemoryUsage` reports `live_bytes`, `peak_bytes`, and `limit_bytes`. Measurements
are concurrent samples, not a transactionally frozen view. The reported peak
is at least the sampled live count, and a query persists any higher observed
live count so later queries cannot forget it during an in-flight admission.
Peak counts include reservations that later
fail allocation. `Value::memory_usage()` reports the same domain after its
originating database is dropped.

The domain counts shared value/tree/head allocations, their reference-count
and budget-ownership headers, byte values, keys, collision leaves, draft key/
observation/write buffers, retained-root catalogs, codec/cache/recovery buffers,
and logical import/export working buffers. Immutable sharing is charged once:
cloning a value or snapshot does not allocate or increase admission. Pinned old
roots and multiple concurrent drafts keep their allocations charged until the
last owner releases them. Successful publication need not immediately free old
roots if an application still holds them.

Internal `ChargedVec` has an exact allocated capacity and never reallocates in
`push`, `extend`, or `resize`: callers reserve explicitly. Growth reserves the
entire new allocation while the old allocation remains charged. This can reject
a growth operation even when its final capacity would fit; the temporary peak
must also fit. Reservation failure preserves the old vector. Admission is an
atomic checked operation shared by all threads. Charges roll back on allocation
failure and remain attached through element destruction, deallocation, and
partially consumed owned iterators, including unwinding destructors.

This is **not a process RSS limit**. Exclusions are allocator metadata/rounding,
stack memory, the fixed budget and allocator ownership blocks, application-owned
inputs and returned `Vec` buffers, foreign FFI handles, and backend allocations (native
path/handle metadata and simulator images/traces/pending writes). Values carried
inside returned buffers still retain their charged core nodes. Output buffers
are explicitly transferred out of charged ownership or use named
`copy_external`/`reserve_external` construction paths. Inspection's returned
metadata vector is an application output. Independent opens, inspection, import,
export, and backup calls create independent domains from their supplied limits;
there is no process-global cap. Replication/CRDT/coordinator application-layer
buffers are outside this core domain. Rust and C database options accept the
same limits and expose the same budget measurements.

## Per-database allocators

Rust and C use `Global` by default. In Rust, `AllocatorHandle::try_new`
fallibly takes ownership of the provider. Set it and the `Limits` on
`DatabaseOptions` for the in-memory, byte-store or native-file constructors.
In C, initialize `wdb_database_options` with `wdb_database_options_init`,
then override any limits and optionally set a `wdb_allocator`. Pass the
options to `wdb_open_with_options`, `wdb_file_create_with_options` or
`wdb_file_open_with_options`. A null descriptor selects `Global`.
`wdb_database_get_limits` and `wdb_database_get_memory_usage` return the
same fields as their Rust counterparts.

Budget charges keep only the budget alive. Shared nodes, charged vectors and
boxes carry the allocator they use to free storage. The selected allocator
serves core layouts, C scan rows and keys, retained-ID scratch space, the
storage adapter's owning box, C handle blocks, and the macOS/Linux parent
path and file leaf name. The owning box, C handle blocks, and path buffers are
outside the live-byte cap. Native create and open reserve each path buffer,
including its terminating zero, in one fallible request. The parent buffer is
freed after opening the directory and before reserving the leaf. The leaf
buffer stays in the file lease. Failure to allocate either buffer precedes
creation or opening of the database file. On Windows, the selected allocator
also provides the temporary UTF-16 buffers for parent normalization, canonical
path lookup, and the assembled file path. The adapter releases them before
returning the lease. Allocator refusal during path construction precedes the
target-file open on all three platforms.

The budget control block uses `Global` and is fallible. A custom provider
allocates one block containing the provider and its reference count. The handle
is a single pointer, and its clones share the block without allocating. The
last handle moves the provider out, frees the block through it, and then drops
the provider. This keeps the C context alive through the block's deallocation.
The default handle uses `Global` directly with no provider block.
Public Rust `Vec` outputs also use `Global`. The engine moves elements from
custom storage into a fallibly reserved output buffer. Values inside it still
own their original core nodes. OS internals and caller-owned backend storage
use their own allocation paths.

The database owns a reference to its allocator. Retained roots, live snapshots
and cloned Rust values keep the allocation owner alive. A value can outlive the
database that created it. C close returns `WDB_BUSY` while opaque handles are
live. The C descriptor is copied at open. A supplied context retain/release pair
keeps the context alive through the final free, including cleanup after failed
open. Callbacks must be thread-safe and must avoid reentering the database.
Allocator refusal reports `OutOfMemory` and releases provisional charges.

Publication semantics are unchanged. A cap failure while preparing a candidate
before backend I/O leaves the committed head unchanged and can be retried in a
fresh transaction after releasing pinned allocations. I/O failures retain their
existing `Io`/`OutcomeUnknown` and recovery requirements. A cap too small to
verify a durable graph prevents opening it; it never authorizes partial recovery
or fallback to an older graph.

Codec append buffers now select bounded geometric capacities; large known
Writer chunks remain exact. Unused capacity counts as live charge. If a rounded
reservation returns `Limit`, a smaller exact reservation may succeed, while
`OutOfMemory` is returned directly. Previously retained slack can still make an
old tight budget insufficient. The [growth comparison](serialization-buffer-growth.md)
measures this tradeoff and separate copy-domain peaks; it does not change the
accounting boundary or establish a process RSS limit.

Serialization caches now retain their lookup indexes inside immutable shared
owners. Unchanged candidates reuse those owners; a first append privately forks
only the affected cache, preserving the old owner's full capacity on failure.
Cache/index copies and the new shared header are fallible and charged before
publication I/O. Native open constructs the encoder indexes once after graph
verification and before stabilization. The [cache-sharing comparison](serialization-cache-sharing.md)
records about 129 KiB more reopened core usage in the compound fixture, reduced
publication clone/rebuild work and increased cold-open work. Old exact budget
thresholds are not guaranteed unchanged.
