# Public allocation hook retirement

The public database and simulator allocation hooks have been removed.
The selected allocator drives integration failures in compaction, retained
ID preparation, schema writes and migrations, index repair, concurrent
allocation, CRDT staging, and C handle creation. The remaining synthetic
counter is compiled only for library unit tests. The `fault-injection`
feature continues to expose the I/O and publication test hooks.

| Boundary | Control and result |
|---|---|
| Compaction and schema writes | Refuse each real provider request in order. Source bytes and previously acknowledged state remain readable on failure. A subsequent attempt can succeed. |
| Typed primary/index consistency | Refusal during an edit or index rebuild preserves the previous primary rows and index state. The completed rebuild verifies index consistency. |
| Value and map limits | Oversized or over-depth operations report Limit before allocating. The map-output case arms both the selected provider and Global. |
| C handle creation | Refuse value and handle allocation separately. The output remains null and the live handle count and provider bytes roll back. The success control releases its handle. |
| Concurrent allocation | Eight threads submit 800 real provider requests. Exactly 100 succeed before persistent refusal. The provider returns to its original live bytes. |
| CRDT migration | Refusing the second target preserves both source peers and exposes the partial first target. Fresh targets converge after retry. |
| Compound resource driver | The current allocation-failure lane selects a provider and refuses its first commit request under pressure. Overlaid replay against archived sources uses the engine hook when that engine exposes it. The other workload lanes use their existing allocator. |

The resource build script declares the selected replay mode from the archived
engine source. The controller records both the build-script and allocator
fixture digests in its driver overlay. This keeps the old workload commands
reproducible across the API transition.
