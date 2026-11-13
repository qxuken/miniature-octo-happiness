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
the Linux extension adds native storage and in-place compaction. Windows
adds local NTFS storage and verified compact copies using the same v2 bytes.
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
- [Windows file storage](#windows-file-storage)
- [Review corrections: scans and unique presence](#review-corrections-scans-and-unique-presence)
- [Snapshot replication](#snapshot-replication)
- [Optional collection merge policies](#optional-collection-merge-policies)
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
multi-process writer coordination, general queries, automatic schema enforcement,
and generated migrations. The [snapshot replication design](#snapshot-replication)
defines a first network extension. The implementation sequence and stopping points
follow in this document.

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

The baseline uses Rust/Cargo 1.99 or later and edition 2024. The default engine
build has no external crate dependencies.
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
root/witness pair, completes all durability barriers, then verifies the graph
and the selected root's matching witness before it can replace a file. Sharing
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
Native Linux/Windows runtime results were pending at this milestone; later
checkpoint 4203e0a passed both. Rosetta execution remains unavailable on this host.

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
x86 Linux, Windows and macOS. Native non-macOS execution was pending at this
milestone; later checkpoint 4203e0a passed Ubuntu and Windows.
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
C ABI, examples and `weekend-tasks` commands as macOS. The later
[Windows adapter](#windows-file-storage) extends the file factories. Other Linux
ABIs return `UnsupportedPlatform` before file I/O. The in-memory and byte-backed
engine remain buildable there.
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

**Remaining scope at this milestone:** native x86_64 Linux execution awaits CI; Windows file
persistence, older libc compatibility, other Linux ABIs, additional filesystems,
network/FUSE/overlay-backed database paths, inaccessible/security-specific
metadata policies, actual device power cuts and host/VM crashes remain unverified.
Ext4 is the tested stack, not a whitelist detected/enforced by the adapter.
Successful fsync/exchange and the format's protected-region/safe-append rules
remain required assumptions. Process-kill tests retain kernel/device caches and
cannot certify those physical assumptions. The 64 MiB image cap, retained-ID
limits, explicit compaction quiescence and no automatic recovery-copy cleanup
remain unchanged.

## Windows file storage

`Database::create_file` and `open_file`, the typed `Store`, the task CLI, and
`wdb_file_create` / `wdb_file_open` support local fixed NTFS. The same `ByteStore`
codec writes v2 images, with the same root, witness, recovery and retention rules.
Rust uses native paths and the C ABI uses explicit-length UTF-8 on Windows.
The existing C exports and status codes cover the new platform.

The adapter resolves and opens the parent before touching the final component.
It checks the drive type and filesystem, rejects alternate data streams and
special device prefixes, and uses `FILE_FLAG_OPEN_REPARSE_POINT` to reject final
reparse points. Sparse, compressed and encrypted files are outside the supported
storage scope. Creation uses `CREATE_NEW` and inherits the parent security policy.
An interrupted creation can leave an incomplete file which recovery rejects.

`CreateFileW` sharing mode zero holds the exclusive file lease through recovery
and publication. Competing opens through the canonical path or a hard-link alias
return `Busy`. The handle also denies ordinary deletion and rename of the open
file. `ByteStore::unlock` closes the handle. Opening again obtains a new lease.
File identities come from `BCryptGenRandom` using the system-preferred RNG.

Positioned I/O uses a synchronous file handle with `FILE_FLAG_WRITE_THROUGH`.
Every protocol barrier calls `File::sync_all` (`FlushFileBuffers`) and propagates
failure. The Windows creation contract relies on NTFS persisting the file and
its namespace metadata through write-through writes and the first successful
flush. The documented NTFS metadata ordering and the underlying storage stack's
4 KiB protected-region and safe-append behavior are assumptions of this adapter.
Process interruption checks exercise recovery with the kernel and device caches
still live. Hardware power loss remains a separate validation requirement.

On Windows, use `db.compact_to(storage::FileStore::create(new_path)?)` to write a
verified compact image. The original stays open and the new file can be reopened
independently. The typed `Store::compact_to` offers the same operation. In-place
`compact_file` / `wdb_file_compact` remain macOS/Linux capabilities. The Unix
protocol relies on holding both leases through an atomic namespace exchange and
synchronizing the directory. Windows `ReplaceFileW` acquires its own handles,
has partial failure states, and its write-through flag is unsupported. Windows
needs a separate durable replacement protocol before enabling that operation.

The Windows CI job runs create/reopen, retained snapshots, file concurrency,
compact copies, Unicode paths, hard-link exclusion, the CLI lifecycle, and
shared-library calls including UTF-8 path validation. The common file worker
terminates a child process before/after selected I/O calls in creation, data
commits, retain/release and recovery, then checks the reopened state. The worker
also checks that process exit releases the exclusive handle. The simulator
continues to exercise torn writes and failed synchronization independently of OS
behavior.

Primary references include Microsoft's [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
[FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers),
[BCryptGenRandom](https://learn.microsoft.com/en-us/windows/win32/api/bcrypt/nf-bcrypt-bcryptgenrandom),
and [ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew).

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

## Snapshot replication

The selected first design uses one configured primary and one passive follower.
The optional Rust `replication` feature implements export, transport framing,
installation, and durable progress. It enables the `sha2` crate for image
verification. The application runs the transfer on its own schedule after local
publication. Automatic promotion and writable replicas require separate designs
for writer authority and divergence.

### Acknowledgments and visibility

| Result | When it can be returned | What the caller can rely on |
| --- | --- | --- |
| Primary `commit() -> Ok(revision)` | The primary completes its v2 data, root and witness barriers and installs the head | The revision is recoverable on the primary under its storage assumptions. New local snapshots can see it. |
| Follower durable acknowledgment | The follower verifies and synchronizes the received image, then commits its manifest entry | The follower can recover the selected image, including its current root and retained catalog. |
| Replication wait reaches its deadline | The caller has a local revision and the follower has yet to confirm the requested progress | The caller can report local success and pending replication, then wait again for that revision. |

The primary releases the commit gate before network transfer or any follower
wait. A disconnected follower can lag while local commits succeed. If the
primary's storage is lost, recovery from the follower starts at its last
recoverable installed revision. Newer local successes may be lost. A local
`OutcomeUnknown` still requires primary recovery. Recovery can find a durable
revision whose original response never reached its caller.

A progress wait can use `installed_revision >= requested_revision` within the
same stream. A newer complete image can supersede intermediate snapshots. The
follower's installed revision describes a recoverable database state. Explicitly
retained snapshots inside that state are the available historical views. If we
later serve reads from the follower, the read interface should report which
installed revision it pinned. The distinction between durable receipt and
visibility also appears in [PostgreSQL's acknowledgment modes](https://www.postgresql.org/docs/current/runtime-config-wal.html#GUC-SYNCHRONOUS-COMMIT).

### Export, transfer and installation

An operator pairs the two endpoints with a fresh stream ID, an expected v2
file identity and authenticated peer identities. The stream ID identifies one
continuous primary history. Compact copies preserve the v2 file identity, so
restoring the primary from an older backup or forking a copy requires a new
stream and explicit enrollment. A file identity by itself cannot distinguish
those histories.

1. **Export.** Capture the current commit revision and call `compact_to` while
   the primary is quiescent. Use a fresh local destination. This includes the
   current and retained roots, schema and index records, and allocation counters.
   Release exclusive access to the primary after the compact image has been
   synchronized, verified and hashed. The export retains its own byte-store
   lease for the transfer and retries. The primary can resume writes while
   that lease is held. The `&mut Database` requirement makes export a local
   pause in this first version.
2. **Transfer.** Send an envelope containing protocol version, stream ID, file
   identity, commit revision, image length and SHA-256 digest, followed by the
   image bytes. The application authenticates the configured peer and supplies
   the connection. The receiver bounds the declared length by the v2 image
   limit before creating a staging file. It exclusively creates a fresh file
   at the inactive slot in its private directory. A transfer carries
   identities and bytes with no receiver path.
3. **Verify.** Write to the new file using the native adapter's synchronization
   and creation barriers. Hash the synchronized file and run full v2 recovery
   verification. The recovered identity, revision and end must match the
   envelope. The selected root must already have its matching v2 durability
   witness. The verifier checks the reachable graph and retained roots, and
   closes the file without rewriting it. The digest covers the same bytes
   at installation and on restart.
4. **Install.** Commit one manifest record containing the stream ID, revision,
   digest and slot in a separate v2 database. The manifest is the follower's
   authoritative selector. Keep the previous image available through this
   commit. Borrowed replica snapshots pin the decoded read view and exclude
   another installation until their borrows end. Separate slots let the
   follower install a copy on Windows through the existing create and sync
   contract.
5. **Acknowledge.** After the manifest commit succeeds, report the installed
   `(stream_id, revision, digest)` to the primary. On restart, recover the manifest
   and verify its referenced image before reporting progress. A manifest commit
   with an uncertain outcome requires recovery before any acknowledgment or
   image cleanup.

The receiver serializes installation and advances monotonically within a
stream. An identical retry returns the existing acknowledgment. An older offer
returns the installed progress. An offer at the installed revision with a
different digest is rejected as a conflicting offer. Senders reuse the same
artifact for retries and ask for installed progress on reconnect before exporting
again. The primary accepts progress only from the configured follower for the
current stream and within its own verified history. A late response cannot
reduce recorded progress.

Run one transfer at a time and coalesce lag into a newer complete snapshot.
Retry interrupted transfers from the start. The follower bounds staging to
two image slots of up to 64 MiB each, plus its v2 manifest. Before staging,
it reclaims only the inactive slot after recovering the manifest and while
holding the manifest lease. An interrupted initial enrollment needs a fresh
directory. Disk or manifest capacity exhaustion pauses replication and leaves
the last verified read view available. The manifest currently grows by one
v2 commit per installation. Long-running manifest rotation and reclamation
need an explicit maintenance policy. If a consumer eventually requires every
revision, it will need a durable change feed with retention and gap recovery
rules.

### Rust API and framing

1. Enable the `replication` feature. Build a `Primary` from the recovered
   byte-backed database and a fresh nonzero stream ID. Use its `Enrollment`
   to create the `Follower` in an existing empty, private local directory.
   Persist the enrollment with both peers' configuration and reuse it on open.
2. Call `Primary::poll` after connecting or recovering from a lost reply.
   If the follower lags, capture one `Export` with `Primary::export_file`
   and send it with `Primary::ship`. Drop the export to release its lease
   before removing its file. The scheduler owns that one export file, retries
   and coalescing. `Follower::serve_authenticated` handles each request.
3. Treat `DurableAck` as a separate result from local `commit()`. An
   application can check its stream and compare its revision with the local
   revision it is waiting on. Set deadlines on the transport and open a fresh
   authenticated channel for each exchange. Peer authentication and transport
   setup belong to the application, for example with mutually authenticated
   TLS and configured peer identities. The stream ID and digest identify
   history and content.

Protocol v1 uses seven ASCII bytes `WDBREPL` and a version byte of 1. The
next byte is 0 for a progress query or 1 for an offer, followed by the
16-byte stream and 16-byte file identities. An offer appends revision and
image length as little-endian u64s and the 32-byte SHA-256 digest. A reply
starts with one status byte. 0 means empty, 1 requests the offered image
body, and 2 returns the stream identity, little-endian u64 revision and
digest. Errors 3 through 10 represent unsupported format, foreign stream
or file, conflict, limit, corruption, recovery required, invalid request
and I/O failure respectively. If requested, the declared length frames
the entire image. Each exchange ends after its terminal reply or a
transport error. A valid stale or duplicate offer can receive a terminal
reply without sending its body.

The follower's `replica-manifest.wdb` stores one `replica-v1` row. Its bytes
contain the stream and file identities, then optionally a slot byte, the
revision and length as little-endian u64s, and the digest. The slot selects
`replica-0.wdb` or `replica-1.wdb`. A recovered manifest selects exactly one
image even when the other slot contains a complete prior or prepared image.
The manifest uses the engine's default limits internally. The caller's limits
apply when decoding the replica on installation and reopen.

### Failure cases and acceptance checks

| Failure or ordering case | Required result |
| --- | --- |
| Primary crashes after local success and before export | Recover the primary and export a verified current image. Follower progress remains at its earlier revision until installation. |
| Local publication returns `OutcomeUnknown` | Recover the primary before exporting. A complete recovered revision can be exported after stabilization. |
| Export fails or is interrupted | Keep the primary's commits intact. Transfer only a completed verified image. |
| Transfer drops, truncates, exceeds the byte cap, or corrupts data | Keep the active manifest and image. Discard or reclaim the incomplete candidate and retry within the staging budget. |
| Wrong stream, file identity, revision or protocol version | Reject the offer before installation. Explicit enrollment starts a new stream. |
| Older image, duplicate image or conflicting digest at the same revision | Preserve the installed generation. Return progress for an older offer, acknowledge an identical retry, and report the conflicting offer. |
| Follower stops before its image is synchronized and verified | Recover the old manifest. An incomplete candidate has no durable acknowledgment. |
| Follower stops after image sync and before manifest commit | Recover the old manifest and preserve its image. The prepared candidate can be reclaimed after recovery. |
| Manifest commit definitely fails | Keep the old image selected. Reclaim the unreferenced candidate on a later staging attempt after any required recovery. |
| Manifest publication returns an uncertain outcome | Recover the manifest and validate the selected image before acknowledging or deleting either generation. |
| Follower stops after the manifest commits or the reply is lost | Recover and report the same installed revision. The sender can resume waiting without repeating the primary transaction. |
| Installed image is missing or damaged on restart | Surface replica failure and require repair or reseeding. An older orphan is insufficient evidence to report progress. |
| Follower is offline or out of space | Local acknowledgments continue under the primary's own storage contract. Follower progress stalls and the sender coalesces future offers. |
| Primary storage is permanently lost while the follower lags | The follower provides its last recoverable revision. Local successes after that revision are outside the replica's recovery point. |

The simulator exercises failed staging sync and each of the manifest's data,
root and witness barriers with both the pending writes dropped and the pending
writes surviving. It reopens the selected state and retries the same offer.
The scripted transport checks malformed and corrupted offers, truncation,
duplicate and delayed offers, and an acknowledgment lost after installation.
The native TCP tests compare current rows, retained roots and secondary
lookups after transfer. A second process is killed during upload and again
after the manifest commit but before its reply. Reopen keeps the old image
in the first case and reports the new revision in the second. A missing or
damaged selected image fails reopen even if a previous image exists.

**Observed validation:** all five replication tests pass in debug and
release on arm64 macOS and aarch64 Linux with a Docker-managed ext4 volume.
The full macOS `python3 tools/check.py` passes workspace Clippy, tests, and
Rust, C and Odin consumer checks in both modes. All-feature cross-checks
pass for x86_64 Linux and Windows MSVC. Windows replication has compilation
coverage here. The native interruption tests exercise process kills under
the existing file adapter assumptions.

### Publication review and refactor

The review reproduced two failures. A follower with a 64-byte value budget
could enroll and verify a scalar snapshot but fail to write its private
manifest selector. Separating the manifest and replica limits lets that
snapshot install and reopen while the follower still rejects a later image
that exceeds the replica budget. Compact-copy verification also accepted a
fresh root with a missing witness. The codec now reports whether the selected
root already has its matching witness. Compaction and replication both require
it. Ordinary writable recovery still synchronizes and witnesses a verified
root, including one whose witness is missing.

Transaction, collection and retention writes use one guarded entry and one
persist-and-install path. The state fields are installed after durable storage
returns success. The writer derives the v2 slot from the revision's parity and
uses the same write-and-sync operation for the append, root and witness.
Definite append errors and uncertain root or witness errors retain their
existing meanings. The Unix replacement driver names each namespace step
explicitly, so its fault observer needs no separate phase flag. The measured
mechanical refactor removes 37 production lines. Follower installation still
synchronizes and verifies the image before committing the manifest and issuing
its separate durable acknowledgment.

The before-and-after simulator comparison produced identical bytes and I/O
traces for ordinary commits, retention changes, odd and even compact copies,
and reopen followed by another commit. Both regressions failed on the
pre-fix code. The resulting tests also exercise the existing fault schedules
and native process interruptions.

**Observed validation:** `python3 tools/check.py` passed on arm64 macOS with
126 tests and six doctests per build mode, plus the Rust, C and Odin consumer
checks. Native aarch64 Linux on a Docker-managed ext4 volume passed the
all-feature workspace suite with 128 tests and six doctests per build mode.
Warnings-denied all-feature cross-checks passed for x86_64 Linux and Windows
MSVC. Windows was checked through compilation.

## Optional collection merge policies

The Rust `crdt` feature provides mergeable types and a generic record
interface for two configured writers. Each actor has its own byte-backed
database and can commit while disconnected. For the standalone counter,
enroll a dedicated local collection on each
side with `Counter::enroll`. The local collection numbers may differ. Both
sides configure the same stable replicated collection ID, policy generation
and ordered pair of actor IDs. Reopen with `Counter::open` and the same
configuration. The reserved counter row stores that configuration, the local
actor and the two components together. Route writes to the row through the
counter interface so raw writes cannot overwrite its merge state.

Each actor advances only its own component. The receiver merges by taking
the larger known total for the sender's component. It checks that the
sender's copy of the receiver's component never exceeds the receiver's
local total. The count is the sum of both components, each bounded by half
of `u64::MAX` so valid states stay within range after a merge. Merging
component totals is associative, commutative and idempotent on states
produced by the configured actors. [State-based CRDTs and grow-only counters](https://dsf.berkeley.edu/cs286/papers/crdt-tr2011.pdf)

An application opens a direct duplex connection authenticated to the configured
peer, sets deadlines and calls `push_authenticated` on one side and
`serve_authenticated` on the other. An offer carries the current counter state.
The receiver validates it, commits any advance in an ordinary local transaction,
and returns its published components as a `MergeAck`. A duplicate or delayed
offer can be acknowledged from existing state without a new commit. If the
reply is lost, the sender can retry on a fresh connection. Each push is
one-way, so the scheduler pushes in both directions to converge. Local
`increment` returns the node's commit revision, and the peer's acknowledgment
reports which components its own database covers. These are separate results.

The persisted row and wire frame use 81 bytes. They start with seven ASCII
bytes `WDBGCTR` and encoding version 1, then a 16-byte collection ID,
a little-endian u64 policy generation, two 16-byte actor IDs, a one-byte
actor index and two little-endian u64 components. The actor index identifies
the frame's sender. The receiver rejects mismatched collection, policy or actor
before changing its state. Concurrent local updates and merges use tracked
reads and may return a transaction conflict for the caller to retry.

Policy generation, state encoding version and local commit revision are
separate. This implementation pins the enrolled policy. A future migration
would have to preserve the old contributions and change the descriptor and
state atomically. Each actor identity belongs to one writable history.
Restoring an old copy under the same identity can lose increments, including
increments no peer has ever seen. A newer copy of that actor's component
detected in a peer frame causes a conflict. Restored writers need a new
identity and an explicit migration. Retained snapshots can read the counter
as it was visible on their node through `Counter::count_at`.

The replicated records use full mergeable state over direct connections.
A sync scheduler can later choose when to exchange state. General deletes,
secondary uniqueness, coordinated writes and discovery have separate rules.
The passive snapshot follower can still back up the local database and its
retained roots.

### Composing a set and a counter

An application defines its own state by implementing `crdt::Record` and opens
it with `Replica::<AppState>::enroll` or `open`. The trait supplies a row key,
a versioned format marker, a fixed payload codec of up to 4096 bytes, the
field joins, and checks for actor-owned fields. `GrowSet`, `GrowCounter`,
`AddWinsSet`, and `MultiValueRegister` provide fixed-width codecs and joins
for composition.
The application owns the record's merge rules and must preserve validity
when combining valid states.

`Replica::update` gives the edit closure the record and its local actor.
Before committing, the replica checks that the edited record still covers the
previous state and that it does not advance the other actor's fields.
`Replica::push_authenticated` returns a separate `ReplicaAck<AppState>`
after the receiving peer has published the merge. The executable study
example declares its layout and calls the generic interface.
[App-owned StudyProgress example](examples/study_progress.rs)

`#[derive(Record)]` is the fixed-width composition shortcut. On
[StudyProgress](examples/support/study.rs), `#[record(...)]` specifies the
`course-progress` row key, the eight-byte format marker, and every field in
wire order. The derive concatenates the nested `Record` payloads without
padding or field headers and delegates joins and peer checks to those same
fields. Rust declaration order is independent of wire order. The application
must change the format marker when it changes the payload layout or codec.
Use a handwritten `Record` for a custom layout or cross-field merge rules.

The study payload is 2,140 bytes: completed lessons (8), practice minutes
(16), planned lessons (2,048), weekly goal (34), then session limit (34).
A wire fixture matches the full 2,205-byte offer, including the 65-byte
envelope, with both the handwritten and derived codecs. The six-line
attribute replaces the 62-line trait implementation and the separate size
constant; the example record module goes from 99 to 39 lines.

The study record lives with the example and uses `Replica<StudyProgress>`.
An update can insert a completed lesson and add practice minutes in one
commit. The completed set joins by union and accepts IDs from 0 through 63.
The counter joins by taking the maximum for each actor. Peers that complete
lessons {1, 3} and {2, 3} offline and log 20 and 15 minutes converge to
{1, 2, 3} and 35 minutes after a push in each direction.

Run `cargo test -p weekend-db --features crdt --test crdt` for the loopback
examples. They cover offline increments, convergence, delayed offers,
storage failure, a lost reply followed by recovery and retry, retained reads,
and rejection of a different policy generation or stale actor state. The
composite test reopens one peer after the merge and checks that a duplicate
offer leaves its revision alone.

### Resolving concurrent adds and removals

`AddWinsSet` is an observed-remove set for the two configured actors and
IDs from 0 through 63. Each add advances its actor's sequence for that ID,
including an add to an already visible ID. A remove records the additions
visible at that replica. A concurrent add can advance beyond that recorded
sequence and survive the merge. Once a replica has seen the concurrent add,
another remove can clear it. [Observed-remove semantics](https://arxiv.org/abs/1210.3368)

Each ID and actor has an add sequence and a removal watermark. Both are
little-endian u64s. The decoder requires the watermark to be at most the
sequence. The join takes their componentwise maxima, and an ID is visible
when any sequence exceeds its removal watermark. That keeps the stored
state monotone even as an ID disappears from the visible set. The fixed
2048-byte payload keeps removal evidence across reopen and delayed replay.
As a standalone `Record` the set uses key `add-wins-set` and marker
`WDBASET` followed by encoding version 1.

The study example composes `planned: AddWinsSet` with its completed set
and practice counter.
The loopback test exercises a concurrent re-add, a later observed removal,
and replay of the original add after simulated crash and reopen.

### Resolving concurrent assignments

`MultiValueRegister` stores the latest assignment sequence and an optional
u64 value for each of the two actors. Assigning a new value advances the
local sequence and retires the assignments the writer has seen. A join
keeps the newer sequence for each actor. At equal sequences, a retired
assignment stays retired. This preserves concurrent choices and keeps a
delayed offer from reviving a superseded value. `values` lists the distinct
visible choices. [State-based multi-value registers](https://www.csa.iisc.ac.in/~raghavan/CleanedPods2021/crdt-shapiro-2011-techreport.pdf)

The standalone register uses marker `WDBMVRG` followed by version 1 and key
`multi-value-register`. Its payload is 34 bytes. In actor order, each entry
stores a little-endian u64 sequence, a presence byte, and a little-endian
u64 value. A retired entry retains its sequence and encodes a zero value
with the presence byte cleared. A live entry requires a positive sequence.

The manual-mode study test starts with offline choices of 60 and 90 minutes.
Each peer reviews both values and resolves them offline, one to 70 and the
other to 80. Each resolution supersedes 60 and 90. The resolutions are
themselves concurrent assignments, so both 70 and 80 remain visible after
the next sync. The manual resolution transaction checks that the register
still matches the reviewed state before accepting a choice.

`MultiValueRegister::resolve_with` gives each field an automatic read
policy. An empty register yields `None` and a single choice yields that
value. For two distinct choices, the function calls the field's policy in
actor order. Peers use the same deterministic policy for the same
collection generation. Change the generation when changing the policy's
meaning.

`ResolutionPolicy` gives a field a named deterministic rule. A
`PolicyRegister<P>` reads the effective value and can apply that rule in
an ordinary `Replica::update` transaction. `resolve(actor)` creates a new
assignment when there are two distinct choices. `MaxRegister` and
`MinRegister` select the higher and lower value respectively. They use
standalone format markers `WDBMAXR` and `WDBMINR` with version 1 and share
the multi-value payload layout. An embedded register gets its policy from
the enclosing record's configuration and generation.

With explicit resolution, StudyProgress peers can each apply `MaxRegister`
offline and commit 80 from {70, 80}. After the next sync, the register
retains both assignment identities and exposes one distinct value, 80.
The manual-mode test checks an offline policy commit across reopen before
sync, convergence, stale review rejection, and replay after a later edit.

`Record::resolve_conflicts` lets a record opt fields into automatic
resolution. After an incoming offer advances the record, the receiver
passes the merged state and local actor to the hook. It validates the
result as a local edit, then commits the merge and resolution together
before replying. A repeated offer covered by the stored state can be
acknowledged without another commit.

StudyProgress uses `#[derive(Resolution)]` with `#[resolve]` on its
`weekly_goal: MaxRegister` and `session_limit: MinRegister` fields.
The derive implements `Resolution::resolve_fields` by visiting the marked
fields in declaration order. `resolve = AUTO_RESOLVE` in `#[record(...)]`
wires that method into the record's sync hook when the flag is true. The
fields declare their policies next to their participation in automatic
resolution. The `crdt` feature reexports both derives. Renamed dependencies
can use `crate = "path"` in the `#[record(...)]` and `#[resolution(...)]`
attributes.

Run `cargo run -p weekend-db --features crdt --example study_progress`.
The peers set weekly goals of 60 and 90 minutes and session limits of 20
and 30 minutes while offline. Both fields converge in one merge commit
per receiver. The committed weekly goal is 90 and the session limit is
20. The test reopens the first receiver after its acknowledgment and
checks that another exchange in both directions leaves the revisions
alone. The expanded example record uses marker `COURSES` with version 6
and policy generation 7 in automatic mode. Its manual mode uses marker
version 5.

### Two file-backed StudyProgress writers

The [study-peer application](apps/study-peer/README.md) gives each actor its
own process, file, private key and configured peer address. The same
`Replica<StudyProgress>` serves edits, incoming merges and outgoing offers
on separate threads. [Rustls](https://docs.rs/rustls/0.23.46/rustls/server/struct.WebPkiClientVerifier.html)
and certificate generation are dependencies of the application. Before
handing a stream to the CRDT engine, both sides
validate TLS and check the presented leaf certificate against the configured
peer's certificate. The application uses a fixed roster and manually
exchanged pins.

Each outgoing attempt opens a fresh connection and offers the current
committed state. The runner polls every 250 ms after success and backs
off to at most two seconds after failures. TCP connection setup and the
subsequent TLS and record exchange each have a three-second deadline.
Both directions run independently. Periodic exchange resumes on process
start, so pending work is recoverable from the stored record without a
separate send queue. A local command reports its own commit revision.
The peer status reports a separate acknowledgment covering the offered
state. For its draft-only edits the application can retry a definite
transaction conflict on a fresh snapshot. It surfaces `OutcomeUnknown`
to the caller, who must recover and determine the outcome before
resubmitting an increment.

A deterministic overlap test holds a local draft while an incoming merge
commits against the same row. The stale draft conflicts and a fresh retry
preserves both increments. The native process test starts with 20 and 15
offline minutes, reconnects the peers, and stops the receiver at the first
reply write after its merge commit. The receiver commits another five
minutes while the sender is waiting, then is killed. After reopening,
retry acknowledges the covered offer at the recovered revision. Sync in
the other direction converges both files at 40 minutes, a 90-minute
weekly goal, and a 20-minute session limit. Additional exchanges in both
directions leave the local revisions at 4 and 5.

The peer and CRDT tests passed in debug and release on arm64 macOS and
aarch64 Linux. The full macOS workspace and Rust, C and Odin consumer
checks passed. The peer app and its tests passed warnings-denied
cross-compilation for x86_64 Windows MSVC.

## Coordinated writes to one file

The [write coordinator demo](apps/write-coordinator/README.md) has two
client processes and one process that holds the database file lease.
Clients connect over loopback and submit conditional replacements for a
single unsigned record. The app uses the core database API.

A read returns the revision and value from the same snapshot. A write
supplies the revision it read. The coordinator handles one request at a
time, so the revision check and the subsequent transaction commit run
before it handles the next request. This demo uses the database revision
as the precondition for its single editable record. It reports
`committed <revision>` after the native commit succeeds. A stale write
returns `conflict <revision> <value>` without a commit. The client can use
that state to recompute its edit and make a new conditional request.
A connection failure before the reply leaves the outcome unknown to the
client. The demo surfaces that failure without replaying the edit.

The process test starts both clients at revision 2 and value 0, then
submits replacements of 20 and 15 before reading either response.
Either client can win. One commits revision 3 and the other gets a
conflict with the winning value. The loser recomputes its edit and
commits 35 at revision 4. Killing and reopening the coordinator
preserves revision 4 and its value, and replaying the original revision
2 request still conflicts.

The test passed in debug and release on macOS and Linux. The coordinator
and its test also passed warnings-denied cross-compilation for Windows MSVC.

## MC/DC decision ledger

The [production readiness roadmap](docs/production-readiness.md) tracks
the remaining module inventory, fuzz targets and release gates.

### TX-V1. Whole-database revision check

The [transaction validation guard](src/engine.rs#L712) rejects a draft
when both conditions hold.

- **A** is `validation == WholeDatabase`.
- **B** is `base.revision != current.revision`.

The [table-driven test](tests/transactions.rs) starts from two existing
records `a = 0` and `b = 0` at revision `r`. The draft writes `a = 1`
in every case. The intervening transaction, when scheduled first,
commits `b = 2` without changing the observed record `a`. Each row shows
the state immediately after attempting the draft's commit.

| Case | Validation mode (A) | Intervening commit (B) | Draft result | Head revision | Published `a`, `b` |
|---|---|---|---|---|---|
| 1 | WholeDatabase (true) | Yes (true) | Conflict | `r + 1` | `0, 2` |
| 2 | Records (false) | Yes (true) | Commit | `r + 2` | `1, 2` |
| 3 | WholeDatabase (true) | No (false) | Commit | `r + 1` | `1, 0` |
| 4 | Records (false) | No (false) | Commit | `r + 1` | `1, 0` |

Cases 1 and 2 show A's independent effect with an advanced head.
Short-circuit evaluation skips the revision comparison in case 2, and
the successful intervening commit establishes the revision mismatch
in the fixture. Cases 1 and 3 show B's independent effect with
`WholeDatabase` selected. Case 4 is the existing record-mode control.

`existing_record_edits_follow_validation_mode` extends the existing
independent-edit test. It keeps both publication orders in record mode
and adds the two whole-database cases with the same draft. It checks the
commit result, head revision, and both published values. The existing
`whole_root_baseline_rejects_unrelated_commits_and_no_partial_writes`
still checks a stale multi-write draft and its terminal state.

All 23 tests in the transaction, concurrency, and retention suites passed
in debug and release on arm64 macOS with rustc 1.99.1. Removing A from the
guard in a disposable copy made case 2 fail. Removing B made case 3 fail.
The table and tests provide MC/DC evidence for both conditions of TX-V1.
Collection validation, record validation, and durable publication are
separate decisions. The workspace coverage script reports line and region
coverage separately.

The verification used the working tree based on `13d61c6`.
The Git blob for `src/engine.rs` was `6696ea74ac00dcfa2eb7934c7c15cdabb13e474d`
and the blob for `tests/transactions.rs` was `0833308ab1f9a0a5856e61b16457a0a48d4ae5ef`.

### TX-V2. Collection and record observations

The test `record_and_collection_validation_distinguish_changes_and_removal`
in [transactions.rs](tests/transactions.rs) keeps the draft read-only
to isolate validation from write preparation. Both checks compare an
observed identity/version with the current head.

| Observation | Intervening action | Draft result |
|---|---|---|
| Existing record `a` | Write another existing record `b` | Commit |
| Existing record `a` | Replace or delete `a` | Conflict |
| Existing record `a` | Drop its collection | Conflict |
| Scan or absent key | Write another collection | Commit |
| Scan or absent key | Write another record in the observed collection | Conflict |
| Scan or absent key | Drop the observed collection | Conflict |

Each successful control advances the head through an unrelated commit.
The record comparison is a single predicate with distinct missing-record,
missing-collection, changed-version, and unchanged-version paths.
The collection comparison similarly distinguishes a missing collection
from a present collection with a changed version. Every row checks the
published revision, unchanged rows after the read-only draft, terminal
transaction state, rejected second commit, and the pinned old snapshot.
Existing tests retain the equal-value, ABA, write-skew, overlay,
collection replacement, and whole-database pairs.

The `txn_schedule` fuzz target adds an independent history of committed
write footprints. It predicts both accepted and rejected drafts using
the documented observation scopes, without reading engine versions.
It checks values during the draft, final state, publication and data
revisions, and pinned and retained snapshots. See the
[fuzz instructions](fuzz/README.md) for its bounded schedule and results.

### PUB-V1. Local commit barriers

The [publication fault test](tests/durability.rs) uses the same staged
two-record edit in every run. Let `r` be the last acknowledged revision.
The test takes the append, root and witness sync positions from a clean
commit trace. It then injects `Fault::Error` at every I/O and checks the
exact trace prefix, result, in-process revision, and reopened revision
and values.

| Case | Injected failure | Pending bytes after power loss | Commit result | Revision in process | Revision reopened |
|---|---|---|---|---|---|
| P1 | None | Drop pending writes | `Ok(r + 1)` | `r + 1` | `r + 1` |
| P2 | Append write or sync | Drop or persist pending writes | `Io` | `r` | `r` |
| P3 | Root write, before any bytes are written | Drop or persist pending writes | `OutcomeUnknown` | `r` | `r` |
| P4 | Root sync | Drop the pending root | `OutcomeUnknown` | `r` | `r` |
| P5 | Root sync | Persist the pending root | `OutcomeUnknown` | `r` | `r + 1` |
| P6 | Witness write or sync | Drop or persist pending writes | `OutcomeUnknown` | `r` | `r + 1` |

P1 paired with P2 at the append sync shows that append success gates
the root write. P1 paired with P4 shows that root sync success gates
the witness write. P1 paired with P6 shows that witness success gates
in-process publication and acknowledgment. These are successive I/O
gates. Each failure masks the stages after it. P4 and P5 hold the commit
error fixed and vary whether the root survives. The test also checks that
retaining only the root's pending write reopens `r + 1` and that retaining
only its first 63 bytes reopens `r`.

### REC-V1. Witness agreement with the selected root

The [recovery guard](src/storage/codec.rs#L705) rejects a selected root if
any valid witness satisfies `A || (B && C)`. A means the witness revision
is newer than the selected root. B means the revisions are equal.
C means the witness and root metadata differ. The test keeps roots 1 and
2 and the same appended bytes in every case. It presents one valid
witness to selected root 2. The mismatched witness keeps revision 2 and
uses the descriptor and end of revision 3 with a repaired checksum.

| Case | Witness | A | B | C | Reopen result |
|---|---|---|---|---|---|
| W1 | Older, revision 1 | False | False | True | Revision 2 |
| W2 | Newer, revision 3 | True | False | True | `Corrupt`, no revision |
| W3 | Matching, revision 2 | False | True | False | Revision 2 |
| W4 | Same revision, different descriptor and end | False | True | True | `Corrupt`, no revision |

W1 and W2 isolate A. W1 and W4 isolate B. W3 and W4 isolate C. The table
records the logical condition values of the inputs. Short-circuiting
skips C in W1 and skips B and C in W2. Rejection leaves the durable bytes
unchanged. The existing corruption test also exercises an acknowledged
root lost while its newer witness remains intact.

### REC-V2. Stabilization before a writable reopen

The existing [volatile-root recovery test](tests/durability.rs) starts
with a failed commit at root sync. The full root for `r + 1` is visible
in volatile bytes and revision `r` is durable. A writable open must sync
the verified root, write its witness, and sync the witness before
returning a handle.

| Case | First reopen | Result of first reopen | Revision reopened after the next crash |
|---|---|---|---|
| S1 | All recovery I/O succeeds | Handle at `r + 1` | `r + 1` |
| S2 | Stabilization sync fails and the pending root is lost | `Io`, no handle | `r` |
| S3 | Stabilization sync fails and the pending root survives | `Io`, no handle | `r + 1` |
| S4 | Witness write tears after stabilization succeeds | `Io`, no handle | `r + 1` |
| S5 | Witness sync fails after stabilization succeeds | `Io`, no handle | `r + 1` |

S1 paired with S2 shows that stabilization success gates the witness
write. S1 paired with S4 or S5 shows that writing and syncing the witness
gate returning a handle. S2 and S3 vary only whether the root survives
the failed stabilization and subsequent crash. Each failed open stops at
the injected I/O. The second open checks the complete old or new record
state associated with the expected revision.

This pass covers local publication, witness agreement, and writable
recovery stabilization. The format parser, graph verifier, root-slot
adjacency check, and follower acknowledgment have separate decisions.

On arm64 macOS, 60 library, durability, compaction, file, replication,
and retention tests passed in both debug and release with all features.
In disposable copies, bypassing each publication gate and each writable
recovery gate triggered a regression. The witness test also detected
independent changes to the newer, same-revision, and agreement conditions
in cases W2, W1, and W4, respectively.

The verification used the working tree based on `13d61c6`. The Git blobs
were `000d289111767aa499624895de1ddbf2788ab14e` for `src/storage/mod.rs`,
`05bc4c409a690d7ba4cf7b63ab7757d430518124` for `src/storage/codec.rs`,
and `fa620ab7c8fb73fb631cb91a435d10201f6c39a1` for `tests/durability.rs`.

### REC-V3. Root-slot selection and adjacency

The [simulator test](tests/durability.rs#L368) saves real root slots from
commits 2 through 5 of the same store. Every case keeps the object bytes
through revision 5. It varies which saved slots appear in the two root
regions and clears both witness regions to isolate slot selection. It
checks both the reopened revision and a record value from that revision.

| Case | Even slot | Odd slot | Reopen result | Reopened value of `b` |
|---|---|---|---|---|
| R1 | 2 | 5 | `Corrupt`, no revision | — |
| R2 | Missing | 5 | Revision 5 | 5 |
| R3 | 2 | Missing | Revision 2 | 0 |
| R4 | 4 | 5 | Revision 5 | 5 |
| R5 | 4 | 3 | Revision 4 | 4 |
| R6 | Missing | Missing | `Corrupt`, no revision | — |

The [adjacency guard](src/storage/codec.rs#L712) rejects when the even slot
is present, the odd slot is present, and their revisions differ by more
than one. R1 and R2 isolate even-slot presence. R1 and R3 isolate odd-slot
presence. The gap comparison is masked when either slot is missing.
R1 and R4 isolate the gap with both slots present and the same newest
revision. R4 and R5 show that the highest revision wins in either slot.
R6 paired with R2 or R3 shows the need for at least one valid root, even
with intact commit objects in the tail. Both rejection cases perform
no write or sync and preserve the durable bytes.

The existing [acknowledged-corruption test](tests/durability.rs) covers
the witness interaction separately. A newer intact witness makes
recovery reject a lost newest root instead of reopening the older one.

All 37 library and durability tests passed in debug and release on arm64
macOS with all features. Six deliberate changes in disposable copies
were caught by their matching cases. They bypassed the gap check,
required each missing slot in turn, preferred each slot in turn, or
changed the error for an empty pair. Verification used the working tree
based on `13d61c6` with Git blobs
`05bc4c409a690d7ba4cf7b63ab7757d430518124` for `src/storage/codec.rs`
and `305ce014951ebd7c9c8825744c9641ce57d6b7bd` for `tests/durability.rs`.

### REC-V4. Verify the selected graph before opening

Recovery selects a root from the slot metadata, then verifies its
descriptor and the graphs reachable from it. A failure in that graph
returns an error with no handle and no recovery writes. If the newest
slot itself is absent, the older slot can be selected when the witness
checks permit it. The existing checksum and corruption tests exercise
damaged roots, payloads, backward references, and unreachable objects.
The [decision tests](src/storage/codec.rs) reuse their simulator fixture
and checksum repair helper.

In each new rejection case, the newest slot is valid and its witness
is cleared. The simulator checks the precise error, that no write or
sync occurred, and that the durable bytes are identical. As a control,
it clears the newest slot from a separate copy of the same damaged
image and verifies that the previous revision opens. That control
establishes that the older graph was usable throughout.

| Valid graph in the pair | Condition changed independently | Result with newest slot present | Result with newest slot cleared |
|---|---|---|---|
| Revision 3, current graph | Commit object's born revision is 2 | `Corrupt` | Revision 2 |
| Revision 3, current graph | Commit payload revision differs from slot, or its data revision exceeds the commit revision | `Corrupt` | Revision 2 |
| Revision 3, current graph | DATA revision disagrees with the descriptor, or DATA born revision disagrees with its payload | `Corrupt` | Revision 2 |
| Revision 3, empty directory | DATA next collection ID is zero | `Corrupt` | Revision 2 |
| Revision 4, two collections | Second collection repeats an ID, reaches the next ID, has revision zero, or has a future revision | `Corrupt` | Revision 3 |
| Revision 3, current graph | Record write revision is zero or exceeds the collection revision | `Corrupt` | Revision 2 |
| Revision 3, current graph | A record refers forward to its directory | `Corrupt` | Revision 2 |
| Revision 3, current graph | A reachable value has wrong magic, kind, version, identity, or reserved bytes | `Corrupt` | Revision 2 |
| Revision 3, current graph | A reachable value has a wrong checksum or nonzero padding | `Corrupt` | Revision 2 |
| Revision 4, retained graph | A value reachable only from the retained revision has invalid boolean bits | `Corrupt` | Revision 3 |

Each alternative in a row is a separate case paired with the clean
image. Structural edits have their object checksums repaired. The
checksum case changes a value's payload without repairing it. The
tests also verify that descriptor revision failures stop before any
DATA read, even when a later check would reject the same image.
The descriptor's end comparison has no feasible failing input after
the preceding checks. A valid slot bounds its descriptor to one region
and a successfully read object occupies at least one aligned region.
The stable-image value cases hold the two object header reads equal.
The ledger records the checks above separately from the remaining
decoder cache and traversal bounds.

### REC-V5. Verify the retained catalog

The [retained-catalog test](src/storage/codec.rs) extends the existing
corruption test using a publishing revision of 7 and current data
revision 4. The retained entries capture revisions 5 and 2, respectively.
The first entry points to data revision 4 and the second to data
revision 2. The persisted next retained ID is 3.

| Condition isolated against a valid catalog | Changed input | Reopen result |
|---|---|---|
| ID fails the ordering check | First ID is zero, or second ID repeats 1 | `Corrupt` |
| ID reaches the next retained ID | Second ID is 3 | `Corrupt` |
| Capture reaches the publishing revision | First capture is 7 | `Corrupt` |
| Data revision exceeds its capture | Second data revision is 3, above capture 2 and below current data 4 | `Corrupt` |
| Data revision exceeds current data | First data revision is 5, equal to capture 5 and above current data 4 | `Corrupt` |

For the last pair, the valid and invalid entries use the same
uncached reference to data revision 2. The valid entry declares 2 and
opens revision 7. The failing entry declares 5, and the trace confirms
recovery rejects the catalog before reading that reference. The
above-capture case also rejects before reading its retained reference.
The reused next-ID, forward-reference, and count cases return
`Corrupt`, `Corrupt`, and `Limit`, respectively. Clearing the newest
slot in each rejected image allows revision 6 to open.

All 46 library, durability, and retention tests passed in debug and
release on arm64 macOS with all features. The decision tests caught
24 independent changes to the audited checks in disposable copies.
Verification used the working tree based on `13d61c6` with Git blob
`13a58f2a49c76f0768a2d8182d1eb1d8f035fdf3` for `src/storage/codec.rs`.

### REC-V6. Validate metadata before choosing a root

The [metadata tests](src/storage/codec.rs#L945) use the existing v2
simulator fixture with roots 1 and 2. They distinguish metadata that
counts as absent from checksummed metadata that must reject the image.
Rejections stop at the relevant check and leave the durable bytes
unchanged. Each malformed field below is paired with a valid fixture.

| Decision | Condition varied | Result |
|---|---|---|
| Physical length | Below the reserved regions, or above the format ceiling | `Corrupt` before any read |
| Header format | Wrong magic or version | `UnsupportedFormat` |
| Header representation | Wrong checksum, reserved word, region size, append start, image limit, object limit, or padding | `Corrupt` before reading slots |
| Header identity | All-zero identity | `Corrupt` before reading slots |
| Slot or witness representation | Valid checksum with wrong magic, version, physical index, identity, or padding | `Corrupt` at that region |
| Slot or witness layout | Wrong revision parity or predecessor, end beyond the physical length, descriptor before append start, or inconsistent descriptor and end | `Corrupt` at that region |
| Coupled alignment controls | Unaligned end or descriptor | `Corrupt` at that region |

The physical-length test uses a scripted `ByteStore` for both invalid
lengths, so testing the upper bound needs no oversized allocation.
For slot layout pairs, changing parity also updates the predecessor.
Changing the physical bound or descriptor floor preserves the
one-region descriptor/end relation. The inconsistent-end case keeps
both offsets aligned. The slot cases clear the newest witness to
isolate malformed metadata from an independent witness rejection.

| Newest slot | Newest witness | Result |
|---|---|---|
| Valid | Zeroed or bad checksum | Revision 2 |
| Zeroed or bad checksum | Zeroed or bad checksum | Revision 1 |
| Zeroed or bad checksum | Valid at revision 2 | `Corrupt` |
| Checksummed but structurally invalid | Absent | `Corrupt` |
| Valid | Checksummed but structurally invalid | `Corrupt` |

The public open path rejects an oversized image before parsing any
metadata. Within the layout guard, an end above the format ceiling
therefore also fails the preceding physical-length check. The two
alignment conditions are coupled with the descriptor/end relation.
An unaligned offset either implies an unaligned other offset or
violates that relation. The tests exercise the rejection, and the
ledger records these three conditions as lacking independent
accept/reject pairs on this path.

The metadata and durability suites passed in debug and release on
arm64 macOS. The tests caught 24 deliberate changes in disposable
copies. The reviewed worktree was checkpointed at `dcd2402` before
this batch. The [fuzz workspace](fuzz/README.md) documents the new raw,
structured, and commit-crash targets, corpus replay, and sanitizer
smoke results.

### REC-V7. Traversal, aliases, and stable object reads

The graph decision tests in [codec.rs](src/storage/codec.rs) enter through
`Database::open_on`. Rejection cases clear the newest witness, assert
no writes or syncs, and check that clearing the newest slot lets the
previous revision open. This isolates graph rejection from witness
agreement and proves the older graph remains usable.

| Control and changed condition | Result |
|---|---|
| Valid revision 3 graph, value offset below append start or unaligned | `Corrupt`, no read of the invalid object |
| Valid backward reference, reference equal to its parent | `Corrupt` before reading the child |
| Valid object size, overflowing header/payload sum or alignment | `Corrupt` after the header read |
| Valid size, aligned size beyond the format cap or object crossing its parent | `Corrupt` after the header read |
| Value born at revision 3, born at revision 4 | `Corrupt` after the header read |
| Valid scalar payload, unknown tag, truncated scalar, or trailing scalar byte | `Corrupt` |
| Ordered record/map keys, duplicate first key | `Corrupt` |
| Same depth-3 graph opened with depth limits 3 and 2 | Revision 3 or `Corrupt` before reading the depth-3 leaf |
| Adjacent values, second value referencing a checksummed object embedded inside the first, in either decode order | Revision 2 or `Corrupt` after the embedded header |
| Shared cached value start, same reference plus one unaligned byte | Revision 2 or `Corrupt` without reading the invalid offset |
| Stable value header, changed born revision on the full read with repaired checksum | Revision 2 or `Corrupt` |
| Shared DATA offset and revision, same offset with a different declared revision | Revision 4 or `Corrupt`, no additional DATA read |
| Distinct DATA offsets and revisions, distinct offsets claiming one revision | Revision 4 or `Corrupt` before reading the second directory |
| Retained next collection ID at the current bound, above that bound | Revision 4 or `Corrupt` |

The unstable-read adapter changes the born revision to another valid
revision and repairs its checksum. This isolates header consistency
from checksum, padding, and born-revision validity. The existing
checksum and padding pairs cover the other terms in that decision.
The overlapping-object fixture first opens with the embedded bytes
unreferenced, then references those exact bytes as a second value. Both
decode orders reject. Recovery indexes each protected region as free,
occupied, or the aligned start of a verified cached value. Interior
regions and cached starts both prevent overlapping objects. Unaligned
references cannot use a rounded-down cache entry.

Several defensive branches need a disposition under the current
public-open invariants. Object count cannot reach `MAX_OBJECTS` after
nonoverlap, alignment, and the 64 MiB image bound are enforced.
Successful unique-object reads consume at most the image bytes plus
one 64-byte header per region, below the work limit of twice the image
cap. Object end arithmetic cannot overflow after offset and aligned
size have been bounded by that cap. On supported 64-bit targets,
converting the stored u64 length to usize cannot fail.

An already cached value crossing its new parent's start would overlap
that parent, which has already passed region occupancy verification. Cached DATA
always uses the selected descriptor as parent, so its previously
verified end cannot exceed that same bound on reuse. These end checks
lack feasible failing pairs on the stable public-open path. The
forward-value guard also has downstream object-bound checks.
Revisit these dispositions if reference direction, caching, alignment,
image bounds, or supported pointer widths change.

This batch records directed decision evidence. Deliberate guard mutation
checks were not run for REC-V7 or TX-V2 in this batch. The full
parser/count/resource inventory and compiler-measured coverage remain open.
The library, durability, retention, schedules, transactions, and
concurrency suites passed in debug and release with all features on
arm64 macOS using rustc 1.99.1. Concurrent allocator/FFI additions
changed the library test count between these runs, so this evidence
records the suites rather than a fixed total. The schedule harness
passed 10,000 AddressSanitizer executions with seed 1, alongside its
stable replay tests and warnings-denied Clippy check.

### MEM-V1. Independent configuration bounds

The public `Database::with_limits` test
`allocation_limits_reject_each_independent_invalid_configuration` uses
one valid boundary configuration and changes exactly one input in each
rejection case. All other conditions remain false.

| Condition changed | Valid boundary | Invalid value | Result |
|---|---|---|---|
| Depth is zero | 64 | 0 | `Invalid` |
| Depth exceeds maximum | 64 | 65 | `Invalid` |
| Entries exceed maximum | 1,000,000 | 1,000,001 | `Invalid` |
| Writes exceed maximum | 1,000,000 | 1,000,001 | `Invalid` |
| Observations exceed maximum | 1,000,000 | 1,000,001 | `Invalid` |
| Key/bytes size exceeds addressable slice | `isize::MAX` | `isize::MAX + 1` | `Invalid` |
| Value size exceeds addressable slice | `isize::MAX` | `isize::MAX + 1` | `Invalid` |
| Transaction size exceeds addressable slice | `isize::MAX` | `isize::MAX + 1` | `Invalid` |

A second accepted configuration uses depth 1 and zero for the other
limits. Configuration acceptance does not imply that every operation
fits the selected budgets. Each of the eight rejection cases also caught
its matching guard being replaced by `false` in a disposable source copy.

### MEM-V2. Allocation budgets, reservation and ownership

The allocator seam tests in `src/memory.rs` complement the existing
public allocation-failure sweeps in `tests/allocation.rs`.

| Decision | Paired observations |
|---|---|
| Reservation fits capacity | A vector containing one byte accepts three additional slots in capacity four with budget zero. A request for four fails with `OutOfMemory` and preserves the vector. |
| Reservation arithmetic overflows | The same nonempty vector rejects `usize::MAX` extra slots with `Limit` before claiming allocation. |
| Finite allocation budget | Budget one allows growth and leaves the next claim returning `OutOfMemory`. Unlimited mode permits repeated claims. |
| Native vector reservation fails | An empty `Vec<u64>` rejects `isize::MAX` elements with `OutOfMemory`, even with unlimited injected budget, and stays empty. |
| Zero-sized allocation/copy | Boxing `()` and copying an empty slice succeed with budget zero. Copying one byte fails. |
| Ownership on failed allocation | Failed shared, boxed and raw allocations each destroy their input exactly once. |
| Shared ownership | A clone consumes no allocation budget. Dropping one of two owners preserves the payload. Dropping the final owner destroys it once. |
| Raw/boxed ownership transfer | A successfully allocated box and a raw allocation consumed once by `free_raw` each destroy their payload once. |

The existing cross-thread test still checks final destruction after 8,000
accesses, while the existing four public sweeps cover nested edits,
transaction preparation, observations/overlays/scans, and collection
publication under injected allocation failure.

These cases establish manual decision evidence at these seams. They do
not exercise a real global allocator returning null, the refcount abort
threshold, or all atomic interleavings. The `copy` length guard above
`isize::MAX` cannot receive an independently valid Rust slice, so it is a
defensive bound under the slice contract. The raw allocator's nonzero
layout assertion is a caller precondition. Its intentional failure and
the refcount abort threshold remain candidates for subprocess tests.

All three memory tests and five allocation tests passed in debug and
release on arm64 macOS with all features and rustc 1.99.1. Eight deliberate
configuration guard mutations were caught. Source blobs were
`2e7e074c839cad80748b4f0a15cd888de7c883bf` for `src/memory.rs` and
`b24ed6e61209a1ff2cc3988870925102d5921ed6` for `tests/allocation.rs`.

### FFI-V1. Input, output and ownership boundaries

The FFI unit tests call the exported C functions with valid allocated
handles and buffers. The five new tests extend the existing allocation
rollback, concurrent acquisition/release and close-quiescence tests.

| Decision | Accepting control | Rejection or alternate outcome |
|---|---|---|
| Byte buffer is null AND capacity is nonzero | Non-null buffer at capacity 3, or null buffer at capacity 0 for empty bytes | Null buffer at capacity 3 returns `Invalid` |
| Byte capacity is below required length | Three-byte value with capacity 3 copies `abc` | Capacities 0 and 2 return `Buffer`, report 3, and leave destination untouched |
| Retained-ID buffer is null AND capacity is nonzero | Non-null buffer at capacity 1, or null buffer at capacity 0 with no retained IDs | Null buffer at capacity 1 returns `Invalid` and required count 0 |
| Retained-ID capacity is below required count | One retained ID and capacity 1 copies ID 1 | Capacity 0 returns `Buffer` and required count 1 |
| Required-size output is missing | Valid output receives size/count | Null output returns `Invalid` without changing the buffer |
| Byte input length is zero | Null pointer with length 0 creates empty bytes | Null pointer with length 1 returns `Invalid` |
| Input length exceeds byte limit | One valid input byte succeeds | Null pointer above the default 1 MiB limit returns `Limit` before forming a slice |
| Value kind, bits and length are supported | Bytes with zero bits, integer with zero byte length | Bytes with nonzero bits, scalar with nonzero length, out-of-range kinds and invalid boolean bits return `Invalid` |
| Snapshot owner matches database | Retaining through its owner succeeds | Retaining through another live database returns `ForeignStore`, resets ID to 0 |
| Foreign handle count can increment | Clone with one source handle succeeds | Synthetic `usize::MAX` count returns `Limit`, keeps count and source intact, nulls clone output |
| Scan index exists | Index 0 returns the sole key and a new value handle | Index 1 and `usize::MAX` return `Bounds`, zero required length, null value output and preserve key buffer |
| Transaction validation mode is supported | Modes 0 and 1 begin and abort | Modes -1 and 2 return `Invalid` and null the output |
| Transaction is active | Existing commit test succeeds after a rejected null output | Commit after abort returns `Closed` and zeros revision output |

For both buffer conjunctions, the empty collection/value cases isolate
each condition while keeping the result otherwise successful. The
nonempty cases separately pin the sizing-query and exact-capacity
behavior. Output-only sentinel pointers are overwritten before use and
are never dereferenced. Null release is tested for all four handle kinds.
The existing allocation test continues to verify rollback of the live
handle count, and the concurrency test verifies busy close followed by
successful close after the final release.

This is a scoped manual decision audit. Invalid non-null pointers,
misalignment, undersized backing allocations, output/input aliasing,
double release and concurrent close violate the C caller contract.
They are excluded from executable input pairs because exercising them
can invoke undefined behavior. The handle-count exhaustion test sets an
otherwise impractical counter state and restores its true count before
release. Native path validation, platform dispatch, panic containment,
every scalar output wrapper and complete exported-symbol coverage remain
separate assurance work. No compiler-measured MC/DC percentage is claimed.

All eight FFI tests passed with all features in debug and release on
arm64 macOS with rustc 1.99.1. Eight deliberate mutations in a disposable
copy were detected by test assertions. The mutations changed each buffer
conjunction to OR, each capacity comparison to less-than-or-equal, each
reported required size to zero, disabled the zero-length input shortcut,
and forced owner equality to fail. Guard-removal mutations that could
allow invalid pointer dereferences were excluded. The tested FFI source
blob is `7fed0fce570ed937d571311cab20474b5c9e8dc3`.

The combined library and allocation suites also passed with nightly
AddressSanitizer on aarch64-apple-darwin. The standard library was not
rebuilt with instrumentation. The all-feature, all-target workspace
Clippy check passed with warnings denied.

### IO-V1. Exact byte transfer progress

Two storage-core tests exercise the exact-transfer helpers with a scripted
ByteStore. For both reads and writes, adapter progress 0 and 3 on a two-byte
request return `Io`, while progress 1 completes in two calls. These pair
against each side of `n == 0 || n > size`. An explicit adapter error stops
after the first call. Empty buffers make no calls, even at `u64::MAX`.
A one-byte result at `u64::MAX` returns `Corrupt` for reads and `Limit` for
writes. Writes beginning at region offset 4095 split into requests at
4095, 4096 and 4097. Reads covering two regions issue two region-sized
requests. These exact helper tests complement the existing public recovery
short-I/O and commit fault sweeps.

### SIM-V1. Lease and persistence controls

Three new public ByteStore tests in `tests/bytestore.rs` verify unleased
I/O rejection, exclusive acquisition, clone-without-lease behavior, crash
invalidation, stale unlock exclusion and reacquisition. The existing
catalog identity test also checks old database leases against new writers.
A matching lease while the disk is unlocked is unreachable through these
public transitions, so this batch does not claim an independent pair for
each internal lease-conjunction condition.

The crash matrix writes 66 independent bytes. DropAll preserves zeros,
PersistAll preserves all 66 writes, and Mask with all bits set preserves
only the first 64 writes. Torn can select write 64, clamps a maximal prefix,
and preserves nothing for prefix zero or a missing write index. Two
writes to the same offset verify reverse survival order for Mask.
Boundary reads at the image cap, above-cap empty writes and overflowing
write offsets return the documented results. A short read fault is consumed
once. A failed sync preserves the prior durable bytes and a following
successful sync persists the complete volatile bytes.

### REPLACE-V1. Ordered namespace error propagation

The new replacement test records the exact visited step prefix for a
failure at each of Exchange, SyncDirectory, SyncReplacement, RemoveOld,
SyncCleanupDirectory and SyncCleanupFile. Each returns `Io` and visits no
later step. The successful control visits all six in order. The existing
namespace power-loss test independently checks the complete current and
retained state under each surviving namespace policy.

### FFI-V2. Path validation and panic containment

The native FFI path test isolates null pointer and zero length against
a valid one-byte path, rejects the 1 MiB-plus-one length before reading
memory, and rejects embedded NUL. Invalid open attempts reset the database
output. The valid conversion control calls the path converter directly,
while existing native file tests exercise actual opens and alias rules.

A new exported-commit test uses a ByteStore that panics after its append
sync. The C boundary reports `Panic` with revision output zero. The next
commit reports `RecoveryRequired`, also with zero output. Releasing the
transaction and closing the owner succeeds, and reopening selects revision
0. This complements the existing native Rust adapter-panic test. An
unwinding build is required for catch_unwind containment. Refcount abort,
abort-on-panic builds, arbitrary pointer validity and exhaustive native
syscall error mapping remain separate assurance obligations.

### Storage boundary results

On arm64 macOS with rustc 1.99.1, the 3 ByteStore, 14 durability and 8 native
file tests passed in debug and release with all features. The 2 transfer
and 2 replacement unit tests passed debug and release. All 10 FFI tests
passed debug and release. The native file tests reused process-kill,
symlink/hard-link exclusion, replaced-inode, renamed-parent and nonregular
path coverage. This run provides native macOS evidence. Linux and Windows
need their own native runs for this revision.

Seven safe mutations were caught in a disposable copy. They tightened
zero-progress and oversized-progress comparisons independently for reads
and writes, ignored replacement errors, allowed mask index 64, and changed
mask application to forward order.
This batch adds executable decision evidence, without claiming exhaustive
MC/DC for all platform adapters or simulator allocation/trace exhaustion.

### MEM-V3. Refcount threshold, modeled ordering and pointer checks

The new subprocess test in src/memory.rs fabricates only the count field
of a valid Shared allocation in a child. At isize::MAX minus one, clone
succeeds and reaches isize::MAX. The test restores the true count of two
before any assertions or destruction. At isize::MAX, clone must abort.
The native macOS parent verifies SIGABRT specifically, so an ordinary
assertion failure cannot satisfy the expected abort. The subprocess test
passed debug and release. Non-Unix builds check abnormal termination
excluding Rust's ordinary test-failure code and still need native evidence.
The subprocess is excluded under Miri.

The isolated tools/atomic-model workspace pins Loom 0.7.2. It models the
production Relaxed increment and AcqRel decrement using three threads,
one clone per worker, one payload write per worker and two-preemption
schedules. The last owner must observe both writes before exactly one
destruction. The final counter is zero. The model passed with a branch
bound of 1,000 and no permutation or duration cutoff. A disposable mutant
using Relaxed decrement failed its visibility assertion. This is a bounded
algorithm model. Native tests and Miri cover raw allocation lifetimes
separately, and production/model drift requires review.

Miri was installed for the available nightly and successfully ran
memory::tests::allocation_ownership_transfers_or_drops_exactly_once and
ffi::tests::byte_buffers_distinguish_null_capacity_and_required_length.
These exercise failed and successful ownership transfer, shared last drop,
Box/raw allocation release and caller-provided byte buffer writes.

A thread-local test seam now replaces only the next raw allocator result
with null, after the ordinary allocation budget check. The shared, boxed
and raw callers return OutOfMemory, destroy each input once and permit a
subsequent allocation. This tests the null-result branches without exhausting
the host or changing allocations in other threads. Actual global allocator
exhaustion remains subject to the allocator/platform contract. Arbitrary
invalid caller pointers remain outside the C contract.

### Replaying exact guard mutations

The new tools/mutations.py snapshots the current tracked and untracked
nonignored files into an isolated directory and records hashes. It runs
each selected baseline first and rejects failed or zero-test baselines.
Each mutation changes a production expression, reruns the matching tests,
restores the expression and saves logs, command and result in evidence.json.
Compile errors do not count as detected mutations. Source-expression drift
fails the runner. The --list and repeated --case prefix options allow
bounded selection. It never mutates the original checkout.

All 25 configured cases were caught in two runs. These cover eight limits,
capacity and refcount abort threshold, eight FFI buffer/input/owner checks,
four exact transfer progress checks, replacement error propagation and two
simulator mask cases. The script, atomic model and lockfile are persistent
replay artifacts. Python compilation, model Clippy and whitespace checks
passed.

### VAL-V1. Weight and child-depth limits before editing

`value_edit_weight_and_child_depth_have_independent_boundaries` covers
four two-condition decisions in `map_set`, `list_set`, `list_push` and
middle `list_insert`. A is resulting weight above the configured maximum.
B is child depth at least the configured maximum. Both maxima are held
fixed at weight 80 and depth 3. The accepted child has depth 2, so the
result reaches depth 3 exactly.

| Pair input | A | B | Outcome |
|---|---|---|---|
| Result weight 80, child depth 2 | False | False | Accepted |
| Result weight 81, child depth 2 | True | False | `Limit` |
| Result weight 80, child depth 3 | False | True | `Limit` |

Each operation uses all three rows. Map replacement uses an existing
one-byte key. List replacement edits a one-element list. Push and middle
insert grow that list to two elements. Byte payload lengths compensate
for container overhead so the conditions vary independently. The test
checks the selected new child and preserves the old root and original
scalar. With fault injection enabled, failing rows have allocation budget
zero. `Limit` proves rejection before path allocation, distinguishing
these guards from the later constructor that would otherwise reject the
same depth or weight after allocations.

Twelve mutations independently remove each weight/depth term or change
the weight comparison from `>` to `>=` at each of the four sites. All
were caught by the existing `values` suite with the new cases. The scoped
denominator is four compound decisions and eight constituent conditions.
This is manual decision-pair evidence and does not claim full value-module
MC/DC or compiler-measured branch coverage.

### TREE-V1. Capacity, hash and key controls

`map_capacity_and_collision_lookup_have_independent_controls` uses an
entry limit of one. An absent key inserts into the empty map, another
absent key at capacity returns `Limit`, and replacing the existing key at
capacity succeeds while preserving length. These three cases isolate the
two terms of `!exists && len >= max_entries`.

The same stored leaf then answers three point lookups. Matching hash and
key return its value. The same hash with a different key returns absence.
The same key with a different injected hash returns absence. Removing
the only entry removes the root, while the original root remains usable.
The existing full-hash-collision/long-prefix test and radix growth/shrink
and suffix-edit tests were reused. The existing path-allocation sweep and
sharing checks also reran.

Four mutations remove each capacity term or bypass the lookup hash or
key comparison. All four were caught. The additional scoped denominator
is one two-condition capacity decision and two single-condition lookup
checks. These checks do not constitute a complete tree branch inventory.

Remaining tree decision work includes explicit independent pairs for
branch collapse (one remaining child versus leaf kind), empty/root/child
traversal transitions, callback-error propagation, collision insert versus
replace and singleton versus multi-entry removal, exact radix capacity
and shrink transitions, and materialization arithmetic and output-budget
branches. Existing tests exercise many outcomes, but they still need a
source-bound independent-pair mapping or reviewed infeasibility proof.
The height/power overflow path also needs an explicit proof against the
configured entry cap. Value kind/store/index checks, constructor scalar
validity, arithmetic overflow and aggregate weight calculations remain
outside VAL-V1's denominator.

Seven value tests and three tree tests passed in debug and release on
arm64 macOS with all features. Warnings-denied Clippy passed. All sixteen
mutations are reproducible with

    python3 tools/mutations.py --case value- --case tree-

Run evidence is in
`/var/folders/m4/r5mf72hn7kx9c5lq4050dzfh0000gn/T/weekend-mutations-vm6a8cx1/evidence.json`.
Source Git blobs were `c8b4e9ae8e06aa5ced9cda3110f8608be0e9c37d`
for `src/value.rs`, `037dc465da551a110b44f0902c6ce10ae3a32261`
for `src/tree.rs`, `ff8ea169c73df1b90bac2bc008a6d3183dfcb23e`
for `tests/values.rs`, and `4e3b41c037377dd18b66f039da95d84abc583d56`
for `tools/mutations.py`.

### FOL-V1. Durable installation and progress

The [follower fault test](src/replication/follower/tests.rs#L174) starts
with primary revision 2 installed and offers revision 3. It checks the
terminal wire reply, then drops the receiver, crashes the simulated
stores, reopens, and polls for progress. The reported acknowledgment and
the reopened replica's revision and row value must agree.

| Case | Installation or failure point | Sender sees | Revision reported after reopening |
|---|---|---|---|
| F1 | Staging sync, verification, and manifest commit succeed | Durable acknowledgment for 3 | 3 |
| F2 | Staging sync fails, with staged bytes lost or surviving | `Io` | 2 |
| F3 | Staging sync succeeds and image verification fails | `Corrupt` | 2 |
| F4 | Manifest append sync fails, with append lost or surviving | `Io` | 2 |
| F5 | Manifest root sync fails and the pending root is lost | `RecoveryRequired` | 2 |
| F6 | Manifest root sync fails and the pending root survives | `RecoveryRequired` | 3 |
| F7 | Manifest witness sync fails after its root sync succeeded | `RecoveryRequired` | 3 |
| F8 | Manifest commit succeeds and the reply is lost | Transport `Io` | 3 |

Pair F1 with F2, F3, or a manifest failure in F4–F7 to test the staging
sync, image verification, and manifest commit gates in order. F5 and F6
hold the sender's error fixed and vary only whether the manifest root
survives. Before reopening, a staging sync failure still allows a poll
to report revision 2. A manifest fault makes the same poll return
`RecoveryRequired`. A retry after reopen uploads the image again if
revision 2 survived, or receives revision 3 immediately if it was
already selected.

The existing [native interruption test](tests/replication.rs#L191)
kills a follower during upload and again after durable selection but
before the reply. It reports the previous revision after the first
restart and the offered revision after the second. If the selected
image is missing or damaged, reopen returns `Corrupt` and reports no
revision even though the old image remains.

### FOL-V2. Image verification

The [verification test](src/replication/follower/tests.rs#L296)
starts with the same installed revision 2 for each case. It holds all
other checks passing while failing one check. The four conditions of the
[replica rejection guard](src/storage/mod.rs#L344) are an identity
mismatch, a revision mismatch, a committed end different from the
declared length, and the absence of a matching witness.

| Case | Input | Terminal reply | Revision reported after reopening |
|---|---|---|---|
| V1 | Valid image of revision 3 | Durable acknowledgment for 3 | 3 |
| V2 | Wrong transfer digest | `Corrupt` | 2 |
| V3 | Valid image with a different file identity and matching transfer digest | `Corrupt` | 2 |
| V4 | Valid image of revision 3 offered as revision 4 | `Corrupt` | 2 |
| V5 | Valid image with a trailing region included in its declared length and digest | `Corrupt` | 2 |
| V6 | Valid root with a damaged witness and matching transfer digest | `Corrupt` | 2 |

V1 and V2 exercise the transfer digest gate. V1 paired with each of V3
through V6 demonstrates the independent effect of the corresponding
condition in the four-condition rejection guard. In each rejected case
the candidate is fully synced and the manifest sees no I/O.

### FOL-V3. Older and duplicate offers

The [lost-reply test](src/replication/follower/tests.rs#L391) reopens at
revision 3 after F8. It exercises the condition that returns the installed
acknowledgment when an offer is older or exactly matches the installed
offer.

| Case | Offer | Older | Exact match | Reply | Revision reported after reopening again |
|---|---|---|---|---|---|
| D1 | Earlier revision 2 | True | False | Durable acknowledgment for 3 | 3 |
| D2 | Identical revision 3 | False | True | Durable acknowledgment for 3 | 3 |
| D3 | Revision 3 with a different digest | False | False | `Conflict` | 3 |

D1 and D3 isolate the older condition. D2 and D3 isolate the exact-match
condition. These requests read the offer header only and leave the
manifest bytes intact. The acknowledgment checks here cover the follower's
selection and reporting decisions. The primary's handling of replies
is a separate decision.

All 37 selected library, durability, and replication tests passed in
debug and release on arm64 macOS. The seven simulator and native
replication tests also passed in both modes on aarch64 Linux. In
disposable copies, the tests caught independent changes to all four
replica metadata conditions, the digest check, the staging sync and
manifest selection gates, the recovery-required progress gate, and both
offer shortcut conditions.

The verification used the working tree based on `13d61c6`. The Git blobs
were `b06eaef7a2cd467d94b7f3cdef1f81a39a354005` for
`src/replication/follower.rs`,
`636b01b0e0292f480dd2c0554decd8adba49f674` for its tests, and
`000d289111767aa499624895de1ddbf2788ab14e` for `src/storage/mod.rs`.

### PRI-V1. Accepting follower progress

The [poll test](src/replication/tests.rs#L67) creates verified exports of
revisions 0, 1, and 2. The primary's verified head is revision 2. Except
for Q7, the test records the acknowledgment for revision 1 before
delivering the reply under test.

| Case | Reply to a progress query | Primary result | Recorded revision |
|---|---|---|---|
| Q1 | Revision 2, correct stream | Accept | 2 |
| Q2 | Exact acknowledgment for revision 1 | Accept | 1 |
| Q3 | Revision 2, wrong stream | `ForeignStore` | 1 |
| Q4 | Revision 3, correct stream | `ForeignStore` | 1 |
| Q5 | Revision 0, correct stream | `Conflict` | 1 |
| Q6 | Revision 1, different digest | `Conflict` | 1 |
| Q7 | Empty, before the primary observes any progress | `Ok(None)` | None |
| Q8 | Empty, after the primary observes revision 1 | `Conflict` | 1 |

Q1 and Q3 isolate the stream mismatch. Q1 and Q4 isolate whether the
reply exceeds the verified head. After those checks, the primary rejects
an older revision or an equal revision with a different acknowledgment.
Q1 and Q5 isolate the older condition. Q1 and Q6 isolate the equal
revision condition while the acknowledgment differs. Q2 and Q6 isolate
acknowledgment equality at the same revision. Q7 and Q8 isolate the
presence of prior progress for an empty reply.

After each rejection, another poll confirms that revision 0 still
conflicts and that the exact acknowledgment for revision 1 is still
accepted. After accepting revision 2, revision 1 conflicts. This
progress is held by the primary instance. A restarted primary polls
the follower to learn its recovered durable progress.

### PRI-V2. Acknowledgment before and after upload

The [ship test](src/replication/tests.rs#L123) offers revision 1 while
the primary has recorded revision 0 and verified through revision 2.
It delivers each reply directly or after `Ready`.

| Case | Acknowledgment | Direct reply | Reply after upload |
|---|---|---|---|
| T1 | Exact acknowledgment for offered revision 1 | Accept revision 1 | Accept revision 1 |
| T2 | Newer verified revision 2, correct stream | Accept revision 2 | `Conflict` |
| T3 | Older revision 0 | `Conflict` | `Conflict` |
| T4 | Revision 1 with a different digest | `Conflict` | `Conflict` |
| T5 | Revision 1 with a different stream | `Conflict` | `Conflict` |

For the direct reply guard, T2 and T4 isolate the newer revision
condition, and T1 and T4 isolate exact acknowledgment equality. After
`Ready`, T1 paired with any rejection shows that the reply must match
the offered stream, revision, and digest. In particular, T2 flips from
acceptance to conflict when it follows an upload. A direct reply leaves
only the offer header on the outgoing channel. `Ready` triggers the
image upload.
Follow-up polls verify that acceptance records the acknowledged
revision and that rejection preserves revision 0.

All nine replication tests passed in debug and release on arm64 macOS.
The two primary tests also detected nine independent changes to the
audited checks in disposable copies. The verification used the working
tree based on `13d61c6`. The Git blobs were
`02feaeaea9d7b196fc4f741a642ba75eb3516356` for `src/replication/mod.rs`
and `9993751adc493658e80ca458ce414326638ae6a4` for its tests.

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

The independent comparison points to checkpoint-oriented embedded application
state as the strongest fit. An application can retain a version, inspect its
records, schema and indexes alongside the current head, and reopen or compact
the database with that version still available. The task tracker exercises
this through explicit migration and table-level undo and redo. The value of
the combination depends on how often a real consumer needs that workflow.
[Typed store](crates/weekend-db-typed/src/store.rs), [task tracker](apps/weekend-tasks/README.md)

The v2 encoder can reuse offsets for unchanged child values. It serializes
the index of each changed container and the current data directory on a data
revision. The in-memory sharing model therefore leaves the disk cost of a
leaf edit dependent on container and directory size. The existing measurements
are same-engine baselines. A relative write-amplification claim needs a matched
workload. [v2 encoder](src/storage/codec.rs), [baseline results](#milestone-10-compaction-and-measurements)

## Closest alternatives

| Alternative | Existing overlap | Consequence for positioning |
| --- | --- | --- |
| xitdb | Embedded, nested persistent collections, file-stored history, cheap undo | Closest abstraction; C packaging and transaction contract must earn their keep |
| Vedis | Portable embedded C, structured commands, transactions, amalgamation | C plus hashes/lists is already available |
| UnQLite | Portable C amalgamation, transactional KV and document storage | Differentiate persistent value graphs from encoded documents |
| LMDB | C, copy-on-write, concurrent snapshot readers | A live MVCC view is not an explicit durable history catalog |
| redb | Embedded Rust, persistent rollback savepoints | Durable checkpoints already exist; distinguish historical reading from restoration |
| SQLite | SQL, indexes and JSON, WAL snapshots, remote copying | Compare against application-managed retained versions with equivalent durability |

### xitdb: the decisive comparison

xitdb already exposes arbitrarily nested maps/lists, structural sharing, memory/file backends and incremental access. Its documented pattern stores successive “moments” in a top-level list; undo appends a reference to an earlier moment. This is stored history, not only live-reader isolation. Writes are serialized; readers can run concurrently. [Official README](https://github.com/xit-vcs/xitdb#readme)

The inspected local implementation loads its root from disk, syncs appended data before publishing the root, and defaults file synchronization on. Its compactor treats a root list as history and copies only its latest moment into a new history. These source facts support normal reopen persistence and explain history loss under that compaction policy; they are not an independent crash-safety certification. [xitdb source](https://github.com/xit-vcs/xitdb/blob/master/src/lib.zig)

The first-party organization also has Java, TypeScript and Go implementations.
The relevant comparison is how each project exposes retention, concurrent
edits and historical reads to the application. [Java](https://github.com/xit-vcs/xitdb-java), [TypeScript](https://github.com/xit-vcs/xitdb-ts), [Go](https://github.com/xit-vcs/xitdb-go)

The xitdb documentation shows how to retain selected history inside the latest
moment so it remains reachable through compaction. That makes xitdb the closest
comparison for implementing the same user-visible retention policy. In
weekend-db the engine owns the retained catalog, and applications address
historical views by stable snapshot IDs. [xitdb compaction](https://github.com/xit-vcs/xitdb#compaction)

### Vedis: packaging and structured storage already overlap

Vedis is an embedded C datastore with disk and memory modes and a cross-platform file format. Its command surface includes hashes, sets, lists, and BEGIN/COMMIT/ROLLBACK. It ships as `vedis.c` plus a header. [Overview](https://vedis.symisc.net/), [commands](https://vedis.symisc.net/commands.html), [amalgamation source](https://github.com/symisc/vedis/blob/master/vedis.c)

Its hash command implementation stores entry data in byte blobs and returns strings; transaction rollback invokes the pager's current-transaction rollback. The reviewed API does not expose named retained historical roots or xitdb-style arbitrarily nested immutable collection handles. That is a bounded finding about this interface, not proof history could never be built above it. [Source: `vedis_cmd_hmget`, `vedis_rollback`](https://github.com/symisc/vedis/blob/master/vedis.c)

### UnQLite: nested documents differ from persistent subtrees

UnQLite already provides in-process transactional C storage, a portable file, an amalgamation, raw binary KV and a Jx9 document layer. [Official repository](https://github.com/symisc/unqlite)

The distinction is visible in code: `CollectionStore` and `CollectionUpdate` call `FastJsonEncode`, then replace a KV record; fetching reads the record and calls `FastJsonDecode`. Nested JSON therefore does not itself supply a public persistent map/list graph whose unchanged subtrees are shared across application versions. The reviewed public transaction interface is begin/commit/rollback, not a retained-snapshot catalog. [Document storage implementation](https://github.com/symisc/unqlite/blob/master/unqlite.c), [public header](https://github.com/symisc/unqlite/blob/master/unqlite.h)

### LMDB and redb: be precise about snapshots

LMDB offers C transactions, copy-on-write pages, concurrent readers and one active writer. Its API documents that long-lived readers prevent page reuse and that freed pages are otherwise recycled. Its normal read transactions pin views during their lifetime; the reviewed API does not provide an application catalog of arbitrary retained roots to reopen later. Separate database copies can preserve versions, with different costs and management. [Official API source](https://github.com/LMDB/lmdb/blob/mdb.master/libraries/liblmdb/lmdb.h)

redb explicitly has `persistent_savepoint`, deletion/listing and `restore_savepoint`; these require appropriate durability and retain otherwise reusable pages. Persistent savepoints survive reopening, as its tests demonstrate. However, `Savepoint` is a restoration token, with no direct historical table-reading API exposed on it. Restoration happens in a write transaction and invalidates later savepoints. This overlaps checkpoint/rollback use cases, without being the same historical-read interface. [WriteTransaction](https://docs.rs/redb/latest/redb/struct.WriteTransaction.html), [Savepoint](https://docs.rs/redb/latest/redb/struct.Savepoint.html), [tests: `persistent_savepoint`](https://github.com/cberner/redb/blob/master/tests/integration_tests.rs)

### SQLite and replication

SQLite is a baseline for SQL constraints, expression indexes and JSON. Its
optional WAL snapshot interface can reopen a historical view until
checkpointing overwrites the needed state. To match a durable retained
catalog, a comparison workload should explicitly store versions or keep
consistent backup copies. [Expression indexes](https://www.sqlite.org/expridx.html), [JSON](https://www.sqlite.org/json1.html), [WAL snapshots](https://www.sqlite.org/c3ref/snapshot_open.html), [Backup](https://www.sqlite.org/backup.html)

Replication already has strong baselines. xitdb can stream incremental
committed bytes and documents staging a full copy to preserve the previous
replica on interruption. SQLite's `sqlite3_rsync` sends changed pages for a
consistent snapshot over SSH while the source can keep accepting writes.
Litestream asynchronously ships WAL changes and supports restore to available
points in its retained log. weekend-db ships the current head together with
the explicitly retained roots, then acknowledges the selected image separately
from the local commit. [xitdb replication](https://github.com/xit-vcs/xitdb#replication), [SQLite remote copy](https://www.sqlite.org/rsync.html), [Litestream replication](https://litestream.io/how-it-works/)

The current prototype fits occasional checkpoints. It has an exclusive writer
process, a 64 MiB image limit, a quiescent export, and full-image network
transfers. Each successful follower installation appends three 4 KiB objects
to its manifest, so a fresh manifest can hold roughly 5,450 installations
before reaching its image limit, excluding abandoned tails. This is an
estimate from the v2 layout. Sustained follower use needs a maintenance policy
for that manifest. [Follower manifest](#snapshot-replication), [v2 encoder](src/storage/codec.rs)

## What the current design can honestly serve

**Recommendations, not evidence of demand:** start with an application that owns one writer process and needs historical inspection of structured state:

- A desktop configuration/workspace store: inspect the settings and metadata associated with named checkpoints after reopening.
- A local build or analysis tool: retain selected dependency graphs or analysis results and read an earlier run without loading complete serialized graphs.
- A game/tooling state inspector: checkpoint stable entities and compare versions while ordinary work continues.

These examples fit keyed records and selective checkpoints. They do not require SQL, replication or collaborative text merging. They still need retention budgets, useful inspection and a concrete consumer.

Do not advertise automatic undo, branching or an editor backend yet. Transactions currently begin at the current head; retained snapshots are readable, but arbitrary historical writes, old-root restoration and a branching API are absent. An application could deliberately create compensating assignments, but that requires its own restoration semantics. Middle list insertion/removal may cost O(n), and two edits to different fields or list positions inside one record conflict. Even unrelated new inserts can conflict through collection-level absence tracking. These are material constraints for large editable documents. The relevant contracts are specified above.

Record-level OCC is conventional concurrency control, not a fundamental invention: private work followed by read/write validation and publication has long precedent. **Inference:** its value here is a clear conflict contract and avoiding unnecessary retries for independent records; measure whether that matters before making it the headline. [Kung and Robinson](https://www.eecs.harvard.edu/~htk/publication/1981-tods-kung-robinson.pdf)

## Validation before expanding scope

Use one checkpoint-heavy application workload to compare weekend-db with
xitdb's explicit history pattern and SQLite or redb with application-managed
history. Match retained versions, durability and crash expectations. Measure
integration effort, allocations, bytes per leaf edit, retained-history growth,
compaction and reopen time, durable commit latency, and conflict/retry rates.
For the replication case, also measure export pause and follower lag.

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
- Snapshot replication starts with a fixed primary, one follower, local commit
  acknowledgment and separate durable follower progress. The optional CRDT
  experiment binds an explicit resolver to versioned collection metadata.
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

## Production operations and typed index repair

### Typed index verification and atomic rebuilding

The typed runtime now exposes `Read::check_indexes`,
`Write::check_indexes`, and `Write::rebuild_indexes` for a registered
table. `IndexReport` counts primary rows, expected and stored index
entries, missing keys, orphaned keys, and mismatched record owners.
`is_consistent()` requires all discrepancy counts to be zero.

Expected indexes are derived from validated stored record envelopes
using the same physical-field key extraction as typed writes. Missing
fields and explicit nulls remain unindexed even with schema defaults.
The verifier validates each record against its own stored schema
version and rejects duplicate physical unique values with
`UniqueViolation(field_id)`. Sorting the expected keys detects duplicate
claims before any repair writes.

Rebuilding stages only missing/mismatched entries and orphan deletions
in the enclosing write transaction. It returns the pre-repair report.
Primary records are not rewritten, preserving unknown fields and their
exact envelopes. Both the primary collection and shared index collection
are scan dependencies, so an intervening write to either conflicts.
Shared-index scans also conservatively conflict with index changes to
other tables. Errors abort the complete typed closure even if caught.

Five new tests cover missing/orphan/invalid-owner entries, preservation
of another table and unknown fields, retained corrupt snapshots, durable
reopening, missing fields with defaults, duplicate claims, interleaved
primary/index writes, caught errors, allocation failure at every core
allocation before successful repair, and interruption at every copy of
the repair commit's I/O trace. The fault test drops or persists pending
writes and verifies that reopened indexes exactly match the whole old
or repaired state at the recovered revision. Retained indexes remain
at their captured state throughout.

All 22 typed integration tests passed in debug and release with all
features on arm64 macOS and rustc 1.99.1. Five active doctests passed in
both modes, including the new API example and three compile-fail cases.
One pre-existing illustrative doctest remains ignored. Warnings-denied
Clippy passed for the crate's all-feature/all-target configuration.

Scope remains one table and one bounded transaction. Keys with that
table's complete ID prefix are repaired, including malformed suffixes.
Unattributable malformed global index keys need separate catalog
inspection. Unsupported record/schema corruption requires an explicit
application repair. Existing scan, collection, observation, and write
limits apply. A consistent rebuild still performs the enclosing write
closure's ordinary commit. Incremental reindexing and index-layout
migration remain future work.

Read-only inspection and verified copying are described in
[operations.md](docs/operations.md). Five simulator and native process tests
passed in debug and release on arm64 macOS, including source/destination
I/O failures, existing destination preservation and retained rows after
restoration. Linux/Windows builds were cross-checked.

## Coordinator request receipts and authentication

Coordinator durable request receipts implemented in apps/write-coordinator.

Backward-compatible `get` and `set` stay available. New `set-id ID EXPECTED
VALUE` writes the counter and a versioned receipt together in one core
transaction with whole-database validation. Duplicate ID plus identical
expected revision/value returns the original `committed N` without a new
commit, even after later writes and process restart. Different payload
for an accepted ID returns `Invalid`. Stale preconditions return conflict
without reserving an ID.

Receipt keys are reserved `receipt/ID` entries in collection 1, so existing
files need no catalog migration or incidental revision change. Each
receipt contains version byte 1 and three little-endian u64 fields for
expected revision, replacement value and committed revision. Decoder
rejects malformed size/version, inconsistent acknowledgment or a future
acknowledgment. Store layout is owned by the coordinator application.

There is a fixed limit of 1,024 successful identified requests, with no
expiry, eviction or silent reuse. New IDs at capacity return Limit while
old receipts remain replayable. IDs are 1–32 ASCII letters/digits/_/-.
The authenticated wire line cap is 256 bytes, including credential and command.
README documents uncertain outcome/reopen/retry and restoration of older
receipt history. Restoring an old backup requires external reconciliation
for request IDs whose receipts were created after that backup.

Two unit tests and two native process tests pass debug and release on
arm64 macOS. Tests include full receipt capacity, stale and changed
payload handling, malformed receipts, process-kill/reopen after a request
whose reply was never read, replay after later commits, distinct IDs
racing on one revision, and divergent payloads racing on one ID. Existing
legacy process test still passes. Adding parallel process tests exposed
an existing timestamp-only temporary-directory collision. An atomic
sequence suffix fixes that test infrastructure collision.

Warnings-denied Clippy passes. All-target cross-checks passed for x86_64
Windows MSVC and x86_64 Linux GNU. Native Windows/Linux execution remains
a separate qualification gate. Service remains localhost only. Receipt rotation,
operator tooling at capacity, external restore/reconciliation identity,
and durable receipts for rejected requests are separate design choices.

Configured localhost authentication

`serve FILE PORT --secret-file SECRET` and `client PORT --secret-file SECRET`
load a fixed 64-hex-character bearer credential from a bounded regular
file read. Unix files with group/other permission bits reject. Windows
ACL provisioning is documented and left to the operator. The server
validates `auth TOKEN COMMAND` before handling any database command or
receipt lookup. Unauthorized requests receive a uniform Unauthorized
reply. Existing no-secret serving is explicitly labeled demo mode on
stderr. Binding remains localhost only. Secret contents are excluded
from process arguments, replies, and error diagnostics.

Network reads now have a total three-second deadline as well as a
256-byte line bound. Reading one byte at a time prevents individual
short reads from resetting the total deadline. Write timeout remains
three seconds. The authenticated process test covers missing and wrong
credentials rejected before writes, correct credentials, receipt replay,
restart, oversized input disconnect, and incomplete input disconnect on
timeout. The next authenticated request succeeds with unchanged state.

Final evidence is 2 unit tests and 3 native process tests passing in both
debug and release on arm64 macOS, warnings-denied Clippy, and Windows/Linux
all-target cross-compilation. Secrets are loaded once at startup, with
rotation by restart. Remote TLS, per-client authorization, rate limits,
Windows ACL validation, and live credential revocation remain outside
this localhost shared-secret contract.

## Logical export and import

Logical format 1 export/import implemented in src/storage/logical.rs,
reexported from storage, and available through weekend-admin export/import.
Exact wire specification and validation contract are docs/logical-format.md.
The format carries current/retained capture revisions, data revisions,
collection IDs/content revisions, record keys/write revisions, next IDs,
identity and every value inline, with CRC64 and no physical offsets.
Data directories are unique by data revision, preserving alias semantics
while nested value storage sharing is removed.

The parser is bounded by 64MiB, 16,384 inline value nodes, existing Limits,
and <=64 retained roots/65 directories. CRC/version/shape/invariants are
checked before file creation. Physical encoding is preflighted, so valid
logical data exceeding physical capacity rejects without a destination.
Exports also verify this expanded representability before returning.
Highly shared sources may therefore return Limit. Installation reuses
Storage::create and verify, preserves lineage identity and all counters,
and publishes with existing durability barriers. Future logical versions
need explicit converters. No schema migration or salvage is inferred.

Four tests pass debug/release on arm64 macOS. They cover all value tags
(including NaN bits), nested shared input, retained gaps, exact replay,
continued IDs, every fixture truncation, CRC-repaired bad metadata/counts,
unsupported version, trailing bytes, depth limit, no destination touches
on malformed inputs, native CLI subprocess export/import, no overwrite,
source bytes unchanged and no creation from invalid input. A checked-in
logical-v1.bin compatibility fixture replays exact bytes and checks current
value 21 versus retained value 11. Preserve it for future compatibility.
Clippy passed before fixture test addition. Windows/Linux tests+CLI cross
checks passed before fixture test addition. Native platform qualification,
long-running parser fuzzing, logical version conversion and streaming
remain explicit future work.

## Follower maintenance and bounded manifests

Implemented passive follower lifecycle additions in src/replication/follower.rs
and reexports in src/replication/mod.rs. Public FollowerStatus diagnostics include
enrollment, durable ack, recovery/maintenance state, selected slot, physical
manifest length and admission ceiling. The native headroom gate runs before
Ready or inactive image creation. The 1 MiB cap reserves four regions for the
three-region manifest selection append. Queries and duplicates remain usable
at the cap. Existing larger manifests can open for maintenance.

cleanup_inactive verifies the selected image again before deleting only reserved
inactive regular-file entries, or both staging entries when nothing is selected.
It returns removal count and is retry-safe. It does not claim durable cleanup
namespace persistence. Missing paths are harmless, nonregular paths fail and
unrelated names remain untouched. Selected-image corruption prevents cleanup.

macOS/Linux compact_manifest consumes the handle, closes its lease, uses the
existing verified atomic replacement and reopens through enrollment/image
verification. The lease handoff permits a competing opener to cause Busy.
All errors require a fresh open. Windows maintenance uses documented
fresh-directory reseeding. Existing create+ship APIs provide explicit reseed
without alias APIs, enrollment reset or deletion of the prior directory.

Seven native replication tests passed debug/release on arm64 macOS. Four
follower simulator fault tests passed debug/release. New cases cover exact
reserved-headroom acceptance, cap rejection before staging, duplicates at cap,
manifest compaction and subsequent installation, idempotent staging cleanup,
selected damage, preserved unrelated entries and fresh-directory reseeding.
A compaction fault test sweeps every native compaction hook and verifies exact
reopened durable ack plus duplicate retry. Existing process-kill and publication
fault suites are reused. All-target all-feature core Clippy passed before the
last test-only boundary addition.

User-facing operational documentation is docs/replication-operations.md.
Transport authentication/deadline/backoff configuration remains application-owned.
Native Linux/Windows qualification was pending at this stage; subsequent
checkpoint 4203e0a passed those native CI suites.

## Distributed fuzzing and codec enforcement

CRDT assurance completed for fixed two-peer actor identity, backup,
reserved-row and policy-evolution contract. docs/crdt-operations.md records
exact supported behavior, app ownership obligations and migration limits.

Concrete validation gap fixed in State::encode: an application Record
codec could encode a different payload than the provided state while
returning success, then publish it despite the stated roundtrip contract.
Encoding now decodes its output and compares with original state before
publication or writing an outgoing frame. New integration test uses a
lossy codec and confirms Invalid, no revision advance and unchanged
reopen state.

New fuzz targets crdt_frames and crdt_schedule, with seven deterministic
seeds generated by examples/crdt_seeds.rs. Frames target exercises both
sender acknowledgment validation and receiver merge on counter frames,
using arbitrary bytes or structural patches to a valid frame. Schedule
target uses a derived counter/set record on two simulated durable stores.
Independent integer components and membership bits predict all states
through local offline edits, send, saved/delayed offers, reply loss and
restart. It checks full per-actor components, convergence and no extra
commits after repeat sync.

11 integration tests pass debug/release arm64 macOS. Fuzz replay and
warnings-denied Clippy pass. Final ASan smoke 5,000 executions per target,
seed1 max_len128, no findings. Logs /tmp/weekend-crdt-final.xWdsGO.
Windows/Linux x86_64 all-feature tests cross-compile. Removing the new codec guard in a disposable copy made its regression
test fail as expected.

Limits remain explicit. Raw writes can forge otherwise valid reserved
rows, so applications must restrict them. Stale rollback is detectable
only when remembered state exposes it and cannot safely resume as the
same actor. Fresh enrollment/migration requires application design.
No new membership, gossip or actor replacement added. Register causal
state and custom policy fuzz models remain future work; their existing
deterministic tests were rerun.

Added snapshot_frames and snapshot_install_schedule targets, wrappers,
module exports and 29 deterministic seeds with separate snapshot_seeds
example. Shared fuzz dependency enables replication alongside colleague's
CRDT feature. Snapshot frame oracle covers errors, exact acknowledgment,
advance, stale and conflicting digests, foreign enrollment and ahead-of-head.
The install model starts with no installed image, uses revision3/4 exports
with retained revision2, predicts dropped reply outcomes, checks exact
current/retained values and durable acknowledgment after every native reopen.
Malformed version, enrollment, opcode and truncated request headers are
covered in addition to upload corruption, truncation, duplicate/stale offers,
dropped Ready/terminal reply, polling and staging cleanup.

Stable replay and all-warning Clippy passed. ASan completed 1000 frame runs
and 587 install schedule runs within the 30-second target budget, no findings.
Evidence target/fuzz-evidence/tmp6nhpcqqd. Added fuzz README instructions and
expanded docs/replication-operations.md with authenticated peer/enrollment
binding, deadlines, bounded retries and fresh stream on restore/fork.
The harness deliberately uses public native follower APIs. Transport faults
are fuzzed here, while simulated disk faults remain in existing follower
unit tests. No claim of transport authentication inside the engine.

Measured branch/condition coverage and the unsupported MC/DC instrumentation
mode are documented in [coverage.md](docs/coverage.md). The calibrated
library-only condition run covered 601 of 857 instrumented branch outcomes.
This scope includes unit-test code and excludes integration test execution.
Manual decision pairs and compiler coverage remain separately reported.

## Explicit complete-root salvage

Implemented explicit complete-root salvage in new src/storage/salvage.rs,
reexported salvage_on/salvage_file/SalvageReport. It uses a read-only ByteStore
metadata mask to independently run the existing codec against each root,
ignoring only the other slot and witnesses. Highest verified complete root
wins. Current and all retained graphs must verify. Only Corrupt permits
trying another candidate; I/O/limits/unsupported-format errors abort. Source
is exclusively leased and untouched. File destination creation happens after
candidate verification and uses exclusive create. Destination copy is verified.
Every success flags possible acknowledged data loss. Normal recovery untouched.

Admin salvage SOURCE NEW_FILE reports recovered revision, retained count,
verified slots and witness/data-loss warning. docs/salvage.md documents procedure,
partial output failures, identity/enrollment and application reconciliation.

Three tests cover all modes including newest graph corrupt, lost witnesses,
newest slot lost with intact witness, neither slot/graph usable, unsupported
header, I/O failure, source unchanged, destination existing, retained roots and
native CLI. Debug/release results are being finalized before checkpoint.
Final results all three tests passed debug and release with all features on
arm64 macOS. Core all-target all-feature Clippy passed. Ready for checkpoint.


## Recovery scale, space admission and native diagnostics

Recovery uses a lazily allocated region occupancy index in place of linear
range searches, with O(1) value-cache lookup. It needs at most 16,384 usize
entries (128 KiB on supported 64-bit targets). Cache entries still preserve
sharing and serialization identity. Header, checksum, padding, birth-revision,
parent, depth and work validation retain their ordering. Each accepted region
is scanned and marked once; CRC, tree reconstruction and DATA-cache searches
remain additional work. The encoder pointer cache is unchanged.

Release measurements on arm64 macOS with rustc 1.99.1 use five samples,
exclude fixture construction and disk-image copying, and verify every row.
Replay with `cargo run --offline --locked --release --example recovery_scale`.

| Records | Image bytes | Previous median ms | Indexed median ms |
|---|---:|---:|---:|
| 1,000 | 4,169,728 | 12.266 | 11.914 |
| 2,000 | 8,298,496 | 20.834 | 19.736 |
| 4,000 | 16,556,032 | 43.489 | 39.323 |
| 8,000 | 33,067,008 | 98.008 | 79.520 |
| 16,000 | 66,093,056 | 231.480 | 160.418 |

The largest image occupies 98.49% of the physical cap; measured median
recovery fell 30.7%. This is a local workload measurement, not a latency
guarantee. All 19 codec tests and 39 storage integration tests passed in
debug/release. Three mutations removing cache alignment or either occupancy
check were detected. Raw and structured image fuzzing each passed 10,000
ASan executions. Linux/Windows cross-checks passed; native qualification is
recorded separately. REC-V7 now describes occupancy rather than range scans.

`Database::storage_space` queries the serialized byte store once and reports
revision, physical and committed length, aligned next append offset, capacity
and remaining append bytes. It does not scan the graph or estimate reclaimable
space. A partial abandoned tail consumes its entire region. Backend length
errors remain retryable; a length below the committed end or above the format
cap poisons publication. Memory-only databases return `Invalid`.

Three headroom integration tests pass debug/release. They independently
exercise retryable length failure, both impossible-length terms and persistent
poisoning. A near-cap fixture with all 64 retained roots permits the final
empty commit, rejects further commit/retain before writes, then compacts and
commits again while preserving all retained rows. No new MC/DC percentage is
claimed. Application headroom thresholds must leave space for expected edits.

Opt-in native diagnostics pair the stable engine error with the first native
syscall failure in the synchronous operation. Categories distinguish permission,
disk-full, quota and other failures; operation and optional raw OS code carry
context. They contain no paths or record values. `storage::with_native_diagnostics`
wraps any synchronous engine Result operation; file open/create/compact and
transaction commit also have convenience wrappers. Private nonallocating TLS
scopes reset, nest and restore on unwind. Terminal errors are captured at the
source before cleanup; interrupted retries do not replace the eventual result.
The stable `Io`/`OutcomeUnknown`/`RecoveryRequired` error remains authoritative.
A custom ByteStore or work dispatched to another thread can yield no diagnostic.

Five diagnostic tests pass debug/release, covering scope isolation, unwind,
first-error retention, native missing-parent failure, category mapping, EINTR
and publication phase classification. Existing file/durability/compaction tests,
Clippy and cross-checks pass. Actual full-device and quota-mount qualification
remains open; category tests do not substitute for it. The C ABI is unchanged.

## Bounded typed migration and macro assurance

`Write::migrate_ids(table, ids, callback)` accepts an explicit bounded ID plan.
It checks count against write and observation limits and rejects zero/duplicate
IDs before reading or invoking callbacks. Missing rows abort the transaction;
current-version rows skip conversion. Input order is preserved. Each old row
invokes its callback once per call with its ID, raw record and mutable model.
There is no automatic retry. The shared per-row migration path preserves
unknown fields and atomic primary/index changes. Whole-table migrate remains.
An application progress record can be written in the same closure. Raw ID
discovery, callback cost and external callback effects remain app-owned.

Four new tests cover resume after simulated reopen, atomic progress, retained
old schemas, unknown fields, physical missing/default semantics, rollback of
callback/missing-row/unique failures, independently bounded observations and
real concurrent selected/unrelated row edits. All 26 typed model tests and six
doctests pass debug/release; one existing renamed-dependency doctest is ignored
and separately executed by the workspace check script. Empty/current-only
batches return zero but a Store::write still performs its normal commit.

`schema_index_migration` fuzzing independently tracks primary rows, schema
versions and unique ownership in a BTreeMap through up to 48 operations.
It checks every bounded key after each operation, including migration rollback,
rebuild and reopen. `derive_tokens` exercises the exact pure expansion functions
for Model/Record/Resolution and parses successful output as Rust syntax. Model's
implementation moved unchanged to a private module for this reuse. Syntax
parsing does not prove downstream type correctness. Twelve actual downstream
compile-fail fixtures separately check malformed attributes and diagnostics.

Both targets have eight total checked-in seeds and passed 1,000 ASan executions
each with seed 1 and max_len 8192. Stable fuzz replay (10 tests), macro tests in
both profiles, typed/CRDT regressions, all-target Clippy and StudyProgress passed.
StudyProgress converges both goals to 90/20 and repeat sync keeps revision 4 on
both peers. The fixed-schema fuzz model does not cover arbitrary schema graphs
or injected storage faults; existing crash harnesses retain that scope.


## Operational copy fuzzing

Three further targets cover logical import/export, backup/restore and bounded
ByteStore progress. The logical target combines raw streams, truncation,
XOR edits and independently recomputed CRC64 edits; an explicit repaired
zero-next-retention-ID image rejects before destination writes. A clean fixture
has an independent revision/current/retained row oracle. Other accepted logical
streams must re-export identically, a codec consistency check rather than an
independent parser proof. Sources stay byte-identical.

Backup schedules inject copy errors, zero progress or failed sync with persisted
bytes, crash, fail a second time during reopen, crash again and inspect the final
state. A successful copy always reopens; any usable interrupted copy must match
the complete source current and retained state. Invalid sources cannot alter an
occupied destination. Progress cases independently vary 0/1/63/4095/4096/8192
bytes for the first 64 reads/writes before returning to full progress, keeping
simulator queue bounds from masking the transfer behavior.

The 66 seeds replay in debug/release; five logical modes and a repaired rejection,
36 progress combinations and 72 copy/recovery schedules passed. All-target fuzz
Clippy passed. Native arm64 macOS ASan completed 500 repaired-logical runs and
100 runs each for backup and progress with seed 1 and bounded time/RSS. These
are smoke runs; native filesystem faults and extended campaigns remain separate.
There are now 17 discovered cargo-fuzz targets.

## Checkpoint-heavy native lifecycle measurement

`examples/storage_lifecycle.rs` fills a new native file to 66,162,688 bytes
with 240 edits of a 256 KiB checkpoint and keeps 64 distinct retained roots.
Every current and retained byte is checked after reopen, backup, restore and
compaction. The tool requires a new output directory and keeps its files for
inspection. Measurements below use an optimized build on the local arm64
macOS host; they are a sample workload, not the user's deployment budget.

| Operation | Samples | p50 ms | p95 ms | p99 ms | Result bytes |
|---|---:|---:|---:|---:|---:|
| Commit 256 KiB checkpoint | 240 | 11.964 | 14.178 | 15.080 | 66,162,688 |
| Retain current snapshot | 64 | 7.003 | 9.918 | 11.060 | 66,162,688 |
| Reopen with 64 retained roots | 5 | 42.791 | 71.245 | 71.245 | 66,162,688 |
| Verified backup | 1 | 169.542 | 169.542 | 169.542 | 17,596,416 |
| Verified restore | 1 | 177.797 | 177.797 | 177.797 | 17,596,416 |
| Compact after releasing 32 roots | 1 | 102.777 | 102.777 | 102.777 | 8,945,664 |

Each edit appended 274,432 bytes. Percentiles use nearest rank; one sample
does not establish a latency distribution. The source remains authoritative
while the verified compact copy is produced. After reopening the compact copy,
a further commit and reopen preserve the remaining 32 roots. File contents
and timings passed on the native adapter; no power-loss/device or RSS bound
is inferred from this run. Replay with `cargo run --offline --locked --release
--example storage_lifecycle -- NEW_DIRECTORY`.


## Compatibility, native faults and decision inventory

`tools/compatibility.py` builds the same downstream driver against current
`1ca5e5ab` and archived `a7dc343` using offline locked Cargo on arm64 macOS
rustc 1.99.1. Both verify current revision 4 and retained revision 2, exact row
counts/values and missing historical keys. Both reject an unsupported header
version without changing file bytes. The frozen pre1 fixture covers the opposite
reader/writer direction. These are named pre1 checkpoints, not a stable-format
release promise. Evidence binds engine, driver and test source hashes.

The typed process-evolution test runs create, partial migration, verification,
backup, restore, historical rollback and explicit remigration in separate
processes. It checks unique indexes, unknown field 99, mixed versions and retained
originals. Source/backup remain at their original mixed state. Debug/release and
Clippy passed. These are orderly process boundaries; crash tests remain separate.

`tools/native_faults.py` creates a bounded private 8 MiB Linux tmpfs in Docker.
An unprivileged UID 10001 receives actual errno 13/Permission on a protected
directory. Filling only a controlled ballast file produces errno 28/DiskFull
during append: commit returns Io, visible revision 3 is unchanged, and reopening
after freeing ballast verifies revision 3 and retained revision 2. Retry commits
and reopens revision 4. Both drills passed on Linux 7.0.14-linuxkit aarch64 with
rustc 1.99.1. A 16 MiB write ceiling, 300-second container timeout and 2 GiB memory
limit prevent unbounded host filling. Tmpfs proves native error behavior, not
power-loss persistence or quota/device failure. See docs/native-fault-drills.md.

The isolated source decision inventory traverses every workspace library, macro
and binary root, keeping platform/feature cfg provenance and excluding definite
test-only code by default. Snapshot ebb8157361edf4c6334f737385ebd0584ec0e050c9f7c8edab024699971e5638
has 2,380 records across 45 files and 10 targets, 18 exclusions and 274 unexpanded
macro/derive blind spots. All records are explicitly unreviewed. Module links to
existing ledger entries do not claim decision coverage. Four fixture tests, strict
Clippy, deterministic replay and independent range/hash verification passed.
See docs/decision-inventory.md for scope and remaining classification work.

Compiler condition instrumentation on 1ca5e5ab printed 1,179/1,545 branch
outcomes after calibration and passing functional tests. The later
[historical audit](docs/historical-coverage-audit.md) found 451 mapping mismatches
in its original log: this aggregate is withdrawn as qualified coverage. Source
stability did not establish compatible profile mappings. Compiler coverage,
source inventory and manual MC/DC pairs remain separate evidence. Macro diagnostic builds need their own compiler
environment; sanitizer qualification remains gated on the corrected CI harness.


## Linux server/macOS client simulated month

The owner selected this primary deployment and `sec@qxuken.dev` as the security
contact. SECURITY.md records private reporting and release responsibility;
all Cargo manifests remain unpublished. The month harness uses pinned engine
1ca5e5ab with typed tasks local to each node and replicated StudyProgress.
Across 30 simulated days it checks 480 task edits, 60 conflict/retry pairs,
60 offline CRDT edits, 120 TCP sync sessions, 60 daily retained roots and eight
receipt-commit/process-exit retries. Every current and retained task/index/CRDT
state is checked against a closed-form oracle. Final backups and restored
copies receive the same full verification. Repeat sync must add zero commits.

Actual Linux aarch64 server/macOS arm64 client runs passed twice. The reproducible
runner uses a private Docker ext4 volume and native APFS client, rustc 1.99.1,
with source archive and per-platform binary hashes. Final files are 6,668,288
bytes each and contain 30 retained daily roots. The second run completed in
30.54 seconds; returned transaction samples have median 7.076 ms and p95
14.710 ms. This is an accelerated calendar on synthetic eight-task databases,
not a 30-day wall-clock soak. It uses test-owned TCP endpoints and exercises
CRDT replies; production TLS and passive follower acknowledgment remain separate.
See docs/monthly-workload.md for exact state, counts, timings and exclusions.

Native CI at 1ca5e5ab passed macOS, Ubuntu and Windows, including compiled
Windows C/C++ consumers, plus coverage. Its ASan/TSan jobs exposed inherited
sanitizer flags in nested macro compile-fail Cargo builds. The diagnostic
subprocess now clears instrumentation/target overrides, retaining its compiler;
the outer runtime suite remains instrumented. The original failure was
reproduced, then targeted ASan/TSan, stable tests, Clippy and poisoned-environment
regressions passed. A fresh complete matrix remains the final combined check.


## Aggregate live allocation admission

`Limits::max_live_bytes` optionally bounds requested core heap layouts in one
Memory domain, including in-flight reservations. Default usize::MAX preserves
aggregate admission behavior. `MemoryUsage` reports live/peak/limit through
Database and Value, including values outliving their database. Shared nodes and
charged internal vectors retain a budget guard until final destruction and
deallocation; clones do not charge again. Drafts, retained roots, byte payloads,
keys, codec caches and logical-format scratch use this domain. Native/backend
allocations, foreign handles, application input/output buffers, allocator overhead
and application-layer typed/replication buffers remain explicit exclusions.
See docs/memory-budget.md.

Growth admits the complete new capacity while the old capacity stays charged,
then uses exact-layout realloc. Null preserves the old buffer and rolls back
the new reservation. The initial allocate/copy implementation caused a measured
recovery slowdown and was replaced before qualification. The realloc version's
local release medians are 108.6 ms at 8,000 scalar records and 204.8 ms at 16,000,
compared with the earlier 79.5/160.4 ms without accounting. These measurements
include counter/header overhead and are not controlled latency guarantees.

Independent review and Miri checked alignment, null rollback, alias ownership,
partial iteration, unwinding destructors and overflow. Nine private and five
public budget tests passed under Miri, with workspace tests/doctests, release
allocation/budget suites, Clippy and Windows/Linux cross-checks. Scoped direct
decision pairs and two additional public equality/rollback tests are recorded in
docs/memory-budget-decisions.md. Five safe admission/telemetry mutations were
caught. Follow-up review found that a sampled peak could decrease before the
admitting thread updated it; a deterministic failing control now passes after
queries atomically persist observed high water. Its seven admission tests passed
again in debug/release and the window regression passed Miri.

The simulated month passed locally with a 16 MiB per-domain cap, peaking at
1,210,643 charged bytes across 420 reported phases. Native process RSS is a
separate measurement. The new cap never permits partial recovery, silent
fallback or publication of a failed candidate. Wire format and C statuses
remain unchanged; Rust Limits has an additional public field.

## Engine and schema source decision audits

The engine audit classifies all 141 production source records, with explicit
execution evidence, local predicate pairs, coupled/invariant records and native
delegations. Ten new public tests and six private boundary/arithmetic tests
cover terminal APIs, record/collection and overlay predicates, revision/ID
exhaustion, poisoned publication, preparation rollback and checked arithmetic.
A small highly shared value DAG reaches transaction logical-weight overflow
while core memory stays below 1 MiB; failed staging preserves the earlier write.
A private checked-charge helper preserves the original operation order and
permits exact arithmetic boundary tests without impossible key allocations.
Debug/release transaction/allocation/schedule tests and targeted Miri passed;
three wrapping-arithmetic mutations were caught. Native RNG failure remains
an explicit delegated obligation. See docs/decisions/engine.md.

The raw schema audit classifies 176 source records. Nine directed tests and the
15 existing schema tests pass debug/release, with Clippy. Cases cover descriptor
and envelope fields, defaults/presence, all value kinds, successor/reservation
rules, descriptor key validation and publication's error/active-state guard.
The audit distinguishes 42 delegated primitive propagations from local predicates.
A valid empty record with max_bytes=7 demonstrates that projecting an eight-byte
field key can legitimately return Limit; those paths are tested, not assumed
infeasible. No production schema changes were needed. See docs/decisions/schema.md.

## Application, tree and namespace fuzzing

The corpus now has 21 targets. coordinator_protocol and peer_transport share
exact extracted private production parsers/command executors with the apps.
Independent arithmetic/receipt maps check state, revision, retry, enrollment,
length/UTF8/auth gates and malformed input. The peer target covers local command
and enrollment boundaries; TLS handshakes and network faults remain separately
qualified. Existing app process tests passed debug/release. Both new targets
passed 1,000 ASan executions and stable replay with 13 permanent seeds.

replacement_namespace exercises the production sequencer against an independent
durable/flushed/volatile namespace model, including every boundary, retry, orphan
and unrelated entry. Model leases and exchange/persistence assumptions are not
native filesystem evidence. tree_adversarial_keys compares long/binary/prefix
keys, float bits, immutable snapshots and radix-boundary lists against BTreeMap/Vec.
The two targets have 49 seeds, passed 1,000 ASan executions each, stable replay and
Clippy. Forced full-hash collisions remain a separate explicit production unit
test; random fuzzing does not claim to discover them. txn_program is consolidated
into the existing schedule harness rather than duplicated as a target alias.

At checkpoint 83954e8, the complete native macOS/Ubuntu/Windows matrix, ASan,
TSan and coverage passed. Linux CI also passed all then-current 17 fuzz targets:
16,409 actual executions under the configured bounds (schema/index stopped at
409 on its time budget; the other targets reached 1,000 each). New work requires
a new combined candidate.

## Private package relocation and Odin bindings

Private packaging now supports native Windows MSVC DLL/import-library delivery
with packaged-header C/C++ clients, dumpbin exports/dependencies, runtime identity
and compiler/SDK provenance. Dynamic CRT prerequisites are recorded; vendor
runtime DLLs are not bundled. All native CI platforms now build private packages.

The generated raw Odin binding mirrors all 49 C declarations and constants, with
a generator that rejects unsupported signatures. Optional --odin qualification
packages and compiles a standalone consumer against the exact library, without
wgpu. It checks identity, binary buffer handling, transactions and retained-file
reopening. Missing requested tools or concrete attribution inputs fail their gate.

This work found a real packaging defect: macOS copied libraries retained a
temporary Cargo install-name, and earlier C/C++ checks passed while that directory
still existed. The linker now uses @rpath/libweekend_db.dylib with @loader_path
clients; Linux uses an explicit SONAME and literal $ORIGIN. Packaging deletes the
build directory before runtime checks, then moves the package so its original
path is unavailable and reruns consumers. Actual macOS Odin/C/C++/ctypes and
Linux C/C++/ctypes relocation passed. Earlier private-package verification does
not establish relocation. Windows execution of the new packager awaits CI.
Eleven regression tests and generated-binding checks pass. Nothing is published
to a registry. See docs/odin-private-package.md and docs/windows-private-package.md.


## Verification diagnostics and further source audits

StorageFailure now carries optional verification stage and structure-start offset;
with_storage_diagnostics names combined opt-in capture, while the previous name
remains available. Engine errors, format, C ABI, recovery selection and barriers
are unchanged. Deepest direct-propagation failure wins; offsets may be invalid
attempted references, not exact corrupt bytes. Nested capture is required when a
caller handles errors and continues; salvage clears discarded probe failures.
The additional public Rust field changes exhaustive pre-1 patterns/literals.
All 61 targeted checks passed debug/release plus Clippy and CLI smoke. See
docs/verification-diagnostics.md.

Tree69, value (79), storage core (57), replacement (2) and simulator (60) records now have
explicit source-audit dispositions. Nine new tree/value tests plus ten existing
tests pass each profile; thirteen new storage tests plus reused suites total 76.
Pairs cover collapse/shrink, real shared-graph weight overflow, four verification
envelope predicates, image bounds, revision/counter exhaustion, trace limits,
tear/sync survival and allocation rollback. Only test code changed for these
audits. Invariants and delegated obligations remain explicit; see docs/decisions.

## C sanitizer calibration and allocation concurrency model

C memory/thread/file consumers and a Rust ASan library with rebuilt std now use
one explicit sanitizer runtime. Four calibration controls require valid C/Rust
reads to succeed and deliberate heap overruns in each language to fail. Apple
Clang 21's incompatible runtime failed; LLVM 23 passed all controls and consumers
on macOS. The oversized C input now allocates its full stated length. Input and
binary hashes are retained; changing sources rejects evidence. Native Linux CI
subsequently passed calibrated C/Rust ASan at checkpoint 4203e0a. See
docs/c-sanitizers.md.

A second Loom model copies allocation admission CAS, release and peak sampling.
Three threads, preemption bound 2 and branch bound 1,000 check exact-cap admission,
competing release, rollback, zero final live bytes and monotone sampled peaks.
Both ownership/budget baselines passed. Load/store admission and non-persisting
peak mutants fail their required assertions. This bounded copied model does not
prove raw-pointer correctness or every schedule; see tools/atomic-model/README.md.

## Qualification at checkpoint 6266359

All native macOS/Ubuntu/Windows, ASan, TSan and coverage jobs passed at 6266359,
including private Windows DLL/import-library C/C++ packaging. All21 Linux ASan
fuzz targets passed with 20,456 actual executions: schema/index 538, tree 918,
other 19 targets 1,000 each. The updated runner binds source/seed hashes and rejects
changed inputs or missing/zero completion reports. Two runner tests and 100 real
raw-image executions passed. Longer campaigns remain.

The capped month passed Linux ext4 server/macOS APFS client with archived 6266359
engines and separately hashed drivers. All 420 reported phases stayed within 16 MiB
core admission; peak charged layouts were 1,210,643 bytes. Current/retained/restore
oracles and repeat-sync no-commit checks passed. See docs/monthly-workload.md for
latencies and scope. Accelerated calendar execution is not continuous-month
uptime or physical power-loss evidence. Later changes need a new combined
candidate. Cargo publication remains disabled.


## Private Rust source distribution

Optional --rust-source packages tracked regular source files with workspace and
consumer lockfiles, verifies relative dependency closure, and compiles a fresh
private Rust consumer. No precompiled rlib portability or vendored/offline SDK
claim is made. Registry inputs remain external and pinned. The consumer checks
source identity, derived typed models and unique index updates, and derived CRDT
Record/Resolution enrollment, offline edits, committed-policy convergence and
repeat-sync stability. Optional --rust-source-isolated builds in bounded Linux
Docker with only the copied source mounted and the original checkout absent.
Native compilation and Linux isolation have separate evidence fields. Fifteen
packager regression tests plus consumer release/Clippy passed; exact clean
candidate qualification follows checkpointing. See docs/rust-source-package.md.

The combined worktree passed python3 tools/check.py after these additions:
workspace tests and consumers in debug/release, strict Clippy, Rust/C/Odin
file/memory/thread examples and wgpu coexistence. This local macOS run precedes
the next clean-source CI and source-package qualification.

Checkpoint 4203e0a passed native macOS/Ubuntu/Windows, ASan/TSan, coverage, bounded fuzz and Linux C/Rust ASan. C calibration and both atomic models/negative controls passed on Linux. Clean private packaging with C/C++/Odin relocation, typed/CRDT Rust consumer and Linux isolated rebuild passed with complete attribution inputs. The later integrated audit and finite macOS soak are recorded below; longer deployment soak and device qualification remain. See docs/c-sanitizers.md and docs/rust-source-package.md.

### Integrated source audit and bounded qualification (2026-11-12)

The [module audit index](docs/decisions/README.md) now classifies all 2,415
explicit source records in the [fresh inventory](docs/decision-inventory.md).
All 47 file hashes and 5,647 recorded ranges were independently checked against
source bytes. Dispositions include direct predicate pairs, execution-only
checks, coupled syntax, invariant proofs and delegated failures. They are not
2,415 independent MC/DC conditions or a compiler coverage percentage. The 284
unexpanded macro/derive sites remain a separate denominator; supplemental FFI
export and generated Model/Record/Resolution mappings document their evidence.
The direct generated Model::patch type-ID control passes debug/release and
strict Clippy, including unknown-field preservation and rejected-record identity.

The corpus has 22 targets and stable replay passed 25 tests. Historical frozen
4203e0a extended fuzzing ran all 21 then-existing targets with `--runs 100000
--seconds 120 --seed 2`, default 2,048 MiB RSS limit and max_len 262144, on
arm64 macOS using nightly 1.101.0 (708c94ef0), LLVM 23.1.5. All targets exited
successfully, with 1,384,848 actual executions. Ten reached 100,000 runs; the
other eleven reached their individual time bounds:

| Target | Executions |
|---|---:|
| commit_crash | 53,690 |
| image_and_ops | 44,207 |
| retention_compact_crash | 30,559 |
| crdt_schedule | 34,238 |
| snapshot_install_schedule | 1,859 |
| schema_index_migration | 5,540 |
| derive_tokens | 60,809 |
| backup_restore | 73,597 |
| coordinator_protocol | 43,932 |
| replacement_namespace | 29,974 |
| tree_adversarial_keys | 6,443 |

These are observed historical results, not a current-candidate campaign. The
extended raw evidence cache has expired; the surviving local log records target
exit statuses. The initial pre-fix register_causality target passed 1,000 ASan
runs with seed 1 and max_len 96. A separate post-fix rerun exited successfully
(`/tmp/register-fix-asan.log`); that surviving summary does not retain an actual
execution count, so the agent-reported 2,000 executions are not independently
recounted here. Its independent causal-event model complements
the 5,461-prefix honest-schedule oracle and rejected forged-retirement frames.
Four focused register tests passed Miri: `register_causal_event_oracle_accepts_bounded_honest_schedules`,
`register_decode_and_same_dot_predicate_controls`,
`register_forged_retirement_frames_never_publish_or_acknowledge`, and
`register_rejects_empty_history_and_unwitnessed_retirements`. These tests support
the new admission guards; they are not a proof for Byzantine or arbitrary
application-defined CRDTs. See [CRDT audit](docs/decisions/crdt.md).

The [macOS soak](docs/soak.md) used frozen engine 4203e0a for 600.402 seconds:
924 rounds, 59,136 paired-key writer commits, 448,149 concurrent reader
iterations and final revision 60,978. It verified retained roots, reopen and
compaction under a 16 MiB core allocation budget. Recorded per-open allocation
peaks were 50,288–51,248 bytes. This finite small-state workload does not replace
long representative deployment, network or device power-loss qualification.
The cross-OS simulated month remains separately scoped evidence.

Checkpoint 4203e0a already passed native macOS/Ubuntu/Windows, ASan/TSan,
coverage, bounded fuzz and calibrated Linux C/Rust ASan plus private packaging.
Tests and the CRDT admission fix added after that checkpoint require their own
candidate CI; existing baseline results are not attributed to the later
source snapshot. The [qualification roadmap](docs/roadmap-status.md) preserves
mixed-version, delegated error injection, expanded decision coverage, longer
soaks, hardware durability and deployment-specific acceptance work.


### Authenticated snapshot CLI after checkpoint 8556dc1

The private `weekend-replicate` application wraps the existing Primary/Follower
protocol with exact-leaf mutual rustls TLS and separate `weekend-snapshot/1`
ALPN. It requires a completed handshake with no pending handshake writes before
passing the stream to core. Stream/file enrollment remains explicit. Capture
holds the primary lease only through local export; the primary closes before
network connection, and captured local revision is reported separately from
follower durable revision/digest. Up to three bounded attempts retain the same
immutable Export. Restart reconciliation polls recovered follower progress and
can compare the saved export hash; restarting a send captures a new export path.

Socket I/O uses one absolute 30-second deadline including client connect; local
verification/flush can outlast it. Serial admission and optional finite accepted
connection count keep the service lifecycle explicit. A poisoned manifest stops
service until reopen. Credentials are bounded regular files in trusted
configuration directories; Unix private keys require owner-only modes and
keygen uses 0700/0600. Windows transport supports externally ACL-protected keys,
while Windows keygen fails closed. The [operations runbook](docs/replication-operations.md#authenticated-native-cli)
documents enrollment, offline status, retries, uncertainty and deployment limits.

Initial native macOS debug/release evidence covers nine unit/native/TLS tests
and one CLI process test: authorized installation, exact pin/ALPN checks,
fragmented authenticated handshake without Finished, both enrollment mismatch
fields, immutable revision 4/value 2 capture while primary advances to
revision 5/value 3, retained revision 2/value 1, dropped reply/reopen/retry with
unchanged digest/manifest, credential caps/FIFO/modes and deterministic service
restart/poll. Strict all-target Clippy passed. This new application adds source
obligations beyond the 2,415-record audit at 8556dc1; those audit totals and prior
platform results are not silently extended to it. No core publication barrier,
wire layout or Cargo publication policy changed.

Independent qualification used the exact archived git tree
`a0aad9f9ff95a2998ca623b654c452ae02658b6b`; application and test code remain
byte-identical to that tree after these documentation updates. Native arm64
macOS and aarch64 Linux on an ext4 Docker volume each passed nine app tests plus
one CLI process test in debug and release. macOS also passed strict app Clippy
and the full Rust/C/Odin workspace check. Optional Linux Clippy was unavailable
in the pinned image. An authenticated macOS-to-Linux transfer installed follower
revision 13 with retained root 1 at revision 11. After follower restart and
primary advancement to revision 14, fresh TLS polling still reported revision 13
and the exact export digest
`70b916bc69f94a1cccac78417093ec238d8185a444543a4cb84d2a5d2b9733ea`.
Evidence: `target/independent-replication-qualification-rerun-a0aad9f9ff95/evidence.json`.
This is bounded native/cross-OS functional evidence, not physical power-loss,
mixed-version or long-duration network qualification.

Separately, audit checkpoint `8556dc1` passed 22 fuzz targets with 21,031 actual
executions in fuzz run `43248620063`. Engine checks run `43248619666` passed
native Ubuntu/Windows, C ASan and Rust ASan. Hosted macOS, coverage and TSan jobs
were cancelled because of
runner capacity; cancellation is not a pass. Those checkpoint results do not
include the new authenticated CLI.

### Bounded mixed-checkpoint snapshot wire evidence

The [private compatibility profile](docs/snapshot-compatibility.md) qualified
`dcd2402adeb034f15a5293c52238524ace9c4070` and
`315068fc38b99d8ec616fdb5a8ee81107b471534` in both primary/follower directions on
native arm64 macOS with rustc 1.99.1. Frozen archives retained their original
Cargo.lock files; only the shared example fixture and explicit manifest entry
were overlaid. The production follower and replication module differ between
these checkpoints. Both directions preserve exact durable revision 4, retained
root 1 at revision 2, and receipts matching an independent SHA-256 of the export,
including reopen and duplicate retry after the primary advances locally to 5.
A poll header with its version byte mutated to 255 is rejected by both receivers
without changing follower files. This negative check is not an upload fault,
future-version negotiation or mixed-version TLS CLI test. Caller-authorized
loopback peers, exact fixture/binary/source hashes and results are bound by
`target/snapshot-compatibility-reviewed/evidence.json`; deployment trust,
certificate rotation, broader checkpoint/OS pairs and physical power loss remain
outside this scoped proof.

### Authenticated snapshot CLI decision audit

The [snapshot CLI ledger](docs/decisions/snapshot-cli.md) classifies all 115
explicit production records introduced at `315068f`; runtime code is unchanged.
Together with the historical 2,415-record audit, the current inventory has 2,530
dispositions across 50 files and 310 macro/derive blind spots. These are source
dispositions, not a measured MC/DC percentage.

New real TLS controls present valid alternate leaves signed by the configured
trust anchor. Both endpoints reject them with the exact leaf-pin diagnostic,
while exact peers install revision 2. Rejection leaves follower manifest bytes
and directory contents unchanged, and reopening reports no durable progress.
Additional controls cover CLI dispatch, permanent retry rejection and private
credential modes. All 14 focused app tests passed macOS debug/release and Linux
debug, with strict macOS Clippy. Windows-safe fixture reads include `b3761af`;
this tranche does not claim a new Windows run. Pending-write-only and client
partial-handshake independence, delegated OS/dependency failures and expanded
macro coverage remain explicit residuals. Exact source/test hashes and the
source-set digest are recorded in the ledger.

### Verified follower backup and explicit restored-stream reseed

The [backup/restore runbook](docs/follower-backup-restore.md) and private harness
qualified frozen `65d142e6ae85e477f0abb21e2ab04d67393db1f3` on native arm64 macOS
with rustc 1.99.1 using its unchanged original Cargo.lock. Existing CLI mTLS
commands installed a snapshot; closed status selected its authoritative image,
whose independent SHA-256 matched the former receipt. Existing admin commands
verified backup and restore into new paths. Full row oracles preserved current
revision 4 and retained root 1 at revision 2. A new stream and new follower then
installed/reopened a separately verified durable receipt. Both wrong-enrollment
offline status and authenticated upload rejected with ForeignStore without
changing old follower files; existing-destination restore rejected unchanged.
The original primary and old follower artifacts remained byte-identical.
Evidence is `target/follower-backup-restore-reviewed/evidence.json`.

This is an explicit quiescent multi-command workflow, not atomic status-to-backup,
automatic failover, primary fencing, recovery of unshipped commits, or a power-loss
test. No abandoned inactive-slot sentinel was injected; selection followed the
verified status rather than scanning candidate images. Application history/schema
reconciliation and broader platform/fault qualification remain separate gates.

Separately, checkpoint `b3761af` native Windows CI job `131310796175` passed the
workspace, C/C++ consumers and private packaging. Ubuntu, C-ASan and fuzz jobs
also passed. Hosted macOS matrix work was still pending at this observation;
these CI results are not evidence for executing the new backup harness on Windows
or for qualifying later untested changes.

### Offline passive follower maintenance CLI

`weekend-replicate cleanup-inactive` and `compact-manifest` now expose the
existing verified Follower operations during an offline maintenance window.
They preserve authoritative selection, verify enrollment, and report maintenance
separately from the follower durable acknowledgment. macOS/Linux compaction
consumes/reopens the handle through existing safe replacement; Windows rejects
before opening files and requires explicit fresh-directory reseeding. No core
protocol, publication barrier, automatic fallback or forced cleanup was added.

The native CLI fixture stages an abandoned inactive file, checks exact wrong-
enrollment rejection without mutation, preserves unrelated entries and selected
bytes, and verifies cleanup idempotence. On macOS/Linux it inflates an abandoned
manifest tail to the admission ceiling, compacts below it, and reopens exact
revision 4/value 2 and retained root 1 at revision 2/value 1 with the same digest.
All 14 app tests passed macOS debug/release and Linux debug; strict macOS Clippy
passed. Windows cross-compilation stopped in ring because Windows SDK headers
were unavailable, so the Windows branch is source-reviewed only in this tranche.
The app ledger now maps 125 records; the global source inventory has 2,540
dispositions across 50 files and 314 macro/derive blind spots, not measured MC/DC.

Before this extension, checkpoint `939d683` Engine run `43258936460` attempt 2
passed every executed job. Native macOS job `131350167575` stopped before runner
execution due to the GitHub account billing/spending-limit annotation. The exact
archived checkpoint passed full local macOS `tools/check.py` and TSan, and fuzz
run `43258936624` passed. This is not a hosted macOS pass; no workflow was changed.
These checkpoint results are separate from the new maintenance tests.

### Exact-head local qualification at 2490631

The [local qualification report](docs/local-qualification-2490631.md) binds
macOS and Docker Linux full checks, calibrated C/Rust ASan, Rust ASan/TSan,
private package consumers, four release feature profiles, bounded atomic models,
25 replay tests and 20,956 ASan fuzz executions to exact commit `2490631`.
Its printed line total 13,702/14,136 and branch total 1,633/1,751 are now
**unqualified raw output**: retained-object replay reproduces 1,046 and 660
LLVM mapping mismatches respectively. The [audit](docs/historical-coverage-audit.md)
withdraws those aggregate claims without changing functional/sanitizer evidence.
Windows cross-linking produced 47 test executables and the release CLI, but no
native Windows tests, NTFS lifecycle or C/C++ package execution occurred for this
head. Hosted billing-blocked jobs executed no tests. Repository/crates remain
private/unpublished; artifact eligibility is not a production-readiness claim.

### Shared private CLI TLS hardening

Study and snapshot CLIs now compile one private TLS module with fixed per-app
profiles. Study retains SAN `study-peer` and three seconds including TCP connect,
requires `study-progress/1`, and now checks handshake completion/write drainage,
exact leaf pins and bounded regular credentials with private Unix key modes.
Credential validation precedes database open/listen; Windows keygen requires
external ACL provisioning. Both study peers must upgrade together because old
CLIs lack ALPN. On-disk records, application wire encodings and local/remote
acknowledgment semantics are unchanged. Snapshot preserves its existing profile.

Independent TCP controls reproduced old study acceptance before Finished and
without protocol ALPN. Shared regressions cover both profiles, signed alternate
leaves under trusted anchors, partial Finished, missing/other-app ALPN and
credential caps/FIFO/modes. macOS and Docker Linux debug/release each passed 33
focused tests plus strict Clippy. Mise-pinned Windows tooling cross-linked five
test executables; no native Windows execution is claimed. See
[shared TLS evidence](docs/shared-app-tls.md) for source scope and residuals.
The current inventory has 2,557 dispositions across 51 files and 316 macro blind
spots, including 44 shared TLS records separately for each binary profile and
the coordinator's two pre-open credential admission records. This is not a
compiler-measured MC/DC closure; historical 2490631 results remain source-bound.

### TLS handshake admission independence

The private shared TLS implementation now exposes a private generic I/O seam
around its existing handshake/admission sequence. A genuine rustls opposite
endpoint and deterministic transport faults establish H/W states `(false,false)`,
`(true,false)` and `(false,true)` for both client and server guards, while exact
leaf pin and ALPN remain valid. Thus each operand independently decides whether
an otherwise authenticated session is admitted. Native DeadlineSocket handling,
absolute budgets and local/follower acknowledgment semantics are unchanged.

Both binary profiles passed 35 focused tests in each debug/release run on macOS
and isolated Docker Linux, plus strict Clippy. Four isolated guard-removal
mutations are detected through the repository mutation runner. The two app
profiles' five Windows executables cross-link;
no Windows tests execute locally. See [the directed pairs](docs/tls-handshake-decisions.md).
The rebound inventory has 2,561 dispositions across 51 files and 316 macro blind
spots; two added caller-propagation records per target remain delegated.
These directed pairs close the specified handshake residuals, without claiming
compiler-measured whole-application MC/DC or physical power-loss qualification.

### Explicit CRDT history migration and stale-backup recovery

The [migration example](docs/crdt-policy-migration.md) converts StudyProgress
practice minutes to seconds and changes the weekly-goal policy from maximum
to minimum under an explicit new wire marker, collection identity, generation
and actor roster. It first reconciles the frozen old pair, preserves reviewed
visible values and seeds their counter aggregate exactly once. Old retained
state stays intact; staging is explicitly non-atomic across stores.

Five directed cases pass debug/release on macOS and isolated Docker Linux,
along with the example and strict Clippy. They cover changed-policy convergence,
obsolete identity/wire rejection without writes, conversion bounds, allocation
failure leaving partial staging, and recovery from a stale durable image through
fresh identities. Reopening destinations and repeat sync preserve stable revisions.
The test also cross-links for Windows; no Windows execution occurred. Restoring
an old actor cannot detect every rollback locally or reconstruct events absent
from all available copies. Native-file cutover and authenticated identity routing
remain operational responsibilities.

### Matched SQLite durability/history baseline

The [private baseline](docs/sqlite-baseline.md) compares the same current map and
four independently persisted historical maps through commit, reopen, backup,
restore and compaction. SQLite stores version rows and retained epoch metadata;
weekend-db uses native retained roots. Both retain obsolete physical history
until explicit reclamation. The report distinguishes SQLite WAL/FULL/fullfsync
configuration, deferred pager reads and backup layout from native publication,
eager recovery and compact copies.

Three alternating-order APFS rounds against frozen core 2490631 pass complete
byte/epoch/ID oracles: 192 data commits, 12 separate pins and 96 reopens per engine.
Observed commit p50/p95/p99 were 9.055/10.071/10.986 ms for weekend-db and
2.988/3.918/3.992 ms for the linked SQLite 3.53.1. Raw WAL/SHM sizes and separate
checkpoint costs remain in the evidence. A canonical-key normalization gap in
the Python oracle was independently reproduced and fixed; five tests and a
separate cross-engine smoke pass under Python optimization. This is a finite
small-state baseline, not a capacity, latency or power-loss guarantee. Larger
workloads, memory/stack envelopes and declared deployment budgets remain open.

### Authenticated peer workload across macOS and Linux

The [private CLI launcher](docs/study-peer-soak.md) now checks 30 offline-edit/sync
episodes through the actual release binaries on macOS/APFS and Docker Linux/ext4.
Local revision replies and peer acknowledgment totals are checked separately.
An independent model predicts the counter, completed lessons and both resolved
fields; after convergence two further covering exchanges must preserve revisions.
Six paired post-acknowledgment process/container SIGKILL restarts reopen exact
values and revisions. Frozen app checkpoint ed8f9f9 finished at revision 65 on
both peers with 1,395 minutes, goal 230, limit 10 and lessons 2 through 61.

Independent review corrected stale-ack acceptance, partial-startup tracking and
cleanup handling before the final run. The final launcher passes under Python
optimization, has bounded subprocess probes, rejects existing/symlink outputs
before external work and verifies its UUID Docker resources are absent. Evidence
and synthetic events are retained locally without copying test keys into the
proof index. This does not qualify a wall-clock month, arbitrary later app edits,
the unexposed planned-topic field, device power loss or native Windows. The
subsequent handshake seam has its own directed and native test evidence.

### Native staging, maintenance exhaustion and resource qualification

The [CRDT migration tests](docs/crdt-policy-migration.md) now acquire both old
native writer leases and stage real files. Six process-kill boundaries reopen
at exact empty/converted/partially retained states, while complete old current
and retained records stay unchanged. Independent children verify every new file;
fresh-identity retries converge without repeat commits. macOS/Linux serial and
default debug/release runs pass after a child-output marker fix. Local leases
do not provide distributed fencing or an operational routing journal.

The [Linux native error drill](docs/native-fault-drills.md) extends actual bounded
tmpfs ENOSPC beyond append to backup, restore and in-place compaction. All four
failures report Io/Write/DiskFull errno 28; permission denial reports errno 13.
Sources and an existing valid backup retain their bytes, partial copies do not
verify, and freeing ballast permits fresh verified maintenance and reopening of
current revision 4/row 18 plus retained revision 2/row 17. tmpfs verifies native
error handling and process behavior, without claiming device durability.

The [resource envelope](docs/resource-envelope.md) separates four bounded axes:
64 retained depth-64 roots, actual near-exact 64 MiB requested-core saturation,
an explicitly synthetic exact image boundary, and naturally filled reachable
payload. The latter stores 72 distinct 896 KiB values (63 MiB), preserves eight
retained prefixes and rejects the next append at 67,014,656 bytes. Full byte
oracles survive warm reopening, backup, restore and compaction on macOS/APFS and
Docker Linux/overlayfs. Draft release recovers admission; pin release and encoder
cache reclamation across close/reopen are measured separately. RSS and the
requested 1 MiB worker-stack result remain workload-specific observations.

Finally, [coverage calibration](docs/coverage-calibration-96e105f.md) now separates
branch-complete from independence-complete inputs for a three-condition decision,
validates exact metric counts, and rejects unexpected source paths. Pinned
library condition instrumentation measured 953/1,133 branch outcomes over 29
files with 124 tests. Both tested compilers reject MC/DC instrumentation; the
tool reports unsupported rather than substituting condition or branch data.
These source-bound cases add evidence without closing native Windows, every
adversarial combination, deployment SLOs or hardware power-loss qualification.

### Frozen local qualification and corrected coverage attribution

Exact [21e5d26](docs/local-qualification-21e5d26.md) passed full macOS and Docker
Linux/ext4 checks, private relocated C/C++/Odin consumers where available, and
copied Rust-source consumers without the original checkout mounted. Independent
hash checks matched 652 Mac and 47 Linux package artifacts plus both 568-file
source manifests. The [extended ASan fuzz campaign](docs/fuzz-qualification-21e5d26.md)
completed all 22 targets with 1,431,321 actual executions; stable corpus replay
passed 25 tests. Bounds, compiler identity and source/seed/lock hashes remain
part of those finite results.

The coverage review found that LLVM can exit zero while rejecting incompatible
function/profile mappings. The driver now rejects warnings. Retained-object
replay and original logs invalidate the earlier 2490631 and 1ca5e5a workspace
aggregates; their raw totals remain preserved, explicitly unqualified. Exact-21
stable line and nightly condition aggregates also reject. The configured future
job explicitly selects the core engine library: its local 124-test line run
used one object with empty LLVM diagnostics. The
[separate-executable companion](docs/coverage-executables.md) preserves clean
parent-only views, empty scopes and unqualified child/build profiles. It does
not combine them into a project percentage. Compiler MC/DC remains unavailable;
manual decision pairs and mutations retain their own scope.

The [typed follow-up](docs/typed-decision-gaps-21e5d26.md) adds six public guard
controls for terminal Writes, duplicate primary IDs, absent deletes, unique
claim ownership, nonfresh initialization and format-marker admission. Eighteen
tests pass Mac/Linux debug/release, and six body-preserving mutations are caught.
Clean same-object condition reports observe both outcomes, including the same
callback instantiation for a poisoned Write. Current and retained primary/index
oracles and simulated image comparisons establish the effects independently of
branch counts. Production runtime source and the static inventory denominator
are unchanged.

The [compatibility runner](docs/storage-compatibility-21e5d26.md) now freezes both
requested commits with original locks, rejects dangling output symlinks and
checks image preservation even under Python -O. Both a7dc343 and 21e5d26 write
files read by both endpoints on Mac/Linux, preserving current revision 4 and
retained revision 2. Snapshot dcd2402 ↔ 21e5d26 passes native Mac loopback with
independently verified receipt digests, reopened retry acknowledgments and
byte-preserving unsupported poll rejection. This is separate from TLS upgrade
or broad schema compatibility qualification.

The coordinator's native capacity scenario fills all 1,024 receipt slots with
original commit revisions, exercises eight lost-reply/restart episodes, rejects
new requests at capacity and mismatched retries, and replays all receipts after
backup/restore and Mac/Linux compaction. No receipt is silently evicted. These
local tests leave native Windows/NTFS execution and broader deployment gates
open. Local commit and follower durable acknowledgments remain separate;
no protocol, publication barrier or engine runtime behavior changed in this batch.


[Fresh sanitizer checks](docs/sanitizer-qualification-21e5d26.md) also use the
unchanged exact-21e5d26 checkout. Mac workspace Rust ASan and TSan each passed
405 reported tests with rebuilt instrumented standard library. Calibrated shared
C/Rust ASan passed memory, thread and native-file consumers on both Mac and
Docker Linux, detecting both intentional C and Rust overflow controls on each.
Linux uses its recorded independent compiler/shared runtime and enables leak
detection; Mac does not claim LeakSanitizer. The Linux consumer profile does not
qualify a full Linux workspace TSan run. These finite diagnostic executions and
private artifact checks have distinct scope; none establishes native Windows or
physical-device power-loss behavior. The results, corrections and remaining
gates are local, with no push, hosted CI execution or publication.


### Production acceptance, package traces and operational progress

The [requirement audit](docs/production-acceptance-audit.md) maps the roadmap's
individual contracts to source-bound evidence and separates local engineering
from native platform/device and owner decisions. The following tranche adds
qualification tools and regression controls; engine/application runtime,
publication barriers, byte/wire formats and the 2,561-record static source
inventory are unchanged.

Four [macro expansion guards](docs/macro-expansion-guards-032dadf.md) now have
direct same-process accepted/rejected controls, warning-free condition outcomes
and detected guard mutations. They complement existing downstream compiler
rejections. They do not turn proc-macro child execution or generated code into
a global MC/DC measurement.

The [serialization scaling experiment](docs/serialization-scale.md) independently
verifies complete current and retained bytes while measuring the existing
VALUE/DATA lookup predicates in disposable instrumented copies. Distinct breadth
128→512 multiplies VALUE comparisons by approximately sixteen; shared identities
multiply them by approximately four. DATA counts remain 516 for full eight-epoch
histories. The 2,048-distinct case admits seven epochs, rejects the eighth with
unchanged state/file length, then completes all maintenance. The physical minimum
for its next values exceeds remaining headroom. Actual clean operation timings
and whole-lifecycle RSS are recorded separately from instrumentation; no index
or claimed speedup was introduced. Any optimization must preserve emission order,
fixed-identity bytes, allocation admission and failure atomicity.

[Optimized artifact consumers](docs/artifact-trace.md) now interpret the same
versioned binary trace against an independent contract model: native Rust/C/Odin
on Mac and Rust/C on private Docker ext4. Every step compares actual statuses,
publication revision, complete nested map/list/binary contents, current and held
historical rows and retained IDs. Independent existing-record commits merge;
stale point/scan/absence observations conflict and become terminal. Retained
views survive reopen and remain alive after durable ID release. Root review
strengthened no-overwrite admission, complete unchanged-buffer checks, reported
length checks and valid-live-handle/nonzero error-output probes. The packages
remain exact-21e5d26 artifacts/source payload, with separately stated feature
profiles and no Windows or Linux Odin extension of evidence.

The [private progress helper](docs/replica-progress.md) provides schema-v1 JSON
around the existing snapshot CLI. A local inspected/captured revision and digest
are distinct from a newly authenticated follower durable receipt. Same revision
requires the same complete export digest; a newer receipt is labeled newer,
without asserting that old retained roots remain selected. Failed exchanges keep
remote progress unknown/unavailable; valid saved observations stay historical.
Source identity, monotonic comparisons, strict bounded parsing and child cleanup
have regression controls, including real lost-response/restart/refusal cases on
Mac/Linux. The helper computes no commit-time lag or durability guarantee from
its wall-clock timestamp. Core local commits still perform no network exchange.

The unchanged native lifecycle driver passed [1,800-second Mac and Linux runs](docs/soak.md#extended-macos-and-linux-controls-at-7a9e661),
with 124,224 and 676,992 writer commits respectively plus concurrent readers,
retained roots and verified maintenance each round. The corrected observer checks
exact revision/retention/progress arithmetic and source/binary stability. This is
longer finite evidence for four small records, not production-scale longevity.
Finally, the [minimum/compiler/dependency review](docs/local-baseline-ffdf506.md)
passes complete Mac Rust/C/Odin checks at Rust 1.99.0 and clean advisory scans
against an immutable 1,490-advisory snapshot. Actual quota tests could not start
on the available kernel and remain unqualified. Native Windows, physical device
failure, final candidate consolidation, remaining feasible decision pairs and
declared service/support budgets remain open; credential cutover follows below.

### Credential cutover, retained-history work and compiler qualification

The existing fixed-pin transport supports an operator-controlled stopped-endpoint
credential change. New native process tests qualify the workflow separately for
[snapshot replication](docs/snapshot-credential-cutover.md) and
[StudyProgress peers](docs/study-credential-cutover.md) on Mac/Linux. They change
no application runtime, enrollment, actor roster, wire format or publication
barrier. Configuration replacement occurs while endpoints are stopped; there is
no live reload, transactional credential-file update or automatic revocation.

The snapshot case verifies complete current/retained rows and independently
hashes exports. Source revision 4 can be locally committed/captured while the
follower still durably reports revision 3. Reachable old-leaf/wrong-pin failures,
including a rejected upload, never produce a follower receipt or change its
selected image/manifest. Killing the newly configured endpoint before exchange
preserves revision 3; a valid retry installs 4. Repeated/restarted polling leaves
receipt, image and manifest unchanged. Review also fixed a test cleanup guard
that could acquire ownership before directory creation succeeded; a sentinel
regression proves failed admission preserves existing contents.

The study case preserves acknowledged offline edits across mixed old/new
credentials and a killed process. Coordinated new credentials converge to
53 minutes, goal 80, limit 20 and lessons `[1,2,3,4]`; complete causal records and
both retained historical records agree with their oracles. Stale configuration
is rejected by live fresh endpoints, and repeat covering exchanges add no
commits. This combined configuration test is not independent MC/DC evidence for
each TLS condition or a qualification of actor-generation migration.

The [history-axis experiment](docs/serialization-history-scale.md) now measures
1/8/32/64 retained roots with complete state and maintenance checks on Mac/Linux.
DATA comparisons are 12/516/16,132/108,036. Accumulated work across growing
history is cubic; each lookup remains linear. Distinct VALUE searches retain
their separate quadratic cost. No index or claimed speedup was introduced.
Review fixed owned-volume cleanup and preserved Mise's Rustup selector in both
qualification launchers. Fresh history and packaged trace runs bind those fixes.

The [Linux minimum-compiler check](docs/local-toolchains-a56083c.md) passes the
full existing Rust/C checker at Rust 1.99.0 against exact a56083c source, with
unchanged locks and explicit ext4 execution. It predates these new test overlays.
A newer pinned nightly, 2026-11-12, still rejects MC/DC instrumentation before
fixture compilation; no coverage percentage follows from that probe. Native
Windows/device/quota evidence, remaining feasible decision pairs, additional
promised binary transitions, final candidate consolidation and owner budgets remain open.

### Actual CLI upgrades and delivered error recovery

The [snapshot CLI upgrade gate](docs/snapshot-cli-upgrade.md) builds unmodified
315068f and 365daf6 release binaries from separate archives and original locks.
Mac/Linux execute both sender directions, persisted follower reopening and
failed-transfer/retry controls. Complete selected image bytes, independent
export digests, receipts and retained rows agree. A local capture at revision 4
does not become a follower acknowledgment while that follower remains at 3.
Both versions use the same snapshot ALPN; this pair supplies no ALPN-mismatch
result or compatibility promise for newer maintenance commands.

The separate [study binary-upgrade gate](docs/study-cli-upgrade.md) uses the
actual pre-ALPN fb80989 and current 365daf6 binaries with unchanged credentials,
model, policy and roster. Mixed versions reject exchange while retaining each
acknowledged offline edit. Coordinated new peers reopen the same files and
converge complete causal records to 36 minutes, goal 80, limit 20 and lessons
`[1,2,3,4]`. Both original retained records survive; restart and repeat exchange
add no commits. This is an availability boundary, not a schema/actor migration.

The [v2 delivered trace](docs/artifact-trace-validation-modes.md) now explicitly
compares Records and WholeDatabase validation in 134 cells through optimized
Mac Rust/C/Odin and Linux Rust/C. Independent-record edits, empty transactions
and retain/release publications distinguish the two modes. Full current and
historical values, terminal states and exact revisions match the independent
model. V1 bytes and its 59-line oracle remain unchanged; malformed v2 modes and
unknown versions reject before database creation. Fourteen unit controls and
thirty additional executions bind the five actual consumer binaries. These
still use the exact private 21e5d26 packages, not a new release candidate.

The [delivered C fault drill](docs/delivered-c-faults.md) additionally checks the
packaged Linux library's IO/OutcomeUnknown outputs, terminal transactions and
recovery-required writes. A fresh process without the injector verifies every
current/retained byte and reconciles an atomic request marker before retry.
Early append/root-write failures reopen revision 3; root/witness-sync errors
reopen 4 in this running-kernel experiment. All retries finish at 4 with old
retained revision 2. Interposed EIO is separate from actual kernel ENOSPC on a
bounded private tmpfs, and neither establishes device power-loss behavior.

Independent review also found assurance-tool defects: an exited leader could
leave a running descendant, one controller trusted copied artifacts without
rehashing/cross-binding sources, and a wrapper could skip volume cleanup after
container removal failed. Directed negative controls catch these
paths; fixed helpers and fresh Mac/Linux runs preserve the prior evidence
without relabeling it. Engine/application runtime, durability barriers and
separate follower acknowledgments are unchanged. Remaining platform/device,
feasible-decision, promised-transition and final-candidate gates stay explicit.

### Hosted platform checks resumed

After the repository became public, the native Windows job for `4370b46`
reached all-target Clippy and found that the Linux tmpfs fault example compiled
its Linux-only in-place compaction calls on Windows. The example now gates its
complete drill to Linux and exits with an explicit unsupported-platform message
elsewhere. This preserves the existing Linux runner contract; Windows compact
copy and the platform-specific replacement API are unchanged. Targeted Clippy
checks pass for macOS, Windows MSVC and Linux GNU targets locally; the macOS
stub exits with status 1 as intended. Those cross-checks supply compile evidence;
the subsequent hosted gate supplies actual native Windows execution.

### Continuous helper controls

The [helper/oracle runner](docs/helper-controls.md) now covers all nineteen
registered Python suites in isolated normal and `-O` workers, with exact test-ID
accounting, explicit optional skips, input-stability checks and bounded output
and runtime. Review found and fixed detached-child and orphaned-grandchild
cleanup gaps; actual process controls verify them, including failed inspection
and malformed journals. This trusted-suite watchdog is not a hostile process
sandbox.

Frozen Mac arm64 and Docker Linux aarch64/ext4 runs each pass 38 workers,
268 tests and four registered skips (272 reported). Copied Linux evidence is
rehashed and its owned container/volume are confirmed absent. The workflow
schedules the same command with Mise-pinned Python 3.14.8 on Mac/Linux; its
separately recorded hosted executions now pass. Native Windows retains the existing portable
helper checks. No engine/application runtime, durability barrier or follower
acknowledgment changed, and no Cargo publication occurred.

### Hosted native qualification

The [cd7cdfe hosted gate](docs/hosted-qualification-cd7cdfe.md) now passes the
native Mac arm64, Linux x86_64 and Windows MSVC x86_64 jobs. Debug/release Rust,
shared-library consumers, compiled Windows C/C++ clients and combined-feature
package relocation all execute. Downloaded package digests and all payloads are
verified against their manifests; source checks account explicitly for Windows
checkout line endings. Odin and packaged Rust source consumers keep their
separate earlier qualification scope.

Hosted helper controls, all 22 bounded fuzz targets, calibrated Linux C/Rust
ASan, atomic models, Rust ASan and the complete PR TSan workspace run pass.
Core-library line/region coverage has clean LLVM diagnostics; branch and MC/DC
remain unmeasured in that report. A duplicate TSan run exhausted its 20-minute
budget near the final receipt test, while another completed the unchanged
workload. The workflow now allows TSan 40 minutes, emits uncaptured capacity
progress and avoids duplicate feature-branch engine matrices. No workloads or
assertions were removed. A runner-capacity cancellation is tracked separately
and retried rather than treated as executed sanitizer evidence.

These results close the previously unexecuted native Windows CI profile, not
physical device/quota, all feature/ABI combinations, remaining decision pairs or
final-candidate/support acceptance. Durability barriers, engine/application
runtime and separate follower durable acknowledgments remain unchanged; crates
remain unpublished.

### Native Linux quota qualification

The [quota gate](docs/native-quota.md) adds a separate private ext4 loop-file
controller, capability calibration and source-bound Rust fixture. It requires
real Linux `EDQUOT`, complete current/retained data checks, terminal and
recovery-required behavior, a fresh-process reopen while full, and a successful
retry after releasing owned ballast. Privilege dropping, exact mount/backing
identity and independently verified cleanup are part of qualification.

Twelve controller controls pass normally and under `-O`; the complete helper
entry point now passes forty workers with 292 passes and four registered skips.
Linux Docker semantic controls verify complete reopen/retry data and reject a
false quota pass when space is available. The hosted Ubuntu 24.04/ext4 job now
observes actual errno 122 with over 47 MiB still available globally. Commit
failure preserves current revision 3 and retained revision 2; a fresh process
reopens that state while full. Freeing ballast allows one retry to 4, and another
fresh process verifies complete current and retained rows. Both owned mount and
loop associations are absent after cleanup. Downloaded proof, source/tool hashes
and full row oracles are independently rechecked. This qualifies native append
EDQUOT/recovery for that profile, not maintenance quota failures or physical
power loss. No engine runtime, durability barrier or follower acknowledgment
semantics changed; Cargo publication remains disabled.

### Quota integration CI completed

The [completed integration checkpoint](docs/roadmap-status.md#completed-quota-integration-checkpoint)
at PR head `e20cece` passes all ten engine jobs and bounded fuzzing. Native
Mac/Linux/Windows, compiled Windows C/C++ consumers, both helper platforms,
Linux quota recovery, Rust/C ASan, Rust TSan, atomic models and library coverage
all complete successfully. Fuzz seed/model replay and all manifest targets also
pass. This closes the pending CI integration work; the detailed quota and
package artifact audits keep their original source scopes. Green CI does not
close remaining MC/DC, physical-device, broader-profile or final release gates.

### Windows delivered C/C++ trace handoff

The [Windows handoff](docs/windows-artifact-trace.md) prepares a native x64 MSVC
runner around the unchanged common C consumer and independent 134-cell oracle.
It compiles both optimized C11 and C++17 with assertions enabled, links only the
delivered import library/header, and checks exact transcripts on local fixed
NTFS. Process-local before/after loader proofs bind the actual DLL path, CNG
SHA-256, source, ABI, format, feature bits and PID to a manifest-pinned package.
Compatibility/input rejection and deliberately wrong hash/path/source controls
run for each language. Owned Windows jobs bound process trees and output.

Twelve portable controls pass normally and under `-O`; the helper registry now
has 42 workers with 316 passes and four registered skips. Windows GNU C/C++
cross-compilation and admission of an existing audited Windows package pass.
These are preparation results. Native MSVC builds and DLL execution remain
pending the Windows agent, so F15's full Windows trace is not yet qualified.
The handoff includes pinned artifact identities, commands and acceptance criteria.
No engine runtime or publication semantics changed; no crates are published.

### Native Windows delivered trace qualification

The [source-bound native results](docs/windows-artifact-trace.md#native-results-2026-11-13)
now pass all twenty executions from clean harness `80eb0c2` against the unchanged
reference DLL source `7a490e5ce24910d5d8ec96abe0fb9d218ea9e870`. Native Windows
11 Pro build 26300 on fixed local NTFS used x64 MSVC toolset 14.51.36231,
compiler 19.51.36247.0, SDK 10.0.26100.0 and Mise Python 3.14.8. Separate
optimized C11/C++17 builds retain assertions and use the delivered header/import
library. Five exact-oracle successes and five expected no-database rejections
pass per language; all twelve controls also pass normally and under `-O`.

Before/after process-local proofs bind relocated DLL path/hash, actual source,
ABI 1, format 2, features 3 and PID. All 23 owned jobs reach zero active
processes; package and harness hashes remain stable. Local evidence is at
`C:\Users\21asg\AppData\Local\weekend-trace-80eb0c2-20261113\result-initial`,
with `evidence.json` SHA-256
`ceab27169583071ffa533a4987b6e54af8d47b6898ec7c4d148469cea46f175e`.
The linked report records full package pins, results, audit and limitations.
No native fixes were necessary. This closes the named F15 Windows finite trace
gate; broader API/MC/DC, failure injection, device durability, other platform
profiles, hosted evidence retention and final release gates remain separate.

The owner subsequently cleaned the Windows worktree. That twenty-case pass
remains a historical result; its raw proof ZIP is unavailable here. Paths and
hashes in the report identify the original run, not currently retained evidence.
Repeat and retain the native gate for the final immutable candidate.

### Charged serialization VALUE identity index

The [index qualification](docs/serialization-identity-index.md) replaces encoder
VALUE linear search with a temporary half-full, power-of-two hash index of
cache positions. Mixed allocation addresses select buckets; `same_node`
confirms identity. The cache retains the Value owners, first-match semantics
and emission order. Integer addresses are never converted back to pointers.
Equal-content distinct nodes remain separate objects. Every encode rebuilds
the index from its existing cache, including recovery-built caches, and drops
it before publication. The disk representation, public API and format versions
are unchanged.

All table allocations are charged and fallible through `Memory::reserve`.
The existing 16,384-object bound limits the 64-bit table to 262,144 bytes;
largest old/new growth overlap is 393,216 bytes. Preparation may hit an existing
tight budget earlier. Allocation failure abandons the candidate before I/O,
preserving prior acknowledged state and normal terminal/retry semantics.
Publication barriers and separate follower durability acknowledgments remain
unchanged. Expected lookup work is constant; collision worst cases remain
bounded linear probes. DATA revision search and cache-vector growth remain
separate scaling work.

Ten fixed-identity lifecycle images match the original linear encoder exactly
on Mac and Linux, including retained/recovered cache reuse and maintenance.
Both scaling axes preserve complete independently checked current/retained
state, file sizes and physical-cap outcomes. The largest distinct workload
drops from 440,169,243 to 147,132 identity comparisons, including rebuild/growth;
DATA counts are unchanged. Debug/release allocation, budget, crash/recovery,
retention and compaction tests pass on Mac and Docker Linux ext4; collision and
budget controls pass pinned Miri. The report records timings, source hashes,
evidence and limitations. This runtime change needs fresh Windows/final-candidate
qualification; the earlier delivered DLL's pass remains source-bound. Cargo
publication remains disabled.
