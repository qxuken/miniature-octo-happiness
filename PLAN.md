# weekend-db: agreed project scope

A portable embedded database built by the user over successive weekends, roughly
6–8 hours at a time. This document captures product decisions; [ROADMAP.md](ROADMAP.md)
defines learning milestones and implementation experiments. Working name:
`weekend-db`. The xitdb checkout remains a separate reference project.

Read [DESIGN.md](DESIGN.md) for the complete target design in one document,
including internal sketches, invariants, algorithms, and references.

## Shape and distribution

- Implement in portable C; Rust is the first client language. C++ remains a
  fallback only if a concrete safety benefit justifies changing this contract.
- Develop in small source files and generate a single C implementation header.
  Keep C tests and examples separate. Prove inclusion/linking from multiple
  translation units and consumption through a C ABI before building the engine.
- Target 64-bit macOS, Linux, and Windows. Start with macOS; distinguish intended
  platforms from tested ones. Do not assume access to every platform.
- Expose a small core interface. Keep application-specific access, migration,
  and eventual code generation outside the storage implementation.

## Values and snapshots

- Support persistent nested maps and lists, scalar values, and byte values from
  the first usable in-memory collection milestone.
- Cloning and editing values preserves prior snapshots through structural
  sharing. Collection operations must not expose mutable aliases that alter
  another record or historical version.
- Support memory and file storage through the same core interface, memory first.
- Always keep the current committed state. Clients explicitly retain/release
  historical snapshots by ID; retained snapshots survive reopening.
- Live handles and durable retention have distinct lifetimes. Later reclamation
  must protect retained roots and state still needed by active handles.

## Transaction model: conservative record-scoped merging

Q23 accepted the smaller model in
[ADR 0007](docs/adr/0007-merge-stable-record-assignments.md), replacing the earlier
field-level/list-operation-rebasing direction.

- A record is a stable outer key and an immutable value, which can contain nested
  maps and lists. Schemas are optional; record identity is not a memory address,
  tree-page position, or list index.
- Each transaction reads a base snapshot and builds a private overlay of final
  `put(key, value)` and `delete(key)` assignments. Reads see its own writes.
- Track observations and write targets using generations independent of value
  equality. An intervening write must be detectable even if a value changes back.
- Under one commit gate, validate dependencies and write targets, apply the
  assignments to the current root, and publish atomically. Never replace the
  current root with an entire stale private root.
- Independent existing records can merge. Two edits inside one record conflict,
  even if they affect different nested fields or list positions. A conflict
  publishes none of the rejected transaction's changes.
- Nested reads depend on their containing record. For missing keys and supported
  scans, initially validate a containing collection generation. This safely
  permits false conflicts, including unrelated inserts into one collection.
- All data-dependent transaction reads must use the tracked transaction
  interface. Introducing an observation such as a scan requires a validation
  rule before claiming its isolation guarantees.
- Prove a whole-root revision-check baseline first. It rejects any intervening
  commit and does not itself satisfy the independent-record merge goal. Refine
  it to record-scoped validation as a separate milestone.
- First tests interleave transaction handles on one thread. Actual thread
  overlap follows later. Publication remains coordinated.
- Transactions are live and process-local, not restartable jobs or writable
  branches from arbitrary retained history. Bound transaction resources and
  report explicit errors rather than retaining unbounded state.
- Automatic list positional rebasing, its operation log, and CRDT semantics are
  optional later experiments. Maps and lists remain core value types.

## Durability and evolution

- A successful file-backed durable commit survives process/OS crashes and power
  loss under documented filesystem/device synchronization assumptions. A future
  weaker mode must expose a distinct explicit contract.
- A failure or lost acknowledgment during commit may leave the caller uncertain
  whether the complete commit happened. Recovery/error handling must account
  for this; an error is not automatically proof of rollback.
- Include a file-format version immediately. Experimental formats may break;
  reject unsupported versions instead of silently interpreting them. Engine
  format converters and a stable-format promise come later.
- Use a storage failure simulator to establish the commit/recovery contract
  before claiming actual file durability. Check encoding integrity, bounds,
  malformed inputs, allocation failures, and errors during recovery.

## Schemas, lookup, and migration

- Store optional immutable versioned schema descriptors in an ordinary catalog.
  Use stable type/field identities; do not use native struct layout as storage.
- The core validates representation integrity. Wrappers or application code
  validate record schemas. Raw collection writes can bypass schema checks.
- Applications supply programmatic record migrations through ordinary
  transactions. Schema metadata/data updates must publish consistently, while
  retained snapshots keep access to their original descriptors.
- Typed accessors can use field IDs and construct lookup keys. This does not
  change the initial record conflict unit into individual fields.
- For lookup by a non-key field, an optional secondary map can be maintained in
  the same transaction as its primary record. Metadata alone does not create an
  index. Start with one handwritten example before generating maintenance code.
- Raw writes bypassing those wrappers cannot be promised index consistency,
  record validity, or uniqueness enforcement by the core. Ordered range lookup
  is a separate capability from exact lookup in a hash map.

## First acceptance scenarios

1. Create nested records containing both maps and lists; mutate cloned values;
   earlier snapshots remain unchanged.
2. Start two transactions at the same base with existing records `a` and `b`.
   A changes `a`, B changes `b`; commit in either order and preserve both writes.
3. Both transactions change `a`, including changes to different fields/lists
   inside `a`: the second conflicts without partial publication.
4. A reads `a` to calculate `b`; B changes `a` and commits first: A conflicts.
5. Missing-key reads, deletion/recreation, equal-value/ABA writes, and supported
   scans are validated. Document safe false conflicts explicitly.
6. After commit, retain an old snapshot, close/reopen, and read old/current data.
7. Simulated failures preserve every acknowledged durable commit under the
   declared model; incomplete commits never appear partially applied.
8. Migrate a typed record in application code; retained historical data and its
   schema descriptor remain readable. Keep an example secondary lookup atomic.

## Explicit implementation exercises

The roadmap deliberately leaves tree layout, allocator strategy, C language
baseline, handle/slice lifetimes, scalar subset, key encoding, checksums,
persistence protocol, test tooling, and exact resource limits as milestone
exercises. Specify and test their contracts before implementing them. These are
not silently fixed by a sketch or inherited wholesale from xitdb.

Performance goals follow representative measurements with matching durability
and history policies. Completion of a learning milestone is not a production
safety certification. Source research and earlier alternatives are retained in
[the transaction review](docs/design/transaction-model-research.md).
