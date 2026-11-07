# weekend-db target design

This document describes a portable embedded database whose main abstraction is
an immutable application-state snapshot. A snapshot contains stable keyed
records; record values can contain nested persistent maps, lists, scalars, and
bytes. Applications read a fixed version, prepare changes privately, and publish
compatible record assignments atomically. Earlier retained versions remain
readable after reopening the database.

The target is a small C-compatible interface and a comprehensible implementation
that can be developed over successive weekends. Rust is the implementation
language and first native consumer. Foreign consumers, including Odin, use a
C header and Rust `cdylib`; the user accepted this distribution on 2026-11-05
after the C-versus-Rust packaging experiment. The in-memory engine through
milestone 5, the milestone-6 byte codec/simulator, and milestone-7 macOS files
and durable retained snapshots are implemented. Milestone 8 adds optional schema
metadata and a handwritten typed Rust application with explicit migrations and
secondary lookup maintenance. Milestone 9 adds synchronized publication, real
thread overlap, stress tests, sanitizers, coverage, and platform build checks.
Milestone 10 adds quiescent compaction, safe macOS file replacement, and baseline
measurements. The Rust convenience workspace and task CLI are implemented;
the subsequent Linux persistence extension adds native file storage and
compaction without changing v2 bytes.
Implemented behavior and its limits are recorded below; this is not a
production-safety claim.

The record-scoped transaction contract, durable commits, explicit retention,
and optional schemas are agreed decisions. The concrete tree shapes, internal
structures and ownership choices are **working sketches** except where the
implemented contracts and experimental v2 byte specification define current
behavior. V2 is versioned and testable, but is not a stable release ABI.
Remaining implementation choices, implementation milestones, reading assignments,
and definitions of done are collected below. This is the single source of truth
for the project; earlier separate plans and decision records are consolidated here.


Use this document as both the target and the implementation workbook:

