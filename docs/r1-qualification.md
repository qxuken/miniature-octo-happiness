# R1 source inventory and guard qualification

Baseline is merged main `4cbcd82a8a7b043c6cc8bac7f1b7cd29d6fe3623`.
Initial qualification used that commit plus the source/test overlay. Final
macOS campaigns use checkpoint `37ced27` plus the review corrections; Linux
uses the merged-main checkout plus the corresponding overlay. Reports preserve
each actual Git source and honestly record `dirty: true`. Exact source/reference
pins, refreshed for the Windows caller correction below, are in
[current-scope.json](decisions/current-scope.json), with definitions,
decision tables and residuals in the [current ledger](decisions/current.md).
Crates remain unpublished.

## Inventory and evidence reuse

The default feature/platform source union now contains **8,171 records across
132 files and 12 Cargo targets**, with no parse/path failures. Source-set SHA-256:
`13dca6b881ed7caa22011bfbb1936805d27033d9d0df48d85cde5a6525f2a399`.
The initial `7437883` checkpoint has 8,172 records and source-set SHA-256
`e417cdcf3b40519c3a5db0173887172476d721166de3b940e053a73a619c3f55`;
removing the duplicate Windows profile decision accounts for the difference.

| Scope | Records |
|---|---:|
| Runtime libraries and applications | 3,023 |
| Administrative, measurement and fault drivers | 156 |
| Qualification tooling | 4,992 |

These are overlapping AST records, not independent Boolean conditions or
machine branches. The prior 2,604-record denominator is historical. New targets
and the Rust xtask migration explain much of the larger scope; they are not
counted as a loss or gain in measured MC/DC.

The checked manifest binds every source file and 61 referenced files. Twenty
full-file pins still match the historical ledgers. Conservative row matching
reuses **721** exact historical row references: P=157, E=113, C=47, I=43, D=361.
The report preserves each original ledger line/status rather than reinterpreting
it as newly measured coverage. Changed files, ambiguous rows and unmatched
positions retain their scoped references and explicit residual obligations.

Eleven named controls cover six source-predicate tables and five caller
propagations, with thirteen maintained guard omissions/boundary mutations. Tests
and mutations must exist in their pinned files; every predicate table supplies
an independence pair for each listed operand. Propagation is reported separately
from Boolean independence. The initial macOS and Linux runs generate byte-identical
`inventory.json` and `review.json`; consecutive local runs are identical too.

The inventory retains **3,442** macro/derive/conditional blind spots and **148**
cfg/test exclusions. Generated FFI/Model/Record/Resolution behavior, implicit
compiler branches, dependencies, coupled native predicates and unexecuted native
outcomes retain their explicit boundaries. The separate tooling scope has
unit/helper/negative-control evidence; its 4,992 records do not become a runtime
MC/DC percentage. No compiler MC/DC measurement or whole-workspace condition
closure is claimed.

## Initial local qualification at 7437883

| Check | macOS arm64/APFS, Rust 1.99.1 | Docker Linux arm64/ext4, Rust 1.99.1 |
|---|---|---|
| All-feature workspace debug | 754 passed, 6 ignored | 791 passed, 8 ignored |
| All-feature workspace release | 754 passed, 6 ignored | 791 passed, 8 ignored |
| Final inventory/reconciliation tests | 14 debug + 14 release | 14 debug + 14 release |
| R1 debug mutants | 13 detected | 13 detected |
| R1 release mutants | 13 detected | 13 detected |
| Strict Clippy | Workspace/all targets and inventory tool | Inventory tool |

All four mutation campaigns have successful nonempty baselines, genuine Cargo
test failures, bounded verified cleanup, restored copied source and unchanged
original scoped inputs. There are **52 detected mutants** total. Compilation
errors, empty targets, timeouts and cleanup failures never qualify. Existing
typed/CRDT/snapshot tests supply their exact current/retained-root, image,
progress and reply assertions; no new engine, wire or acknowledgment behavior
was introduced. The admin helper preserves the original lock/write/sync/unlock
order and error propagation and is exercised through its actual ByteStore caller.

