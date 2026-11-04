# List rebasing and record schemas

The list-rebasing branch below is historical and deferred from the initial
roadmap by [ADR 0007](../adr/0007-merge-stable-record-assignments.md). The initial
transaction changeset contains final assignments at stable record keys; edits
within one record/list conflict instead of being positionally rebased. Optional
versioned schemas with stable field IDs, app-side migrations, and wrapper/app
schema validation remain accepted. Schema compatibility details below are
proposals unless noted otherwise.

## Deferred experiment: rebase list operations using revision history

The desired behavior is more permissive than whole-list conflicts:

```text
Base: [A, B]
T1: insert X at index 0
T2: replace the element at index 1 with C
T1 commits: [X, A, B]
T2 rebases index 1 to index 2 and commits: [X, A, C]
```

Retained roots make base and current state available for comparison. They do
not necessarily preserve editing intent. With base `[A, A]` and current `[A]`,
the value states alone cannot distinguish deleting the first occurrence from
deleting the second. A diff can select an edit script, but that script is not
necessarily the edit the other transaction actually performed.

Historical approach, to revisit only if list rebasing becomes a later project:

- Capture explicit insert, remove, and replace operations in each changeset.
- Give committed changes a revision and retain enough logical operation
  history to rebase every active transaction from its base revision.
- Transform pending positions through intervening list operations, respecting
  the transaction's own earlier operations as well.
- Define tie-breaking for simultaneous insertions at one gap and conflicts for
  replacement/removal of the same target before implementing the merge.
  Accepted initial policy: same-gap insertions follow commit order; competing
  changes to the same existing target conflict.
- Keep the earlier point-read validation rule. A list rebase does not excuse
  invalidated observations. Define element observations separately from
  observations of list length, order, and membership.
- Pin required base snapshots and operation history while transactions remain
  active. Retaining a snapshot for reading need not imply retaining an unlimited
  operation log; writes based on arbitrary old snapshots require a separate
  policy.

Historical lifetime/resource scope: live process-local transactions only, with a
configurable history budget and explicit too-old/limit error. Transactions do
not survive restart. Moves, sorting, and range splices are later extensions.

This is local operation rebasing, not a claim to implement a distributed CRDT.
A logical operation history is also not automatically a write-ahead log: whether
it must survive restart depends on which recovery and transaction-lifetime
contracts we choose.

An alternative is stable list-element identities. Automerge demonstrates this
approach: index-based calls resolve to element IDs and operations target those
IDs. See its [merge rules](https://automerge.org/docs/reference/under-the-hood/merge-rules/).
Position transformation and stable element IDs are alternatives for a future
list-merging experiment. Neither is required by the current record-scoped model.

## Separate three migration concerns

| Concern | Example | Proposed treatment |
| --- | --- | --- |
| Engine file format | Node encoding or offset representation changes | Explicit format version; reject unsupported experimental formats; converters later |
| Record schema | Add or rename a field | Versioned schema descriptors and explicit compatibility rules |
| Application meaning | Split a name, change units, or reinterpret a value | Explicit transformation supplied by the application |

Native struct layout is not a portable stored schema. A typed wrapper can
instead encode individual fields using stable logical identities, independent
of their in-memory offsets or declaration order.

Proposed catalog, using the engine's existing map facilities:

```text
schema_catalog[(type_id, schema_version)] -> immutable field descriptors
record -> type_id, schema_version, fields[field_id -> value]
```

This is a logical organization, not a decision to build a second physical hash
table implementation. A schema fingerprint could help identify a descriptor,
but hashing a field name does not preserve its identity across a rename, and a
hash alone is not a migration recipe. Stable IDs must not change with source
declaration order or be silently reused for new meanings.

Examples to settle before generating wrappers:

- Adding an optional field: whether absence remains explicit or an accessor
  supplies a schema-specific default.
- Renaming a field: retain its field ID and change its descriptive name.
- Changing field meaning or type: explicit conversion or a new field identity.
- Reading and updating records with unknown fields: whether wrappers preserve
  fields they do not understand instead of replacing the entire record.
- Reading retained history: the old record's schema descriptor must remain
  resolvable; replacing a global latest schema must not silently reinterpret
  historical data.

Schema descriptors should be immutable per version and reachable from the
database snapshot that uses them. Publishing schema/data changes should use the
same transaction mechanism. Ordinary maps and lists remain usable without a
registered schema; typed-record validation is provided by wrappers or explicit
application code, not mandatory core enforcement.

Record migrations are supplied programmatically by the application. Generated
wrappers can check field types and expose field accessors. The initial core
receives final record assignments; stable field IDs do not imply field-level
concurrent merging within a record.
They cannot infer semantic transformations such as converting cents into a
different unit or splitting one field into several. Migration timing (eager,
on-read, or explicit views) and treatment of retained historical roots remain
open.

Protocol Buffers provides useful precedent for stable field numbers and never
reusing removed numbers; this is inspiration, not a dependency or a proposal to
use its encoding. See its [schema evolution guide](https://protobuf.dev/programming-guides/proto3/#updating).

## Schemas and lookup

Schema metadata can guide generated field accessors and stable key construction.
It does not itself accelerate searching records by a non-key field. A separate
secondary map such as `users_by_email[email] -> user_id` can provide an exact
lookup; its entries must be updated atomically with primary records and retained
in the same snapshot. A schema declaration can describe an index, but building,
maintaining, and migrating that index remain actual work, not a free consequence
of storing the descriptor.

Initially implement one such map explicitly in application/wrapper code if it
is useful. Generate the maintenance code only after that contract is understood.
Raw writes bypassing wrapper validation/index maintenance cannot be promised
schema validity or uniqueness constraints by the core. Ordered range lookup
would additionally need an ordered storage structure; it is not supplied by a
hash-based lookup map.