- [Architecture and invariants](#1-the-state-that-the-database-stores)
- [Primary reading references](#references-and-rationale-map)
- [Language and distribution decision](#language-and-distribution-rust-with-a-c-abi)
- [Implemented in-memory contract](#implemented-in-memory-contract)
- [macOS files and retained snapshots](#milestone-7-files-and-byte-format-v2)
- [Schemas and typed Rust example](#milestone-8-schema-and-application-contract)
- [Concurrency and platform evidence](#milestone-9-concurrency-and-platform-contract)
- [Compaction and measurements](#milestone-10-compaction-and-measurements)
- [Rust convenience package and task application](#rust-convenience-package-and-task-application)
- [Linux persistence and recovery results](#linux-persistence-and-recovery-results)
- [Review corrections: scans and unique presence](#review-corrections-scans-and-unique-presence)
- [Milestones and definitions of done](#implementation-milestones)
- [Comparison with existing solutions](#comparison-and-reason-to-build-this)
- [Decision summary](#decision-summary-and-deferred-alternatives)
- [Vocabulary](#vocabulary) and [shared definition of done](#shared-definition-of-done)

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

A database close must not free state still used by live handles. The implemented
C contract rejects close while handles remain and requires external quiescence
for close and exclusive access for handle release. Transactions have one owner,
can transfer between threads, and distinct drafts prepare concurrently. Database,
snapshot, immutable value, and materialized scan handles can be shared. Head
acquisition uses a mutex; traversal uses immutable, atomically counted nodes.
Coordination is in one process under one writer authority per store; a file
adapter must not permit an uncoordinated second writer. Multi-process protocols
and lock-free publication are later work.

Rust won the language experiment. The engine exports `wdb_` symbols from a
`cdylib` and ships [`include/weekend_db.h`](include/weekend_db.h). Native Rust
consumers use the `rlib`. The C probe's generated implementation header remains
experiment evidence; C-only source distribution is not the engine contract.

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

## 6. Experimental persistence protocol

An append-only copy-on-write store fits the persistent graph: append newly
reachable objects, then publish a small root descriptor. It can provide atomic
commit without a separate WAL because the appended objects already are the new
representation. Milestone 6 selected v1; milestone 7 extends it to v2 below.
it is not a stable release format or a hardware durability certification.
redb's [design](https://github.com/cberner/redb/blob/master/docs/design.md)
is useful precedent for copy-on-write storage and redundant commit metadata;
its allocator and page-reuse design should not be imported wholesale.

```text
immutable file header: magic, format version, file identity, format parameters
commit slot A: revision, commit descriptor reference, committed end, checksum
commit slot B: revision, commit descriptor reference, committed end, checksum
acknowledgment witness A: independently protected durable revision lower bound
acknowledgment witness B: independently protected durable revision lower bound
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
write that slot, synchronize it, then write/synchronize its witness before
exposing/acknowledging the new commit. Keep the previous slot's published state untouched until this sequence
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

### Milestone 6 byte format v1

This is the historical milestone-6 encoding, superseded by the v2 amendment
below. Shared layouts/assumptions remain applicable except where v2 overrides
them. The encoding stores logical values
and directories rather than Rust structures or the in-memory trie layout.
No native pointers, host padding, host endianness, or routing hashes are stored.

**Adapter assumptions:** one exclusive writer lease covers recovery/publication;
reads see its volatile bytes; successful sync durably orders every preceding
write; errors may perform partial I/O or persist unsynced bytes. A torn write
cannot damage another protected region. An aligned safe append cannot damage
preceding durable regions or truncate durable bytes. Previously synchronized
bytes survive modeled power loss. No write is assumed atomic. These are declared
requirements, not established hardware guarantees. The milestone-7 adapter
implements the macOS locking/sync/lifecycle policy described below.

Integers, lengths, and offsets are little-endian; unspecified/reserved bytes
must be zero. Lengths/references are checked `u64` byte counts/offsets. A protected
region is **4,096 bytes**. Images are capped at **64 MiB**, graph traversal at
16,384 objects and 128 MiB of encoded read work including duplicate header reads.
Caller `Limits` also constrain keys, counts, nesting, and logical value weight;
a stricter reader policy may return `Limit` for an otherwise valid image.

| Region / byte offset | Contents |
| --- | --- |
| 0 / 0 | Immutable header |
| 1 / 4,096 | Root slot A, index 0 |
| 2 / 8,192 | Root slot B, index 1 |
| 3 / 12,288 | Acknowledgment witness A |
| 4 / 16,384 | Acknowledgment witness B |
| 5 onward / 20,480 | Aligned objects and unreachable append garbage |

The one-region header contains magic `WEEKDB01` at 0; `u32` version 1 at 8;
zero `u32` flags at 12; region size `u64` at 16; append start `u64` at 24;
nonzero 16-byte file identity at 32; maximum image bytes `u64` at 48; maximum
objects `u64` at 56; checksum `u64` at 64; zero fill from 72 onward. These format
parameters must match the v1 constants. Creation accepts an explicit nonzero
identity; a real adapter needs a uniqueness policy. Unknown header magic/version
returns `UnsupportedFormat`.

Slots/witnesses each fill a region. Their magic is `WDBSLOT1` / `WDBACK01`.
Both contain `u32` version at 8, `u32` slot index at 12, identity at 16, revision
`u64` at 32, commit-object offset `u64` at 40, committed end `u64` at 48, previous
revision `u64` at 56, checksum `u64` at 64, and zero fill from 72 onward. Previous
revision is `revision - 1`, or zero for initial revision 0. Slot index equals
`revision & 1`; two valid slots have consecutive revisions. Committed end is
aligned, within physical length/image cap, and exactly one region after the
commit-object offset. All-zero metadata means unused. Identity, revision,
descriptor, committed boundary, and physical slot are all checked together.

Every object is region-aligned and occupies
`align_up(64 + payload_length, 4096)` bytes:

| Offset | Encoding |
| --- | --- |
| 0 | Eight-byte magic `WDBOBJ01` |
| 8 | `u32` kind: value 1, data state 2, commit state 3 |
| 12 | `u32` object version 1 |
| 16 | `u64` payload length |
| 24 | `u64` birth revision |
| 32 | 16-byte file identity |
| 48 | `u64` checksum |
| 56 | Eight zero bytes |
| 64 | Payload followed by zero padding to the aligned end |

All checksums use **CRC-64/ECMA-182**, polynomial `0x42f0e1eba9ea3693`, initial
value/xor-out zero, non-reflected. They cover the entire encoded region/object,
including metadata and padding, replacing the checksum's own eight bytes with
zero. The check value for `123456789` is `0x6c40df5f0b497347`. Checksums detect
damage, not authenticity, and representation checks apply even when they match.

Value payloads begin with a `u64` tag: none 0, boolean 1, signed integer 2,
unsigned integer 3, binary64 float 4, bytes 5, map 6, list 7. Tags 0–4 have one
additional `u64`: none requires zero, boolean 0/1, signed integers use
two's-complement bits, and floats preserve IEEE-754 bits. Bytes have a `u64`
length and exactly that many bytes. Maps have a `u64` count followed by
`(key_length:u64, full_key_bytes, child_offset:u64)` in strictly increasing binary
key order. Lists have a `u64` count followed by child offsets in index order.
Empty keys/bytes and stored none are valid. Duplicate/unsorted keys, unknown
tags, trailing payload bytes, and noncanonical scalars are rejected.

Data-state payload: `(data_revision:u64, next_collection_id:u64,
collection_count:u64)`, then collections in strictly increasing ID order. Each
collection is `(id:u64, content_revision:u64, record_count:u64)`, followed by
records in strictly increasing binary key order. A record is
`(key_length:u64, key_bytes, write_revision:u64, value_offset:u64)`. IDs are
nonzero and below the persisted nonzero next ID. Record/collection revisions
are nonzero and satisfy
`write_revision <= content_revision <= data_revision <= commit_revision`.
The next-ID counter survives deletion/reopening, preventing silent reuse.

Commit-state payload: four `u64`s for commit revision, data-object offset, data
revision, and retained-root count. The implemented retention count is zero;
nonzero currently returns `UnsupportedFormat`. The commit object occupies one
region at the published boundary and its birth revision equals the slot revision.
Data-object birth revision equals data revision. Value-object birth revisions
must not exceed the selected commit revision. Read-only commits reuse the data
object while advancing commit revision. V2 adds durable retain/release below.

All references point strictly backward to aligned objects, whose complete
extents precede the referencing object and stay inside the accepted committed
boundary. Recovery rejects overlapping distinct objects, forward references,
cycles, wrong kinds/identities, overflow, invalid lengths/revisions, nonzero
reserved/padding bytes, and exceeded limits. It validates the full reachable
graph before exposure. Decode memoizes offsets to preserve shared values and
checks aggregate depth/weight while rebuilding maps/lists. Unreferenced old
objects and trailing bytes are not members of the accepted graph.

**Layout cost:** small values occupy a region each. A changed map/list rewrites
its logical reference array; a data change rewrites the collection/record
directory. Unchanged value objects share offsets across records/commits, but
in-memory hash/radix branches are not yet individually persisted. Logarithmic
disk writes for leaf edits are not established by this codec. The identity cache
pins committed value handles until close so allocator address reuse cannot alias
an old offset. Image/object caps bound the prototype; tuning/reclamation remain
later work. This deliberately leaves room for a versioned physical tree format.

### Publication, recovery, and simulator decisions

While holding the existing commit gate, validate observations, build the candidate
native head, encode reachable objects, and allocate the replacement identity
cache. Preparation allocation/limit errors write nothing and leave the store
usable. Then:

1. Append children, changed values/directories, and commit descriptor after the
   aligned physical end. Never reuse a partially occupied tail region.
2. Sync appended bytes before any root references them.
3. Write the inactive root slot and sync it, leaving the current slot intact.
4. Write that slot's independent acknowledgment witness and sync it.
5. Install the in-process head/cache and acknowledge success. No fallible
   preparation remains after the durability barriers.

Creation writes the header, zero metadata, and initial empty objects before
publishing revision 0 in slot A through these barriers. Interrupted creation has
no prior acknowledged state: opening may recover complete revision 0 or reject
an incomplete image. Creating over existing bytes is rejected without changes.
Short writes are completed without crossing a protected region; zero progress
fails. Physical shortening below the accepted end is corruption and blocks writes.

A definite append/first-sync failure returns `Io`, aborts the draft, and requires
recovery. Once a root-slot write is attempted, any storage error returns
`OutcomeUnknown`, fails the draft, and requires recovery. In-process reads retain
the last acknowledged head; subsequent catalog writes/commits return
`RecoveryRequired`. An uncertain commit may recover as the entire old or new
state. Drop the failed handle and reopen under an exclusive lease to determine it.

Recovery checks bounded header/slot/witness metadata. An unused or checksum-invalid
metadata region may represent an interrupted write; metadata with matching
checksum but invalid representation is rejected. Choose the highest valid root
slot, require its revision to be at least every valid witness, and require any
same-revision witness to match its descriptor/end. Do not scan the tail for newer
objects. Failure to verify a valid newest slot's graph, or any read error, stops
recovery; it does not silently fall back to an older graph.

Before returning a usable handle, recovery verifies the whole graph, syncs it,
then writes/syncs its witness. This also stabilizes a complete but unsynced root
visible when reopening without a power loss. A recovery error returns no healthy
handle and releases the writer lease. Verification errors precede recovery writes.
Native store identities change on reopen; persisted local collection numbers
and the next-ID counter remain meaningful in the newly opened database.

Two slots alone cannot distinguish a torn publication from later loss of an
acknowledged root. Its intact witness forbids silent rollback. In addition to
modeled power loss, tests tolerate damage to one witness when the newest root
is intact and reject damage to an acknowledged slot or referenced payload.
This does not cover simultaneous loss of a slot and its independent witness,
checksum collisions, malicious replacement, or loss of synchronized bytes.
Destroying all evidence of an acknowledgment cannot be detected by local
checksums alone. The protocol is not a backup or arbitrary-corruption repair.

`ByteStore` provides exclusive lock/unlock, length, short read/write, and sync.
`MemoryStore` runs the same codec/protocol in process memory. `SimulatedDisk`
keeps volatile/durable images and pending writes separately, records I/O events,
and injects errors, short operations, tears, or failed syncs at zero-based I/O
sites. Failed sync/power loss can persist none, all, a reverse-order subset of
pending writes, or a torn prefix of one write. A crash invalidates old writer
leases; stale handles cannot unlock newer writers. Engine and simulator allocation
failures are injectable independently. Traces are capped at 262,144 events and
pending writes at 32,768 calls; starting a new fault trace clears event history.
`MemoryStore` disables trace recording. Both adapters are process-memory tools,
with no filesystem or real shutdown durability.

Unreachable tails remain for a later append/compactor. Fault schedules exercise
every I/O boundary in a fixed commit, seven tear prefixes from zero through a
whole region, positive/zero short operations, all three commit sync barriers,
recovery stabilization, and interrupted creation. This is bounded exploration
under declared assumptions, not exhaustive hardware testing.

### Milestone 7 files and byte format v2

This section records the original macOS milestone. The later
[Linux extension](#linux-persistence-and-recovery-results) broadens file support
and moves the common protocol into `src/storage/file.rs`; the macOS primitives
remain in `src/storage/macos.rs`. Earlier platform-result tables below record
what had been executed at each milestone, not the current support matrix.

**Implemented 2026-11-06.** Rust callers use `Database::create_file(path, limits)`
and `open_file(path, limits)` initially on macOS. [`MacFileStore`](src/storage/file.rs)
implements the same `ByteStore` publication/recovery protocol used by memory and
simulator adapters. The shared library exposes `wdb_file_create` / `wdb_file_open`
and retention functions in [`include/weekend_db.h`](include/weekend_db.h).
The C factories return `UnsupportedPlatform` outside macOS; native file methods
are compiled only on macOS. `wdb_open` remains the in-memory factory.

The new format is **v2**: header, root-slot, witness, and object version fields
are 2. Existing family magics retain their `01` suffix; the explicit version
fields determine compatibility. Region positions, object kinds/headers, CRCs,
value encodings, data-state directories, backward-reference rules, image caps,
and publication barriers remain as specified for v1. V1 images are rejected as
`UnsupportedFormat`; no implicit migration or compatibility reader is provided.

The v2 commit payload replaces v1's four integers:

| Payload offset | Encoding |
| --- | --- |
| 0 | `u64` commit revision |
| 8 | `u64` current data-object offset |
| 16 | `u64` current data revision |
| 24 | `u64` next retained-snapshot ID, initially 1 |
| 32 | `u64` retained entry count, at most 64 |
| 40 onward | Sorted entries, each four `u64`s: ID, captured snapshot commit revision, data-object offset, data revision |

IDs are nonzero, strictly increasing in the catalog, and below the persisted next
ID. The next ID survives release/reopen and is never silently reused; exhaustion
returns `Exhausted`. A retained snapshot's data revision cannot exceed either
its captured commit revision or current data revision; its captured commit
revision is below the publishing commit. Its persisted collection-ID counter
cannot exceed the current counter. All data references precede the commit object.
The 64-entry cap keeps the complete commit descriptor in one protected region.

Current and retained data objects are decoded/memoized once per offset, including
when many IDs reference one data version. Distinct offsets claiming one data
revision are rejected. Shared values are reconstructed across current/historical
roots. Full-graph verification includes every retained root before recovery
exposes the database or refreshes its witness. Released, unreachable objects
are ignored and remain eligible for later compaction.

Retained entries reference immutable **data** roots plus their captured revision
stamp, never whole commit roots or another retention catalog. This prevents a
retention cycle. Data-object offset memoization also lets metadata-only operations
reuse existing directories and values. A historical snapshot includes all its
collections and ordinary schema/lookup records; retaining it does not rewrite
its values or interpret them using current metadata.

The native API is:

- `retain_snapshot(&snapshot) -> Result<u64>` publishes a local ID for a live
  current or historical snapshot from the same database. Foreign roots fail.
- `retained_snapshot(id) -> Result<Snapshot>` opens a live historical view.
- `retained_ids() -> Result<Vec<u64>>` lists sorted IDs from the current catalog.
- `release_retained(id)` publishes removal; releasing an absent ID is `NotFound`.

Retention on a file store survives reopening. On a memory backend it lasts only
as long as that backend's bytes. A live snapshot pins its data independently of
catalog membership; releasing its durable ID never invalidates the live reader.
Retain/release are immediate metadata commits, use the ordinary failure barriers,
advance commit revision, and preserve current data revision. An uncertain result
requires reopen/listing to determine whether its catalog change was published.
Every data commit uses the **current** retention catalog under the commit gate,
so stale record transactions neither resurrect released IDs nor lose new ones.
Record observations are unaffected by retention-only changes; whole-database
validation conservatively conflicts. Release promises neither file truncation
nor immediate memory reclamation; append caches and old bytes remain until
close/compaction. Schema enforcement and historical restoration remain out of scope.

**File adapter decisions.** The implementation uses checked macOS SDK constants
and owned file descriptors, with no external crate dependency. It opens the parent
directory first and uses `openat` on the explicit final name, anchoring creation
and directory synchronization to the same directory. The final component must
be a regular file; trailing slash/dot/dot-dot components and embedded NULs are
rejected. Parent-directory symlinks may resolve normally; final symlinks are
rejected with `O_NOFOLLOW`. Paths may contain spaces, Unicode, or native bytes.

Opening uses `O_RDWR | O_EXLOCK | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC`.
`O_EXLOCK` atomically acquires an advisory inode lock at open, including creation,
avoiding an unlocked empty-file window. A logical `ByteStore` lease reasserts the
flock; unlock releases it, with descriptor close as the final cleanup. Separate
opens in the same process, hard-link aliases, and another process cannot become
a second cooperating writer; contention returns `Busy`. These are exclusive
read/write opens, not a multi-process reader/writer protocol. Locks are advisory:
external code must not ignore them or unlink/replace/rename the active store.
`O_CLOEXEC` prevents exec'd children from inheriting the descriptor.

Creation adds `O_CREAT | O_EXCL` with mode `0600` subject to umask, never
`O_TRUNC`. Existing paths return `AlreadyExists` without altering bytes. A fresh
16-byte nonzero file identity comes from macOS `arc4random_buf`. The adapter
retries interrupted system calls and preserves short positioned I/O results for
the storage layer to complete. Missing files return `NotFound`; I/O/sync errors
propagate through the existing `Io` / `OutcomeUnknown` / `RecoveryRequired` rules.
Failed initial creation may leave an incomplete or already published image;
there is no automatic unlink/repair that could discard a usable store.

Every sync uses `fcntl(F_FULLFSYNC)`, with **no weaker fallback** if unsupported.
The first creation sync performs full file sync, parent-directory `fsync`, then
another full file sync to request device-cache ordering after directory metadata.
This completes before root-slot publication; all subsequent barriers use full
file sync. Close discards unpublished drafts and releases owned descriptors/locks;
it is not a late commit/durability barrier. Std file destruction does not report
close errors; publication must already have completed its explicit syncs. Milestone 7 did not add replacement; milestone 10 specifies the offline
compaction protocol in §7 below.

Primary API references: Apple's [open](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/open.2.html),
[flock](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/flock.2.html),
[fcntl](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html),
and [fsync](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html)
manuals. `F_FULLFSYNC` requests drive-cache flushing beyond ordinary `fsync`;
the API cannot independently prove that every filesystem/device honors it.

**Tested scope:** arm64 macOS 27.0.1, local APFS, reported device block size
4,096 bytes. The protocol's 4,096-byte damage-containment and safe-append rules
remain explicit assumptions, not facts established by that block-size report.
Network filesystems, other macOS storage stacks, actual device power cuts,
directory replacement, and Linux/Windows file adapters are unverified. The
simulator remains required because process death retains kernel/device caches.

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

The implemented milestone-10 compactor requires database quiescence (details
below). Traverse current data, all
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
    type: UserType, version: 1,
    fields: { 1: "Alice", 2: 25 }
}
```

This can be a convention expressed with core maps, not another mandatory
storage type. Milestone 8 fixes the initial key/envelope convention below. A schema fingerprint
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

### Milestone 8 schema and application contract

**Implemented 2026-11-06.** [`src/schema.rs`](src/schema.rs) is an optional Rust
access layer over ordinary values/records. It exposes `Schema`, `FieldSpec`,
`Field`, `Record`, `ReadView`, and domain errors wrapping engine errors.
[`examples/support/users.rs`](examples/support/users.rs) is the handwritten
application wrapper used by both [`examples/typed.rs`](examples/typed.rs) and
[`tests/schema.rs`](tests/schema.rs). This adds no engine value tag, C schema
API, byte-format change, query language, generated accessor, or schema compiler.
At milestone 8 the format remained v2 and the C ABI had 43 functions; milestone
10 adds one offline compaction function without changing the format.

The metadata convention is explicit:

- Type IDs, schema versions, field IDs, and application user IDs are nonzero
  `u64`s. They are assigned constants, not Rust `TypeId`s, addresses, declaration
  positions, or hashes of names. Field identity is scoped to its type.
- Schema-registry keys are 16 bytes: little-endian type ID followed by version.
  A descriptor is a map with unsigned `type` / `version`, a `fields` map, and a
  `reserved` list of unsigned retired IDs. Each field key is exactly the
  little-endian eight-byte field ID.
- Field descriptors contain nonempty UTF-8 byte `name`, unsigned `kind`, boolean
  `required` / `nullable`, and an optional stored `default` value. Kinds use the
  existing `Kind` numbers 1–7; none is represented by `nullable`, not a none-only
  field kind. Duplicate IDs/names, overlapping or duplicate reserved IDs, invalid
  kinds/defaults, and malformed metadata are rejected.
- Record envelopes contain unsigned `type` / `version` and a `fields` map with
  the same eight-byte ID keys. `Record` validates envelope/key shape; callers
  use `Schema::validate` for known-field constraints. Other envelope keys and
  unknown, nonreserved field IDs are allowed and preserved. Builders can form
  intermediate records before required fields have been assigned.
- For an absent field, reads use its declared default, otherwise report
  `MissingField` when required, otherwise return absence. A present none is a
  separate value: it requires `nullable` and never triggers a default. Other
  present values must match the declared top-level kind. Nested collection
  contents need application validation; there is no recursive schema language.

`Schema::publish` reads the registry through the caller's transaction, then
inserts a new descriptor or accepts an identical existing one. A different
descriptor at the same key returns `ImmutableDescriptor`. Publication of version
N requires N−1 and validates the transition: existing IDs keep their kinds,
removed IDs become reserved, and all prior reservations remain reserved.
Names may change without changing IDs. Semantic meaning beyond kind compatibility
still requires application judgment; equal scalar kinds do not prove equal units.
No raw engine constraint prevents rewriting a descriptor outside this API.

`ReadView` adapts core transactions and snapshots. The supplied transaction
implementation records dependencies for both primary/lookup reads and descriptor
reads; the snapshot implementation resolves all of them from its fixed data
root. Snapshot queries remain untracked and cannot establish a transaction's
dependencies. Record versions select their exact descriptors, including historical
defaults/labels. Reads never silently convert a record to the latest schema.

The example's `Users` wrapper owns three collection identities: schema registry,
primary users, and email lookup. `Users::create` creates these collections with
the existing immediate catalog operations; setup can therefore leave empty
collections on failure. The application must keep the returned numeric IDs in
its configuration/manifest and rebind them with `Users::from_ids` after reopen.
There is no collection-name discovery layer. `install_v1` and initial records
can be staged together in one transaction after collection creation.

`USER_TYPE` is `0x75736572`. Primary user keys are eight-byte little-endian IDs;
email keys are exact, case-sensitive UTF-8 bytes, mapping to an unsigned user ID.
Names/emails must be nonempty UTF-8; the example does not implement RFC email
validation, case folding, normalization, ordered search, or query planning.

| Stable field ID | v1 | v2 | v3 |
| --- | --- | --- | --- |
| 1 | required byte name, label `name` | same identity/kind, label `display_name` | unchanged |
| 2 | required byte email | unchanged | unchanged |
| 3 | required unsigned age in years | unchanged | removed and permanently reserved |
| 4 | undefined/unknown | optional nullable byte nickname; absent defaults to `guest` | unchanged |
| 5 | undefined/unknown | undefined/unknown | required unsigned age in months |

The owned typed `User` retains its complete `Record` and `Schema` as well as
checked known values. Getters expose UTF-8 names/email, explicit
`Age::Years` / `Age::Months`, and separate nickname presence/effective-default
access. Setters re-read through the transaction and patch persistent field maps;
they never serialize a struct containing only known fields back over a record.
The legacy `set_name_v1` accepts v1/v2, changes only ID 1, keeps the stored version,
and preserves v2 nickname plus unknown field/envelope keys. It rejects v3's
changed age contract. The general setters support versions 1–3; future versions
are rejected rather than guessed. Field setters still assign the entire record
and use the engine's record-scoped conflict rules.

`insert_v1`, `set_email`, and `delete` maintain the email collection explicitly.
Email change verifies the old lookup binding and reads the target key before
staging removal, new binding, and primary assignment. A lookup query reads the
binding, primary record, and actual descriptor, and checks that the email agrees.
Competing claims invalidate through the core's observations; a retry can report
`UniqueViolation`. An unrelated absent-email observation may conservatively
conflict through the lookup collection revision. Independent existing-record name
updates can still merge. Encountered bad bindings return `IndexInconsistent`;
there is no automatic index rebuild or global consistency audit.

**Write-error contract:** every example write/migration method and
`Schema::publish` aborts the caller's entire active draft on error, including
earlier staged work. This prevents a caller from committing a half-staged
primary/index update after allocation or validation failure. Successful calls
leave an active draft for explicit commit. Read errors do not publish writes;
pure record/schema builders have no transaction side effects. Commit failures
retain the existing conflict/I/O/unknown-outcome rules. No callback is replayed
and retries require a new transaction and recomputation.

Migrations are explicit, all-at-once application operations:

1. `migrate_to_v2` publishes descriptor 2 and scans/converts existing v1 envelopes
   to version 2. ID 1 keeps its value/identity while its label changes; nickname
   defaults remain read-time policy, not materialized values. Unknown keys stay.
2. `migrate_to_v3` requires descriptor 2 to exist, publishes descriptor 3, scans
   users, validates source records/lookups, computes checked `years * 12`, removes
   known ID 3, and assigns new ID 5. It preserves nickname/other unknown keys.
   Overflow or a preexisting unknown value at target ID 5 aborts the whole draft
   rather than overwriting data. Already-target-version rows are validated and
   skipped; downgrades are rejected.

Descriptor and record assignments share the same transaction; a concurrent user
write or relevant schema/index change can invalidate it. Email values/IDs do not
change during the conversion, so lookup bindings are verified and retained;
an explicit email update can be included in the same migration transaction.
Retained snapshots continue to resolve their old primary rows, descriptors, and
lookup bindings together after file reopen. No retained data is rewritten.

The batch must fit the existing write/observation/memory limits; no batched
migration protocol or automatic rollout is provided. Supported mixed versions
are readable. Legacy `insert_v1` remains an explicit old-writer API, so an
application must retire incompatible writers when it requires a v3-only store;
the migration establishes the converted set at its commit, not a permanent ban
on later old-version inserts. Raw writers bypass descriptor immutability, schema
validity, and uniqueness/index-maintenance guarantees. These conventions assume
an initially consistent store maintained by validated writers.

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
testing shipped configurations. Test release builds and the selected distribution artifact
as well as instrumented development builds, if C source distribution is selected.

Measure reads, nested updates, list middle edits, batches, allocation traffic,
conflict rates, retained-history growth, and durable commit latency. Compare
matching durability and retention policies. The coarse conflict model and
append-only growth are deliberate starting costs, not performance claims.

## 11. Decisions still open before implementation freezes

The language, in-memory allocator/handle contract, scalar set, map hash policy,
list layout, and initial resource limits are selected below. Milestone 6 also
selected experimental v1 encoding, CRCs, protected-region assumptions, full-graph
verification, publication witnesses, and the `ByteStore` seam in §6. Milestone 7
extends the format to v2 and selects macOS locking/sync/creation semantics.
Format stability,
physical tree serialization, and performance tuning remain future decisions.

The target deliberately defers field-level conflict domains, list rebasing,
replication, multi-process writer coordination, general queries, automatic
schema enforcement, and generated migrations. These can be revisited after a
measured workload justifies their additional invariants. The implementation sequence and stopping points follow in this document.

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
  constraints to check when embedding Rust behind a C interface.
- [Automerge merge rules](https://automerge.org/docs/reference/under-the-hood/merge-rules/):
  an intentionally deferred approach to list-operation identity.


## How to use this plan

Budget roughly 6–8 hours per weekend. A milestone is a usable stopping point,
not a promise to finish in one weekend. Concurrency and durability may each
need several. Implementation is now agent-assisted at the user's request. Keep experiments small and
record the chosen invariant, a counterexample it prevents, and the evidence
that the implementation preserves it.

Read the state, collections, and transaction sections first. Read storage
protocol details when approaching persistence. Each milestone below has a
focused reading assignment: extract an implementation idea and its assumptions,
rather than reproduce another database wholesale.

The first useful result is an in-memory store with persistent nested values.
The first durable result is a deliberately limited experimental file store.
Neither is a production-safety claim. Target 64-bit macOS first, then test Linux
and Windows when available; intended portability and tested support differ.

## Language and distribution: Rust with a C ABI

**Accepted 2026-11-05:** implement the engine in Rust, expose a native `rlib`,
and distribute a C header plus `cdylib` for Odin and other foreign callers.
The user accepted shipping a shared library to favor iteration speed and Rust's
ownership checks. There is no engine single-header or C-only source promise.

The [`probe/`](probe/README.md) comparison exercised the same immutable nested
value slice and cleanup/error paths in C and Rust. Both debug/release Odin builds
reproduced duplicate `_rust_eh_personality` symbols when combining the independent
Rust static library with Odin's prebuilt static wgpu-native. Rust `cdylib` and C
variants worked. The engine's own Odin example now links its `cdylib` alongside
that static wgpu-native archive and runs in both configurations. Independent
Rust static libraries in this combination are excluded from the distribution
contract; duplicate-symbol suppression is not a supported workaround. The
[Rust linkage reference](https://doc.rust-lang.org/reference/linkage.html#mixed-rust-and-foreign-codebases)
explains the broader integration constraint.

Baseline: Rust/Cargo 1.99 or later, edition 2024; no external crate dependencies.
Tested on arm64 macOS 27.0.1 with rustc/Cargo 1.99.1, Apple clang 21.0.0, and
Odin `dev-2026-10:a22aa1984`. `cargo build --release` builds the library/header
consumer artifact at `target/release/libweekend_db.dylib` on macOS. The header
is checked in. Shared-library relocation/installer packaging and Windows
execution remain unverified. The Linux extension below records subsequent native
Linux results. Consumers must arrange the loader search path; local checks set
an rpath to the build directory on macOS and Linux.

Run `python3 tools/check.py` for formatting, Clippy, debug/release Rust tests and
examples, and C/Odin shared-library consumers. Odin requires its installed
`vendor:wgpu` static archive; `--skip-odin` explicitly omits that integration.
`cargo clean` removes engine artifacts. Probe artifacts have separate cleanup
instructions. No language choice establishes crash safety; file guarantees remain
conditional on the declared storage assumptions and simulator/platform evidence.

## Implemented in-memory contract

The code in [`src/`](src/lib.rs), [`tests/`](tests), and [`examples/`](examples)
implements milestones 2–5. The public Rust interface is `Database`, `Value`,
`Snapshot`, `Transaction`, `CollectionId`, `Limits`, and explicit `Error` results.
The C interface uses opaque handles with matching status codes.

- **Values:** none, boolean, signed/unsigned 64-bit integer, binary64 float,
  explicit-length bytes, byte-keyed maps, and indexed lists. Absence is `None`
  outside a `Value`; a stored `Kind::None` is present. Float equality compares
  bits, preserving NaN payloads and distinguishing signed zero. Binary keys use
  full byte equality; empty keys and zero bytes are valid. Every edit returns a
  new value. Independent clones and owned children remain valid after parents
  are dropped. Values from another database are rejected; import/remapping is
  not yet implemented.
- **Maps:** dense 16-way hash trie, consuming low nibbles of a specified 64-bit
  FNV-1a hash (offset basis `0xcbf29ce484222325`, multiplier `0x100000001b3`).
  Leaves retain complete keys and equal-hash collision containers. Hash routing
  is bounded to sixteen levels; collision comparisons are linear in the bounded
  container. FNV is deterministic and not adversarially hardened. Path copying
  shares untouched branches. This is a memory layout choice, not a disk ABI.
- **Lists:** dense radix tree with sixteen children/items per node. Lookup,
  replacement, and append copy only the affected path; growth/shrink at radix
  boundaries preserves historical roots. Middle insertion/removal rebuild the
  affected suffix using path copying, costing O((n-index) log16 n). Exact nested
  depth metadata is currently recomputed on map edits, list replacement, and
  list removal; this adds a child scan (O(n) for maps, O(n log16 n) for lists)
  without copying unchanged subtrees. Metadata aggregation is a future tuning
  opportunity; logarithmic total update time is not promised yet.
- **Ownership/allocation:** a small fallible, atomic reference-counted
  allocation module owns graph nodes; dynamic buffers use `try_reserve_exact`.
  Normal engine allocation failure returns `OutOfMemory`, including partial
  trie construction and foreign handle creation. Cloning shared handles does
  not allocate. The unreachable-in-practice reference-count overflow guard
  aborts rather than wrapping, as with standard `Arc`; revision and collection-ID
  exhaustion return `Exhausted`. Unsafe allocation code is isolated in
  `memory.rs`; raw-pointer and extended foreign-handle lifetimes are in `ffi.rs`.
- **Lifetimes:** native snapshots/transactions borrow the database; explicit
  owned values retain their allocations independently and may outlive it. C
  handles each own a live lease: close returns `BUSY` while any value, snapshot,
  transaction, or materialized scan remains. Release each exactly once; null
  releases succeed. Foreign slices are borrowed for the call and copied when
  stored, and output bytes go into caller buffers. Raw forged/dangling pointers,
  concurrent release/use, and double release are caller errors. Database and
  immutable handles may be shared; transactions require exclusive ownership.
  C close requires external quiescence, including acquisition/release calls.
  Milestone 9 specifies synchronization and transfer rules below.
- **Collections:** identities include the native store identity and a monotonic
  local ID. Create/drop are immediate atomic catalog commits, outside a draft;
  there is no name catalog or transactional catalog editing yet. Dropped IDs
  never reappear, and old pinned snapshots retain the old collection. A draft
  can access only collections present at its begin. Unrelated catalog changes
  do not invalidate existing-record reads in record mode.
- **Transactions:** begin pins the current head, point reads track base record
  revisions, and each blind write captures the same base dependency. Missing
  reads/writes, scans, and counts track the collection content revision. Reads
  of private insertions/deletions retain base dependencies. Scans register before
  returning even empty results and include the final overlay, in sorted binary
  key order. Historical snapshot reads are untracked and cannot be used as
  transactional observations. Nested reads track their containing record only.
- **Publication:** `Validation::Records` is the default; `WholeDatabase` preserves
  the milestone-4 baseline. One mutex covers validation, rebasing final
  assignments onto the current state, persistence, and head replacement.
  Deletions materialize before puts so a final batch can replace entries at the
  collection limit. Every accepted assignment, including equal-value puts and
  absent deletes, stamps the collection revision. Present records receive the
  new write revision. Read-only commits advance commit revision while preserving
  data revision. No callback is replayed. Conflicts end `Aborted`; commit
  preparation errors end `Failed`; success ends `Committed`. All are terminal.
  Ordinary operation errors leave a draft active, possibly with conservative
  observations already registered, and never publish writes. Drop discards a
  draft. Supported observation schedules are tested; this is not a general
  serializability certification.
- **Limits:** defaults are 1 MiB per key/byte string, 16 MiB logical weight per
  nested value (16 bytes per value plus keys/payload/child weights, counting
  shared occurrences), depth 64, 4,096 entries per map/list/collection/catalog,
  1,024 final writes, 4,096 observations, and a 32 MiB logical draft budget.
  Draft accounting includes observation/write structures, owned keys, and final
  value weights. Materialized collection and nested-map scans use the same byte
  budget: `entry_count * size_of::<(Vec<u8>, Value)>() + sum(key_lengths)`, with
  checked arithmetic. The complete result is preflighted before reserving output
  or copying keys; equality fits, overflow/excess returns `Limit`, and an empty
  scan fits a zero budget. Child values remain shared handles; their payload is
  not copied or charged again to the scan buffer. `Limits` is configurable within
  checked hard bounds. This bounds an individual operation/draft; it is not a total process-memory quota or a limit
  on all caller-owned handles or database history.

Evidence includes an independent copying value/record model, 9,600 deterministic
random nested operations with saved historical roots, forced full-hash collisions
and sixteen-level shared prefixes, radix boundaries through 4,096 elements,
600 short randomized transaction schedules compared with sequential commit order,
explicit conflict regressions, revision/identity exhaustion, and allocation-failure
sweeps. Debug/release consumer checks and an AddressSanitizer run pass on the
tested macOS host. Failure diagnostics include deterministic seeds for replay.
Hardware power-cut certification remains unverified. Milestone 10 implements
compaction; milestone 9 adds
actual thread tests and the platform evidence below. Milestones 6–7 add the codec, simulated durability,
corruption tests, macOS files, and durable retain/release in §6. Milestone 8 adds
the optional schema layer and typed application described in §8.
Native Rust, C, and Odin consumers exercise both memory and file factories.

## Milestone 9 concurrency and platform contract

**Implemented 2026-11-06.** The native database supports actual overlapping
threads under one writer authority. This changes ownership/synchronization,
not conflict scopes or experimental byte format v2. The C ABI had 43 functions
at milestone 9; milestone 10 adds the offline file compactor.
The existing file lock still excludes another database writer/process.

`Database`, `Snapshot`, `Value`, materialized `Rows`, `MemoryStore`, and
`SimulatedDisk` are `Send + Sync`. `Transaction` is `Send` and deliberately
`!Sync`: one owner prepares/commits/aborts it, and synchronized transfer is
allowed. Native lifetimes keep the database alive while snapshots/drafts borrow
it; owned values can outlive the database and move to another thread. The tests
assert positive trait bounds and a compile-fail doctest checks `Transaction`'s
negative bound. A caller can put a transaction behind its own mutex if needed.

| Shared mutable bookkeeping | Synchronization / owner |
| --- | --- |
| Current head, next collection ID, retention catalog and next retained ID | One `Mutex<State>` per database |
| Storage backend, accepted slot/revision/end, value/data caches and recovery flag | Same state mutex; `ByteStore: Send`, serialized exclusive calls |
| Shared node and memory-context lifetime counts | `AtomicUsize`; relaxed increment, acquire/release decrement, checked overflow abort |
| Fault-injection allocation budget | Atomic compare/update; one budget across threads, MAX means unlimited |
| Process-local store identity counter | Checked `AtomicU64` allocation |
| Simulated volatile/durable bytes, pending I/O, trace, faults, epochs and writer lease | Shared disk mutex; cloning never clones a writer lease |
| Foreign database live-handle count | Checked atomic increment and atomic decrement |
| Transaction observations, final overlay, byte budget and terminal state | Private draft; exclusive mutable access |
| Node contents, limits, snapshot head and snapshot collection counter | Immutable after construction |

The fallible `Shared<T>` allocator retains nonallocating clones and requires
`T: Send + Sync` for its unsafe `Send`/`Sync` implementations. Every decrement
uses `AcqRel`, so final destruction observes prior owners' accesses without an
atomic fence that ThreadSanitizer cannot recognize. A destructor test checks
cross-thread access and exactly-once final destruction. Allocation-failure
injection uses one atomic budget; concurrent callers race for that budget,
not for a deterministic assignment of failure to a particular thread.

Head acquisition briefly holds the state mutex. Existing snapshot traversal,
value reads/edits, and transaction observation/overlay preparation do not hold
the publication mutex. Commit waits for that mutex, validates against its
current head, rebases final assignments, builds/serializes the candidate,
performs all storage syncs, and only then installs the acknowledged head.
Retain/release and collection create/drop use the same gate. Ordinary contention
waits rather than returning `Busy`; `Busy` still describes store leases and
quiescent C close with live handles. No fairness or latency bound is promised;
acquiring a new head can wait behind file sync. Already acquired snapshots stay
readable during that sync. Shortening the persistence critical section remains
future measured work.

A custom `ByteStore` must not re-enter its database from any method: the mutex
is not reentrant. If an adapter panics while publication holds the gate, Rust
unwinds (the C boundary returns `Panic`) and the mutex remains poisoned. Last
acknowledged immutable heads remain readable; every later publication returns
`RecoveryRequired`, and `requires_recovery()` includes mutex poison. Close and
reopen to run recovery and determine disk state; never clear poison and resume
writes silently. The paused-sync regression unwinds after each of append, root-slot and witness
sync, checks the recovered old/new root, and proves a pinned reader proceeds
while the writer holds the gate. Adapter
implementations must also make unwinding cleanup safe.

C/Odin callers may share the database and immutable value/snapshot/scan handles.
A transaction requires exclusive access, and its ownership may transfer with
caller synchronization. Releasing a handle requires no overlapping use of that
handle. **Database close requires external quiescence of every call on that
database and its handles, including acquisitions and releases.** An atomic
handle count cannot prevent a raw database pointer from being freed just before
another call acquires a handle. `Busy` is a lifetime check, not a concurrent-close
protocol. Callers synchronize pointer publication, lifetime and output buffers;
invalid pointers or violations remain caller errors. The C header and foreign
boundary document these requirements. Native Rust borrows enforce database
lifetime without this raw-pointer protocol.

The protocol follows Rust's [Send/Sync rules](https://doc.rust-lang.org/nomicon/send-and-sync.html)
and [reference-count ordering](https://doc.rust-lang.org/nomicon/arc-mutex/arc-drop.html).
The [sanitizer guide](https://doc.rust-lang.org/unstable-book/compiler-flags/sanitizer.html#threadsanitizer)
requires instrumentation of synchronization and notes the fence limitation.
SQLite's [testing account](https://www.sqlite.org/testing.html) informs the
separate model, fault, coverage, sanitizer and distribution checks; passing this
suite does not establish SQLite's coverage or reliability claims.

**Evidence and reproducible checks.** `tests/concurrency.rs` adds barrier-forced
compatible commits prepared before any writer publishes, eight contested writers with explicit retries
(800 accepted increments, no lost update), paired-record atomicity for live
readers, shared pinned history, overlapping durable retain/release, transferred
drafts, cross-thread value clone/edit/drop, and an atomic allocation-failure
budget. The same four-writer/four-reader/one-retainer stress runs against native
memory (80 rounds), the real codec on `MemoryStore` (12 rounds), and macOS files
(12 rounds); both byte-backed cases close/reopen and inspect current and retained
roots. All accepted revisions are unique and the final count is exact.

`tests/schedules.rs` adds 256 deterministic seeds × four actual thread programs
(up to eight operations each). Successful programs' observations and final
records match the independent sequential model in accepted revision order;
conflicting drafts publish nothing and pinned history stays unchanged. Diagnostics
include seed, program and transaction. `tests/schema.rs` forces simultaneous
email claims and checks one winner, whole primary/index publication, old snapshot
visibility and a unique-violation result on explicit retry. Codec mutation cases
increase from 1,200 to 2,400, half with repaired checksums; truncation, 600 existing
logical schedules, 9,600 nested-value operations and durability faults remain.

`examples/threads.c` drives four pthread writers (400 compatible commits) and
two readers through the actual shared library in debug/release. A foreign-boundary
unit test races eight threads through 8,000 value-clone/snapshot-acquire/release
cycles and checks the exact remaining handle count and quiescent close.
`tools/ffi_smoke.py` loads the actual shared library with `ctypes`, checking
record roundtrip, live-handle `Busy`, release and close; non-macOS file factories
must return `UnsupportedPlatform`. `tools/check.py` now includes these checks.

Verified locally on arm64 macOS 27.0.1 / APFS, Rust/Cargo 1.99.1, Apple clang
21.0.0 and the installed Odin `dev-2026-10:a22aa1984`:

| Check | Observed result |
| --- | --- |
| `python3 tools/check.py` | Formatting, warnings-denied Clippy, **81 tests + one compile-fail doctest in both debug and release**, native Rust examples, C memory/file/pthread consumers, shared-library ABI smoke and Odin memory/file consumers with static wgpu all pass |
| AddressSanitizer | All 81 tests and the doctest pass using `tools/sanitizers.py address`, nightly 1.101.0 (`708c94ef0`, 2026-11-03), arm64 macOS and rebuilt std |
| ThreadSanitizer | The 80-test full suite and doctest, plus the added foreign-handle concurrency test (81 cases total), pass with the same nightly/target and rebuilt std; no race reports |
| Cross-target Rust checks | Warnings-denied `cargo check --all-features --all-targets` passes in debug and release for `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, and `x86_64-apple-darwin` |
| x86 macOS distribution | `cargo build --release --target x86_64-apple-darwin` links the `cdylib`; execution unavailable (`Bad CPU type`, Rosetta absent) |
| Linux/Windows runtime/distribution | Native CI jobs configured; **not executed here**. Cross-checking is compilation without platform linking/execution; no local Linux/Windows runtime result is claimed |
| Native Rust source coverage | **94.33% lines (3,292/3,490), 92.61% regions (6,065/6,549)** with LLVM 23.1.1 tools matched to rustc 1.99.1 |

The coverage report records 98.30% engine lines, 97.59% allocation lines,
97.36% codec lines and 91.21% schema lines. Gaps include value error paths
(87.34%), foreign-boundary Rust coverage (64.81%; external consumers excluded),
and error formatting. No branch/MC/DC claim or coverage threshold is introduced.
The concurrency and randomized thread-schedule suites also pass ten consecutive
runs in each of debug and release. The expanded panic-at-each-sync regression
passes separately in debug, release, ASan and TSan. Generated reports and
execution logs are under ignored `target/`, not checked-in proof artifacts. Reproduce with the commands above; the CI definition records
future runs independently.

`tools/sanitizers.py address|thread` rebuilds the nightly standard library so its
synchronization is instrumented. Prerequisites are nightly and `rust-src`; the
first std build may fetch its dependencies. `tools/coverage.py` uses the active
compiler's `llvm-tools-preview`, runs the native Rust suite and writes text/HTML
to `target/coverage`. It excludes external C/Odin consumers, includes inline Rust
unit-test bodies, and reports source line/region coverage, not branch or MC/DC
coverage. It is evidence of exercised paths, not proof of race freedom or recovery
correctness. CI adds native debug/release Rust tests, shared-library loading and
release artifacts for macOS/Linux/Windows, Linux C consumers, and macOS ASan/TSan.
Odin remains a locally checked macOS integration; Linux/Windows file adapters and
Odin distributions are not implemented or claimed.

Minimal named regressions cover paired-record visibility, write skew, scan and
absence conflicts, whole-database validation, post-sync adapter unwind, handle
lifetime counting, and competing email claims. Random tests retain deterministic
seeds for replay; no newly discovered random failure required a minimized fixture.
Keep any future failing seed/image and reduce it before adding its regression.

Before calling a configuration release-tested:

- Run `python3 tools/check.py` in that configuration, including the intended
  C/Odin consumers; label any explicit skip.
- Run ASan and TSan on supported hosts; record compiler, target and std
  instrumentation. Check that only the intended distribution artifact is shipped.
- Run model/operation/corruption and fault suites, record coverage gaps, and keep
  minimized regressions for observed failures.
- Check real-file reopen, locks, interruption and retained roots on each claimed
  file adapter/filesystem; preserve the documented power-loss assumptions.
- Confirm header/API thread ownership, close quiescence, resource/format limits,
  unsupported factories and migration policy match the shipped artifact.

Lock-free publication, field-level conflicts, multi-process writers, and hardware
power-cut certification remain outside milestone 9. Milestone 10 adds compaction
and measurements below.

## Milestone 10 compaction and measurements

**Implemented 2026-11-06.** Compaction is an offline rewrite of reachable state,
not a new logical commit, format migration, or restoration of an old snapshot.
The image remains experimental v2. Its file identity, current commit/data
revisions, collection IDs and revision stamps, next collection ID, retained
IDs/captured revisions, and next retained ID are preserved. Reopening still
creates a fresh native context; rebind numeric collection IDs as with any reopen.

The native APIs are:

- `Database::compact_to(&mut self, destination: impl ByteStore)` writes a fully
  synced, verified compact copy to an empty, exclusively leased destination.
  The source must already use `ByteStore` (`Invalid` for a native-only database).
  It stays open and unchanged; this API does not switch the source backend.
  The destination lease is released before return. It can be opened independently
  as a checkpoint/fork; subsequent source commits do not update the copy.
- `Database::compact_file(path, limits)` on macOS opens/recover-verifies the
  source, creates a sibling copy, and replaces the canonical file using the
  protocol below. Close every database handle for that inode first. The file
  lock enforces this even when an open database has no live snapshot/transaction.
- Both return `storage::CompactionStats { before_bytes, after_bytes }`: data-fork
  lengths, not physical free-space counters. OS snapshots, open descriptors,
  resource forks and filesystem allocation policy can retain physical blocks.
- C/Odin use `wdb_file_compact(path, len, &before, &after)`. The header now exports
  **44 functions**. Counts reset to zero on error; other platforms return
  `UnsupportedPlatform`. File compaction in the C and Odin examples closes,
  compacts, reopens, and reads retained data through the actual shared library.

The `&mut Database` requirement statically excludes live borrowed snapshots and
transactions during `compact_to`; a compile-fail doctest enforces it. There is
no draft survival or implicit transaction replay. Owned values can outlive the
source and remain usable: they own memory, not disk offsets or a file lease.
The file API requires all database handles to be closed. C callers retain their
existing close-quiescence obligations and must release C value handles before
close. This is database quiescence, not a global stop of unrelated databases.

**Copy and verification.** Reachability consists of the current data root and
all explicitly retained data roots. Collection directories, ordinary schema
registries, primary records, secondary lookup collections, nested values and
monotonic counters are copied without interpreting application schemas.
Released history, obsolete commit descriptors, unreachable serialization-cache
entries and abandoned append tails are excluded. Durable retention does not
implicitly capture earlier retention catalogs.

The encoder starts with empty value/data caches, memoizes shared `Value` nodes
and data versions across every root, writes a new header and one complete v2
root/witness pair, completes all durability barriers, then invokes the ordinary
full-graph recovery verifier on the result before it can replace a file. Sharing
is preserved at v2's stored value/data-object granularity. Internal HAMT/radix
branches are still reconstructed from logical containers; physical tree-page
serialization is future work. No export/import conversion or ID remapping is
needed. The existing 64 MiB image cap, object/work/depth limits and fallible
allocator remain in force. Peak memory includes decoded reachable state, encoded
candidate bytes, caches and verification state; this is not streaming compaction
or a global memory quota. Enough disk space for old and new files is required.

Verification found and minimized a fresh-image parity bug: initial publication
had assumed revision zero and selected slot A. A compact image may begin at an
odd revision, so creation now chooses the first slot from the existing revision's
parity. Revision-zero creation behavior is unchanged. Tests cover both parities,
empty and metadata-only histories, and a subsequent commit on the copied image.

**macOS replacement protocol.** The adapter anchors every leaf operation to one
open parent-directory descriptor. Parent symlink resolution is the same as an
ordinary open; final symlinks, malformed leaves, and hard-link sources are
rejected. Replacing one hard-link name cannot preserve a writer authority for
another name. Existing locks still require cooperating users: concurrent external
rename, unlink, hard-link creation or directory replacement is outside the contract.

1. Open the original with the normal nonblocking exclusive inode lock; reject
   `nlink != 1`. Fully verify/stabilize recovery and keep that descriptor locked.
2. Create a random `.wdb-compact-<32 hex digits>.tmp` sibling with `O_EXCL`, mode
   `0600`, and an atomic exclusive inode lock. Never truncate an existing name.
   Build, sync and verify the complete compact image. Creation includes the
   existing parent-directory fsync/full-file-sync barrier.
3. Copy source metadata using `fcopyfile(COPYFILE_METADATA)` (POSIX information,
   ACLs and extended attributes, under macOS metadata policy), then full-sync
   the replacement, then run full-graph verification again so the final check
   includes metadata-dependent data interpretation. Inode identity/ctime change. Recheck both
   directory entries against their held inode identities and single-link counts.
4. Atomically exchange canonical and scratch names with
   `renameatx_np(RENAME_SWAP)`. Both inodes stay locked: a cooperating open sees
   `Busy` both before and after the exchange, with no canonical-name gap.
5. Fsync the held parent directory, then `F_FULLFSYNC` the new file. The old
   complete store remains at the scratch name until this barrier succeeds.
6. Unlink the old scratch name, fsync the parent again, and full-sync the new
   file again. Only then report success and release the descriptors/leases.

This uses the shared replacement driver in `src/storage/replacement.rs`; the
same exchange/directory-sync/device-sync/remove sequence drives the namespace
crash model. No ordinary-rename fallback is attempted if exchange is unsupported.
The installed Apple SDK's `rename(2)` manual and `sys/stdio.h` specify the atomic
swap and flag value. The [Apple XNU header](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/stdio.h),
[copyfile API](https://github.com/apple-oss-distributions/copyfile/blob/main/copyfile.h),
and Apple's [fsync](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html)
and [fcntl](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html)
manuals are the primary references.

Before exchange, failures leave the canonical original usable. Source recovery
may refresh its witness, so the file API does not promise byte-identical source
metadata. Destination publication uncertainty is reported as `Io` here because
it cannot yet affect the canonical path. From the exchange phase onward any
failure returns `OutcomeUnknown`: reopen the canonical path to verify whether
its physical image is old or compacted. Both represent the same acknowledged
logical state. There is no automatic exchange rollback.

Errors or process death may leave incomplete candidates or complete old backups
at scratch names. **No directory scan or automatic orphan deletion is performed.**
After an error, first verify/reopen the canonical database; preserve scratch
copies until that succeeds and any required backup is secured. Successful
compaction removes only its own old scratch name. Unrelated siblings are never
selected or removed. Cleanup failures can leave the old file consuming space
while the canonical compact file is already usable.

**Durability scope and tests.** The namespace model assumes exchange is atomic
under recovery (old or swapped mappings), directory fsync submits namespace
updates, and the following full sync persists those ordered updates before old
unlink. It explores durable-only, submitted, and all pending metadata survival at
every before/after boundary, using real source/compacted v2 images and checking
current and retained records after reopen (39 boundary/policy cases). These
filesystem/device assumptions are additional to the v2 byte-store model; atomic
visibility alone is not proof of power-loss durability.

`tests/compaction.rs` exercises I/O failures, short/torn writes, failed syncs,
allocation failures through copy and verification, source byte preservation for
`compact_to`, all 64 retention slots, released-ID gaps, unchanged next counters,
shared roots, dropped collections, abandoned tails and a subsequent independent
source commit. The typed case preserves schema versions 1/2/3, an unknown field,
explicit age conversion and historical/current email lookups. A codec regression
checks pointer sharing after compaction and reopen. Real-file tests check mode,
ACL and xattr preservation, rejection of aliases, source quiescence, an unbroken
lease at every hook, and errors before/after every observed I/O/namespace step.

The process-interruption helper now kills compaction before/after copy writes,
source/target syncs, metadata copy, name checks, exchange, directory/device syncs,
unlink and cleanup. The real-file suite passes **140 total kill/acknowledgment
cases**, including **55 additional compaction cases**; current/retained state is
checked after reopening. These are process-death tests, not physical power cuts.
Tested replacement scope is arm64 macOS 27.0.1, local APFS. Other filesystems,
network storage, Linux/Windows file adapters and device power-cut certification
remain unverified. Test-only compaction hooks require `fault-injection` and are
not a production callback or recovery interface.

**Observed validation:** `python3 tools/check.py` passes formatting,
warnings-denied Clippy, **92 tests and two compile-fail doctests in debug and
release**, Rust memory/simulator/file/typed examples, shared-library C ABI smoke,
C memory/file/pthread consumers, and Odin memory/file consumers linked with the
static wgpu archive. Both `tools/sanitizers.py address` and `thread` pass all
92 tests and both doctests with rebuilt std on nightly 1.101.0 (`708c94ef0`,
2026-11-03), arm64 macOS. Current native Rust coverage is **94.87% of lines
(3,717/3,918)** and **93.08% of regions (6,768/7,271)**; it includes inline unit
tests and excludes external C/Odin consumers, as in milestone 9. Branch/MC/DC
coverage is not measured. File adapter lines are 95.80%; value and foreign-boundary
error paths remain coverage gaps. The final post-metadata verification change
also passes the nine-case compaction suite under both sanitizers. Default-feature and all-feature Clippy both
pass without warnings. Warnings-denied cross-checks pass for Linux/Windows in
debug and release and x86 macOS in debug; the x86 macOS release `cdylib` links.
Native Linux/Windows runtime results remain pending CI, and Rosetta execution
remains unavailable on this host.

**Benchmark policy and baseline.** `tools/benchmark.rs` is a dependency-free
release binary, and `tools/benchmark.py` records three runs, raw CSV and
median/min/max summaries. The original pre-compaction implementation samples are
saved in [`docs/benchmarks/milestone10-baseline.csv`](docs/benchmarks/milestone10-baseline.csv).
They were captured before the engine changes in this milestone. The extended
matched workload below uses the same current binary before and after file
compaction. File reopen measurements use identical verification/retained-read
work; head-read measurements use identical acquisition/lookup work. Retention
policy stays unchanged until the separately labeled final release/compact case.

Pre-implementation baseline medians (three runs; raw samples in the baseline
CSV) are:

| Workload | Operations/run | Median per unit |
| --- | ---: | ---: |
| Pinned memory point read | 100,000 | 16.4 ns |
| Two-level nested map edit | 10,000 | 281 ns |
| 256-item list middle insert/remove pair | 1,000 | 42.0 µs |
| Memory batch of 16 record assignments | 500 | 7.26 µs |
| Memory conflicting transaction pair | 2,000 | 563 ns |
| Codec-backed memory commit | 64 | 81.8 µs |
| Single-record file commit with full sync | 64 | 8.87 ms |
| File history update, including two retains per run | 128 | 9.19 ms |

The final measurement run was taken after the test/build traffic finished.
[`milestone10-measured.csv`](docs/benchmarks/milestone10-measured.csv) preserves
all three raw extended runs; [`milestone10-summary.csv`](docs/benchmarks/milestone10-summary.csv)
contains every current median/min/max. Original baseline reopen/read loops and
the extended matched pair have different bookkeeping, so only the extended
before/after pair is used for compaction latency comparisons.

| Matched file scenario | Median | Min–max of three run means |
| --- | ---: | ---: |
| Single-record full-sync commit | 9.17 ms/commit | 8.89–9.39 ms |
| 16-record full-sync batch | 9.36 ms/batch | 8.85–9.66 ms |
| Reopen/verify and read retained roots, before compaction | 4.36 ms/open | 4.11–4.79 ms |
| Same reopen/retained-read work, after compaction | 4.23 ms/open | 4.07–5.65 ms |
| Acquire current head/read record, before compaction | 148 ns/read | 146–154 ns |
| Same current-head read, after compaction | 146 ns/read | 44.5–152 ns |
| Offline file compaction, keeping both retained roots | 31.98 ms | 28.27–36.24 ms |
| Release both retained IDs, then compact again | 29.07 ms/compaction | 23.19–29.13 ms |

The history image is **3,190,784 bytes before compaction and 86,016 bytes after**,
a **97.30% reduction while preserving both retained roots**. Releasing those
IDs and compacting again leaves **45,056 bytes**. Current revision and every
retained revision remain unchanged by each compaction itself; explicit retention
release advances commit revision normally. Retained roots therefore have a
visible storage cost and are never silently discarded to improve the result.

Measurements use Rust 1.99.1, `cargo build --release`, overflow checks enabled,
no fault-injection, sanitizer or coverage instrumentation, Apple M3 Pro, and
local APFS. They are warm-process/cache microbenchmarks with three samples,
not cold-cache I/O, a throughput saturation test, percentile latency study,
matched competitor comparison, or a claim about other hardware. Native memory
has no durability barriers; the codec memory adapter simulates barriers; file
cases use the same full-sync policy (three publication barriers per commit).
Cross-backend latency ratios would conflate these guarantees. A batch unit is
one 16-record transaction; a conflict-pair unit includes one accepted commit
and one expected rejected commit. A list-pair unit inserts/removes at index 128
of a 256-item list. The history workload performs 128 updates of an 8 KiB payload,
with two extra retain commits at iterations 0 and 64; its per-iteration time
includes those retain operations. File counts are logical lengths, not allocated
blocks or host free-space measurements.

**Measurement-driven next targets.** Full-sync file commit time stays near
9.2–9.4 ms for one record and a 16-record batch; batching amortizes the measured
cost to about 0.585 ms per assignment in that workload, under the same commit
policy. Retain the existing barriers and investigate application batching before
changing durability semantics. Middle list edits are the expensive native value
case here (42.0 µs baseline; 55.8 µs current median), consistent with the known
suffix rebuild and depth-metadata scans. A focused next experiment should profile
those scans and evaluate cached child aggregates or a different sequence layout,
with the existing model/history tests retained.

Compaction substantially reduces image length. The measured reopen median
changes from 4.36 to 4.23 ms, and the before/after sample ranges overlap;
these samples do not establish a latency improvement. Recovery already follows
reachable references in either image, which bounds read work independently of
obsolete append history. Larger graph/retention
workloads and more samples should precede read-latency optimization claims.
Small native timings also vary between the original baseline and extended run;
CPU placement/frequency and layout were not controlled. No competitor ranking,
throughput claim, or generalized regression attribution follows from these runs.

Reproduce the current measurement pipeline with a **new** output directory:

```sh
python3 tools/benchmark.py --output target/my-benchmark-run --compact
```

Milestones 1–10 now have implemented acceptance evidence for the stated scope.
Next work should use the measured workload and unchanged durability/retention
policy to evaluate the selected bottleneck, keep minimized regressions, and
continue native Linux/Windows validation when those CI runners execute. There
is no general production-readiness or stable format/ABI claim.

## Rust convenience package and task application

**Implemented 2026-11-06 following milestones 1–10.** The user chose a Rust
convenience package and a task-tracker CLI as the first complete application.
The workspace now contains the existing `weekend-db` engine, optional
`weekend-db-typed` runtime, separate `weekend-db-macros` proc-macro package, and
`weekend-tasks` application. Engine format v2, the 44-function C interface,
Odin/shared-library packaging and the core's zero external runtime dependencies
are unchanged. The convenience layer remains an optional access convention;
raw engine writes still bypass it. These local packages are not published.

The convenience interface concentrates schema/conversion, named collection
registration, observation tracking and unique-index maintenance behind
`Store`, `Table<T>`, `Document<T>`, and read/write transaction closures. This
follows the deep-module principle of moving repetitive implementation out of
callers while retaining explicit ownership, migration and failure semantics.
The application defines its models and domain commands; it does not hand-write
schema descriptors or primary/index synchronization.

`#[derive(Model)]` requires an explicit nonzero type ID, schema version, and
stable ID on every named field. It generates descriptor construction, checked
owned conversion, stable-field patching, numeric field constants and typed
unique-index constants. Helper attributes support exact scalar uniqueness,
deterministic native defaults, stored labels, reserved IDs and an explicit crate
path for renamed dependencies. Generic/tuple structs and ambiguous declarations
are rejected with source diagnostics. Full trait qualification prevents generated
field constants named `VERSION` or `TYPE_ID` from shadowing model metadata.
The macro is built on pinned `syn` 2 / `quote` / `proc-macro2`, with four external
build dependencies including `unicode-ident`. Generated/runtime code adds no
new unsafe code. The core crate does not depend on the convenience crates.

Built-in codecs support owned strings/bytes, booleans, fixed-width integers,
f64, nested lists, optional fields and explicit presence. Integer narrowing and
UTF-8 are checked. `Option<T>` intentionally collapses missing/null on reads and
writes None as null. `Presence<T>` retains Missing/Null/Value, including under
a descriptor default; `Document::schema` and the raw record expose the effective
stored-schema default separately. Nested nullable types are rejected. Custom
codecs can distinguish invalid input from allocation failure. Rust models/default
expressions use ordinary owned Rust allocations, with engine limits and typed
conversion errors retained; there is no process-wide memory quota.

The runtime reserves collections 1/2/3 for its format/catalog, schema registry
and exact unique lookups. Named record collections are registered thereafter.
Primary keys are separate nonzero u64s encoded little-endian. Index keys contain
record-collection ID, field ID, scalar kind and exact scalar bytes, preventing
cross-table/field collisions. Null and physically missing values have no unique
entry, even when the stored schema provides a non-null default. Index maintenance
uses raw field presence after schema validation; effective defaults remain a read
projection rule. Only explicitly stored non-null values claim unique keys. A
lookup entry pointing to a physically missing field is `IndexInconsistent`;
lookup verification never substitutes a schema default. Float lookups retain bit
equality. Unique layouts are immutable per named table
in this version. Reindexing, general queries and ordered/nonunique indexes are
future work. Shared index-collection absence observations are conservative and
can conflict across otherwise unrelated tables.

Creation initializes only a fresh revision-zero engine database. Open requires
the convenience catalog; it never adopts or repairs an arbitrary existing engine
image. Table registration requires exclusive `&mut Store` and publishes the
schema/catalog together. Engine collection creation is immediate, so failed
registration may leave an unregistered empty collection and failed initialization
may leave a partial fresh image. The original does not get silently erased.

A read closure uses a fixed current or explicitly retained snapshot. A write
closure prepares one core transaction and executes **once**; it has no automatic
retry or side-effect replay. Typed operation errors abort all prior writes even
if ignored. A sticky failure flag also blocks commit after a caught callback
panic. Nested `Store::write` calls are independent transactions; pass the writer
to helpers when one atomic batch is intended. Index changes and primary writes
are checked and staged together; inconsistent raw indexes are reported.

`Document<T>` retains the complete envelope and its stored schema. Updates load
inside the transaction and patch known stable IDs into that raw record;
unknown fields and envelope keys survive. There is no detached-struct save.
Older compatible models may update newer records without downgrading their
version. Once a table advances, older models cannot insert new older-version
rows. Registration of a successor descriptor does not migrate records. Newer
models can project older fields using explicit defaults, but updates return
`MigrationRequired` until `Write::migrate` is called. Stored-schema defaults
remain version specific. Prospective known-ID collisions, retired/incompatible
fields, conversion failures and overflow abort the batch. Migration callbacks
inspect the old record explicitly; generated patching removes reserved IDs.

Checkpoint methods use the actual durable retention catalog. `restore_table`
restores one table's record data and unique indexes in a single transaction;
it does not rewind the schema/catalog, other tables or application ID allocators.
Historical descriptors remain available and unknown fields are preserved.
Restoration and migration must fit the existing bounded transaction. A restored
older record can require an explicit migration before a newer writer updates it.
Tables must have existed at the selected checkpoint.

The CLI's task models use stable type ID `0x7461736b`; v1 contains slug, title,
completion and tags, and v2 adds a nullable note plus priority default zero.
Slugs have an exact unique lookup. A separate counter model (`0x7461736c`)
allocates task IDs atomically with insertion. Duplicate inserts do not consume
IDs; delete and undo keep the counter advancing. Commands cover creation,
CRUD, tags/notes/priorities, historical listing, checkpoints, undo/redo, explicit
upgrade and closed-file compaction. Read commands do not create new logical
commits once setup is present. `demo` runs the actual models and transactions
in memory on every supported build; file commands use the existing macOS adapter.

**Validation/results:** the engine's 92 regression tests remain passing, with
14 typed-runtime integration tests, one macro diagnostic test (covering eight
invalid declarations), and three application tests: **110 ordinary tests in
debug/release**, plus **six passing Rust doctests** (including compile-fail
contracts). The standalone renamed-dependency consumer compiles and runs in both
profiles; its README illustration is intentionally not a standalone doctest.
Formatting and warnings-denied Clippy pass with default and all features.
`tools/check.py` now exercises every workspace package, the portable app demo,
the real CLI lifecycle and renamed consumer, plus the existing Rust/C/Odin
shared-library consumers with static wgpu. The C export count remains 44.

The typed/app suites also pass AddressSanitizer and ThreadSanitizer with
instrumented std on nightly 1.101.0, arm64 macOS. Coverage tooling now reports
workspace sources and proc-macro expansion, excluding registry/toolchain code,
external consumers and standalone test files: **92.65% lines (4,894/5,282)** and
**90.14% regions (8,848/9,816)**. This expanded denominator differs from the
milestone-10 engine-only report; branch/MC/DC coverage is not claimed. Errors and
less-common CLI paths remain gaps. Warnings-denied workspace checks compile for
x86 Linux, Windows and macOS; native non-macOS execution remains pending CI.
Fresh CI jobs fetch the pinned macro dependencies before the offline checks.

Integration cases cover concurrent unique claims, schema/primary lookup conflict
dependencies, one-shot callbacks, caught-panic/ignored-error abort, allocation
failure sweeps, every durable commit I/O boundary with retained snapshots,
semantic retirement and overflow, prospective-ID collision, version-specific
defaults, raw missing/null presence, unknown/envelope preservation through old
models, context rejection, compaction/reopen, and inconsistent index detection.
Scalar checks include integer boundaries, binary borrowed lookups, nested optional
lists, NaN payloads and signed-zero uniqueness. The CLI lifecycle runs separate
real-file processes through create/add/edit/checkpoint/history/undo/upgrade/
compaction. The ID test verifies that duplicate insert, delete and undo do not
reuse task identities. A persisted two-task sample was also exercised manually
under the ignored `target/weekend-task-demo-rehqqvm0/` directory.

These conventions remain experimental. Index-layout migration, arbitrary primary
key types, nested generated model codecs, general queries, multi-process writer
coordination and non-macOS file adapters were outside the initial convenience
extension; Linux file support is implemented below. Engine
transaction/snapshot limits remain visible rather than being bypassed by hidden
batching or retries.

Usage and supported declarations are documented in
[`crates/weekend-db-typed/README.md`](crates/weekend-db-typed/README.md); runnable
commands are in [`apps/weekend-tasks/README.md`](apps/weekend-tasks/README.md).
The original handwritten milestone-8 wrapper remains independent regression
and example evidence. Generated access does not change its guarantees.

## Linux persistence and recovery results

**Implemented 2026-11-06.** Native file creation, reopening and offline compaction
now support **Linux x86_64 and aarch64** through the same `Database`, typed `Store`,
C ABI, examples and `weekend-tasks` commands as macOS. Windows file factories
still return `UnsupportedPlatform`. Other Linux ABIs also return that error
before file I/O; the in-memory/byte-backed engine remains buildable there.
The engine adds no external crate dependency, no C symbols, and no format change:
the experimental on-disk format remains **v2**, and the C header still exports
**44 functions**. This section supersedes earlier milestones' Linux file/runtime
limitations; those older test results remain historical evidence.

### Adapter and writer authority

[`src/storage/file.rs`](src/storage/file.rs) owns the shared path validation,
positioned I/O, logical lease, creation barriers, and compaction protocol.
[`src/storage/linux.rs`](src/storage/linux.rs) supplies Linux primitives;
[`src/storage/macos.rs`](src/storage/macos.rs) retains the macOS primitives.
`storage::FileStore` is the platform adapter, with `LinuxFileStore` and the
existing `MacFileStore` names as platform-specific aliases.

The adapters open the held parent with `O_DIRECTORY`, rejecting non-directory
parents (including FIFOs) without blocking. The Linux adapter then uses `openat` with
`O_RDWR | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC`. Creation adds `O_CREAT | O_EXCL`,
mode `0600` subject to umask, and never truncates an existing path. Only regular
files are accepted. Existing leaf validation, native path bytes, bounded path
lengths, and rejection of final symlinks also apply. Positioned reads/writes
preserve short results for the byte layer; interrupted calls are retried.
File identities and scratch names use nonzero 16-byte kernel `getrandom` output,
with no time/PID/random-number fallback. Errors retain the established
`Io` / `OutcomeUnknown` / `RecoveryRequired` publication meanings.

Linux has no macOS-style atomic `O_EXLOCK` open. After `openat`, the adapter takes
`flock(LOCK_EX | LOCK_NB)` and **reopens/checks the anchored leaf's device/inode
under that lease before recovering or writing database bytes**. An opener paused
before flock may otherwise acquire an obsolete inode after a compactor swaps
and closes it. A mismatch/missing leaf returns `Busy`; callers may reopen the
canonical name. The check also runs on every logical `ByteStore::lock`, rejecting
manual reuse of an unlocked adapter after its inode has been replaced. The
common check now protects both adapters. Cooperating opens, hard-link aliases
and separate processes share inode exclusion; no sidecar lock is introduced.

Creation necessarily has an open-to-flock interval on Linux. A competing opener
may briefly see an incomplete new file and receive a format/corruption error,
or win the lease and make creation return `Busy`. It cannot recover an
uninitialized image as a usable database or overwrite existing data. Failed
creation may leave an incomplete image, following the existing no-auto-unlink
policy. An open database is not a multi-process reader/writer service. Advisory
locks require cooperation; external pathname/metadata mutation is outside the
contract. Descriptor close releases the lease after process death.

Flag values are explicit per supported ABI. The first ARM64 execution exposed
an incorrect initial assumption that `O_NOFOLLOW` matched x86_64: ARM64 uses
`1 << 15`, x86_64 uses `1 << 17`. The flags were checked against the running
Linux headers and the kernel's [arm64 header](https://github.com/torvalds/linux/blob/master/arch/arm64/include/uapi/asm/fcntl.h)
and [generic header](https://github.com/torvalds/linux/blob/master/include/uapi/asm-generic/fcntl.h).
Both symlink-open and symlink-compaction regression cases now pass on ARM64;
x86_64 execution remains delegated to the Linux CI runner.

### Sync and replacement decisions

Every Linux durability barrier uses `File::sync_all` (`fsync`), with no weaker
fallback. Initial creation synchronizes the file, then the held parent directory,
then the file again before the byte layer publishes a root. Recovery still
verifies the graph and stabilizes/witnesses the selected generation before
returning it. Ordinary commits retain the same append/root/witness ordering as
macOS and the simulator. Linux fsync is a platform contract; this is not a claim
that it is identical to Apple's `F_FULLFSYNC` on every storage stack.

Offline Linux compaction follows the same replacement driver and quiescence
rules as macOS: lock/verify the source, reject hard-link sources, create/lock a
random sibling, write/sync/verify all current and retained roots, copy metadata,
sync and verify again, and recheck both names. It exchanges names with
`renameat2(RENAME_EXCHANGE)`, fsyncs the parent and replacement file, removes the
old scratch name only after that barrier, then fsyncs parent/file again. Both
inodes remain leased throughout. Unsupported exchange/filesystem operations
fail without an ordinary rename fallback. Errors from the exchange phase onward
remain conservatively `OutcomeUnknown`; reopen the canonical path. Incomplete
candidates and old recovery copies are never automatically scanned or deleted.

Linux metadata copying preserves uid/gid, permission and special mode bits,
and all enumerable extended attributes, including POSIX access ACLs. Ownership
is set before mode/attributes; inherited target-only attributes are removed.
Each attribute/name list is bounded at 64 KiB and copied with fallible storage;
unsupported/denied metadata operations or changed lengths fail before exchange.
The replacement's owner/group/mode are checked before publication. Arbitrary
external metadata mutation is unsupported. Inaccessible attributes,
filesystem-specific inode flags and timestamps are not a preservation guarantee;
inode identity/ctime change. This is a narrower, explicit Linux metadata policy,
not an emulation of every macOS `COPYFILE_METADATA` behavior.

Primary contracts: Linux [flock](https://man7.org/linux/man-pages/man2/flock.2.html),
[fsync](https://man7.org/linux/man-pages/man2/fsync.2.html),
[renameat2](https://man7.org/linux/man-pages/man2/rename.2.html),
[getrandom](https://man7.org/linux/man-pages/man2/getrandom.2.html), and
[flistxattr](https://man7.org/linux/man-pages/man2/listxattr.2.html)/
[fgetxattr](https://man7.org/linux/man-pages/man2/getxattr.2.html)/
[fsetxattr](https://man7.org/linux/man-pages/man2/setxattr.2.html).
Compaction requires kernel/filesystem exchange support (the libc interface is
available with glibc 2.28+); no compatibility shim was added for older libc builds.

### Observed validation

Executed on aarch64 Debian 12 (glibc 2.36), Rust/Cargo 1.99.1, under Docker's
Linux 7.0.14 VM on the Apple M3 host. All database/test temporary paths were on a
Docker-managed **ext4 volume** (`/dev/vda1`, 4,096-byte filesystem blocks), not the
macOS source bind mount. macOS regression scope remains arm64 macOS 27.0.1/APFS.

| Check | Observed result |
| --- | --- |
| Linux `python3 tools/check.py --skip-odin` | Warnings-denied workspace Clippy/formatting, **115 tests + 6 doctests in each of debug/release**, the real CLI lifecycle, renamed-dependency consumer, Rust memory/simulator/file/typed examples, shared-library ABI smoke and C memory/file/pthread consumers all pass |
| macOS checks | Full debug/release check script with **113 tests + 6 doctests** per mode, Rust/C consumers, and Odin memory/file plus static-wgpu consumers pass; the final full check script passes both modes |
| Real-file interruption | **140 before/after SIGKILL/acknowledgment cases**, including creation, data commits, retention, release, recovery and compaction, pass on Linux; every reopen checks whole batches/current state and retained roots, and acknowledged results survive |
| Linux writer exclusion | Same-process opens, hard-link aliases and a separate worker process return `Busy`; SIGKILL releases the holder and reopening succeeds |
| Inode replacement races | A deterministic Linux unit case pauses between raw open and lease acquisition across a completed compaction and rejects the old inode; a shared regression rejects manually relocking an unlocked/replaced adapter |
| Linux metadata | Mode, binary/empty user xattrs and POSIX access ACL survive replacement; a separate case removes inherited target ACLs absent from the source; manual root-mediated compaction also preserves a different uid/gid (`12345:23456`) and mode `0640` |
| Unprivileged Linux | All **29 file, compaction, concurrency and CLI lifecycle tests** pass as UID 65534 on the ext4 volume; normal operation does not depend on container root privileges |
| Shared protocol sanitizers | macOS ASan and TSan pass the affected file/compaction/concurrency suites (**25 tests** each), including process interruption; this is not Linux sanitizer execution |
| Native-byte and parent handling | Linux creates/reopens an invalid-UTF-8 leaf; both platforms continue writing after their held parent directory is renamed, then reopen at the new path; nonregular leaves and parents are rejected without blocking |
| Cross-platform v2 image | A task database created on macOS reopens on Linux, retains its historical checkpoint, accepts edits and compaction, then reopens on macOS with both current edits and retained history intact |
| Cross-target compilation | Warnings-denied workspace/all-target/all-feature checks pass for x86_64 Linux, Windows MSVC and macOS; these checks do not execute those target binaries |

The first Linux run's symlink regression and the first macOS path test provided
useful platform-specific failures: the former exposed the ARM64 flag difference;
the latter showed APFS rejects the test's invalid UTF-8 leaf. The Linux native-byte
case is retained and the macOS variant uses a Unicode leaf. Both execute the
same held-parent rename/commit/reopen assertions.

[`tools/check.py`](tools/check.py) now respects `CARGO_TARGET_DIR`, runs the Rust
and C file consumers on Linux, and keeps Odin specific to macOS. The ctypes ABI
smoke exercises create/Busy/compact/reopen on supported file platforms while
preserving unsupported-platform checks elsewhere. The Linux CI matrix job now
executes these file, subprocess interruption and C consumer paths through the
same script, rather than only memory consumers. Reproduce on a supported Linux
host with `cargo fetch --locked` followed by
`python3 tools/check.py --skip-odin`. Local raw logs and the round-trip image
artifacts are under ignored `target/linux-persistence-*` and
`target/linux-interop-n63jxv6v/`; the tested aarch64 Linux shared library and
CLI are saved in `target/linux-persistence-artifacts/aarch64-unknown-linux-gnu/`.
The temporary container/volume were removed after validation. These are evidence
from this run, not a checked-in certification artifact.

**Remaining scope:** native x86_64 Linux execution awaits CI; Windows file
persistence, older libc compatibility, other Linux ABIs, additional filesystems,
network/FUSE/overlay-backed database paths, inaccessible/security-specific
metadata policies, actual device power cuts and host/VM crashes remain unverified.
Ext4 is the tested stack, not a whitelist detected/enforced by the adapter.
Successful fsync/exchange and the format's protected-region/safe-append rules
remain required assumptions. Process-kill tests retain kernel/device caches and
cannot certify those physical assumptions. The 64 MiB image cap, retained-ID
limits, explicit compaction quiescence and no automatic recovery-copy cleanup
remain unchanged.

## Review corrections: scans and unique presence

The full working-tree review against the design-only `7858feb` baseline found
two reproduced contract mismatches. Both are fixed without a byte-format or C ABI
change, with regressions first observed failing against the previous code.

**Nested-map scan budgeting.** `Value::map_entries` previously allocated/copied
all entries despite a smaller `Limits::max_transaction_bytes`; a zero-budget map
scan copied a 1,024-byte key. The same probe with allocation failure armed returned
`OutOfMemory` before detecting any budget violation. Collection scans already
accounted for output entries and key copies. Both paths now use
`Map::materialize` in [`src/tree.rs`](src/tree.rs), so one preflighted policy
controls checked arithmetic, entry/key accounting, fallible allocation and binary
key ordering. A limit failure leaves the persistent map/value handles usable.
This also covers C `wdb_map_scan` through `Value::map_entries`, and internal
codec/schema materializations obey the same per-operation bound.

**Unique indexes use physical presence.** A unique `Presence<String>` field with
a non-null schema default previously indexed `Presence::Missing` as the default:
a second missing row failed uniqueness, and querying the default returned a row
whose projection was still `Missing`. Unique key construction now validates the
stored schema and reads the raw field, skipping physical absence and explicit
null. Explicit values equal to the default still participate in uniqueness.
Changing Value to Missing/Null releases its key; inserting/deleting a missing row
cannot claim or remove another row's entry. Read projections and explicit
migrations still use their documented schema-default behavior; projection alone
does not create an index entry. If an update/migration actually stores a non-null
projected value, that explicit value is indexed normally.

Lookups verify physical presence before accepting an index hit. A stale
legacy/default-derived or raw-corrupted entry is reported as `IndexInconsistent`,
rather than resolving through a default or silently repairing data. Existing
experimental images with such entries need an explicit affected-index rebuild
or recreation; this correction introduces no automatic reindex/conversion tool.
Stores whose index entries already describe physical values require no rebuild.
The convenience catalog and core format versions remain unchanged.

**Regression coverage.** Five added tests cover zero/exact/one-byte-short budgets,
empty and binary keys, cumulative multi-entry overhead, shared large children,
and rejection before output allocation under fault injection. Typed tests cover
multiple Missing/Null rows with a non-null default, missing/default read behavior,
transactional and snapshot lookups, explicit-value uniqueness and rollback,
Value-to-Missing/Null transitions, deletion beside another row's index, retained
snapshots, restoration, compaction/reopening, migration across different defaults,
and rejection of stale default-derived index hits in reads and writes.

**Observed validation:** the full `python3 tools/check.py` passes on arm64
macOS/APFS: warnings-denied workspace Clippy/formatting, **118 ordinary tests +
6 doctests per mode** in debug/release, renamed-dependency consumer, Rust examples,
C ABI and memory/file/pthread consumers, plus Odin memory/file consumers with
static wgpu. On aarch64 Debian 12/Linux 7.0.14 with all database paths on the
Docker-managed ext4 volume, the same script with `--skip-odin` passes **120 tests
+ 6 doctests per mode**, including real-file reopen/interruption and the task CLI.
The two Linux-only cases explain the test-count difference.

The affected core value/allocation/transaction/concurrency and typed-model suites
pass **46 tests each under ASan and TSan** on arm64 macOS with rebuilt nightly
std. Warnings-denied workspace/all-target/all-feature cross-checks pass for
x86_64 Linux and Windows MSVC; these are compilation checks, not native execution.
No new unsafe code, dependencies or disk/C ABI version changes were introduced.
Prior storage/power-loss/platform limitations remain as documented above.

Reproduction logs for the five failing-before regressions and successful checks
are under ignored `target/review-fixes-*`. The corrected Linux shared library and
CLI are retained at `target/review-fixes-linux-artifacts/aarch64-unknown-linux-gnu/`;
the temporary validation container/volume are removed after saving them.

## Implementation milestones

These follow the architecture above. Complete the acceptance cases before adding
the next layer; split a milestone across weekends whenever needed.

## Milestone 1. Choose a language and prove the consumer boundary

**Complete (2026-11-05):** matching C and Rust probes and C/Rust/Odin
consumers are implemented in [`probe/`](probe/README.md). Debug/release builds
reproduce `_rust_eh_personality` duplicate symbols when Odin links the independent
Rust static library alongside its prebuilt static wgpu-native. C static/shared
and Rust `cdylib` variants pass that integration. C's generated single-header
form also passes. Ownership/error contracts, exact commands, toolchains, and
limitations are documented with the probe. The user accepted Rust with a native
Rust library and a C header/`cdylib` for foreign consumers, excluding independent
Rust static libraries in that Odin combination. The engine's own examples now
prove that boundary in debug/release. The probe's infallible-allocation shortcut
has been replaced by fallible allocation in the engine. No stable file format
has been selected.

Write the same tiny vertical slice in the two strongest candidates: C and Rust
unless a concrete embedding requirement makes Zig more useful. Include a nested
value, an immutable clone, one private edit, an error path, and explicit cleanup.
Expose a minimal C ABI and call it from Rust and Odin. Link the Odin example
with the existing prebuilt wgpu-native dependency to reproduce or rule out the
previous symbol collision. This is a packaging/ownership probe, not a database.

Compare how much code and effort each version requires to change one value
operation, diagnose a lifetime mistake, and add a failure path. Time-box the
comparison to the first weekend; leave an unmet requirement explicit instead of
expanding the experiment into competing engines.

**Read:** the language/distribution section above and the official Rust linkage
reference; consult Zig's overview only if testing Zig. Write the smallest desired
consumer example before freezing signatures.

**Definition of done:** choose an implementation language and distribution
contract; a fresh checkout builds the probe with documented commands; both
consumer examples run, including the wgpu-native combination, or the chosen
contract explicitly excludes an unacceptable combination. Document slice/handle
lifetimes, allocation, errors, and close behavior in at most one page. If C source
packaging wins, prove split-source and generated-header builds and inclusion
from two C translation units without duplicate symbols. Otherwise replace the
single-header promise with the demonstrated library/header/toolchain contract.

## Milestone 2. Build a simple reference model

**Complete (2026-11-05):** `tests/support/mod.rs` defines recursively copied
logical values; transaction schedule tests use independent `BTreeMap` records
and copied historical states as the sequential oracle. Scalar/key/absence and
bounds semantics are fixed in the implemented contract above.

**Read:** architecture §§1–2. Use Clojure’s persistent-collection semantics to distinguish logical immutability from the later tree implementation. The reference model should favor clarity over sharing.

Use straightforward in-memory keyed records, nested maps/lists, and copied
snapshots. Select a fixed first scalar set and define absence, bounds, key
identity, and cloning semantics. Use deliberately small, deterministic fixtures.
Add the slow model's logical operations before optimizing representation.

**Definition of done:** nested map/list updates, independent clones, and historical reads
have clear expected outcomes. This model becomes the oracle for persistent
collections and transaction schedules.

## Milestone 3. Implement persistent collections

**Complete (2026-11-05):** `src/tree.rs` and `src/value.rs` implement immutable
hash/radix trees with shared unchanged nodes. Random model checks, old-root and
owned-child lifetime tests, collision/prefix tests, radix growth/shrink, and
fallible-allocation regressions pass. Exact layout and current metadata-scan
costs are documented above; no disk representation is implied.

**Read:** architecture §2, Bagwell’s Ideal Hash Trees, and Clojure’s collection documentation. Sketch hash routing, full-key collisions, path copying, and list indexing before coding.

Replace whole-state copying with structural sharing and path copying behind the
collection interface, retaining both maps and lists. Choose one simple tree
layout, write down its invariants, and defer tuning. Preserve full map key
identity even if routing uses hashes. Shared immutable storage must not become
shared mutable object identity between records.

**Definition of done:** random operation sequences agree with the reference model, old
snapshots never change, clones diverge independently, and allocation failures
leave reachable committed state intact. Test routing collisions and shared-value
lifetimes. Record the chosen layout and its tradeoffs.

## Milestone 4. Build a whole-root transaction baseline

**Complete (2026-11-05):** `begin_with_validation(WholeDatabase)` provides the
baseline alongside the default record mode. Tests cover final overlay reads,
atomic abort/conflict, unrelated-write rejection, and equal-value/ABA generation
changes. No application callbacks are stored or replayed.

**Read:** architecture §§4–5 and the private-work/validation/publication parts of Kung and Robinson. Start with the root revision rule; do not implement the full paper’s machinery.

Implement a base snapshot, private final put/delete overlay, read-your-writes,
and atomic commit/abort. Give commits monotonic revisions. At publication check
that the current revision still equals the base revision; otherwise conflict.
Initially interleave two handles on one thread.

**Definition of done:** a conflict/abort publishes none of its writes, only one of two drafts
at one base can commit, and intervening equal-value/ABA writes are detected.
This intentionally rejects unrelated concurrent edits: it is a correctness
baseline, not the completed merge feature. Do not replay old callbacks on retry.

## Milestone 5. Refine to conservative record-scoped merging

**Complete (2026-11-05):** default `begin()` uses record observations and
collection-level absence/scan observations. Tests cover both compatible commit
orders, same-record nested edits, write skew, metadata dependencies, empty scans,
counts/overlay reads, absence ABA, unrelated-insert false conflicts, namespace
identity, preparation failure atomicity, and 600 replayable randomized schedules
against a sequential oracle. The C shared library and Odin/wgpu example exercise
the same engine. Supported scope and conservative conflicts are documented above.

**Read:** architecture §§4–5 and PostgreSQL’s isolation examples. Turn write skew, empty scans, missing keys, and ABA into explicit schedules for this contract.

Track observed present-record generations and write-target generations. A
nested access tracks the containing record. Use a containing collection
version for missing-key reads and any supported scans; bump it on every
relevant write. This avoids relying on equality or an indefinite missing-key
history, at the cost of false conflicts.

Under one commit gate validate dependencies, apply only final assignments to the
current root, and publish. Preserve other transactions' independent changes.
Cover namespace/ancestor replacement consistently; do not use physical node
addresses as conflict identities. Add observations to the interface only with
an accompanying validation rule.

**Definition of done:** two transactions updating different existing records commit in both
orders; edits within one record, including its lists, conflict; stale/missing
reads and supported scans invalidate safely; and failures never publish partial
changes. Explicitly test unrelated-insert false conflicts. Compare short
transaction schedules with a simple sequential oracle and minimize failures.
Do not claim serializability before covering every supported observation.

## Milestone 6. Specify the byte format and simulated durability

**Complete (2026-11-05, historical results):** §6 specifies the experimental v1 bytes, checksums,
protected regions, witness-based publication/recovery, resource bounds, and
failure statuses. `Database::create_on`/`open_on` exercise the real engine and
codec through `ByteStore`, `MemoryStore`, or `SimulatedDisk`. A real file adapter
and durable retention were subsequently added in milestone 7 below, with v2.

**Verification on arm64 macOS:** `python3 tools/check.py` passes formatting,
Clippy with warnings denied, **42 tests in both debug and release**, both Rust
examples, and the C/Odin shared-library examples including static wgpu-native.
The same 42 tests pass under AddressSanitizer with installed
`rustc 1.101.0-nightly (2026-11-03)`. Reproduce that run with:

```sh
RUSTFLAGS='-Zsanitizer=address' CARGO_TARGET_DIR=target/asan \
  cargo +nightly test --offline --all-features --target aarch64-apple-darwin
```

| Evidence | Result |
| --- | --- |
| Errors at every I/O site in the fixed commit, with five crash policies | Whole old/new state; definite prepublication errors recover only old |
| Seven tear prefixes, durable/volatile tears, zero/positive short I/O, and failed sync persistence choices | No partial record batch; acknowledged commits survive |
| Recovery read/write/sync errors, stabilization of an uncertain volatile root, and interrupted creation | No healthy handle on error; old acknowledged state remains protected |
| Engine preparation, simulator I/O, decode, and recovery allocation sweeps | No partial head or leaked writer lease; safe retry/reopen |
| 600 deterministic durable transactions across 12 seeds, with 96 crash/reopen points | Match independent `BTreeMap` oracle and preserve acknowledged revisions |
| 1,200 deterministic byte mutations, half with repaired checksums, plus sampled truncations | Bounded checked rejection or valid graph; no panic |
| Targeted checksummed malformed tags/counts/keys, cycles, offsets, padding, generations, identity, and versions | Checked errors; unsupported versions/features rejected |
| Acknowledged-slot, witness, referenced-payload, and unreachable-tail damage | No silent rollback within the declared single-region corruption policy |
| Shared values and dropped/recreated collections across reopening | Shared identities reconstructed and collection numbers never reused |

Preserved regressions include completing short writes without crossing protected
regions, adopting a complete unsynced root only after recovery barriers, and
skipping abandoned/torn append tails without publishing them. The existing
9,600 nested-value operations and 600 transaction schedules still pass. These
are deterministic bounded tests, not coverage, mutation-score, hardware power-loss,
or arbitrary-corruption certification.

**Read:** architecture §6, SQLite’s atomic-commit analysis, redb’s storage design, and SQLite’s testing account. List every assumed atomicity/sync guarantee before choosing metadata layout.

Specify integer encoding, lengths/offset bounds, value tags, format version,
checksums, root metadata, and persistence ordering. Choose and document a
recovery protocol before writing a real file adapter. An append-only
copy-on-write format with independently protected root slots is a candidate to
evaluate, not a preselected implementation.

Build a simulator separating volatile and durable bytes. Inject short writes,
allocation/I/O/sync failures, permitted reordered or dropped unsynced writes,
and torn writes under a declared failure model. Include failures in recovery and
an ambiguous commit outcome. Add corruption/operation fuzzing and regression
cases here, not only after all storage work is complete.

**Definition of done:** reopening after explored failures yields a complete allowed state,
preserves all acknowledged durable commits under the model, rejects unsupported
formats, and reports malformed data safely. Define whether a failed commit
requires handle recovery before additional writes.

## Milestone 7. Add the macOS file adapter and retained snapshots

**Complete (2026-11-06, milestone-7 results):** the macOS adapter, v2 retention format, native/C APIs,
and Rust/C/Odin file examples are implemented. See the current
[file/format contract](#milestone-7-files-and-byte-format-v2) for the exact bytes,
lifecycle rules, historical read semantics, conditional assumptions, and limits.

**Results on arm64 macOS 27.0.1 / local APFS / 4,096-byte reported device blocks:**
`python3 tools/check.py` passes formatting, Clippy with warnings denied,
**55 tests in debug and release**, all three native Rust examples, and both
memory/file C and Odin shared-library consumers. Odin continues to link the
prebuilt static wgpu-native alongside the Rust shared library. The same 55 tests
also pass under the milestone-6 AddressSanitizer command and installed nightly.

| Evidence added | Result |
| --- | --- |
| 85 deterministic SIGKILL boundary/acknowledgment cases | Data batches/catalogs recover whole; every acknowledged outcome survives process termination |
| Kill boundaries before/after ByteStore I/O during commits, retain, release, creation, and selected recovery operations | Expected old/new state, complete initial revision 0, or rejection of incomplete creation |
| A separately held writer process, same-process opens, and hard-link aliases | Contenders get `Busy`; SIGKILL releases the inode lock |
| Retained historical/current versions through file close/reopen and native/C/Odin consumers | Both versions are readable; abort remains unpublished |
| Release while a historical live handle remains | Reader stays valid; released ID remains absent after reopen |
| Stale record transactions crossing retain/release | Current catalog preserved; no dropped entries or resurrected IDs; whole-root baseline conflicts |
| Retain/release I/O faults with four crash policies and allocation-failure sweeps | Whole catalog recovery, no partial in-process head, no reuse of accepted IDs |
| Retained-only payload corruption versus released unreachable garbage | Corruption of reachable history rejected; released garbage does not become a root |
| 64-entry boundary, ID exhaustion, malformed v2 catalogs, and explicit v1 rejection | Checked resource/format errors; no silent wraparound or migration |
| Existing codec/simulator, 1,200 byte mutations, 600 durable operations, 9,600 value operations, and 600 transaction schedules | Remain passing with the v2 extension |

The test-only `file-worker` binary is built with `fault-injection`. Its parent
waits for a flushed before/after-I/O marker, sends SIGKILL, waits for exit, then
reopens and verifies the current and retained roots. The 85-case matrix includes
separate acknowledged outcomes; a second test covers inter-process exclusion.
It instruments ByteStore boundaries, not every internal kernel instruction.
These tests kill processes, not devices or the host: they complement rather than
replace the simulator's unsynced-byte loss/reordering/torn-write model.

Run the file demo with a new explicit path:

```sh
cargo run --example file -- /tmp/weekend-demo.db
python3 tools/check.py
cargo test --all-features --test files -- --nocapture
```

The example leaves its requested database in place. The check script and tests
use isolated temporary directories and remove their own artifacts. Creation
refuses to overwrite an existing demo file. No replacement, compaction, schema
enforcement, or multi-process concurrent-write feature is implied by this milestone.

**Read:** architecture §§6–7 and the selected platform’s official file/sync API documentation before implementing its adapter. Write down creation, sync, close, and replacement assumptions.

Apply the tested format/protocol to real file operations. Handle creation,
publication, synchronization, errors, and reopening. Persist snapshot retain/
release metadata. Distinguish live snapshot handles from durable retention;
closing/releasing a handle must obey the ownership contract from milestone 1.

**Definition of done:** Rust writes, commits, retains a prior snapshot, reopens, and reads both
versions. Aborted state stays unpublished, and process-interruption tests agree
with the contract. Keep simulator tests: process termination alone does not
model all unsynced-write loss or reordering. These tests establish evidence
within the fault model, not immunity to arbitrary hardware failure.

## Milestone 8. Add schema metadata and one typed Rust example

**Complete (2026-11-06):** `schema` implements the optional descriptor/record
convention; `examples/support/users.rs` supplies handwritten typed access, legacy
unknown-preserving updates, explicit v1→v2→v3 migrations, and an email lookup.
The exact encoding, compatibility window, defaults, abort-on-error behavior, and
raw-writer limitations are specified in the
[implemented contract](#milestone-8-schema-and-application-contract). Storage
format v2 and the 43-function C ABI are unchanged.

**Verification:** `python3 tools/check.py` passes formatting, Clippy with warnings
denied, **69 tests in debug and release**, all four native Rust examples, and
memory/file C and Odin consumers. The typed example runs both in memory and
against an actual macOS file and checks retained versions after reopening.
The same 69 tests pass under the documented AddressSanitizer command/nightly.
The 14 new schema/application tests establish:

| Evidence | Result |
| --- | --- |
| Stable keys, immutable registration, consecutive evolution, retired IDs, malformed descriptors/records | Checked errors; no silent field renumbering or reuse |
| Absent nickname, explicit none, and stored text | Distinct presence; version-specific default only for absence |
| Legacy name edit of v2 with new nickname, nested unknown field 99, and an unknown envelope key | All unmodified data and version preserved |
| Explicit years-to-months conversion, repeat migration, unsupported legacy writer | Correct units/new ID; old ID reserved; no automatic read conversion |
| Overflow or occupied unknown target ID after earlier rows were staged | Whole draft aborted; no partial row/schema publication |
| Concurrent email claims, same-record edits, schema-revision changes, lookup→primary decisions, migration scans | Safe conflict or explicit uniqueness error; rejected assignments do not publish |
| Independent existing-record typed updates | Both commit and retain each other's changes |
| Retained v1/v2 schemas, rows, unknown fields, and email bindings through reopen | Historical versions remain coherent while current state uses v3 |
| Every I/O boundary of fixed email-update and migration/index batches, with three crash policies | Whole old/new state across all three collections; retained baseline preserved |
| Allocation failures through email change, deletion, and migration preparation | Draft aborted and storage unchanged; no partially staged batch can commit |
| Intentional raw index corruption | Core accepts raw data; validated lookup/update reports encountered inconsistency |

The previous 85 real-file SIGKILL cases, byte/operation fuzzing, retention tests,
and transaction schedule/model checks still pass. This is evidence for the
supported handwritten access surface, not unconditional schema/uniqueness
enforcement for every engine writer or a general query/isolation certification.

```sh
cargo run --example typed
cargo run --example typed -- /tmp/weekend-typed.db
cargo test --all-features --test schema
```

The optional file argument must name a new macOS file; the example leaves it in
place. The check script uses private temporary paths. No schema compiler,
automatic index generator, automatic migration retry, or engine-format converter
is introduced by milestone 8.

**Read:** architecture §§8–9 and Protocol Buffers’ schema-evolution rules. Borrow stable IDs and compatibility discipline, not its wire format by default.

Store immutable versioned descriptors as ordinary data. Define stable type/field
IDs, absence/default rules, and unknown-field preservation. A handwritten Rust
wrapper can expose field setters, but commits a final record assignment; field
IDs do not imply field-level concurrent merging.

Write an app-side migration and one explicit secondary lookup map, such as email
to record ID. Update primary data, schema/catalog references, and lookup entries
atomically. Let schema metadata guide accessors; do not build a schema compiler
or a general query engine.

**Definition of done:** a rename, optional field, and semantic conversion behave explicitly;
retained records retain their old schema descriptors; unknown fields survive
supported updates; and secondary lookup stays consistent under failures and
conflicts. Document that bypassing wrapper maintenance bypasses these guarantees.
File-format migration is a different future exercise.

## Milestone 9. Add real thread overlap and platform checks

**Completed 2026-11-06.** Synchronization decisions, actual tests, platform scope,
coverage and the release checklist are recorded in the
[implemented contract](#milestone-9-concurrency-and-platform-contract).

**Read:** architecture §§3 and 10, SQLite’s testing account, and the selected language’s official threading/FFI rules. Enumerate all shared mutable bookkeeping.

Synchronize publication/shared bookkeeping and specify which handles may be
shared across threads. Keep transaction preparation private and snapshot reads
stable. Stress simultaneous readers and compatible/conflicting commits.

**Definition of done:** thread tests and applicable sanitizers pass on tested configurations.
Expand operation/corruption fuzzing and report coverage. Add Linux/Windows
build/test jobs when available, including release builds and the selected distribution artifact;
label platforms accurately rather than assuming support. Establish minimized
regressions and a release checklist. Keep multi-process writers out of scope.

## Milestone 10. Reclaim space and measure

**Completed 2026-11-06.** The implemented APIs, replacement protocol, tests,
measurements and limits are recorded in
[compaction and measurements](#milestone-10-compaction-and-measurements).

**Read:** architecture §§7 and 10, then the comparison below. Define reachable roots and matched benchmark policies before choosing a reclamation or performance target.

Start compaction with a documented quiescent requirement if that keeps the
contract smaller. Copy reachable values into a new store while preserving
sharing, retained roots, and schema/lookup metadata. Define safe replacement
under the platform's file semantics and test interrupted replacement. Do not
discard the only good store before the replacement is complete.

Measure point reads, nested updates, list edits, batches, conflicts, retained
history growth, and durable commit latency. Compare equal durability, workloads,
and retention policies. Set optimization targets from actual bottlenecks.

**Definition of done:** compaction preserves retained state and its metadata through modeled
failures, and measurements identify the next useful experiment. Possible next
projects are layout tuning, finer key scopes, richer generated wrappers, engine-format
conversion, explicit three-way merges, list rebasing, or replication.


## Comparison and reason to build this

**Inference:** yes, there is a credible specialist niche: a portable application-state store with direct nested persistent values, explicitly retained historical roots that survive reopening, and atomic record-scoped optimistic transactions. The opportunity is this combination and its integration experience. Neither “embedded NoSQL” nor “immutable database” alone distinguishes it. This review establishes differences among close alternatives, not universal competitor absence or proof that developers want another engine.

“Direct nested values” matters: changing one leaf should share unchanged stored subtrees without decoding and rewriting the entire containing document. “Historical” means an application-retained version remains addressable after shutdown and compaction, not merely a read transaction observing a fixed current view. Both are explicit targets, still requiring implementation evidence. The relevant contracts are specified above.

## Closest alternatives

| Alternative | Existing overlap | Consequence for positioning |
| --- | --- | --- |
| xitdb | Embedded, nested persistent collections, file-stored history, cheap undo | Closest abstraction; C packaging and transaction contract must earn their keep |
| Vedis | Portable embedded C, structured commands, transactions, amalgamation | C plus hashes/lists is already available |
| UnQLite | Portable C amalgamation, transactional KV and document storage | Differentiate persistent value graphs from encoded documents |
| LMDB | C, copy-on-write, concurrent snapshot readers | A live MVCC view is not an explicit durable history catalog |
| redb | Embedded Rust, persistent rollback savepoints | Durable checkpoints already exist; distinguish historical reading from restoration |

### xitdb: the decisive comparison

xitdb already exposes arbitrarily nested maps/lists, structural sharing, memory/file backends and incremental access. Its documented pattern stores successive “moments” in a top-level list; undo appends a reference to an earlier moment. This is stored history, not only live-reader isolation. Writes are serialized; readers can run concurrently. [Official README](https://github.com/xit-vcs/xitdb#readme)

The inspected local implementation loads its root from disk, syncs appended data before publishing the root, and defaults file synchronization on. Its compactor treats a root list as history and copies only its latest moment into a new history. These source facts support normal reopen persistence and explain history loss under that compaction policy; they are not an independent crash-safety certification. [xitdb source](https://github.com/xit-vcs/xitdb/blob/master/src/lib.zig)

The first-party organization also has [Java](https://github.com/xit-vcs/xitdb-java), [TypeScript](https://github.com/xit-vcs/xitdb-ts) and [Go](https://github.com/xit-vcs/xitdb-go) implementations. **Inference:** “usable outside Zig” is insufficient differentiation. A convenient native distribution with Rust bindings, strict immutable private values, selective retention through compaction and compatible independent-record commits is a more defensible comparison. The last three are weekend-db targets, not benchmarked advantages. The relevant contracts are specified above.

**Inference from reachability:** an xitdb application could also keep named references to old values inside its latest moment, making those values reachable by compaction. Explicit retention is therefore better viewed as a proposed built-in policy and interface guarantee, not a fundamentally exclusive capability. Much of the proposed product experience could potentially be built above xitdb rather than requiring a new engine.

### Vedis: packaging and structured storage already overlap

Vedis is an embedded C datastore with disk and memory modes and a cross-platform file format. Its command surface includes hashes, sets, lists, and BEGIN/COMMIT/ROLLBACK. It ships as `vedis.c` plus a header. [Overview](https://vedis.symisc.net/), [commands](https://vedis.symisc.net/commands.html), [amalgamation source](https://github.com/symisc/vedis/blob/master/vedis.c)

Its hash command implementation stores entry data in byte blobs and returns strings; transaction rollback invokes the pager's current-transaction rollback. The reviewed API does not expose named retained historical roots or xitdb-style arbitrarily nested immutable collection handles. That is a bounded finding about this interface, not proof history could never be built above it. [Source: `vedis_cmd_hmget`, `vedis_rollback`](https://github.com/symisc/vedis/blob/master/vedis.c)

### UnQLite: nested documents differ from persistent subtrees

UnQLite already provides in-process transactional C storage, a portable file, an amalgamation, raw binary KV and a Jx9 document layer. [Official repository](https://github.com/symisc/unqlite)

The distinction is visible in code: `CollectionStore` and `CollectionUpdate` call `FastJsonEncode`, then replace a KV record; fetching reads the record and calls `FastJsonDecode`. Nested JSON therefore does not itself supply a public persistent map/list graph whose unchanged subtrees are shared across application versions. The reviewed public transaction interface is begin/commit/rollback, not a retained-snapshot catalog. [Document storage implementation](https://github.com/symisc/unqlite/blob/master/unqlite.c), [public header](https://github.com/symisc/unqlite/blob/master/unqlite.h)

### LMDB and redb: be precise about snapshots

LMDB offers C transactions, copy-on-write pages, concurrent readers and one active writer. Its API documents that long-lived readers prevent page reuse and that freed pages are otherwise recycled. Its normal read transactions pin views during their lifetime; the reviewed API does not provide an application catalog of arbitrary retained roots to reopen later. Separate database copies can preserve versions, with different costs and management. [Official API source](https://github.com/LMDB/lmdb/blob/mdb.master/libraries/liblmdb/lmdb.h)

redb explicitly has `persistent_savepoint`, deletion/listing and `restore_savepoint`; these require appropriate durability and retain otherwise reusable pages. Persistent savepoints survive reopening, as its tests demonstrate. However, `Savepoint` is a restoration token, with no direct historical table-reading API exposed on it. Restoration happens in a write transaction and invalidates later savepoints. This overlaps checkpoint/rollback use cases, without being the same historical-read interface. [WriteTransaction](https://docs.rs/redb/latest/redb/struct.WriteTransaction.html), [Savepoint](https://docs.rs/redb/latest/redb/struct.Savepoint.html), [tests: `persistent_savepoint`](https://github.com/cberner/redb/blob/master/tests/integration_tests.rs)

## What the current design can honestly serve

**Recommendations, not evidence of demand:** start with an application that owns one writer process and needs historical inspection of structured state:

- A desktop configuration/workspace store: inspect the settings and metadata associated with named checkpoints after reopening.
- A local build or analysis tool: retain selected dependency graphs or analysis results and read an earlier run without loading complete serialized graphs.
- A game/tooling state inspector: checkpoint stable entities and compare versions while ordinary work continues.

These examples fit keyed records and selective checkpoints. They do not require SQL, replication or collaborative text merging. They still need retention budgets, useful inspection and a concrete consumer.

Do not advertise automatic undo, branching or an editor backend yet. Transactions currently begin at the current head; retained snapshots are readable, but arbitrary historical writes, old-root restoration and a branching API are absent. An application could deliberately create compensating assignments, but that requires its own restoration semantics. Middle list insertion/removal may cost O(n), and two edits to different fields or list positions inside one record conflict. Even unrelated new inserts can conflict through collection-level absence tracking. These are material constraints for large editable documents. The relevant contracts are specified above.

Record-level OCC is conventional concurrency control, not a fundamental invention: private work followed by read/write validation and publication has long precedent. **Inference:** its value here is a clear conflict contract and avoiding unnecessary retries for independent records; measure whether that matters before making it the headline. [Kung and Robinson](https://www.eecs.harvard.edu/~htk/publication/1981-tods-kung-robinson.pdf)

## Validation before expanding scope

**Recommendation:** validate one consumer against xitdb and an encoded-document baseline with matching durability and retention policies. Measure integration effort, allocations, bytes written for a leaf update, retained-history growth, reopen latency, durable commit latency and conflict/retry rates. A result is useful only if the consumer values the difference enough to adopt the API.

Require crash/reopen tests preserving acknowledged commits and retained roots; compaction must preserve selected history and reclaim released history. Exercise same-record conflicts, independent existing records, absence/scan dependencies and allocation failures. Explicitly demonstrate historical inspection; do not imply restoration from it. The relevant contracts are specified above.

The present design also lacks a stable disk format, proven recovery, inspection/export/repair tools, established bindings, ordered range queries and automatic index/schema enforcement. **Inference:** these delivery costs may outweigh any algorithmic advantage for an early adopter. A small trustworthy state store with one convincing workload would establish usefulness beyond learning; expanding toward a general database before that would not establish demand. The relevant contracts are specified above.


## Decision summary and deferred alternatives

- Persistent maps and lists are early core types. Structural sharing makes
  historical values cheap to preserve; it does not settle concurrent writes.
- Use private final record assignments and conservative observation validation.
  Fine-grained changeset replay was rejected for the initial version because
  list positions, ancestor replacement, delete/recreate, missing reads, and
  history retention multiply the rules to verify. Record boundaries provide a
  smaller contract while still accepting independent existing-record edits.
- CRDTs remain a separate experiment. They require explicit identities and merge
  semantics; accepting concurrent edits does not itself preserve application
  invariants. Distributed replication is outside this plan.
- Durable commits are the default for file storage. An explicit weaker mode can
  be designed later. Append-only copy-on-write publication is the candidate;
  a separate WAL is not currently required.
- Retention is explicit. A live view and a persisted historical checkpoint have
  different lifetimes. Releasing retention is not an immediate reclamation promise.
- File formats are versioned from the beginning and initially experimental.
  Application record migrations use normal transactions; format conversion is
  a separate future tool. Schema/index enforcement belongs to the wrapper.
- Retained history supports inspection, not automatic undo, writable branches,
  or restore-as-new-commit. SQL, general queries, multi-process writers,
  automatic callback retries, list rebasing, and code generation are deferred.
- Implementation is Rust, accepted after milestone 1. Native Rust uses `rlib`;
  Odin/foreign callers use the C header and `cdylib`. Independent Rust static
  libraries alongside the tested wgpu-native archive are excluded. The engine
  does not offer C-only or single-header source distribution.

## Vocabulary

| Term | Meaning |
| --- | --- |
| Snapshot | Immutable view of application data at one version. |
| Retained snapshot | Snapshot explicitly kept across close/reopen until released; not a backup. |
| Base snapshot | The version from which a live transaction starts. |
| Record | Stable collection/key entry; its entire nested value is one conflict unit. |
| Changeset | Final record assignments/removals, not a list operation history or byte diff. |
| Transaction | Base reads, tracked observations, and proposed assignments that commit together. |
| Conflict | Failed validation after intervening changes, including safe false conflicts. |
| Merge | Apply compatible record assignments to the current state; no last-writer-wins rule. |
| Record schema | Versioned logical descriptor with stable type/field identities. |
| Record migration | Application transformation between record meanings/versions. |

## Shared definition of done

A milestone is complete when its stated behavior runs from a fresh checkout,
its relevant acceptance cases pass, and limitations are written beside the API.
Preserve minimized regression cases for discovered bugs. Test the artifact a
consumer receives, including release configurations, not only an instrumented
development build. Update this document when an experiment changes a decision.

Before calling a durable release trustworthy, require evidence across logical
model checks, transaction schedules, allocation failures, simulated storage
failures, malformed-file fuzzing, real reopen/interruption tests, and actual
supported platforms. State the fault model and untested assumptions. Coverage
is feedback about exercised paths, not a correctness certificate. Performance
claims require reproducible workloads with matched durability and retention.