Linux uses image
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
Checkout, build, campaign copies and TMPDIR are all on the named ext4 volume;
mount options are `rw,relatime,discard,noauto_da_alloc`. Source tar/bundle and
registry inputs are read-only mounts and networking is disabled. The row-matching extension was checked in a separate overlay. After the spec
review, the explicit G queue, single-source ledger declarations and TX-V1
controls were overlaid and both workspace profiles, utility checks and both
mutation profiles were rerun. This is container
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

## Retention and restoration

The private archive `target/archived-proof/r1-inventory-7437883.tar.zst` has
SHA-256 `3343d50674b21f612ebe3d2964a5b78bfccd7464e380923601bbfafedb518801`.
Fresh restoration verifies all **6,004 indexed files**, totaling 205,939,557
bytes. The relative index SHA-256 is
`36a11ad169220433436a50f91a7030022aa5612ddd5971ac5432e38cbfdcd3d0`.
The retained original inventory binary reproduces both reports byte for byte
against the restored source. Source commit `7437883`, its Git bundle/archive,
actual controller binaries, all four final campaign source copies/logs, rejected
preparation, and independent review notes are retained. Later documentation-only
updates do not change that code or its source/reference pins.

The Linux copies include literal AppleDouble `._` metadata entries from the
initial source transfer. All recorded copied-input hashes, including those
entries, are preserved and checked; none is skipped. macOS bsdtar normalization
consumed those entries during two rejected restoration attempts. GNU tar on the
owned ext4 volume followed by Docker's byte-preserving copy restores all files
and passes the complete index. Use a metadata-preserving extractor for this
proof. The metadata entries are not additional Rust targets or MC/DC records.

Independent replay checks 52 genuine mutant failures against their intended
named tests, 28 successful nonempty baselines, cleanup/timeout records and every
restored copied-source hash. Runtime/test/fixture inputs match the checked
manifest across both OSes. Off-machine custody remains a release gate.

## Windows caller correction after hosted CI

The first [PR #2 Engine run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44059515752)
passed the R1 gate, macOS/Linux native jobs, sanitizers and helper checks; its
three Windows jobs failed to compile xtask. The shared mutation command builder
had gained a required profile argument, but the Windows-only caller retained
the old two-argument call. This is a tooling regression, not a failed database
durability outcome. The failed jobs receive no Windows qualification credit.

`cargo +1.99.1 check --offline --locked -p xtask --all-targets --target
x86_64-pc-windows-msvc` reproduces the exact E0061 diagnostic locally. The
Windows caller now forwards its current profile to the shared builder once.
Its pure command construction is also compiled by host unit tests, covering
both RNG cases, both profiles and present/absent toolchain selection. The
regression first reproduces the missing argument, then passes with exact
argument assertions; the profile option precedes the libtest separator and
each release command has exactly one `--release`.

The same Windows cross-check passes on Rust 1.99.1 and minimum Rust 1.99.0.
Strict xtask Clippy passes for the host and Windows target. The complete host
xtask suite passes 228 tests with one ignored in each of debug and release.
A deliberate profile omission fails the new exact-argument assertion; restored
source passes again. This is a regression control, not another native RNG run.
The source manifest is rebound to the changed tooling file; runtime, referenced
engine/application tests and mutation-fixture inputs are unchanged. The initial archive and its reports
retain their original identities. Native Windows execution and the corrected
branch's hosted checks remain pending until the rerun finishes.

## Remaining work

The source inventory and finite guard-control batch are refreshed. **R1 remains
open** for the individual reconciliation/expanded-code obligations in the ledger;
source reference reconciliation is not proof that every scoped E/D/tooling
record has an independence pair. The report has 7,438 explicit per-record G
reasons: 2,420 runtime, 26 driver and 4,992 tooling records. G denotes an
unresolved current mapping, not proof that no prior test exists. Two overlapping
logical records reference the exact source ranges of named parent pairs as C.
Named controls and historical rows can overlap, so these counts are not an
independence percentage. Further manual pairs should use the refreshed
IDs and source pins. Native/device, deployment budgets, supported compatibility,
advisory/distribution and final release acceptance remain in the
[unfinished roadmap](remaining-roadmap.md).
