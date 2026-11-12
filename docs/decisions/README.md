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
| Typed runtime | 315 | [typed runtime](typed-runtime.md) |
| Distributed apps | 205 | [distributed apps](distributed-apps.md) |
| CLI/measurement/fault drivers | 246 | [command line](command-line.md) |

All **2,415 explicit source records** have manual dispositions. This includes
delegated obligations, execution-only evidence, invariants and coupled syntax;
it does not mean every condition has an independence pair. The inventory still
reports 284 unexpanded macro/derive sites. FFI templates and generated Model,
Record and Resolution behavior have supplemental mappings in their ledgers;
these are not a complete compiler-expanded denominator.

[Memory admission pairs](../memory-budget-decisions.md) and the
[manual ledger](../../DESIGN.md#mcdc-decision-ledger) retain separate scopes.
Reconcile spans/hashes after edits. The [inventory](../decision-inventory.md)
tracks configuration union, macro blind spots and residual source obligations.
