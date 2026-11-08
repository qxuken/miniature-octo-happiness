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
stack memory, the fixed budget-control allocation, application-owned inputs and
returned `Vec` buffers, foreign FFI handles, and backend allocations (native
path/handle metadata and simulator images/traces/pending writes). Values carried
inside returned buffers still retain their charged core nodes. Output buffers
are explicitly transferred out of charged ownership or use named
`copy_external`/`reserve_external` construction paths. Inspection's returned
metadata vector is an application output. Independent opens, inspection, import,
export, and backup calls create independent domains from their supplied limits;
there is no process-global cap. Replication/CRDT/coordinator application-layer
buffers are outside this core domain. C API defaults remain unchanged; this
configuration is currently exposed through the Rust `Limits` API.

Publication semantics are unchanged. A cap failure while preparing a candidate
before backend I/O leaves the committed head unchanged and can be retried in a
fresh transaction after releasing pinned allocations. I/O failures retain their
existing `Io`/`OutcomeUnknown` and recovery requirements. A cap too small to
verify a durable graph prevents opening it; it never authorizes partial recovery
or fallback to an older graph.
