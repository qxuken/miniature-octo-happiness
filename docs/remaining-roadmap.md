# Work remaining after the first review checkpoints

PRs #1, #2, #3 and #4 are review checkpoints. The production-readiness goal remains open.
The [e2d2268 qualification](candidate-qualification-e2d2268.md) records the
completed checks. This list is the handoff for subsequent work; the full
requirements and historical evidence remain in
[production-readiness.md](production-readiness.md) and the
[acceptance audit](production-acceptance-audit.md).

## Completed at this checkpoint

- [x] Freeze one source for the four-platform, sixteen-package matrix.
- [x] Verify debug/release workspace tests, minimum Rust, exact delivered
  libraries, relocation and the named Rust/C/C++/Odin consumers.
- [x] Finish the 22-target extended ASan campaign and 31-test stable corpus
  replay in debug and release; retain actual counts and executables.
- [x] Bind three seventeen-case microbenchmark samples to the same source.
- [x] Review all fourteen required successful CI jobs, original logs and
  artifact digests, including workspace ASan/TSan, C/Rust ASan, quota,
  Windows and helper checks.
- [x] Verify the snapshot flush regression on macOS/APFS and Linux/ext4.
  Keep zero-progress read rejection and separate local/follower acknowledgments.
- [x] Retain and restore the checkpoint proof; keep Cargo packages unpublished.

## Follow-up implementation and qualification

These are separate work items. A passing historical run retains its original
source, platform, feature mask and workload scope.

| ID | Unfinished step | Completion evidence |
|---|---|---|
| R1 | The [merged-main inventory refresh and named control batch](r1-qualification.md) records every source and an explicit per-record reconciliation result. Current engine TX-V1, admin export, typed, CRDT and snapshot controls are repeatable. PR #2's 16 checks pass, including native Windows. The [Cargo catalog](decision-inventory-catalog.md) expands discovery to all 21 manifests/fourteen workspaces and 66 non-test targets, preserving zero-record targets and binding per-target scope and historical eligibility. | **R1 remains open:** reconcile the individual G queue in the [current ledger](decisions/current.md), plus generated/compiler, delegated/native and tooling obligations. Source reference discovery and a finite guard batch do not close the full per-decision requirement. Added contexts remain unqualified; four new functional mutants do not establish whole-tool MC/DC. |
| R2 | The [operational refresh](r2-qualification.md) qualifies engine `6b657e5`: 39 operator outcomes on each Mac/Linux lane, 30 authenticated Study rounds/six paired restarts, and 31 snapshot installations/32 polls with four reconciled lost replies. PR #3's final head passes all 19 checks, including native Windows's 41 operator outcomes. The [pending-pair matrix](coordinator-pending-restore.md) adds unresolved intents, a second lost reply and fresh current/retained observers; PR #4 head `7df2dc2` passes all 19 checks. PR #5 head `2bc8e79` passes all 22 checks, including post-download private package recovery. The [two-writer handoff](coordinator-operator-handoff.md#hosted-native-follow-up-at-pr-6) passes both CLI profiles on native macOS, Linux and Windows: six independently audited 85-outcome runs at PR #6 head `3de695f`, whose full 22-check set is green. The frozen macOS/Docker Linux result retains controller `f5287e6` and selected CLIs `2bc8e79`. | **R2 remains open:** qualify other promised delivered-artifact/deployment profiles and obtain actual operator/deployment acceptance. Current/retained roots, exact receipts and repeat-sync stability remain required. Local commit and follower durable acknowledgment stay separate. Authority selection, external endpoint fencing and intended-account qualification remain explicit deployment duties. |
| R3 | Qualify deployment workloads and resource budgets for the proposed Linux server/macOS client. Extend compound near-cap cases with affected-cache forks, independent/shared DAG shapes, cold-open index construction, full-width retained histories and longer contention/maintenance schedules. | Measure p50/p95/p99 response and queue latency, commit rate, RSS/stack, disk growth, maintenance pauses, restore time, recovery point and follower lag. Compare the matched SQLite durability/history profile. Agree on budgets before claiming service limits; accelerated logical months are not wall-clock soaks. |
| R4 | Select the supported API/ABI/disk/wire/CLI version window and qualify its release endpoints, including schema/index and policy migration, upgrade, rejection and rollback. | Exact historical/candidate binaries and original locked inputs pass every promised direction, preserving original files, retained schemas and identities. Existing pinned version pairs do not imply arbitrary compatibility. Future format converters require their own qualification. |
| R5 | Close remaining native deployment profiles. Exercise promised Windows administrative/service workflows under the intended ordinary account, permissions and ACLs; run each additional advertised architecture/filesystem combination. | Native exact-artifact traces, interruption/error outcomes and restoration evidence identify the actual account, filesystem and toolchain. Existing Windows NTFS, owner-quota and delivered-DLL passes retain their named scope. |
| R6 | The [2026-11-18 dependency refresh](locked-dependency-review-3de695f.md) freshly scans all fourteen lockfiles at `3de695f` against frozen RustSec `233155b`: zero reported vulnerabilities or warnings, 1,527 parsed advisories. All 1,595 proof payloads verify after extraction; all fourteen restored scans reproduce exact stdout/stderr digests. | **R6 remains open:** reconcile the eventual release locks, every promised feature/distribution's licenses/notices and source/artifact identities, registry/yanked metadata and owner approval. The dated snapshot retains no advisory exceptions. Unchanged lock bytes alone are carry-forward, not a new scan or distribution approval. |
| R7 | Freeze the eventual release and consolidate its final evidence, including the follow-up workloads. Establish durable off-machine proof custody, retention and restoration ownership. | One immutable release source binds its promised package/platform, sanitizer, fuzz, decision, compatibility and operational results. Fresh extraction validates the index and reruns the verifier. The local PR proof is a checkpoint, not a retention service. |

