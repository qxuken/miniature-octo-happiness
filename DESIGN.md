# weekend-db target design

This document describes a portable embedded database whose main abstraction is
an immutable application-state snapshot. A snapshot contains stable keyed
records; record values can contain nested persistent maps, lists, scalars, and
bytes. Applications read a fixed version, prepare changes privately, and publish
compatible record assignments atomically. Earlier retained versions remain
readable after reopening the database.

The target is a small C interface and a comprehensible implementation that can
be developed over successive weekends. Rust is the first consumer. The engine
supports memory and file storage; typed access, schema validation, application
migrations, and optional secondary lookups sit above it. This is a design target,
not a description of implemented or production-verified behavior.

The record-scoped transaction contract, durable commits, explicit retention,
and optional schemas are agreed decisions. The concrete tree shapes, internal
structures, ownership choices, and publication format below are **working
sketches**. They make the intended logic concrete without freezing an untested
file format. Remaining implementation choices are collected at the end.

## 1. The state that the database stores

A **record** is an entry identified by a stable collection identity and a byte
key. Its value may be a scalar, bytes, or an arbitrarily nested map/list. The
identity of a record does not depend on its memory address, physical tree node,
record schema, or position inside a list.

For example, `users[42]` can hold a map with `name` and `tags` fields. Changing
`users[42].name` constructs a new value for record 42. Changing a list inside
that record has the same transaction conflict scope. Two transactions editing
users 42 and 99 can merge; two transactions editing different fields inside
user 42 conflict initially. Stable schema field IDs make representation and
access more reliable, but do not automatically create finer conflict scopes.

A useful logical sketch is:

```text
RecordKey       = (collection_id, full_key_bytes)
Record          = { write_revision, immutable_value }
Collection      = { identity, content_revision, records_root }
DataState       = { data_revision, collections_root }
CommitState     = { commit_revision, data_state_ref, retained_snapshots_root }
RetainedSnapshot = { snapshot_id -> data_state_ref }
```

Schemas and secondary lookup maps are ordinary records in designated
collections. They participate in the same transaction mechanism as application
data. A snapshot therefore resolves the data and the metadata belonging to that
version together.

Separating `DataState` from `CommitState` avoids a retention cycle: a retained
snapshot references an immutable data version, not an entire commit object
whose retention catalog includes that snapshot. Retain/release operations can
publish new metadata without changing old data. An implementation may combine
these structures physically, but must preserve this reachability model.

Collection IDs are never silently reused. If name-based collection discovery or
collection deletion is exposed, resolving a name is itself a tracked metadata
lookup. A replacement collection receives a new identity. Reading a record
checks that its collection still exists with the expected identity; unrelated
record updates must not invalidate a successful point read merely because a
physical collections-root pointer changed.

## 2. Persistent values and candidate tree layouts

Values are immutable. `map_set(old, key, value)` or `list_set(old, index, value)`
returns a new value while `old` remains valid. The implementation copies the
changed path and shares unchanged subtrees. Sharing storage never creates a
mutable alias between records or snapshots. Pure persistent operations are the
initial model; in-place mutation of private nodes is a later optimization that
requires a separate ownership proof.

```text
old record value ----> old map root ----> old name
                            |
                            +----------> shared tags list
                                              ^
new record value ----> new map root -----------+
                            |
                            +----------> new name
```

Copying this path is a storage operation. It does not mean concurrent edits to
the two versions will be merged below their containing record's conflict scope.

The following is an abstract representation, not C memory layout or a disk ABI:

```text
Value = tag + scalar_bits_or_reference
  tags: none, boolean, signed_integer, unsigned_integer, float, bytes, map, list

MapBranch = { routing_metadata, child_refs[] }
MapEntry  = { routing_hash, full_key_bytes, value }
ListNode  = { child_refs[] or values[] }
ListRoot  = { length, height, node_ref }
```

Absence is separate from a stored `none` value. Byte values have explicit
lengths and may contain zero bytes. A reference distinguishes committed storage
from transaction-private objects. It is not an arbitrary application pointer;
references from another store or a closed transaction cannot be accepted as
local committed objects without explicit copying/remapping.

