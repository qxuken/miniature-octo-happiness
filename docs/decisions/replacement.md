# Namespace replacement source decision audit

`src/storage/replacement.rs` has **two production AST records**: one fixed
six-step iteration and one `?` propagation. This is the whole source inventory for
this orchestration module, not a claim that exchange/fsync/unlink implementations
or filesystem behavior have only two decisions.

Existing `storage::replacement::tests::namespace_publication_stops_at_each_failed_barrier`
is sufficient: success visits Exchange, SyncDirectory, SyncReplacement,
RemoveOld, SyncCleanupDirectory, SyncCleanupFile in that order. Injecting failure
at each of the six steps returns Io and visits exactly that prefix; no later step
is invoked. The loop's range is fixed, so zero iterations is infeasible after
entry; an error can stop at any step. These are iteration/error controls, not a
fabricated compound Boolean MC/DC table. No duplicate new test was added.

`power_loss_at_every_namespace_boundary_keeps_a_complete_canonical_store`
checks durable/flushed/volatile namespace survival at twelve before/after
boundaries and successful completion. Whichever canonical image survives has
complete current and retained values. This is an explicit namespace model;
atomic exchange and ordered namespace persistence remain adapter/filesystem
assumptions. Native compaction fault/process tests and deployed filesystem
qualification provide separate evidence, not implied coverage of native code.

Status E below means scoped execution/propagation evidence. Macro/trait-generated
or platform code is outside this module inventory. The existing
[REPLACE-V1 ledger](../../DESIGN.md#replace-v1-ordered-namespace-error-propagation)
is reused without expanding its assurance claim.

## Source records

Snapshot SHA-256: `76e5098896ecc0ede681526cbb8ba75fbe40b833e528eef95d748715de84cd07`.
Production record identities below are line:column plus the first 12 hex digits
of the inventory source-token hash; test-only additions are excluded by the tool.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 14:5 | `for_iteration` / `publish` | E | Six barrier-prefix controls / namespace survival | `10c22bcf4c76` |
| 22:20 | `try_propagation` / `publish` | E | Six barrier-prefix controls / namespace survival | `8a8de823d5ed` |

Disposition totals: E=2; **2 records**.

## Executed validation

On the local macOS host, all-feature debug and release runs passed: 38 storage
unit tests, 8 `storage_decisions`, 3 `bytestore`, 14 `durability`, 3 `headroom`,
and 10 `compaction` tests (76 tests per profile). Strict Clippy for the library
and new integration target passed. The source inventory completed with zero
parse failures. This run does not add Windows, Linux, 32-bit, or physical
power-loss evidence; it does not claim mutation or compiler MC/DC measurement.
