# Current source reconciliation after merged main

R1 baseline: `4cbcd82a8a7b043c6cc8bac7f1b7cd29d6fe3623` (PR #1 merged).
The reviewed manifest is [current-scope.json](current-scope.json). It pins every
inventoried source and referenced test/ledger file. Its generated `review.json`
binds each source record to a scope, module references and explicit residual
obligations. Raw AST records still say `unreviewed`: a reference is not an
automatic upgrade to predicate independence or a claim of test execution.

The R2 tooling refresh advances the checked manifest from merged PR #2
`6b657e516c13f678cd54c816db8a53ec5a11e395`. It adds the snapshot-month controller
and Windows incident-runner adaptation. Runtime sources, historical row
bindings and named R1 controls are unchanged. New tooling decisions retain
individual G obligations; operational test execution does not promote them to
MC/DC pairs. The R1 report retains its original checkpoint counts and source.

The pending-journal follow-up uses merged PR #3 baseline `b5493a3`. Its new
process fixture and recovery notes are referenced explicitly. Production
runtime/tooling sources and the 8,239-record denominator are unchanged; the
four recovery schedules do not close individual condition-pair obligations.

The delivered-source follow-up uses PR #4 head `7df2dc2` as its fixed baseline.
The coordinator recovery controller and Windows package verification are new
tooling inputs. Their tests and negative controls retain their explicit scope;
they do not promote the new source records or macro blind spots to MC/DC pairs.
Runtime sources and the existing R1 named controls are unchanged.
The operator-handoff follow-up uses PR #5 head `2bc8e79` as its fixed baseline.
It adds two assigned journals, credential and mixed-set checks, and a separate
raw consumer. The consumer's standalone workspace is source-pinned as a
reference; its decisions are outside this workspace AST denominator and remain
an explicit tooling coverage obligation. No new condition independence or
compiler coverage is inferred from the rehearsal.
The refreshed source inventory has 8,360 records in 134 files across 12 targets:
3,023 runtime, 156 driver and 5,181 tooling records, with 3,596 macro/attribute
blind spots. New tooling records keep individual G obligations.

The [Linux executable-fixture follow-up](../helper-controls.md#linux-executable-fixture-regression)
adds a test-only request clone and an isolated inherited-writer regression.
Production decision counts remain 8,360 across the same 134 files and twelve
targets. The conditional test-only derive adds one explicit attribute blind
spot, bringing the current total to 3,597. Source and reference pins are
refreshed; helper execution does not promote any individual G obligation.

The [repository Cargo catalog](../decision-inventory-catalog.md) advances the
review baseline to PR #7 head `b32a868`. It includes all 21 manifests/fourteen
workspaces and 66 non-test targets, with 46 integration targets explicitly
excluded from the production view. Review schema 2 pins every Cargo input and
target, including twenty-two zero-record fuzz wrappers. Existing target IDs,
named source controls and historical eligibility are preserved. New target
contexts receive tooling scopes and individual G obligations, even when they
share runtime source files. The current denominator is 10,482 records across
215 files, with 4,946 explicit macro/attribute blind spots. Runtime and driver
counts stay 3,023 and 156; the tooling denominator is 7,303. This expands the
unqualified queue and does not infer compiler MC/DC or new execution credit.
The [frozen follow-up](../decision-inventory-catalog.md#frozen-follow-up-at-bea0e71)
verifies both Mac/Linux tool profiles and all seventeen R1 mutants per profile.
All 777 copied inputs and 347 declared campaign inputs match `bea0e71`; all
restored copies and process-retirement reports pass. Production and test-mode
inventories are byte-identical across platforms. The individual G queue remains
9,749 records: 2,420 runtime, 26 driver and 7,303 tooling. Native Windows and
final-head CI results retain their own source scopes.

The [codec arithmetic follow-up](codec-arithmetic-current.md) starts at PR #8
head `edec275` and adds three named single-condition pairs, five scoped
propagation rows and one delegated 64-bit conversion row. Production codec
behavior is unchanged. The current denominator remains 10,482 records across
215 files and 66 targets, with 4,946 blind spots. Fourteen named controls and
731 exact ledger rows leave **9,740 individual G records**: 2,411 runtime,
26 driver and 7,303 tooling. The checksum-table G row remains a gap despite its
exact pin. Reconciliation now requires non-gap historical evidence before
assigning a historical disposition; an independent named control can still
qualify that same record. The inventory regression and its guard mutation
exercise both the preserved gap and the separate named-control qualification.
The prior checkpoint had no historical G rows, so this correction does not
change its published 9,749-gap result. Platform execution is recorded separately
in the follow-up; neither a source reference nor a delegated row measures MC/DC.

The [current metadata/root-selection batch](codec-metadata-current.md) advances
from PR #9 head `2d1f81e`. Its codec file and tests are unchanged, so the nine
arithmetic bindings remain intact. Fourteen additional named controls bind
existing real-image/header tests, with 35 additional guard mutants. The 37 new
ledger rows are 13 P, 14 coupled C, six partial B, three scoped E and one fixed-loop
I. The cap/alignment limitations are preserved rather than assigned complete
independence. The checked manifest now has 28 named controls and 768 exact ledger
references, leaving **9,703 individual G records** (2,374 runtime / 26 driver /
7,303 tooling). The denominator remains 10,482 records / 215 files / 66 targets /
4,946 blind spots. Backend I/O propagation, the remaining graph/cache records
and compiler/delegated/native/tooling gates retain their own obligations.

R1 remains open for individual reconciliation outside the reviewed bindings.
Every unmatched record now has its own `G` disposition and reason naming its
current target, owner, kind, conditions and alternatives. Group references are
candidates for that work, not a substitute for a mapped pair. This makes the
remaining queue explicit instead of treating a file-level reference as closure.

## Interpreting the refreshed denominator

The prior 2,604-record checkpoint predates both later runtime changes and the
Rust xtask conversion. The refreshed union includes runtime libraries/apps,
shipped administrative commands, measurement/fault drivers and qualification
tooling. The report separates runtime, driver and tooling counts. Shared source
keeps each Cargo target identity; matching paths do not collapse binary profiles.

The existing ledgers preserve P/E/C/I/D/B/G dispositions at their original source
locations. The reconciliation reports which historical full-file pins still
match. A changed file receives current scoped references and explicit remaining
obligations, not a guessed transplant of old line numbers. Even an unchanged
file pin does not prove that every old condition was independent or that all
of its dependencies/platforms were exercised in a new run.

Unchanged-file rows are additionally rebound only when their exact source
range/position, kind, owner and any explicit Cargo-target column match. The
report preserves the original status and ledger row separately. A `?` keyword
hash alone, an ambiguous row or an unmatched file/target never earns that
binding. Historical statuses are references, not newly measured coverage.

Position-only tables for schema and tree use an explicit reviewed single-source
ledger declaration. No source basename is guessed. Overlapping short-circuit
syntax can reference a named parent pair only when its complete source range
matches that parent's atom range; unrelated equal text cannot earn C.

The manifest's named controls select one exact target/file/owner/kind/condition
set, require existing parsed `#[test]` functions and maintained mutation names,
and validate each Boolean decision table's independence pairs. Propagation
controls are labelled separately. The source, referenced test and fixture hashes
must match; missing/ambiguous sources, tests, controls, ranges, target counts and
file dispositions reject. This is source traceability, not compiler MC/DC.

## TX-V1: current validation-mode guard

The current `Transaction::validate` guard is rebound to
`tests/transactions.rs::existing_record_edits_follow_validation_mode` and engine
T1. A=`validation == WholeDatabase`, B=`base revision != current revision`:
(T,T) conflicts; (F,T), (T,F), (F,F) commit. The first/second rows isolate A and
first/third isolate B. The identical draft edits one record while an optional
intervening commit edits another, avoiding a point-dependency conflict that
could mask the mode predicate. Current revisions, values and the retained old
snapshot are asserted. Two maintained mutants force A or B true; each must fail
the same current test suite. Runtime validation code is unchanged.

## ADMIN-W1: export progress and ordering

The admin export loop now lives in private `export_bytes`, called by the real
`export` command using the same `FileStore`. Its lock, write, sync and unlock
order and error propagation are preserved. No public storage interface changes.
Tests use the existing `ByteStore` contract rather than fabricated descriptors
or kernel return values. Successful native CLI export/import/reopen tests remain
the separate actual-file evidence.

For A=`written == 0`, B=`written > remaining`, the same six-byte buffer supplies:

| Returned progress | A | B | Result |
|---:|---|---|---|
| 1, then 2, then 3 | F | F | Exact suffixes at offsets 0, 1, 3; sync and unlock |
| 0 | T | F | Exact Io; no further write, sync or unlock |
| 7 | F | T | Exact Io; no further write, sync or unlock |

The rejecting rows paired with the valid row independently control A and B.
B has a defined conceptual false value when A masks its evaluation. The loop's
empty-input control calls no write but still syncs/unlocks. Exact-length progress
stops at EOF. Lock Busy, write Limit and sync Io preserve the first failure and
stop before subsequent stages. These callback/error-propagation controls do not
claim actual kernel short-write or error injection.

Four tests in `tools/admin.rs::export_decisions` cover those outcomes. Four
maintained mutants remove each invalid-progress guard, change `<` to `<=`, or
swallow the final sync error. A second write after zero progress fails the
finite callback script immediately; an infinite test timeout cannot qualify.

## Reused controls made repeatable

- Typed T11: the existing permissive manual decoder and exact error assertions
  independently detect the stored-record type guard in `decode`, the separate
  guard in `record_keys`, and old-model insert rejection after catalog upgrade.
  Their current/retained-row and write-poisoning assertions are reused.
- CRDT C4: the existing exhausted automatic-resolution test preserves the whole
  current/retained record and image, sends no success reply, and verifies a
  successful boundary control plus read-only replay. Its caller propagation
  omission is now maintained in xtask.
- Snapshot R4/R5/R6: existing upload-flush, ship-request and poll-request tests
  use exact transcripts, preserve prior progress and leave the next reply unread
  on failure. Their caller omissions are now maintained in xtask. A local commit
  still does not acknowledge follower durability.

The 56 historical Unix mutation cases keep their original provenance. Thirteen
R1 additions bring the finite registry to 69. `--release` additionally runs the
Unix controller with optimized test builds; existing default commands remain
unchanged. Windows keeps its separately pinned two-case, two-profile registry.

R1 exposed a qualification-controller defect: an external `#[cfg(test)] mod
tests;` was mistaken for the beginning of an inline test body, hiding later
production guards from mutation admission. The new regression distinguishes
both forms while preserving inline test bytes. The controller also accepts a
single `--bin` target: filtering all binaries would include unrelated zero-test
targets, which remain an invalid baseline. `tools/admin.rs` is now included in
the campaign input snapshot, with a source-change regression.

## Explicit residuals

- Existing E and D evidence remains scoped to its tested calling paths and
  platform/dependency outcomes. Every possible residual at every `?` is not
  inferred from one callee test. Native identity callback controls and kernel
  fault occurrence remain distinct.
- Macro/derive/conditional-attribute blind spots remain enumerated separately.
  FFI templates and Model/Record/Resolution generators have their existing
  supplemental tests; no complete compiler-expanded denominator is claimed.
- Qualification tooling has scoped unit, helper, negative-control and platform
  evidence. Its thousands of source records are not added to a runtime MC/DC
  percentage or automatically marked covered by a green workflow.
- Supported platform/feature unions, arbitrary application `Record` algebra,
  native calls, standard-library/dependency internals and compiler implicit
  branches retain their explicit acceptance boundaries.

Results and retained proof identities belong in the R1 qualification report;
they are not inferred by this manifest or substituted for measured compiler
MC/DC. All crates remain unpublished.