A straightforward first map is a fixed-fanout hash trie, with 16 branches as a
reasonable experimental starting point. Each level consumes four routing-hash
bits. Leaves retain the complete key, and equal full hashes are handled by a
collision container with full-key comparisons. Hashes route searches; they do
not define key identity. The algorithm, seed policy, and encoding must be
specified if the tree is persisted. Resource limits must also cover pathological
collisions and depth. Bitmap compression can follow a correct dense prototype.
Bagwell's [Ideal Hash Trees](https://lampwww.epfl.ch/papers/idealhashtrees.pdf)
is the primary structural reference, not evidence for this engine's performance.

For lists, start with an indexed radix tree for lookup, replacement, and append.
Middle insertion/removal may initially rebuild the affected suffix, with linear
work; make that cost part of the interface contract. A count-augmented B-tree is
an alternative if arbitrary insertion/removal becomes a measured priority.
This choice changes list implementation, not the record-scoped transaction
contract. Clojure's [persistent collection documentation](https://clojure.org/reference/data_structures)
is useful precedent for structural sharing and indexed persistent vectors.

A transaction may allocate several intermediate values before choosing its
final record assignments. Unreachable intermediates need not be serialized.
The safe initial memory strategy can be private arenas discarded on abort and
append-only committed arenas reclaimed by compaction/close. This trades memory
reuse for simpler lifetimes; its resource cost must be visible and bounded.
Do not promise cheap reclamation merely because a snapshot's retention was
released.

Disk encoding is explicit. Never serialize `sizeof(struct)`, C padding, native
pointers, bitfields, or host endianness. Specify scalar representations and
checked offset/length arithmetic before implementing the codec. A nine-byte
slot or a particular node capacity is an experiment, not an inherited xitdb
compatibility requirement.

## 3. The public interface and module seams

The external interface exposes opaque database, snapshot, and transaction
handles; immutable value access; collection operations; record get/put/delete;
commit/abort; and explicit snapshot retain/release. Names and exact signatures
remain sketches. Callers should not need to understand file offsets, physical
nodes, or commit-slot selection.

The collection module owns structural sharing and value lifetimes. The
transaction module owns observations, final assignments, validation, and commit
coordination. The storage module owns checked byte encoding, reads/writes,
synchronization, and recovery. Its storage seam has real adapters: a memory
byte store, a file, and a failure simulator. The slow logical reference model is
an oracle for behavior, not a substitute for exercising the real codec.

Foreign callers pass explicit-length slices and receive explicit status values.
Input slices are borrowed for the call unless another lifetime is documented.
The simplest initial output contract copies bytes into caller-owned buffers;
borrowed zero-copy views can be added only with a pinning/lifetime contract.
Values do not outlive their owning snapshot/transaction or an explicit value
handle. A Rust wrapper can encode these dependencies in lifetimes; the C
interface must document them regardless of wrapper checks.

A database close must not free state still used by live handles. A candidate
contract is to reject close while handles remain and make release explicit.
Transaction handles are single-thread-owned; distinct transactions may later
prepare concurrently. Snapshot acquisition/release and shared bookkeeping still
need synchronization even though the reachable values are immutable. Initial
coordination is in one process under one writer authority per store; a file
adapter must not permit an uncoordinated second writer. Multi-process protocols
and lock-free publication are later work.

