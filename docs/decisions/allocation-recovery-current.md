# Selected allocator refusal in durable preparation and recovery

This R1 batch starts at merged main `479ccef`. The sweeps run through
`DatabaseOptions::with_allocator` and the public database operations. The
shared test allocator retains a one-shot mode for layout and lifetime tests.
Its persistent mode counts each allocation and growth request from the
selected provider, refuses every request after the allowed prefix, and
checks that the operation reports `OutOfMemory` if it reaches a refusal.

The first in-memory commit check failed with the original hook armed at
zero. The engine returned `OutOfMemory` before the selected allocator saw a
request. The migrated test checks the real allocator boundary and restores
the exact charged bytes and outstanding provider bytes after the draft drops.

| Control | Successful path | Refused path |
|---|---|---|
| ALLOC-BUDGET-BOOTSTRAP | Cold open creates budget state and verifies the current and retained roots | Zero permitted requests reject the budget allocation before backend stabilization. The existing image reopens and the provider has no leaked block. |
| ALLOC-RETAIN-FORK | A new retained ID persists without advancing the data revision | The retained catalog, live charge and image stay at the previous state and reopen there. |
| ALLOC-RELEASE-FORK | The retained ID is absent after reopen. Current data and its revision are preserved. | The original retained ID and contents reopen, with the same image and live charge as before the attempt. |
| ALLOC-COMMIT-PREPARATION | A nested edit reaches the same revision and byte image as the default allocator. Older retained rows remain readable. | The image, current revision, retained rows and both memory counters stay at the prior state after the draft drops. No write or sync runs. |
| ALLOC-REOPEN-VALUE-INDEX | Cold open rebuilds the VALUE index before exposing the verified graph | The caller gets `OutOfMemory`, the partial graph is freed and the lease can be acquired again. No stabilization write or sync runs. |
| ALLOC-REOPEN-DATA-INDEX | Cold open rebuilds the DATA index before exposing the verified graph | The caller gets the same error and cleanup guarantees with VALUE scratch already allocated. |

Each named propagation control changes only `OutOfMemory` to `Invalid` at
its selected call site in an isolated source copy. The baseline must pass and
the exact integration test must fail on the changed copy. These controls
establish scoped error propagation. Live-byte `Limit` pairs, capacity and
allocator internals retain their existing ledger entries.

The simulator backend and public plain-`Vec` scan sweeps still use the legacy
allocation hook. Their Global buffers require separate failure coverage before
removing that hook. Existing crash and publication I/O fault tests also remain
independent of this batch.
