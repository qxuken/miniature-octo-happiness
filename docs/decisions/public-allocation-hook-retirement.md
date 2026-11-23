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

## Verification at 66d8fe3

Rust 1.100.0 debug and release runs passed the core and typed suites on macOS
arm64 (421 passed and two explicit ignores per profile) and Docker Linux
arm64/ext4 (429 passed and two explicit ignores per profile). Both storage
golden histories ran in each profile. The macOS build with CRDT enabled and
without `fault-injection` passed 386 tests with one explicit ignore.
The C-handle rollback test passed under Miri. Strict Clippy passed for the
core and typed packages on macOS and the Windows MSVC target, and for the
standalone compound driver on macOS.

All six existing allocation propagation controls and five live-budget
controls were detected on macOS in debug and release. The retained reports
bind the frozen tree and record 82 verified command retirements.

The compound controller's native, fresh-inspection and eight fault cases
passed on macOS for frozen engine `66d8fe31250db0b11d795df4e17b87afe035ed9e`
and archived engine `cde6f2dcdb7bd8ad98d31dcd45303c77f292331f`.
The current Linux ext4 driver also completed all eight fault cases. Provider
refusal after 91,053,796 live core bytes and with two concurrent drafts
reopened revision 212 and reached revision 213 on retry, with all 64
retained roots checked. The driver additionally compiled against
pre-allocator engine `c57c123` using Rust 1.99.1.

| Evidence | SHA-256 |
|---|---|
| Current macOS compound worker report | `19ddb3b2e59ca0d74796fca345c518439c5bf7f66cd8b9ed09785f4f924e25f9` |
| Archived macOS compound worker report | `c695183202d6a31e02fc67f1cc8760f829170372a4c50674013e3ef308c22843` |
| Current Docker Linux fault rows | `6cfa48210e16a3056991d2afdf8594a16de0d6a73dc79a2d42725a3fe76f5f84` |

The inventory reconciles 10,590 decisions. All 19 recorded simulator and
Rust output propagation rows bind to the post-removal source. The resource
build script and shared allocator add tooling records. The 9,906 open
records include 2,536 runtime records.
