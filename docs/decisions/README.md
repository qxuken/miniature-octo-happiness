# Source audit index

These ledgers classify explicit source records, including overlapping logical
syntax and propagation sites. Counts are not independent MC/DC conditions or
compiler-measured branches. Each ledger binds source/test hashes and separates
predicate pairs, execution evidence, coupled syntax, invariants and delegation.
Propagation evidence does not imply every possible callee error was injected.

| Module | Records | Ledger |
|---|---:|---|
| Engine | 141 | [engine](engine.md) |
| Raw schema | 176 | [schema](schema.md) |
| Tree | 69 | [tree](tree.md) |
| Value | 79 | [value](value.md) |
| Storage core | 57 | [storage](storage-core.md) |
| Replacement | 2 | [replacement](replacement.md) |
| Simulator | 60 | [simulator](simulator.md) |
| Memory | 37 | [memory](memory.md) |
| FFI source | 20 | [FFI and generated exports](ffi.md) |
| Durable codec | 253 | [codec](codec.md) |
| Native adapters | 180 | [native storage](native-storage.md) |
| Diagnostics | 20 | [diagnostics](diagnostics.md) |
| Proc macros | 108 | [macros and generated code](macros.md) |
| CRDT | 121 | [CRDT](crdt.md) |
| Inspection/logical/salvage | 186 | [operations](operations.md) |
| Snapshot replication | 140 | [replication](replication.md) |
| Authenticated snapshot CLI | 125 | [snapshot CLI](snapshot-cli.md) |
| Typed runtime | 315 | [typed runtime](typed-runtime.md) |
| Distributed apps | 205 | [distributed apps](distributed-apps.md) |
| CLI/measurement/fault drivers | 246 | [command line](command-line.md) |

All **2,540 explicit source records** have manual dispositions: the 2,415-record
audit checkpoint plus 125 authenticated snapshot CLI records. This includes
delegated obligations, execution-only evidence, invariants and coupled syntax;
it does not mean every condition has an independence pair. The inventory still
reports 314 unexpanded macro/derive sites across 50 files. FFI templates and generated Model,
Record and Resolution behavior have supplemental mappings in their ledgers;
these are not a complete compiler-expanded denominator.

[Memory admission pairs](../memory-budget-decisions.md) and the
[manual ledger](../../DESIGN.md#mcdc-decision-ledger) retain separate scopes.
Reconcile spans/hashes after edits. The [inventory](../decision-inventory.md)
tracks configuration union, macro blind spots and residual source obligations.

The [snapshot CLI audit](snapshot-cli.md) retains the original decisions from
315068f, adds ten offline-maintenance wrapper records, and includes real chain-trusted/leaf-pin-mismatched TLS controls on both
endpoints. Pending-write-only/client-partial-handshake independence and delegated
OS/dependency errors remain explicit residuals. Fourteen focused app tests passed
macOS debug/release and Linux debug; this does not extend platform results to
unexecuted configurations or claim measured whole-app MC/DC.

Current source-set SHA-256: `133bdb4a94e46015da5aac537daaf358a28fb9b862d3a1bcea5319fb6da8b8d7`.