Develop small C source files and generate one header with an implementation
switch. Prefix exported symbols and keep helpers internal. Compile both normal
and amalgamated distributions from multiple translation units. This source-level
integration is deliberate: the Rust reference documents composition problems
between independently built Rust static libraries, a packaging concern that
motivated the C implementation. [Rust linkage](https://doc.rust-lang.org/reference/linkage.html#mixed-rust-and-foreign-codebases)

## 4. Transactions contain final assignments

A transaction pins the current committed data snapshot at begin. It holds a
private overlay of final record assignments/removals. Repeated puts to one key
replace its pending assignment; there is no positional list-operation journal.
Reads consult the overlay first and otherwise the base snapshot, providing
read-your-writes without exposing another transaction's unpublished state.

```text
Transaction {
    base_commit_revision
    pinned_base_data
    point_observations: RecordKey -> expected_record_revision
    collection_observations: CollectionId -> expected_content_revision
    writes: RecordKey -> Put(immutable_value) | Delete
    private_allocations
    state: active | committed | aborted | failed
}
```

Each write captures a dependency on its target in the **base** state, even if
it is a blind overwrite. Reading a record already created by the transaction
does not assert that the new record existed in the base. Reading after a private
delete does not confuse private absence with an absence observed in committed
data. Implement observation capture centrally rather than in each wrapper.

The observation rules are deliberately conservative:

| Observation | Validation against the current committed state |
| --- | --- |
| Existing record or nested value | Same collection identity; record exists with the same write revision |
| Missing record, including a new write target | Same collection identity and collection content revision |
| Collection scan, count, or membership-dependent query | Same collection identity and content revision |
| Read of schema/name/lookup metadata | Apply the same rules to its ordinary record/collection |
| Read of the transaction's own overlay | Retain required base dependencies; do not invent observations of uncommitted records |

Every committed assignment changes its record generation even if the resulting
value compares equal. Every relevant collection write changes the collection
content generation, including accepted deletes that leave no current record.
Use monotonic revisions without wraparound or silent reuse; report exhaustion
rather than violating the comparison rule. Do not use node address equality as
proof that a record was not changed.

This catches ABA: a record changed and later restored to its original value
still has a different generation. Missing-key reads need collection tracking
because an insert followed by deletion can otherwise appear identical to the
original absence. The cost is false conflicts: two unrelated new inserts in one
collection can conflict even though two unrelated existing-record updates can
merge. More precise absence metadata is a later optimization, not a reason to
omit this check.

A transactional scan must register its collection observation before returning
results, including an empty result, and combine the base with the overlay.
Untracked historical snapshot reads cannot be used to claim transaction
isolation for decisions derived outside the transaction interface.

## 5. Validate and publish against the current root

The first implementation milestone uses a whole-database revision check. Any
intervening commit conflicts. This is a useful correctness baseline, but does
not satisfy the target of merging independent records. The next milestone
refines validation to the observations above.

The target commit path is conceptually:

```text
commit(tx):
    acquire commit gate
    try:
        require tx is active and database does not require recovery
        current = current committed state

        validate collection identities, observations, and write dependencies
        on conflict: abort tx; discard private state; return CONFLICT

        next_revision = checked successor of current commit revision
        candidate = current data state
        for each final assignment in tx.writes:
            apply assignment; stamp touched records and collections
        preserve current retention metadata and unrelated records
        on preparation failure: abort tx without publication; return error

        persist candidate and publish commit metadata
        after durable publication: expose new head; finish tx; return OK
        on definite prepublication storage failure: abort tx; require recovery;
            return IO_ERROR
        on publication uncertainty: fail tx; require recovery;
            return OUTCOME_UNKNOWN
    finally:
        release commit gate
```

Validation, materialization against `current`, and publication share the gate.
This avoids a race where a validated candidate becomes stale before publishing.
The gate initially covers storage synchronization too: shorter critical sections
require another protocol and are an optimization, not an assumption.

There are two independent persistence orders to respect. The new root must not
point to undurable data; and the in-process head must not expose a commit as
successful before its durability contract is met. No user migration, callback,
notification, or other application side effect executes automatically while
rebasing a transaction. A caller retries a conflict by starting a fresh
transaction and recomputing its logic, with externally visible side effects
kept outside the uncommitted work.

Why read validation matters is visible in a small example. Start with `a = b =
true`. T1 reads `b` and sets `a = false`; T2 reads `a` and sets `b = false`.
Their writes are disjoint, but accepting both observations produces a result
neither sequential execution would produce. Validation rejects the second
commit. An empty scan or an index-based lookup has equivalent dependency issues.
The [original OCC paper](https://www.eecs.harvard.edu/~htk/publication/1981-tods-kung-robinson.pdf)
provides the private-work/validation/publication framework; PostgreSQL's
[isolation documentation](https://www.postgresql.org/docs/18/transaction-iso.html)
explains serialization anomalies and predicate observations. This design is not
PostgreSQL's SSI implementation, and must not claim serializability before its
complete supported observation surface has been verified.

Coarse record boundaries keep list semantics ordinary. If two transactions edit
different positions inside one record's list, the second conflicts. Inserting
before an element never silently retargets another transaction's write. Adding
fine-grained field scopes or list identity/rebasing later would be a deliberate
contract change. Automerge's [ID-based list merge rules](https://automerge.org/docs/reference/under-the-hood/merge-rules/)
are a reference for that separate experiment, not part of this target.

## 6. Candidate persistence protocol

An append-only copy-on-write store fits the persistent graph: append newly
reachable objects, then publish a small root descriptor. It can provide atomic
commit without a separate WAL because the appended objects already are the new
representation. This is the working candidate, not a proven protocol or fixed
file layout. redb's [design](https://github.com/cberner/redb/blob/master/docs/design.md)
is useful precedent for copy-on-write storage and redundant commit metadata;
its allocator and page-reuse design should not be imported wholesale.

```text
immutable file header: magic, format version, file identity, format parameters
commit slot A: revision, commit descriptor reference, committed end, checksum
commit slot B: revision, commit descriptor reference, committed end, checksum
append area: checked object blocks and immutable commit/data descriptors
```

Slots must occupy independently protected write regions under the declared
failure model. Likewise, appending bytes must not damage the preceding durable
commit through a shared torn-write unit. Alignment/padding, protected-region
size, and safe-append assumptions must be specified and tested; placing two
headers at different byte offsets is not itself sufficient. Never assume a
C struct write or a small write is power-fail atomic.

While holding the commit gate, serialize newly reachable objects, preserving
shared references through a private-object-to-offset map. Validate every length,
offset, and integer conversion. Append the complete candidate and its commit
descriptor; synchronize it before writing the inactive/older commit slot. Then
write that slot, synchronize it, and only afterward expose/acknowledge the new
commit. Keep the previous slot's published state untouched until this sequence
has completed. Creation and replacement also need the platform's directory and
file-lifecycle synchronization rules.

Checksums cover actual encoded object payloads and relevant metadata, not only
the root's offset. They detect damage, not reconstruct arbitrary lost bytes.
The codec must establish how a candidate root is verified: full traversal is
simple but expensive; validated child checksums and lazy reads need an explicit
policy for corrupted referenced payloads. Selecting a header with a valid
checksum does not prove its entire object graph is readable.

The file identity, revision, descriptor reference, and committed boundary must
be bound by the metadata validation. Referenced offsets must lie within the
accepted committed boundary, not merely somewhere inside the current file.
Checksums do not replace checked parsing or establish authenticity of malicious
input; representation validation remains necessary even for matching checksums.

Recovery examines bounded, versioned metadata and accepts a complete published
generation under the specified fault model. A durable append with no valid
published slot is not automatically a commit: scanning for the highest trailing
object must not accidentally publish a transaction that never reached its
publication step. Unreferenced tail data can be discarded or left for compaction.
A bad newest slot after an interrupted publication may permit the older slot;
arbitrary damage to an already acknowledged generation must not be silently
hidden as a successful rollback. The verification/recovery policy must
separate interruption cases from corruption outside its assumptions.

Failure before publication may safely leave garbage. Failure once publication
has begun can make the outcome uncertain, even if the call returned an error.
Mark the database handle as requiring recovery before further writes. Reopening
must produce an allowed whole state, and callers must be told when they cannot
infer whether a commit happened. The exact status names are still interface
sketches; the distinction is mandatory.

SQLite's [atomic-commit analysis](https://www.sqlite.org/atomiccommit.html)
is the reference for making storage assumptions and ordering explicit. Its
[journal/WAL isolation explanation](https://www.sqlite.org/isolation.html)
also helps distinguish durable publication from concurrent editing. A WAL is a
possible alternative if later workloads justify it; adding one is not an
independent guarantee of durability or concurrent writers.

## 7. Retention, lifetimes, and compaction

A live snapshot pins an immutable data root in the open database. Durable
retention records a snapshot ID and data-root reference in committed metadata.
Releasing a live handle and releasing durable retention are different actions;
neither may invalidate another live reader. Retain/release publication follows
the same failure rules as ordinary commits.

When data changes, carry forward the **current** retention catalog. A stale
transaction must not resurrect a retention entry released by another commit or
drop a recently retained snapshot. Metadata-only commits may conservatively
conflict with the whole-root baseline, but record-scoped validation need not
invalidate unrelated record reads just because retention changed.

An append-only prototype does not need a free-page allocator. Aborted private
allocations can be discarded, while committed space remains until compaction.
Bound transaction memory, value sizes, nesting depth, and revision counters;
report resource errors instead of allowing silent corruption or unbounded
recursive processing of malformed input.

Initial compaction can require a quiescent store. Traverse current data, all
retained roots, their schema/index metadata, and any additional roots required
by the ownership contract. Copy them into a new store with an old-reference to
new-reference map so sharing survives. Commit and verify the replacement before
replacing the original. Failure must leave at least one complete usable store.
The platform adapter owns the tested replacement protocol. Old IDs/generations
must either remain meaningful or be explicitly remapped; active transaction
survival across replacement is not promised.

## 8. Schemas make records interpretable

A schema describes a logical record, not the memory layout of a C or Rust
struct. A useful representation is an ordinary catalog entry keyed by stable
type identity and schema version. Record fields use stable field IDs, and the
record identifies which descriptor interprets them. A descriptor is immutable
once published; a new version is a new entry.

```text
schemas[(UserType, 1)] = {
    fields: { 1: {name: "name", kind: bytes}, 2: {name: "age", kind: uint} }
}
users[42] = {
    type: UserType, schema_version: 1,
    fields: { 1: "Alice", 2: 25 }
}
```

This can be a convention expressed with core maps, not another mandatory
storage type. Exact key encoding is still to be chosen. A schema fingerprint
can identify a descriptor, but does not replace stable field identity or specify
a migration. A source rename keeps the same field ID; source declaration order
and field-name hashes must not silently renumber stored fields. Removed IDs
must not acquire new meanings. Protocol Buffers' [field evolution rules](https://protobuf.dev/programming-guides/proto3/#updating)
provide useful precedent without committing this project to its encoding.

Wrappers check schema compatibility and field types. They must distinguish
missing fields from explicit `none` and define defaults per schema version.
Unknown-field handling must be explicit; an update through an older supported
wrapper should preserve fields it does not understand rather than overwrite a
whole decoded native struct and erase them. A setter can expose field-level
convenience while producing a final record assignment in the transaction.

The raw core still permits schemaless records and raw writes. Therefore schema
validity, uniqueness, and generated-index consistency are guarantees of the
validated access layer, not unconditional properties of every byte written
through the core. Expanding that guarantee would require engine enforcement or
restricting the writable interface, which is outside the initial target.

## 9. Lookup and migration remain explicit work

A schema can guide typed field access and stable key construction. Searching by
email requires data organized for that lookup, for example
`users_by_email[email] -> user_id`. Update this map and the primary record in
one transaction. A uniqueness check must read the existing lookup through the
transaction interface before writing, so competing insertions invalidate safely.
Raw writers bypassing that convention cannot be promised uniqueness.

A lookup followed by a record read tracks both observations. A schema or index
catalog change also participates in validation when consulted. Retained
snapshots resolve the primary and secondary roots from the same data version.
Metadata describing an index does not build or maintain its entries; start with
one handwritten index before generating that maintenance. Range lookup would
need ordered storage and observation tracking, not just a hash-map declaration.

Record migration is application code using ordinary transactions. Adding an
optional field may need only a descriptor/default rule; a rename may retain its
ID; a type/meaning change or a split requires a transformation. Publish the
new descriptor and migrated data consistently. Old retained snapshots continue
to resolve the old descriptor and old data. They are not rewritten in place.

An all-at-once migration is atomic but can be large and conflict-prone. A batched
migration can leave mixed record versions and needs an application rollout
protocol and compatible readers. Choosing between these is application work,
not an implicit engine feature. A typed read from old data into a new struct is
also a conversion policy, not proof that persistent data was migrated.

Engine-format migration is separate: a tool needs an old-format reader and a
new-format writer, or a specifically implemented converter. Initial experimental
files may simply be rejected by a newer incompatible engine. Never confuse an
application schema-version field with the engine file-format version.

## 10. Evidence required before trusting the design

The reference model establishes logical results; the persistent implementation
must agree on deterministic and randomized nested-value operations. Interleaved
transaction schedules cover independent records, same-record edits, absence,
scans, schema/index dependencies, ABA, aborts, and allocation failures. A conflict
must never expose a subset of the rejected assignments. Actual thread tests
follow a verified single-thread schedule model.

The storage simulator maintains separate volatile and durable images and
explores failure points, permitted write reorder/drop behavior, torn writes,
and errors during recovery. After each interruption, reopen and verify the
complete reachable graph and retained metadata. All acknowledged durable commits
must survive under the model; an interrupted commit may be present or absent
but never partially visible. A process-kill test complements this simulator;
it does not replace power-loss modeling.

Fuzz both operation sequences and stored bytes. Check lengths and offsets,
cycles/depth/resource exhaustion, malformed tags, hash collisions, stale handles,
and use-after-free through sanitizers and targeted tests. Coverage and mutation
checks evaluate whether the tests exercise important decisions; they are not
proof of durable semantics. SQLite's [testing account](https://www.sqlite.org/testing.html)
is the reference for combining coverage with fault injection, fuzzing, and
testing shipped configurations. Test release builds and the generated C header
as well as instrumented development builds.

Measure reads, nested updates, list middle edits, batches, allocation traffic,
conflict rates, retained-history growth, and durable commit latency. Compare
matching durability and retention policies. The coarse conflict model and
append-only growth are deliberate starting costs, not performance claims.

## 11. Decisions still open before implementation freezes

The first implementation exercises must choose the C baseline, allocator and
handle contract, exact scalar/key encodings, map hashing policy, list layout,
checksums, protected-write assumptions, root verification strategy, storage
callbacks, and resource limits. Each needs a small contract and acceptance tests.
The working sketches above suggest a path but do not quietly settle them.

The target deliberately defers field-level conflict domains, list rebasing,
replication, multi-process writer coordination, general queries, automatic
schema enforcement, and generated migrations. These can be revisited after a
measured workload justifies their additional invariants. The implementation
sequence and stopping points are in [ROADMAP.md](ROADMAP.md); the compact agreed
scope is in [PLAN.md](PLAN.md).

## References and rationale map

- [Bagwell, Ideal Hash Trees](https://lampwww.epfl.ch/papers/idealhashtrees.pdf):
  primary map-tree background; no borrowed benchmark claims.
- [Clojure persistent collections](https://clojure.org/reference/data_structures):
  value semantics, structural sharing, and indexed vector precedent.
- [Kung and Robinson, optimistic concurrency](https://www.eecs.harvard.edu/~htk/publication/1981-tods-kung-robinson.pdf):
  private work, validation, and publication.
- [PostgreSQL transaction isolation](https://www.postgresql.org/docs/18/transaction-iso.html):
  dependency/phantom reasoning; not the algorithm chosen here.
- [redb storage design](https://github.com/cberner/redb/blob/master/docs/design.md):
  copy-on-write and redundant publication metadata, with different reclamation.
- [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html): persistence
  assumptions, ordering, and failure cases.
- [SQLite isolation](https://www.sqlite.org/isolation.html): snapshot readers,
  serialized writers, and stale-snapshot behavior.
- [SQLite testing](https://www.sqlite.org/testing.html): fault simulation,
  malformed inputs, coverage, and delivered-build testing.
- [Protocol Buffers schema evolution](https://protobuf.dev/programming-guides/proto3/#updating):
  stable field numbers and compatibility discipline.
- [Rust linkage](https://doc.rust-lang.org/reference/linkage.html#mixed-rust-and-foreign-codebases):
  the source-distribution rationale for a C implementation.
- [Automerge merge rules](https://automerge.org/docs/reference/under-the-hood/merge-rules/):
  an intentionally deferred approach to list-operation identity.
- [ADR 0007](docs/adr/0007-merge-stable-record-assignments.md): the accepted local
  decision to use stable record assignments instead of positional rebasing.
