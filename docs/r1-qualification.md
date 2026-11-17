# R1 source inventory and guard qualification

Baseline is merged main `4cbcd82a8a7b043c6cc8bac7f1b7cd29d6fe3623`.
The local qualification uses that clean commit plus this branch's source/test
overlay; campaign reports honestly record `dirty: true`. Exact source/reference
pins are in [current-scope.json](decisions/current-scope.json), with definitions,
decision tables and residuals in the [current ledger](decisions/current.md).
Crates remain unpublished.

## Inventory and evidence reuse

The default feature/platform source union now contains **8,172 records across
132 files and 12 Cargo targets**, with no parse/path failures. Source-set SHA-256:
`ff5ea077bad7ad7a3ef0c68ea2d1ea613fc297d6350ff9287173210d303f26fa`.

| Scope | Records |
|---|---:|
| Runtime libraries and applications | 3,023 |
| Administrative, measurement and fault drivers | 156 |
| Qualification tooling | 4,993 |

These are overlapping AST records, not independent Boolean conditions or
machine branches. The prior 2,604-record denominator is historical. New targets
and the Rust xtask migration explain much of the larger scope; they are not
counted as a loss or gain in measured MC/DC.

The checked manifest binds every source file and 61 referenced files. Twenty
full-file pins still match the historical ledgers. Conservative row matching
reuses **478** exact historical dispositions: P=102, E=48, C=28, I=2, D=298.
The report preserves each original ledger line/status rather than reinterpreting
it as newly measured coverage. Changed files, ambiguous rows and unmatched
positions retain their scoped references and explicit residual obligations.

Ten named controls cover five source-predicate tables and five caller
propagations, with eleven maintained guard omissions/boundary mutations. Tests
and mutations must exist in their pinned files; every predicate table supplies
an independence pair for each listed operand. Propagation is reported separately
from Boolean independence. Both macOS and Linux generate byte-identical
`inventory.json` and `review.json`; consecutive local runs are identical too.

The inventory retains **3,442** macro/derive/conditional blind spots and **148**
cfg/test exclusions. Generated FFI/Model/Record/Resolution behavior, implicit
compiler branches, dependencies, coupled native predicates and unexecuted native
outcomes retain their explicit boundaries. The separate tooling scope has
unit/helper/negative-control evidence; its 4,993 records do not become a runtime
MC/DC percentage. No compiler MC/DC measurement or whole-workspace condition
closure is claimed.

## Executed checks

| Check | macOS arm64/APFS, Rust 1.99.1 | Docker Linux arm64/ext4, Rust 1.99.1 |
|---|---|---|
| All-feature workspace debug | 754 passed, 6 ignored | 791 passed, 8 ignored |
| All-feature workspace release | 754 passed, 6 ignored | 791 passed, 8 ignored |
| Final inventory/reconciliation tests | 12 debug + 12 release | 12 debug + 12 release |
| R1 debug mutants | 11 detected | 11 detected |
| R1 release mutants | 11 detected | 11 detected |
| Strict Clippy | Workspace/all targets and inventory tool | Inventory tool |

All four mutation campaigns have successful nonempty baselines, genuine Cargo
test failures, bounded verified cleanup, restored copied source and unchanged
original scoped inputs. There are **44 detected mutants** total. Compilation
errors, empty targets, timeouts and cleanup failures never qualify. Existing
typed/CRDT/snapshot tests supply their exact current/retained-root, image,
progress and reply assertions; no new engine, wire or acknowledgment behavior
was introduced. The admin helper preserves the original lock/write/sync/unlock
order and error propagation and is exercised through its actual ByteStore caller.

Linux uses image
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
Checkout, build, campaign copies and TMPDIR are all on the named ext4 volume;
mount options are `rw,relatime,discard,noauto_da_alloc`. Source tar/bundle and
registry inputs are read-only mounts and networking is disabled. The final
reconciler's row-matching extension was overlaid and its tests, Clippy and reports
rerun separately; runtime/mutation inputs stayed unchanged. This is container
filesystem execution, not physical power-loss qualification.

Native Windows execution of the changed admin tests and the final hosted
configuration is delegated to the new PR's CI. The separate Windows RNG registry
still has two cases and both build profiles. The workflow adds a source
reconciliation gate plus both R1 mutation profiles and retains their reports.

## Failed preparation and permanent regressions

The first registry admission found zero occurrences in replication source,
before any mutation ran: the old controller truncated source at an external
`#[cfg(test)] mod tests;` declaration. The regression now distinguishes it from
an inline test body. A subsequent admin baseline correctly rejected unrelated
zero-test binaries selected by `--bins`; the controller now supports one `--bin`
target. Both failed preparations remain recorded and receive no mutant credit.

`tools/admin.rs` was absent from the old campaign input snapshot. It is now
included, and a source-change regression verifies that its digest changes.
The checks for compiler failure, no matched tests, timeout and unverified cleanup
remain unchanged. No runtime publication barrier or follower durability
acknowledgment was weakened.

The Docker source extraction emitted Apple provenance-xattr warnings. File byte
pins and final cross-host reports match; no warning is counted as an engine
failure or hidden by a product-test retry.

## Remaining work

This closes the R1 source-refresh and named guard-control batch. The individual
independence/expanded-code obligations in the current ledger remain explicit;
source reference reconciliation is not proof that every scoped E/D/tooling
record has an independence pair. Further manual pairs should use the refreshed
IDs and source pins. Native/device, deployment budgets, supported compatibility,
advisory/distribution and final release acceptance remain in the
[unfinished roadmap](remaining-roadmap.md).