The [fuzz acceptance table](production-acceptance-audit.md#fuzz-and-dynamic-analysis-acceptance)
also retains bounded-oracle gaps: broader repeated/compound failures,
adversarial tree complexity, representative compilation of generated derives,
transport fault profiles and custom backend conformance. Longer campaigns
should follow those gaps and promote every discovered defect to a deterministic
regression. No finite campaign establishes SQLite-equivalent assurance.

The [PR #7 Linux helper correction](helper-controls.md#linux-executable-fixture-regression)
adds an isolated inherited-writer regression and preserves production launch
errors, supervision limits and cleanup checks. Both local xtask profiles pass
on macOS and Docker Linux. The standalone workspace/example/build-script
inventory is now implemented. R1 next requires individual reconciliation of the
expanded G queue and its compiler/delegated boundaries; all R2–R7 release gates
remain open.
The [frozen catalog qualification](decision-inventory-catalog.md#frozen-follow-up-at-bea0e71)
passes debug/release on macOS and Docker Linux, with 31 inventory tests and
seventeen detected R1 mutants per profile. Original and copied input identities,
restoration and command retirement are independently checked. All 9,749
individual G records remain explicit at that checkpoint. PR #8 head `edec275`
subsequently passes all 23 hosted checks, including native Windows inventory
contracts in both profiles, ASan and TSan. Its Windows inventory and review JSON
match the local final-head reports exactly; all 322 source/reference/Cargo pins
match the tested source. These hosted results retain their own source scope.

The next [codec arithmetic batch](decisions/codec-arithmetic-current.md)
reconciles nine current records using three named pairs, five propagation rows
and a delegated 64-bit conversion. It fixes historical-G promotion and retains
the pinned checksum-table G row as an unresolved example. The expanded queue
now has 9,740 G records (2,411 runtime / 26 driver / 7,303 tooling); the ten new
mutations and their actual platform results are recorded in that ledger.
Frozen `a255842` passes both profiles on Mac/Linux: 32 inventory tests,
341/348 core passes, 239/268 xtask passes and all 27 R1 mutants per profile.
All four campaigns' copied/restored inputs and 96 command retirements verify;
the 4,346-payload local proof passes fresh extraction and audit. PR #9 head
`2d1f81e` subsequently passes all 23 checks, including native Windows, ASan/TSan
and bounded fuzzing. Windows inventory contracts pass 31 tests per profile;
its archive digest and all 323 declared source/reference/Cargo pins verify, and
both reports match the final local reports byte-for-byte. Its tested merge tree
equals the pushed head. Those hosted results retain their own source scope;
eventual release custody remains open.
The [metadata/root-selection follow-up](decisions/codec-metadata-current.md)
reuses existing tests and adds 35 mutation controls. It gives 37 current records
explicit dispositions while retaining six B rows for partial layout guards.
The current queue is 9,703 G records (2,374 runtime / 26 driver / 7,303 tooling).
The unchanged codec file keeps the previous arithmetic bindings intact; backend
I/O and the remaining graph/cache/compiler obligations are still open. Frozen
`795ea8a` passes both profiles on Mac/Linux: 32 inventory tests, 341/348 core
passes, 239/268 xtask passes and all 62 R1 mutants per profile. Independent audits
bind 779 copied/restored files, 347 declared inputs and 207 command retirements
per campaign; the intended named test must fail for each new control. The
5,679-payload local proof passes fresh extraction and both audit replays. Changed
source, false partial-row promotion and unrelated test-failure controls reject.
Native Windows/final-head CI for this new source retain their separate gates.
PR #10 head `5c4aa6f` reaches the macOS source job's 20-minute limit after debug
finishes and before release completes. The [CI shard correction](r1-ci-shards.md)
keeps all 62 controls in both profiles, divides each Unix selection into three
disjoint jobs, and preserves the original command/job bounds. Its one new
tooling propagation record remains G: the current queue is 9,704 (2,374 runtime /
26 driver / 7,304 tooling), with 10,483 records and 4,950 blind spots. Complete
same-head shard results pass at `e5ca577` with all 27 checks green. The
[hosted audit](r1-ci-shards.md#hosted-results-at-e5ca577) confirms the full
62-control union in each profile on Linux and macOS.
The [object and recovery I/O follow-up](decisions/codec-graph-current.md) adds
35 exact dispositions and 33 mutation controls. It verifies that recovery
rejects a damaged newest root before an out-of-bounds object read, even when
the older root opens by itself. The manifest leaves 9,669 G records
(2,339 runtime / 26 driver / 7,304 tooling). VALUE/DATA decoding, descriptor
payloads, the retained catalog and cache decisions remain in the R1 queue.
Frozen `4d2e3eb` passes debug and release on macOS and Docker Linux, including
all 95 R1 controls per profile. Its local proof passes fresh extraction and
replay of the evidence audits across 13,621 payloads. The graph ledger lists
the exact scope, fingerprints and earlier rejected candidates.
The [hosted result](decisions/codec-graph-current.md#hosted-results-at-a1a95a1)
passes all 27 checks at `a1a95a1`. All seven artifact digests and both
95-control Unix unions verify.
The [COMMIT and retained catalog follow-up](decisions/codec-catalog-current.md)
covers 29 further source records. It tests an invalid next ID with zero
entries, truncation at every descriptor word and one trailing byte. The
malformed newest slot rejects, and the previous slot opens with its own
retained rows when selected independently. The 27 added mutation controls
exercise both validation and propagation. The remaining R1 queue has 9,640
G records (2,310 runtime / 26 driver / 7,304 tooling).
Frozen `89e1bb2` passes on macOS and Docker Linux in debug and release,
with 122/122 controls detected per profile. Its archive verifies all
14,689 payloads after fresh extraction and audit replay. The catalog's
hosted Unix matrix has four shards under the existing deadlines.
The [DATA directory follow-up](decisions/codec-data-current.md) maps 32
more decisions and adds 29 controls. It verifies key ordering, exact
payload length and limit failures. Each damaged newest DATA object is
rejected, and the previous slot independently opens with its retained
catalog. The queue now has 9,608 G records (2,278 runtime / 26 driver /
7,304 tooling).
Frozen `0c1969a` passes debug and release on macOS and Docker Linux, including
151/151 controls per profile. Its archive independently verifies and replays
15,817 payloads after fresh extraction. The hosted Unix matrix uses five
shards and preserves the deadlines.
PR #14's initial hosted run passed 29 of 30 engine jobs. The exact-dylib
APFS fixture failed during private-volume preparation. Its [sync refusal
regression](delivered-c-faults.md#ballast-sync-refusal-on-macos) confirms native
allocation refusal from a fresh descriptor after a ballast sync reports
`ENOSPC`. With three added tooling records, that head has 9,611 G records
(2,278 runtime / 26 driver / 7,307 tooling).
The [helper registry](helper-controls.md) explicitly accounts for the new
native APFS test. Its complete macOS audit passed 246 tests per profile
across four suites, with three registered skips. The APFS test passed in
its separate native run. The full local exact-dylib and near-cap copy,
exchange and cleanup cases passed with verified image retirement.
The [VALUE reader follow-up](decisions/codec-value-current.md) maps 30 more
decisions and adds 35 controls. The fixture checks every truncated prefix
and one trailing byte for integer, bytes, map and list payloads, plus an
unknown tag. It matches current, previous and retained rows. The queue now
has 9,581 G records (2,248 runtime / 26 driver / 7,307 tooling).
Frozen `1450e64` passes on macOS and Docker Linux with all 186 controls per
profile. The restored proof checks 17,137 payloads, both platform audits
and the same source bundle. Hosted qualification uses six Unix shards.
The [named integration controls](mutations.md) enforce exact test selection
and failure attribution for 61 existing cases. Five new tooling records
keep G, leaving 9,586 G records (2,248 runtime / 26 driver / 7,312 tooling).
Frozen `a42358e` verifies all 61 exact integrations in both profiles on
macOS and Docker Linux. The restored proof replays 880 command reports
and checks 12,103 payloads.
The [encoder append batch](decisions/codec-append-current.md) reconciles 21
runtime decisions and adds 19 controls. It covers exact-cap admission,
key-prefix and body allocation, equivalent staging layouts and writes that
stop on the first error. The queue has 9,565 G records
(2,227 runtime / 26 driver / 7,312 tooling).
All R1 and R2–R7 completion gates above remain open.

## Deployment and owner gates

- [ ] Select exact supported OS/architecture/filesystem/device/feature profiles,
  support window and API/ABI/format/wire stability promises.
- [ ] Accept the workload, memory, latency, recovery and replication budgets
  from R3, including transfer size/rate and maintenance quiescence.
- [ ] Accept coordinator slot assignment, client journaling/receipt retirement,
  stale-restore policy, fixed CRDT roster/policy migration and raw reserved-row
  ownership rules.
- [ ] Qualify actual device power interruption and deployed storage cache/flush
  behavior. Simulators, process kills, containers and CI do not close this gate.
- [ ] Approve support/security response commitments, license/notice review,
  proof custody and the final release checklist.

Gossip/discovery, dynamic actor rosters, automatic follower promotion/fencing,
remote coordinator authorization/HA, encryption, online maintenance and larger
image limits are conditional future profiles. Decide their requirements before
adding them to the release scope. The intended distributed release currently
includes passive snapshots, fixed two-actor CRDT peers and the coordinator.

All 21 Cargo manifests remain `publish = false`. Crates.io publication is not
authorized. Project naming and publication can be decided independently later.
