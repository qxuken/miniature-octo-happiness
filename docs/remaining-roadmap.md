# Work remaining after the first review checkpoints

The [private production PR roadmap](production-pr-roadmap.md) turns the
remaining release gates into reviewable batches and defines their stack order.

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
| R6 | The [November 25 dependency and notice review](locked-dependency-review-afffd2d.md) scanned all 14 locks, checked all 142 registry version checksums and yank flags, and compared 272 notice files across 16 delivered packages with their sources. | Recheck the final release's exact lockfiles, feature sets, toolchains and delivered materials. Legal, support and release approvals remain open. |
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
PR #15 at `d21e1e5` passed all 32 engine jobs and bounded fuzzing. The
hosted audit verified all 13 reconciliation archives and the full 186-case
Unix selection in each profile. [Hosted engine checks](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44297082826).
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
Frozen `906b8ed` passes all 19 new controls and the full core, xtask and
inventory suites in both profiles on macOS and Docker Linux. The restored
proof replays both audits and checks 10,476 payloads.
The [VALUE encoder batch](decisions/codec-encode-value-current.md) reconciles
33 runtime decisions. It adds 28 payload, cache and allocation controls
and one mutation for exact byte-position selection. The queue has 9,534 G
records (2,194 runtime / 26 driver / 7,314 tooling).
Frozen `cf3a5a2` passes all 29 new controls and the full core, xtask and
inventory suites in both profiles on macOS and Docker Linux. The restored
proof validates 10,826 payloads and replays both platform audits.
The [DATA encoder batch](decisions/codec-encode-data-current.md) reconciles
24 runtime decisions and adds 25 controls for cache admission, exact DATA
bytes and allocation failure. Its ledger binds the existing golden history
with reopened and retained roots. The queue has 9,510 G records
(2,170 runtime / 26 driver / 7,314 tooling) at that checkpoint.
Frozen `45e5705` passed the core, xtask and inventory suites in debug and
release on macOS and Docker Linux arm64/ext4, with all 25 DATA controls
detected per profile. The restored proof verifies 10,588 payloads and
replays the exact-source audit for both hosts.
The [COMMIT encoder batch](decisions/codec-encode-commit-current.md) reconciles
15 runtime decisions and adds 17 controls. It covers descriptor fields,
retained DATA references and cache finalization. The queue has 9,495 G
records (2,155 runtime / 26 driver / 7,314 tooling). Cache and index internals
retain their own R1 obligations.
Frozen `a09bc83` passed the core, xtask and inventory suites and detected
all 17 COMMIT controls per profile on macOS and Docker Linux arm64/ext4.
The restored proof verifies 10,402 payloads and reproduces the audit for
both hosts.

The [selected allocator batch](decisions/allocation-recovery-current.md)
moves the nested-edit, candidate, catalog, retain and release sweeps to the
selected allocator and adds a cold-reopen sweep. Core refusals preserve
the prior image, live-byte charge, current revision and retained roots.
Successful nested commits match the default allocator's exact bytes.
At `545d214`, the macOS and Docker Linux core and typed suites passed in
debug and release alongside all six new propagation controls and five
existing live-budget controls. The R1 queue is 9,916 individual gaps
(2,556 runtime / 26 driver / 7,334 tooling). The simulator and raw Rust
output failure coverage are separate follow-ups before retiring the legacy
allocation hook.

The [simulator allocator slice](decisions/simulator-allocator-current.md)
uses an independent selected allocator for simulator storage and backend
failure sweeps. The constructor, trace, write, crash, sync and reopen checks
pass on macOS and Docker Linux in both profiles.

The [Rust output slice](decisions/rust-output-allocation.md) exercises real
Global refusal in map, scan, retained ID, simulator, inspection and logical
export outputs. Observation, overlay and owned scans use selected-allocator
refusals.

The [public allocation hook retirement](decisions/public-allocation-hook-retirement.md)
migrates the remaining integration callers and the compound resource driver.
The [codec allocation tests](decisions/codec-allocator-tests.md) drive
real provider refusal across encoding, recovery, cache reuse and fresh
image creation. The [cold-open and Linux metadata tests](decisions/storage-allocator-tests.md)
exercise real refusals while checking current and retained roots,
lease cleanup, and metadata and ACL preservation.

The [unit allocator migration](decisions/unit-allocator-retirement.md)
finishes batch 2 on the stack. Tree, value and memory tests use the
provider and the synthetic counter has been removed. Both golden histories
pass on macOS and Docker Linux, and 22 allocator and encoder controls
pass in each macOS profile.

The [codec cache and index ledger](decisions/codec-cache-index-current.md)
records batch 3 on the stack. It binds 28 source decisions to directed
pairs, propagation tests, scoped execution and coupling. Both profiles
caught all 31 new controls on macOS. The 17 golden images match across
both profiles on macOS and Docker Linux.

The [transaction observation and overlay scan ledger](decisions/transaction-observation-scan-current.md)
starts batch 4 on the stack. It binds 39 source decisions and detects
all 35 named controls in both macOS profiles. The provider sweeps verify
draft usability, released scratch, and the dependency that survives after
successful observation. A separate BTreeMap checks ordered overlay rows
at capacity.

The [assignment and commit preparation ledger](decisions/transaction-assignment-commit-current.md)
records 31 more source dispositions. The assignment sweep checks which
observations survive refusal while the prior edit remains usable. The
durable sweep includes replacement and deletion and preserves both roots
before publication. Both macOS profiles detected 30 new controls and three
existing controls.

The [persistent tree ledger](decisions/tree-current.md) records all 72
tree decisions, with 71 fewer open runtime records. The sweeps cover
collision insertion and replacement, removal, binary row budgets, and
list growth and clearing under provider refusal. Both macOS profiles
detected all 49 named controls.

The [owned value ledger](decisions/value-current.md) covers all 81 value
decisions. A copying model checks completed nested edits, and the refusal
sweep preserves the source values and live charges. Both macOS profiles
detected all 52 controls after the removal and error fixtures were
strengthened.

The [transaction write-charge bound](decisions/write-charge-current.md)
removes the last recorded batch 4 residual. A saturated write subtotal
exceeds every valid transaction cap and is rejected before allocating
write storage. Both macOS profiles detected 32 controls, including the
two sum boundaries and the upper cap guard. Batch 5 starts with ByteStore
transfer progress and publication barriers.

The [ByteStore and publication ledger](decisions/bytestore-publication-current.md)
starts batch 5 with 20 more exact source dispositions. It records which
revision reopens at each simulated publication failure and checks the
retained rows throughout. Debug and release pass on macOS and Docker Linux
at `e8d76f1`. Both macOS profiles detect all 27 selected controls.
The queue has 9,628 open records (2,258 runtime / 26 driver / 7,344 tooling).

The [creation and reopen ledger](decisions/storage-creation-reopen-current.md)
maps 15 more source records at `de38aac`. It checks which initialization
failures leave no usable image, and which stabilization failures reopen
revision 3 or 4 with revision 2 still retained. Both profiles pass on
macOS and Docker Linux. Both macOS profiles detect all 24 selected
controls. The queue has 9,613 open records (2,243 runtime / 26 driver /
7,344 tooling).

The [compact-copy and headroom ledger](decisions/compact-copy-current.md)
adds 21 exact dispositions at `fa87e4b`. The sweep faults every destination
callback and checks source bytes, lease release, and exact bytes and retained
rows when a destination reopens. Debug and release pass on macOS and Docker
Linux. Both macOS profiles detect all 22 selected controls. The queue has
9,592 open records (2,222 runtime / 26 driver / 7,344 tooling).

The [native replacement ledger](decisions/native-replacement-current.md)
maps 18 more runtime records at `23eb940`. It checks which inode remains
canonical at each before/after failure, when the old scratch file is
removed, and which names survive each simulated persistence choice.
Both profiles pass on macOS and Docker Linux. Both macOS profiles detect
all 21 selected controls. The deeper native adapter decisions remain
in batch 5, and follower durability has its own checks in batch 9.
The queue has 9,574 open records (2,204 runtime / 26 driver / 7,344 tooling).

The [Unix writer lease ledger](decisions/unix-leases-current.md) records a
macOS bug reproduced by the Linux duplicate-descriptor fixture. The adapter
now unlocks before closing, and the regression passes on both platforms.
The stale-path check releases the rejected inode while the original adapter
is still alive. Both macOS profiles detect all 22 controls at `15b2993`.
The queue has 9,548 open records (2,178 runtime / 26 driver / 7,344 tooling).

The [Unix pathname ledger](decisions/unix-path-admission-current.md) maps
19 more source records at `c28cad3`. Both platforms pass the same refusal
sweep for parent and leaf allocation during create and reopen. Native failures
preserve the operation and errno, and both macOS profiles detect all 20
selected controls. The queue has 9,529 open records (2,159 runtime /
26 driver / 7,344 tooling).

The [Unix sync ledger](decisions/unix-sync-current.md) maps nine additional
records at `a1b2dd5`. A failure at any creation barrier preserves its error
and makes the adapter repeat all three barriers on retry. The compaction
trace distinguishes source and target length, read, write and sync. Both
profiles pass on macOS and Docker Linux, and both macOS profiles detect
all 13 selected controls. The queue has 9,520 open records (2,150 runtime /
26 driver / 7,344 tooling).

The [macOS primitive ledger](decisions/macos-primitives-current.md) maps
all six records at `67c91b2`. Actual file sync, exchange and metadata copy
exercise success and native refusal. Both macOS profiles detect all 12
native controls. The Unix controller selects host-specific cases before
sharding. Its macOS and Linux lists agree on the 620 shared R1 names.
The queue has 9,513 open records (2,144 runtime / 26 driver / 7,343 tooling).

The [Linux primitive ledger](decisions/linux-primitives-current.md) maps
24 of the 40 Linux adapter records at `355447a`. It records real flock,
sync and exchange outcomes, bounded entropy and metadata reads, and
successful ext4 metadata retry after all three selected allocation refusals.
Both Linux profiles detect all 31 selected controls. The native fixtures
also pass under UID 1000 on ext4. The queue has 9,489 open records
(2,120 runtime / 26 driver / 7,343 tooling).

The [Linux metadata ledger](decisions/linux-metadata-current.md) adds
16 dispositions and rebinds the earlier 24 on `6d3d308`. The adapter
rejects malformed attribute lists before issuing native attribute
mutations. The fixtures exercise metadata read refusal, independent
ownership and mode checks, and native EBADF for each mutating syscall.
Both Linux profiles detect all 24 selected controls. The queue has
9,474 open records (2,105 runtime / 26 driver / 7,343 tooling).
Deployed-device persistence remains open.

The [Windows pathname ledger](decisions/windows-paths-current.md) records
27 helper dispositions at `a28cca6`, including three partial let-chain
records. The adapter rejects over-cap normalized and assembled UTF-16
paths before growth. The shared path tests pass on macOS and Docker Linux
in both profiles, and macOS detects all 30 controls per profile. Windows
cross-target Clippy passes. Native Win32 execution and the intended NTFS
deployment remain separate gates. The queue has 9,453 open records
(2,084 runtime / 26 driver / 7,343 tooling).

The [Unix scratch and positioned I/O ledger](decisions/unix-scratch-io-current.md)
records five new dispositions at `d31aba8`. Scratch names use one selected
allocation before entropy. Native fixtures check exclusive-create
collision, unlink, short reads, EOF, offsets and read-only write refusal.
Both profiles on macOS and Docker Linux pass storage, recovery and
golden-image tests and detect all 12 named controls. The queue has 9,447
open records (2,078 runtime / 26 driver / 7,343 tooling).

The [Linux architecture admission ledger](decisions/linux-architecture-current.md)
records both file adapter guards at `dbf19a5`. Native AArch64 admits file
operations. Emulated RISC-V rejects them before path admission and can commit
and reopen through a portable ByteStore. Both profiles detect both guard
mutations on both targets. The queue has 9,445 open records
(2,076 runtime / 26 driver / 7,343 tooling).

The [Unix descriptor ledger](decisions/unix-descriptors-current.md) qualifies
eleven real EMFILE outcomes at `49c64fd` on macOS and Docker Linux. Each
case preserves the canonical inode, current and retained images, and
available descriptor count. A same-process retry matches the simulator
reference bytes after freeing handles. Linux repeats both profiles as UID
and GID 1000. Six propagation dispositions leave 9,439 open records
(2,070 runtime / 26 driver / 7,343 tooling).

The [Unix metadata ledger](decisions/unix-metadata-reads-current.md) binds
eight new propagation dispositions at `0e22413`. A shared metadata query
preserves retry and diagnostics. Scripted read failures exercise open and
claim lease release, source link-count validation, and each of the four
final name checks. Complete scratch custody, exact current and retained
reopen, and retry to reference bytes pass in both profiles on macOS and
Docker Linux. Kernel metadata errors remain a deployment evidence gate.
The queue has 9,431 open records (2,062 runtime / 26 driver / 7,343 tooling).

The [Unix file clone ledger](decisions/unix-clone-failures-current.md) records
real EMFILE at both compaction file clones at `309ceae`. Both profiles on
macOS and Docker Linux preserve the canonical inode, bytes and exact
current and retained roots, then retry to the simulator reference image.
Linux repeats both profiles as UID and GID 1000. The nine named controls
cover the new clone paths and the earlier descriptor fixtures. Two new
propagation dispositions leave 9,429 open records
(2,060 runtime / 26 driver / 7,343 tooling).

The [Windows admission ledger](decisions/windows-admission-current.md)
qualifies the shared fixed-drive and NTFS checks, directory and leaf
attributes, and create/write mode selection at `438b6cb`. Both profiles
on macOS and Docker Linux detect all 14 controls. Windows cross-target
Clippy passes. The Win32 entry points and deployment still require native
evidence. The queue has 9,419 open records
(2,050 runtime / 26 driver / 7,343 tooling).

The [Windows handle ledger](decisions/windows-handle-io-current.md) qualifies
the shared lease transitions, I/O dispatch, and error mapping at
`34cb30c`. Both profiles pass the selected engine and storage suites on
macOS and Docker Linux and detect all 13 controls. After release build
deadlines, exact-source replays qualified the affected optimized cases.
Miri on macOS verifies the isolated handle-lifetime fixture. The added
read-only lease and reopen fixture cross-compiles for Windows and retains
its native execution gate. The queue has 9,411 open records
(2,042 runtime / 26 driver / 7,343 tooling).

The [Windows file identity ledger](decisions/windows-rng-results-current.md)
qualifies the shared RNG return handling at `a1e36bf`. Both profiles detect
all four controls on macOS and Docker Linux. The native Windows runner
uses the same two controls and preserves their exact failure assertions.
Native execution remains queued. The queue has 9,408 open records
(2,039 runtime / 26 driver / 7,343 tooling).

The [Windows file open ledger](decisions/windows-open-results-current.md)
qualifies the invalid-handle decision and raw operation diagnostics at
`9dc1e6a`. Both profiles pass the selected suites and detect both controls
on macOS and Docker Linux. The native regression checks missing paths,
an existing file and a held lease, then reopens the current and retained
roots. It cross-compiles for Windows and awaits native execution. The
queue has 9,407 open records (2,038 runtime / 26 driver / 7,343 tooling).

Batch 6 starts with [FFI handle accounting](decisions/ffi-handle-accounting-current.md)
at `be140f5`. Debug and release Rust and shared-library C checks pass on
macOS and Docker Linux. Miri verifies the real allocation-refusal path
with strict provenance. Both hosts catch all three source controls and
both Loom controls. Two runtime propagation gaps close and eight new
tooling records remain open. The queue has 9,413 open records (2,036
runtime / 26 driver / 7,351 tooling). Exact delivered consumers,
additional export decisions and the remainder of batch 6 stay open.

The [FFI boundary ledger](decisions/ffi-boundaries-current.md) adds explicit
controls for input, output, options, path bytes and panic status at
`9f5d81f`. Both profiles on macOS and Docker Linux catch all 23 controls.
The packaged C/C++/Odin consumers and dynamic ABI smoke pass before
and after relocation on both hosts. Native Windows UTF-8
propagation, generated export decisions and remaining delivered profiles stay open.
The queue has 9,391 open records (2,014 runtime / 26 driver / 7,351 tooling).

The [generated C export ledger](decisions/ffi-generated-exports-current.md)
records ten named mutants at `280754c` and the differing revisions for
records and whole-database validation. The private C and C++ packages
verify allocator context lifetime, four open refusals and snapshot retry
through the exact shared libraries. The Rust, C and Odin 134-cell traces
match on macOS and Docker Linux.

The [C file-factory ledger](decisions/ffi-file-factories-current.md)
records four compiled export controls at `c4c0bd4`. Packaged C and C++
consumers sweep 21 create and 43 reopen refusals on macOS and Docker
Linux, preserving exact bytes and both roots through failed reopen.
Native Windows and the remaining generated paths stay open. The queue
has 9,396 open records (2,014 runtime / 26 driver / 7,356 tooling).

The [foreign allocator ledger](decisions/foreign-allocator-current.md) records
callback descriptor and context pairs, plus zero-byte layout dispatch at
`9822b4c`. Debug and release detect all eight controls on macOS and Docker
Linux. Miri passes the eight portable allocator fixtures with strict
provenance. The queue has 9,387 open records (2,005 runtime / 26 driver /
7,356 tooling). Native Windows execution and the remaining provider ownership
decisions stay open.

The [provider ownership ledger](decisions/provider-ownership-current.md)
records the race to release the selected provider, its reference threshold
and shared or boxed allocation refusal at `ab467f8`. Debug and release detect
all nine selected controls on macOS and Docker Linux. The bounded Loom model
verifies the final owner's memory visibility on both hosts and catches a
Relaxed decrement. The queue has 9,376 open records (1,993 runtime /
26 driver / 7,357 tooling). The new model dispatch and native Windows
execution remain open.

The [C transaction and snapshot ledger](decisions/ffi-transaction-reads-current.md)
records six generated exports at `e34a9c2`. A committed deletion advances to
revision 3 while the captured revision 2 still returns the seeded row. An
allocation refusal while acquiring a scan value clears the output and leaves
the scan and its row usable. The C11 and C++17 consumers pass with the exact
relocated packages on macOS and Docker Linux. Native Windows remains open.
The six macro invocations retain their inventory boundaries.

The [typed unique-claim ledger](decisions/typed-unique-claims-current.md)
records eleven controls at `29f4f54` and verifies the current and retained
indexes through replacement, allocation refusal and reopen. macOS and Docker
Linux pass in debug and release. Compiler condition counts record both outcomes
of six guards within named model instantiations. The queue has 9,365 open
records (1,982 runtime / 26 driver / 7,357 tooling). Other typed paths and
native Windows execution remain open.

The [typed index allocator follow-up](decisions/typed-index-allocator-current.md)
fixes eight observed Global requests in a consistency check with a selected
allocator. Check and repair pass with zero Global requests at `2ea387d`, and
the retained view preserves the earlier index state. Both local hosts pass
the typed and selected core suites in debug and release and detect all seven
controls. Miri passes the index allocator and snapshot iterator fixtures.
Live-byte accounting includes the held scan keys. Native Windows remains open.

The [typed migration recovery matrix](decisions/typed-migration-recovery-current.md)
records 62 simulated failures at `7fcd992`. Reopen and retry preserve a complete
old or new migration batch, including its progress row, unique claims and
retained schema. Both local hosts pass debug and release and detect all thirteen
controls. The queue has 9,352 open records (1,969 runtime / 26 driver / 7,357
tooling). Native Windows execution and release-version qualification remain open.

The [derive admission ledger](decisions/derive-admission-current.md) binds the
first `Model`, `Record` and `Resolution` expansion controls, golden generated
schema and existing CRDT wire and sync oracles. Both local hosts pass debug and
release and detect fourteen controls at `e40727b`, including two earlier scan
controls. The portable command fixture also completes 1,040 release suites
under concurrent Linux launch stress. The queue has 9,340 open records
(1,957 runtime / 26 driver / 7,357 tooling). Native Windows execution and
the other generated targets remain open.

The [hosted R1 shard ledger](r1-ci-shards.md) records Linux cancellations
at 20 minutes and the integration shard that reached 60 minutes with passing
mutation reports in both profiles. Unix jobs now have 90 minutes to run the
expanded controls and upload evidence. Hosted verification remains open.

The [snapshot acknowledgment ledger](decisions/snapshot-ack-current.md)
connects 16 controls to exact reopened receipts and sender admission at
`1cf3014`. A failed Ready flush reopens revision 3 and a failed terminal
flush reopens revision 4. Both preserve retained revision 2. Debug and release
pass on macOS and Docker Linux, and Miri passes the flush fixture. The queue
has 9,332 open records (1,949 runtime / 26 driver / 7,357 tooling). Native
Windows and delivered transport fault qualification remain open.

The [snapshot capture ledger](decisions/snapshot-capture-current.md) checks
43 destination faults during live capture and 18 read faults during reopen at
`729d3ba`. It verifies exact bytes and current and retained roots after retry,
and preserves the sender's earlier verified bound and follower receipt on
failure. The queue has 9,310 open records (1,927 runtime / 26 driver / 7,357
tooling). Native Windows and deployed transport and device cases remain open.

The [snapshot sender ledger](decisions/snapshot-sender-current.md) verifies
poll, initial and terminal reply failures, progress preservation, quiescent
capture and native saved-image reopen at `d5e06b2`. All explicit source
records under `src/replication/` now have scoped dispositions. The inventory
has 9,284 open records (1,901 runtime / 26 driver / 7,357 tooling). Native
Windows and application transport and deployment evidence remain open.

The [CRDT admission ledger](decisions/crdt-admission-current.md) verifies
actor and policy rejections in both sync roles at `50086ab`. The current row
at revision 4 and the retained row at revision 2 survive reopen. A valid peer
advances to revision 5, and repeated sync returns the same receipt without a
commit.

The [register causality ledger](decisions/crdt-causality-current.md) checks
exact wire bytes at 5,461 bounded event prefixes, distinguishes genuine
retirements from unsupported histories, and verifies minimum and maximum
resolution at sequence exhaustion. An edit by a resolver for the other actor
emits no receipt and preserves the current and retained rows. The inventory
still has 9,284 open records. Native Windows and deployed peer qualification
remain open.

The [coordinator and client-journal batch](decisions/coordinator-service-current.md)
groups admission, dispatch, receipt replay and journal recovery in one PR.
The replay case preserves current revision 6, retained revision 4 and the
durable image. All 32 named controls pass in debug and release on macOS and
Docker Linux. The R1 queue is 9,262 records (1,877 runtime / 26 driver /
7,359 tooling). Hosted Windows passed the debug and release suites, and
Windows and Linux both passed the packaged pending-restore replay after
download. The subsequent native Windows mutation campaign passed. Deployed
operator qualification remains open. Subsequent work follows the
[grouped roadmap](production-pr-roadmap.md#stack-order) through candidate freeze.

The [compound recovery batch](decisions/compound-recovery-current.md) exercises
selected-allocator refusal and competing disk faults, plus bounded-memory
reopen under repeated simulated crashes. The corrected oracle passes 44,040
macOS ASan executions. Broader backend schedules and physical device
qualification remain open.

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

The [accelerated month at 5f49a59](monthly-workload.md#recovery-checkpoint-at-5f49a59)
adds three same-source workload runs to R3, including Docker Linux/ext4 with
a macOS client. The supported deployment and its sustained soak remain open.

The [600-second native soaks at 5f49a59](soak.md#workload-checkpoint-at-5f49a59)
verified 701 macOS and 3,202 Docker Linux rounds through backup and restore.
The month report now includes per-peer commit and reopen tails. Qualification
on the intended deployment, owner-approved budgets and off-machine proof
custody remain open.

The [Windows coordinator campaign](mutations.md#windows-coordinator-campaign)
now selects the same 32 cases as the Unix controller. Its source pins and
per-case assertion diagnostics passed native Windows debug and release.
Independent review verified all 128 outcomes and saved test binaries. The reconciled inventory includes
10,627 source records across 222 files after the workload and controller
changes.

The [49ddab5 delivery checkpoint](delivery-checkpoint-49ddab5.md) verified
all 16 private packages, three benchmark runs and the 22-target fuzz
campaign against one source. The fuzz archive restored its empty failure
directories and passed offline verification. R7 still requires a final
approved source and off-machine proof custody.

The [follower decoder error pairs](decisions/replica-decode-current.md)
at `243bb55` passed on arm64 macOS and Docker Linux in debug and release.
Both mutants failed the designated test in every campaign. The four campaigns
produced 44 command reports. The local proof SHA-256 is
`1136ed15fded883e479f2f694a3bc78d7891c8e1e7e3bc8b2691069f30926fa8`.
Reconciliation leaves 1,875 runtime, 26 driver and 7,384 tooling gaps.

For the Unix descriptor controls, a child libtest failure now appears as
indented diagnostic text inside the parent's result. The original source
and target duplicate failures on hosted Linux both showed the expected
`Busy` versus `Io` assertion. At `e8066ab` all six descriptor controls
passed in debug and release on arm64 macOS and Docker Linux under the
unchanged strict transcript verifier.

The [hosted fuzz campaign](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45604498847)
tested merge commit `4a37229e1b3be3835e79a346dc325df9357a0c3e`.
The downloaded ZIP matched GitHub's SHA-256
`81f7984f935fe13b73c676b6aeeddcf38f0a1df1342f3c91cc56ae2749a3441a`.
Replaying its tar archive against that commit verified all 22 targets and
20,918 completed executions. As a negative control, replaying the earlier
loose-file download against its own commit failed at the missing
`open_image_bytes/artifacts` directory. This qualifies restoration of
this bounded CI campaign. The final release still needs same-source fuzz evidence.

The [64-root matched SQLite workload](sqlite-baseline.md#sixty-four-retained-roots-on-macos-and-linux)
passed all current and retained byte checks across three rounds per engine
on both macOS/APFS and Docker Linux/ext4 at `49ddab5`. R3 still needs
deployment-device measurements, concurrency and agreed service budgets.

### Retain admission and counter exhaustion

Four R1 controls at `0697196` reuse three existing tests of
`Database::retain_snapshot`.

| Decision | Admitted input | Rejected input | Named mutation |
| --- | --- | --- | --- |
| Snapshot belongs to this database | A local snapshot receives ID 1 | A snapshot from another database returns `ForeignStore` before ID 1 is assigned | Invert pointer equality |
| Retained catalog has space | Entry 64 receives ID 64 | Attempting entry 65 returns `Limit` and leaves the revision unchanged. Reopen sees 64 entries. Releasing one allows ID 65. | Reject when 63 slots are occupied |
| Publication revision can advance | Revision advances from `u64::MAX - 1` to `u64::MAX` | The next retain returns `Exhausted` and preserves revision, IDs and live-byte usage | Substitute `Limit` on revision overflow |
| Next retention ID has a successor | The final assignable ID is `u64::MAX - 1` | The next retain returns `Exhausted` and preserves the revision and last assigned ID | Substitute `Limit` on ID overflow |

The owner and capacity pairs use the simulated durable store. The two
overflow pairs use in-memory state initialized immediately below each
boundary. The latter exercise error propagation at the retain caller.

All four original controls passed and all four mutations failed the designated
test in debug and release on arm64 macOS and Docker Linux. The four campaigns
retain 76 command reports in `retain-admission-0697196.tar.gz`, SHA-256
`87d5e11d55b4312dcf68fdeaab6d77178a02cc6243c25130ff8db677de0d8093`.
Reconcile with `cargo xtask mutations --case r1-retain-admission- --toolchain
1.100.0 --output "$OUTPUT"` and repeat with `--release` and a fresh output.
The inventory now has 1,871 runtime, 26 driver and 7,388 tooling gaps.

### Retention publication errors and retry

At `61bb3ed`, the retention fault test retries both a retain and a release
on the same writer immediately after each injected publication I/O failure.
Both calls must report `RecoveryRequired`, the simulator operation count
must stay unchanged, and a held snapshot must still read its historical row.
The test then crashes and reopens the database under each existing
persistence schedule and verifies the recovered catalog, current value and
each retained value.

| Caller boundary | Successful path | Error path and expected observation |
| --- | --- | --- |
| Storage guard in `lock_writer`, then writer lock in `publication_state` | Healthy storage admits the writer | Recovery-required storage rejects entry before another byte-store operation |
| Publication state in each of retain and release | A healthy operation can publish | Both calls propagate `RecoveryRequired` on the same poisoned writer |
| Head allocation in each operation | The new catalog survives reopen | Allocator refusal returns `OutOfMemory`; the old revision, catalog, bytes and live allocations are preserved |
| Publish in each operation | The new catalog becomes visible | Simulated I/O failure propagates `Io` or `OutcomeUnknown`; a definite failure reopens the old catalog |
| Release lookup | An existing ID can be released | Releasing the absent ID after reopen returns `NotFound` |
| Release revision increment | Publication can reach the maximum revision | The next release reports `Exhausted` and leaves the catalog unchanged |

On arm64 macOS and Docker Linux, debug and release accepted the original
source and caught every one of the ten mutations. All four campaigns
verified process cleanup. The 152 command reports and their transcripts
are retained in `retention-flow-61bb3ed.tar.gz`, SHA-256
`c1a31dc4f9d6fe95f437ca1df004c39caf04e57ce5dc177bcc9135ad31305d42`.
The reconciled inventory has 1,861 runtime, 26 driver and 7,388 tooling gaps.

### Native Windows coordinator evidence

The [hosted Windows coordinator job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45604499033/artifacts/15017677903)
at test merge `4a37229e1b3be3835e79a346dc325df9357a0c3e` ran all 32 coordinator controls in debug and release. The downloaded
ZIP matched GitHub's SHA-256
`3ed68b8ab235b30a04d5d3fab88d60ef0e482b15470560c29dc19044a80e727e`.
Review matched 863 tracked files, original and mutated source, executable
and transcript hashes, exact assertion text and cleanup for all 128 test
outcomes. This Windows run covers the coordinator controls only.

### Reading a retained catalog under allocator refusal

The catalog-read fixture at `12f6b81` opens a durable database with one
historical root and a different current value. It refuses every request
to the database allocator while exercising these cases.

| Call | Observed result |
| --- | --- |
| Look up missing ID 2 | `NotFound` |
| Look up retained ID 1 | The snapshot reads the historical value |
| List IDs while the selected allocator refuses the catalog buffer | `OutOfMemory`, propagated through both the buffer reservation and public listing call |
| List IDs after clearing the refusal | The caller receives exactly ID 1 |

The fixture observes one refused allocation across the three calls under
refusal, unchanged live bytes, revision and durable image, and zero additional
byte-store operations through the successful retry. After close the custom
allocator has no outstanding allocations. A fresh open reads the original
catalog and current value. The allocation failure covers the engine's
catalog buffer. Conversion into the caller-owned Rust vector occurs later.

All three named controls caught their designated mutation in debug and
release on arm64 macOS and Docker Linux. Review confirmed the exact
`NotFound` and `OutOfMemory` assertion diagnostics and process cleanup.
The 56 command reports and transcripts are in `catalog-read-12f6b81.tar.gz`,
SHA-256 `fbc15318013ee037a451f9382bf2218637c08fe38c8ac4b0d888e294594db78c`.
Reconciliation leaves 1,858 runtime, 26 driver and 7,388 tooling gaps.

### Hosted package artifacts at the PR #82 test merge

Both hosted x64 delivery jobs used Rust 1.100.0 and clean source
`4a37229e1b3be3835e79a346dc325df9357a0c3e`. A fresh download and
extraction matched the GitHub archive digest and every file in each
job's recorded artifact inventory.

| Download | SHA-256 | Files checked |
| --- | --- | ---: |
| [Linux x64](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45604499033/artifacts/15019070168) | `c6733c662d8659e0e1e1f40503ce5ca87936ad34444456d0f2fe118df1194330` | 4,034 |
| [Windows x64](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45604499033/artifacts/15018521638) | `dbd2ca096c5d3484d6b6b704c6bf8ed63863134b30b2dfc340e933b8a20fdabd` | 2,287 |

The combined, base, CRDT-only and replication-only packages on each
platform contained 60, 5, 16 and 23 files respectively under
`notices/`. Their relative paths and contents matched the corresponding
[previously reviewed packages](locked-dependency-review-afffd2d.md),
including the inventories themselves. The local proof retains both ZIPs,
the complete tested Git bundle and the hosted fuzz ZIP for the same merge.

### macOS compatibility at the PR #82 test merge

Three native arm64 macOS/APFS compatibility runs built both endpoint
consumers from their original lockfiles with Rust 1.100.0. The current
endpoint in every pair was `4a37229e1b3be3835e79a346dc325df9357a0c3e`.

| Profile | Historical endpoint | Verified behavior |
| --- | --- | --- |
| Storage | `a7dc343` | Each reader opened both writers' images at revision 4 with retained revision 2. Reading and rejecting an unsupported format preserved the respective image bytes. |
| Snapshot | `dcd2402` | Each sender shipped revision 4 to the opposite follower, which reopened with retained revision 2 and repeated the durable receipt. The primary independently reached revision 5. |
| Typed | `1ca5e5a` | A staged migration exited before committing. Reopen preserved the original bytes. The historical client preserved unknown fields on edit, and rollback from the pre-upgrade backup allowed historical writes without changing the upgraded database. |

The three reports passed, with 86 command records. Review independently
rehashed the six consumer binaries, six identity probes, six original
lockfile copies and all 305 recorded artifact entries. The local proof
`delivery-4a37229/compat-macos.tar.gz` has SHA-256
`b95d620afaa9819c6b0867e583136a3c158348b9eeffd08df6d3312ed999cec8`.
R4 still requires an agreed support window and its release endpoints.

### Restored four-platform delivery at 4a37229

The arm64 macOS/APFS and Docker Linux/ext4 delivery runs completed at
the same PR #82 test merge as the hosted x64 packages. All four lanes
passed their Rust 1.100.0 debug/release workspace checks and delivered
consumer traces. The local Linux setup initially lacked the linker driver
for Odin. The successful run used the image with Clang, an ext4 build and
runtime directory, and networking disabled.

A fresh extraction verified all 16 packages together with three 17-case
macOS benchmark samples and all 22 targets in the hosted fuzz archive.
The newly restored arm64 inventories contained 4,039 files on macOS and
4,042 on Linux. Across the full matrix a separate audit checked 132
dependency entries and matched all 272 recorded package notice files to
their sources.

An offline Linux container then checked the saved input hashes, cloned
the saved Git bundle, rebuilt the verifier, extracted the four package
lanes, benchmark and fuzz evidence, and reproduced the macOS report.
Only the verifier executable hash differed. The [checkpoint checksums](delivery-checkpoint-4a37229.sha256)
identify the raw archives, notice audit and both verification receipts.
The restore ran on the development host. Off-machine custody, deployment
acceptance and final release qualification remain open.

The same three historical endpoints also passed against `4a37229` on
arm64 Linux/ext4 in a network-disabled container. After extraction on
macOS, the review checked the Linux consumer binaries, identity probes,
original lockfiles and all 305 artifact entries. Each pair used the same
source archives, fixture and original lockfile hashes on both platforms.
The Linux report contains 30 storage, 24 snapshot and 32 typed command
records. The [checksum manifest](delivery-checkpoint-4a37229.sha256)
includes the Linux compatibility archive.

The release workflow now qualifies the selected checkout against these
historical endpoints on macOS, Linux and Windows during manual and tag
runs. The exact workflow shell step passed all three profiles in a fresh
shallow macOS checkout, including retrieval of the historical objects.
Actionlint 1.7.12 passed. The new hosted release job still needs its first
execution, including native Windows.

### Distributed CLI upgrades at 4a37229

Native macOS arm64 and Docker Linux arm64 on ext4 passed both CLI upgrade
profiles against candidate `4a37229`.

- Snapshot replication from `315068f` produced durable follower receipts for
  revisions 3, 4 and 5 and preserved retained revision 2. With an incorrect
  certificate pin, local capture reached 4 while the follower stayed at 3
  with identical files. Repeat polling also left its files unchanged.
- Study peers from `fb80989` converged before upgrade. During a mixed-version
  sync, protocol negotiation failed with zero acknowledgments and preserved
  local commits. After both peers upgraded, they converged on total 36, goal
  80, limit 20 and lessons 1–4. Both retained roots survived. Repeating sync
  and restarting added no commit. Controller `9554b4c` qualified the
  test-only module addition using actual release build dependencies. The
  earlier controller stopped before launching the peers.
- Fresh extraction verified 205 snapshot artifact hashes and 45 Study
  artifact hashes per platform, the three exported snapshot digests, both
  Study dependency files and the absence of private key files. The
  [checksum manifest](delivery-checkpoint-4a37229.sha256) identifies both
  archives and the controller source bundle.

The five-profile release workflow shell step also passed in a fresh shallow
macOS checkout at `9554b4c`. Hosted release execution and native Windows
results for the selected endpoints remain open.

### Windows delivered DLL traces at 4a37229

Two Windows x64 builds of the same source produced distinct DLL bytes.
Each trace matched its own original package manifest, DLL and loaded-module
attestations. The release-matrix package (DLL SHA-256 beginning
`d45cc978`) passed 30 cases across C, C++ and Odin. The private package
from PR #82 run 45604499033 (`1596fd13`) passed 20 across C and C++.
For each consumer, all 134 full-trace lines matched the frozen source
oracle byte for byte. All five success cases matched expected output and
reported the same process and DLL before and after execution. Each of
the five rejection cases exited unsuccessfully without trace output or
a completion attestation. Post-download verification rehashed all 1,129
release trace entries and all 170 PR trace entries. The
[archive manifest](delivery-checkpoint-4a37229.sha256) pins both package
and trace downloads. These are native ABI results for this checkpoint.
The Windows distributed CLI upgrade gate remains open.

A checkout control for the five-profile candidate workflow reproduced a
different fixture digest with Git automatic CRLF conversion enabled. The
same setting changed every historical lock during the independent prefetch
step. With the Windows checkout setting applied, the Study fixture matched
its committed blob. With the archive settings applied, all five locks
matched their historical blobs, even when the surrounding Git configuration
still requested CRLF conversion. The workflow checks those lock identities
before fetching dependencies. Actionlint 1.7.12 passed. This control ran
on macOS and covers Git byte conversion only.

### Five-profile compatibility workflow at 63eed2c

The exact release shell step passed all five profiles on native macOS arm64
and Docker Linux arm64/ext4. Both began from shallow checkouts. A local Git
mirror supplied the historical objects and cached registry dependencies
supplied Cargo. Linux ran without network access. All ten profile runs used
candidate `63eed2c` and their configured historical endpoints. Fresh archive
reads verified 555 declared files on each host (102 storage, 96 snapshot,
107 typed, 205 snapshot CLI, 45 Study CLI), the worker receipt, and all
three exported snapshot digests. Both archives contain the same historical
and candidate source tar hashes and exclude private keys. The source
bundle, exact workflow step, logs and filtered archives are pinned in the
[compatibility manifest](candidate-compatibility-63eed2c.sha256). Hosted
release and native Windows qualification remain open for this revision.

### Release fuzz workflow and stable replay

Previously only the weekly schedule selected the extended ASan campaign.
Manual qualification runs and tags now request 100,000 executions or 120
seconds per target, whichever bound is reached first. They use the pinned
`nightly-2026-11-12` compiler. Weekly runs retain rolling nightly at the same
bounds. Pull requests and ordinary main pushes retain 1,000 executions or
20 seconds per target. Extended jobs have a 90-minute outer timeout and
replay the stable library tests in both debug and release. Each replay
retains Cargo's transcript in the workflow artifact.

At source `4a37229` the workflow replay step passed all 33 tests in each
profile on native macOS arm64 using Rust 1.100.0. The
[checkpoint manifest](delivery-checkpoint-4a37229.sha256) pins both logs.
An invocation check confirmed all three toolchain/budget combinations and
verified that an injected nonzero status from either replay fails the shell
step despite transcript capture. Actionlint 1.7.12 passed. Hosted extended
qualification requires its own restored campaign receipt.

### Hosted sanitizer and atomic model results at 4a37229

PR #82 run 45604499033 built the same merge revision as the four-platform
package checkpoint. Across its four diagnostic campaigns, the downloaded
archives matched GitHub's digests. All 363 recorded source hashes in each
campaign matched the frozen checkout. All 41 command records matched the
saved process reports and output lengths and verified process cleanup.

| Campaign | Observed result |
|---|---|
| Linux workspace ASan | 937 passed, 8 ignored across 65 test summaries |
| macOS workspace TSan | 893 passed, 8 ignored across 65 test summaries. Both deliberate races emitted a TSan data-race report and exited by SIGABRT. The errno reuse control succeeded. |
| Linux C/Rust ASan | All three memory, thread and file consumers succeeded. Both invalid calibration cases reported a heap-buffer overflow and exited 86. Both valid controls succeeded. |
| Bounded atomic models | All three baselines succeeded. All five broken variants exited 101 with the required assertion text. |

The [checkpoint manifest](delivery-checkpoint-4a37229.sha256) pins the three
downloaded archives. These results cover instrumented builds. The exact
delivered libraries have their separate consumer traces.

### R1 snapshot get, scan and count at 61a154a

The existing `snapshot_collection_and_key_errors_have_valid_controls`
test supplies the following pairs using one database and one snapshot.

| Decision or propagation | Successful input | Rejected input |
|---|---|---|
| Shared key length guard and snapshot get propagation | Empty key at the configured zero-byte limit returns absence | One byte exceeds the limit and returns `Limit` |
| Collection ownership for get, scan and count | Local collection passes ownership validation | The same numeric ID from another database returns `ForeignStore` |
| Collection lookup for get, scan and count | Existing empty collection returns absence, empty rows or zero | Missing ID in the same database returns `NotFound` |

Eight named mutations change the key boundary or substitute an incorrect
error at one of the seven propagation sites. The named test caught all eight
in debug and release on native macOS arm64 and Docker Linux arm64. Fresh
extraction verified the source bindings, test diagnostics and process
cleanup for 116 command receipts. The macOS archive SHA-256 is
`f7be44f868693b1bbacb6c11989153c3172dda9eb0a870d6a4349be220a2668c`
and the Linux archive SHA-256 is
`fe401ab8c6c647eca464abcba75fbdd073b43ce1245b266557986c71f267a7d6`.
Reconciliation found 10,639 decisions, zero binding failures and 1,850
runtime, 26 driver and 7,396 tooling gaps. Compiler MC/DC remains open.

### Same-source distributed workloads at 4a37229

The authenticated snapshot month and two-peer Study workload ran against
the four-platform package checkpoint. Both used a native macOS arm64/APFS
client and Docker Linux arm64/ext4 on the same physical Mac. Both compilers
reported Rust 1.100.0. Both campaigns retired their owned container and
volume. The [checkpoint manifest](delivery-checkpoint-4a37229.sha256)
pins the workload archives.

The snapshot primary made 31 installations and 32 polls. On each of the
30 workload days its local revision was one ahead of the captured revision.
Days 7, 14, 21 and 28 lost the shipping reply. A separate poll returned the
durable captured revision in each case. The final export and reopened
follower matched byte for byte at revision 93 (36,851,712 bytes).
Fresh extraction verified 124 artifact hashes and the 31 raw CSV rows.

| Snapshot operation | Samples | Median | p95 |
|---|---:|---:|---:|
| Local commit | 30 | 11.7 ms | 17.6 ms |
| Capture | 30 | 114.6 ms | 265.2 ms |
| Ship with a reply | 26 | 151.2 ms | 279.0 ms |
| Poll | 30 | 1.4 ms | 14.7 ms |

Percentiles use nearest rank across workload days. The final catch-up export
is excluded. These measurements include Docker Desktop networking and
concurrent host qualification activity.

Both Study peers converged each round and held their revisions on repeat
sync. Six paired interruptions preserved the recorded states on reopening.
At round 30 both peers reported revision 64, 1,395 minutes, weekly goal
230, daily limit 10 and lessons 2–61. The archive restored both final
database hashes, the round observations and all 14 process exit receipts.
It contains 231 of the 233 inventoried artifacts. The two synthetic private
keys were hash-checked in the private working directory and omitted from
the archive. Intended-device operation and owner-approved latency budgets
remain open.

### Hosted R1 controls and operator recovery at 4a37229

All 28 checks on PR #82 head `54e1891` passed. Its tested merge revision
was `4a37229`. All eleven R1 archives matched GitHub's published digests.
Their inventory and review JSON files were byte-identical across platforms.
The nine Linux shards caught exactly the 1,029 eligible named controls in
each profile. The Darwin job caught the selected 126 in each profile.
The post-download audit matched all 8,860 Unix process reports to their
transcript lengths, cleanups and exit codes. Every expected baseline ran a
nonempty passing test selection, and every mutant returned 101 with its
recorded failure diagnostic. The separate Windows coordinator result
covers 32 controls in both profiles. This CI run predates the local
retention and snapshot-read control additions.

Native macOS/APFS and Docker Linux/ext4 also ran operator recovery against
debug and release builds of the administrative and coordinator CLIs at
this revision. Each of the four profiles completed all 85 commands.
Recovery preserved the current value 21 at revision 4 and retained value
11 at revision 2. With both coordinator journals restored, the two
resumed writes received revisions 6 and 7 and finished at value 111.
Mixed journals, a stale server and the previous credential were rejected.
Fresh extraction checked the four pairs of CLI binaries and all 286
declared artifact hashes per profile. The [checkpoint manifest](delivery-checkpoint-4a37229.sha256)
pins the raw archives and binaries. Intended deployment fencing and
operator acceptance remain open.

The [extended campaign at 4a37229](fuzz-qualification-4a37229.md) passed
1,399,569 ASan executions across all 22 targets. Independent macOS and
Docker Linux restoration bound the results to the checkpoint's 16 packages
and three benchmark runs. The candidate operator workflow now also
[rehearses debug and optimized recovery](r2-qualification.md#candidate-operator-recovery)
on manual and tagged runs.

### R1 collection creation and removal at e73afbd

Fourteen source records in `check_collection`, `create_collection` and
`drop_collection` now have named controls. Two are single-condition
decisions and twelve are error-propagation sites.

| Operation | Passing case | Error case and observation |
|---|---|---|
| Validate collection ownership | The local collection ID resolves | The same numeric ID from another database returns `ForeignStore` |
| Remove a collection | An existing collection is removed and its pinned snapshot remains readable | A missing collection returns `NotFound` |
| Admit create or drop | A writable database accepts the operation | After a publication I/O error both operations return `RecoveryRequired` |
| Increment revisions or allocate the next collection ID | The final representable revision or assignable ID succeeds | Exhaustion returns `Exhausted` without advancing the revision |
| Prepare the catalog tree and head | With sufficient allocations, the current revision advances once | Selected-allocator refusal during insertion, removal or head allocation preserves current and pinned collections and both live-byte counts |
| Publish create or drop | After reopening, the next ID can be created and the original collection dropped | A simulated first I/O failure returns `Io`. After a discard crash, both the current and retained roots reopen with the original two records |

All fourteen mutants failed their designated test in debug and release on
macOS arm64 and Docker Linux arm64. Fresh extraction rechecked all 204
process receipts, named baseline and mutant outcomes, cleanup, source
bindings and transcript lengths. All twelve mutation registry tests and
strict Clippy for the affected integration test passed. The
[archive manifest](decisions/catalog-lifecycle-e73afbd.sha256) pins both
campaign archives and the tested source. Reconciliation recorded 10,639
decisions, zero binding failures, and 1,836 runtime, 26 driver and 7,396
tooling gaps.

### Native Windows fixed-endpoint upgrades at PR #82

The Windows x64/NTFS job on test merge `1029d2c` passed its five historical
profiles. The engine endpoints differed by profile.

| Profile | Historical | Successor |
|---|---|---|
| Embedded storage | `a7dc343` | `21e5d26` |
| Snapshot wire | `dcd2402` | `315068f` |
| Typed migration | `1ca5e5a` | `6cc8ef3` |
| Snapshot CLI | `315068f` | `365daf6` |
| Study CLI | `fb80989` | `365daf6` |

The four downloaded archives matched GitHub's digests. Review verified
416 artifact hashes and the stdout, stderr and cleanup receipts for all
176 commands. The five explicitly stopped snapshot listeners retired
with the expected supervised status. Snapshot exports matched the durable
receipts at revisions 3, 4 and 5. An incorrect pin preserved follower
revision 3 while local capture reached 4. Study peers rejected the mixed
protocol and converged after both upgraded, with stable repeat sync and
restart. Typed recovery preserved the precommit state, unknown fields and
indexes. The Windows gate using the selected *candidate* successor still
requires a release run.

A subsequent [Study dependency check](study-cli-upgrade.md) replayed both
saved Windows dependency files and three altered controls per file.
macOS and Docker Linux both passed the full Study upgrade to PR head
`2e9f717` with controller `855573c`. Freshly extracted evidence matched
all 45 worker artifact hashes on each platform. The [checksum manifest](windows-compat-1029d2c.sha256)
pins all four Windows downloads and both follow-up archives.

### Reducing repeated builds in R1 campaigns

The [shard scheduling change](r1-ci-shards.md#grouping-pristine-baselines-within-a-shard)
groups pristine baselines inside each job. Reconciliation of `23c97c7`
counts 10,640 source decisions, including one additional tooling loop.
The open counts are 1,836 runtime, 26 driver and 7,397 tooling records.
All 14 catalog-lifecycle controls were caught in debug and release on
both macOS and Linux. Five distinct baselines per campaign now share
compilation work. Each of the four comparisons retained all 47 test
and clean commands and reduced total supervised time. A wrong-test
negative control stopped before the first mutation.

### Writer recovery and primary enrollment

Four engine inventory records now have controls on `2c5a95f`. The two
records for the optional storage use the same state transition because
the inventory records both the conditional and its pattern.

| Source decision | Passing control | Alternative and observable outcome |
|---|---|---|
| Propagate writer lock error | A healthy mutex admits collection creation | A poisoned mutex returns `RecoveryRequired` and the acknowledged snapshot stays readable |
| Enter the optional storage branch | A memory-only writer admits collection creation | A byte-backed writer checks storage health before enrolling a primary |
| Match the storage pattern | A memory-only writer skips the storage guard | A present storage object with an injected publication failure rejects enrollment before any further I/O |
| Match the mutex result in `requires_recovery` | A healthy memory writer reports false | The same writer reports true after mutex poison |

The existing retention fault test continued to pass when a mutation
bypassed the first storage guard. Later publication checks still blocked
the write. The added regression exercises `Primary::new`, which needs a
healthy source before creating an enrollment. It checks a healthy durable
source, a memory-only source, failure at the first publication I/O, and a
crash and reopen at the acknowledged revision with the original enrollment.

All four mutations were caught on macOS and Docker Linux in both debug and
release. Review of the extracted evidence checked the 16 named failures,
72 supervised command receipts and copied source hashes. The regular macOS
replication suite passed 10 tests and skipped its separately scheduled
native manifest-growth workload. Reconciliation leaves 1,832 runtime,
26 driver and 7,397 tooling gaps. The [evidence manifest](decisions/writer-recovery-2c5a95f.sha256)
pins the source bundle and both archives.

### Packaged coordinator recovery on native Linux and Windows

[Hosted run 45616580373](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45616580373)
exercised private packages built from merge source `1029d2c` on x86-64
Linux and Windows. Both runners built debug and release coordinator tools
from the package and ran them from a relocated directory. Each runner
then downloaded its uploaded package and repeated both profiles. The four
archive digests match the [local custody manifest](coordinator-recovery-1029d2c.sha256).

An independent archive read matched all 942 Linux and 949 Windows payload
hashes and all 865 source pins per package against the Git tree. All four
campaigns recorded the same package-manifest digest before and after
recovery on their respective platform. The review checked 40 command
receipts, 116 campaign artifact hashes and the retained coordinator and
driver executables against the hashes recorded for their profile.

Each of the eight profile runs restored both journal prefixes, with and
without forwarding the pending write. The empty prefix finished at
revision 5 with value 42 and receipt sequence 1. The populated prefix
finished at revision 6 with value 99 and receipt sequence 2. All 32 cases
lost the second reply, replayed locally with a stable result, preserved
the two retained roots and left the original files intact. Qualification
under the intended deployment account and on the selected release source
remains open.

### Database constructor error propagation

Seven source records in `Database::with_options`, `create_on_with_options`
and `open_on_with_options` now bind the existing tests to individual
constructor failures. Each mutation translates one error at its call site
to `Invalid`. The expected result distinguishes allocator refusal from
store-identity exhaustion.

| Call path | Failure witness | Success and state checks |
|---|---|---|
| Allocate budget control and initial head | Creation refuses allocations 0 and 1 respectively with `OutOfMemory` | The allocation sweep ends with revision 0 and the supplied identity |
| Allocate the process-local store identity | The isolated child reaches the end of the identity range and receives `Exhausted` | The previous store keeps the final assignable identity and its readable revision |
| Initialize a database and create its durable image | The creation sweep refuses both initialization and storage preparation | No write precedes a refusal, allocation charges retire, and the lease can be reacquired |
| Initialize a database and recover its durable image | The reopen sweep returns `OutOfMemory` during both initialization and graph recovery | The image stays byte-identical and a fresh opener verifies the current and retained values |

All seven mutations on `cb2202e` triggered the intended test failure in
both debug and release on macOS and Docker Linux. Review of the extracted
archives checked all 28 failures, 112 supervised command receipts and the
copied engine, test and registry hashes. The [archive checksums](decisions/constructor-cb2202e.sha256)
identify both campaigns and their source bundle. The inventory has
1,825 runtime, 26 driver and 7,397 tooling gaps remaining.

The identity mutation exposed a weakness in the subprocess test. The
parent accepted any successful child exit, and its diagnostic omitted the
child harness's captured assertion. On `7b37843` the parent requires the
named child test to pass and includes the child's stdout and stderr on
failure. Both hosts passed the normal test in debug and release. Selecting
a nonexistent child produced a zero exit with zero child tests and failed
the parent. Breaking the last-identity assertion also failed the parent
and exposed the child's actual identity value in its diagnostic.
The [negative-control checksums](decisions/identity-child-7b37843.sha256)
pin the logs, reproduction script and source.

### Native file constructor admission

Five propagation controls exercise validation, entropy and backend errors
during file creation and validation and backend errors during reopen.
The invalid-limits test now supplies a path that panics if accessed on
reopen. The valid control creates and reopens revision 0 with a known
identity. The native file tests also check that an attempted duplicate
creation preserves the original bytes and that a second opener respects
the writer lease.

On `79cb0ef`, all five mutations triggered the intended failure in debug
and release on macOS and Docker Linux. The [proof manifest](decisions/native-ctor-79cb0ef.sha256)
pins the four campaigns (20 catches, 84 supervised commands) and the
allocation, transaction and engine decision suites (31 named tests per
host and profile). Windows execution of this batch remains pending.

Editing the engine test module changed the full-file hash at `7b37843`.
That temporarily returned 15 historically classified engine records to
the open queue. The [source comparison](decisions/typed-index-allocator-current.md#engine-source-binding-after-constructor-test-changes)
verified the unchanged production bytes and all 150 engine inventory
records before restoring those classifications.
Reconciliation leaves 1,820 runtime, 26 driver and 7,397 tooling gaps.

### Publication, snapshot capture and owned scans

Thirteen decisions in `src/engine.rs` now have named controls on `74644aa`.

| Path | Outcomes checked |
|---|---|
| Publish a revision | A memory database advances without a backend. A durable database persists before installing the new head. A failed write propagates its I/O error and preserves the acknowledged revision and retained snapshot. |
| Inspect storage space and enroll a primary | A healthy byte store succeeds. A memory-only database returns `Invalid` and a writer requiring recovery returns `RecoveryRequired`. |
| Capture a live replication snapshot | The same source checks apply. A capture budget too small to copy the retained entries returns `Limit`. A retry captures the acknowledged revision. |
| Copy the retained catalog | The allocation sweep checks both successful preparation and allocator refusal before publication. |
| Iterate an owned snapshot scan | The iterator succeeds for a valid empty collection and distinguishes a foreign ID from a missing local collection. |

All thirteen mutations triggered their named failures on macOS and Docker
Linux in both profiles. The [checksum manifest](decisions/engine-handoff-74644aa.sha256)
pins the archives and the source bundle. Review checked 52 catches and
196 supervised commands. The first macOS release run detected that the
checkout changed while it was running. Its replacement ran from a detached
checkout and passed the source-identity check.

The follow-up test on `0b0352c` recorded zero destination I/O for the
rejected captures and observed writes on the successful retry. It passed
on both platforms in debug and release. All 150 inventory records in
`src/engine.rs` now have a disposition: 134 have named controls, 15 retain
their historical scoped classification and one shares an enclosing
decision. Across the project, 1,807 runtime, 26 driver and 7,397 tooling
gaps remain. Compiler MC/DC remains open.

### Schema decisions after allocator routing

The schema source has 176 decisions. The [current audit](decisions/schema-allocator-current.md)
maps the original rows to the current byte ranges. Five map enumerations
switched to allocator-owned scratch. An allocation sweep now checks every
request observed during descriptor decoding, successor validation, record
validation and record decoding. Each refusal returns the engine's
`OutOfMemory`, releases its scratch and permits a successful retry.

Five mutations individually changed the error at the five enumeration
sites. All five triggered the named regression in debug and release on
macOS and Docker Linux. The same 25 schema tests passed on all four runs.
The [archive manifest](decisions/schema-scratch-7f7a2b6.sha256) identifies
the source bundle and both archived campaigns. Review verified 20 catches
and 80 supervised commands.

Of the 171 historical rows, 35 retain predicate evidence, 49 retain
execution evidence, 34 have invariant proofs, 16 share enclosing decisions
and 37 delegate propagation to the engine. The current ledger has 1,631
runtime, 26 driver and 7,397 tooling gaps. These counts describe source
reconciliation; compiler-measured MC/DC remains open.

### Logical export and restore decisions

The [logical storage ledger](decisions/logical-storage-current.md) now
accounts for all 152 source records. Four changed propagation sites have
named controls. The other 148 retain their historical classifications.
A separate mutation billed map iteration to the value's allocator. The
new regression caught it by refusing allocation on the writer while the
value's allocator remained available. Its success control checked the
exact wire bytes of a nested map constructed in reverse key order.

The physical-size preflight returned the expected error for a shared graph
whose logical expansion exceeded the restore limits. The committed logical
fixture also round-tripped byte for byte with its current and retained values.
A separate exploratory mutation at the final output transfer survived.
Source inspection confirmed that logical export's Global buffer is returned
without a copy or allocation there. The earlier Global refusal path passed
its dedicated test in both profiles on both hosts.

On `bb9d47b`, macOS and Docker Linux each passed all five mutations in
debug and release. Review of the [archived evidence](decisions/logical-storage-bb9d47b.sha256)
checked 20 catches and 84 supervised commands. Each profile passed all
12 integration tests and the output-refusal test. The selected storage
unit suite passed 122 tests on macOS and 132 on Linux. Reconciliation
leaves 1,479 runtime, 26 driver and 7,397 tooling gaps.

### Inspection and verified backup copies

A backup may have synced a complete destination image when its independent
verification read fails. On `159791b` the regression injects an I/O failure
at the first header read after the destination's final sync. The caller
receives `Io` instead of size statistics. After a simulated crash, the
destination matches the clean backup byte for byte and reopens with the
current and retained values. A retry to a fresh destination returns the
expected sizes. The source remains byte-identical throughout.

The [inspection ledger](decisions/inspection-backup-current.md) records
eight named propagation controls among its eighteen source decisions.
Two additional mutations skip verification or retain the destination's
serialization caches into the verification read. All ten were caught
on macOS and Docker Linux in debug and release. Each run also passed
all six inspection integration tests and both unit tests. The
[archive manifest](decisions/inspection-backup-159791b.sha256) pins
the source and both campaigns. Review verified 40 catches and 152
supervised command receipts. Reconciliation leaves 1,462 runtime,
26 driver and 7,397 tooling gaps.

### Administrative command dispatch

The [current admin ledger](decisions/admin-dispatch-current.md) accounts
for the last 26 unclassified driver records. Tests distinguish an unknown
two-argument command from `compact` and verify the input file is unchanged.
Unix compaction succeeds on a closed database and propagates `Busy` while
the writer lease is held. Scripted export checks propagate the original
lock and write errors.

On `fb32ce3`, five named mutations triggered the intended failures in
both profiles on macOS and Docker Linux. Each profile passed all 19
selected command and integration tests. Review verified 20 catches and
88 supervised command receipts. The [archive manifest](decisions/admin-dispatch-fb32ce3.sha256)
also pins the [native Windows log](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45616580373/job/139213422461)
for `1029d2c`, which passed the existing unsupported-compaction test in
debug and release. The Windows compact guard remains partially verified
until the new same-arity case runs natively. The inventory now has zero
unclassified driver records, 1,462 runtime gaps and 7,397 tooling gaps.

### Typed store and physical index keys

On `2c75210`, the typed suite passed 67 tests in each of debug and release
on macOS arm64 and Docker Linux arm64. Each run listed the same 67 tests and
one ignored documentation example. The raw lookup checked all five scalar
claims after one insert. Six mutations changed collection byte order, field
byte order, the kind tag, Boolean width, float byte order or the error returned
after a refused allocation. Each mutation failed its intended test in all four
runs. The archived logs verify 24 catches and 96 supervised commands.

The [current typed store ledger](decisions/typed-store-current.md) binds
97 decisions in unchanged functions and seven decisions in the index encoder.
Source reconciliation at `7b465ab` leaves 1,358 runtime and 7,397 tooling
records unclassified, including 130 in the typed store. Driver gaps remain
at zero. The [archive checksums](decisions/typed-key-2c75210.sha256) cover
the source bundle, reconciliation and both platform runs. These are local
proof copies pending off-machine custody.

### Typed catalog and registration

The catalog test on `51d3301` distinguishes a scalar replacing the unique-ID
list from a scalar of the wrong kind inside a valid list. Both return
`InvalidCatalog` and preserve the revision. A valid entry and five independent
changes cover zero and duplicate IDs, reserved collection IDs, zero type and
zero version. All six mutations failed in the named test on macOS arm64 and
Docker Linux arm64, in debug and release. All four unchanged runs passed the
same 24 runtime-decision tests. The archived receipts cover 92 commands.

The [catalog ledger](decisions/typed-catalog-current.md) and decision-pair
tables at `6512770` account for 52 additional source records. There are
78 outstanding in the typed store, 1,306 across runtime targets and 7,397 in
tooling. Off-machine custody is still open. The [checksums](decisions/typed-catalog-51d3301.sha256)
identify both platform archives, the source bundle and reconciliation report.

### Typed lookup admission and repair

On `9cfbac2`, macOS arm64 and Docker Linux arm64 each passed 68 typed
tests in debug and release, with one ignored documentation example. An
unindexed field rejected lookup before application encoding. On an indexed
field the same encoder returned its application error, and catching that
error still aborted earlier draft edits. Both the current and retained root
were checked. Seven mutations failed their intended tests in each run,
including removal of the physical field check and the owner-key comparison.

The [lookup ledger](decisions/typed-lookup-current.md) and five named controls
cover the last 78 unclassified source records in the typed store. Reconciliation
at `903757b` leaves 1,228 runtime and 7,397 tooling records unclassified. The
source dispositions distinguish named pairs, execution and delegated errors.
The [archive manifest](decisions/typed-lookup-9cfbac2.sha256) identifies both
platform runs, 116 supervised command receipts, reconciliation and the source
bundle. Off-machine custody remains open.

### Historical typed decision matching

The inventory reader stripped the closing backtick from the typed ledger’s
combined kind/owner column. On `720f64e` it preserves the quoted owner and
requires both the source position and byte range when both appear. The
regression failed before the fix. It accepts the original row and rejects a
different owner, filename, line or byte range. All 34 inventory tests passed
in debug and release on macOS arm64 and Docker Linux arm64.

The two platforms produced byte-identical inventory and review reports. Each
of the 56 recovered rows matches the original ledger’s owner, kind, position,
range, target scope and file digest. The changes account for all 32 remaining
typed codec rows and all 24 typed library rows. Every other previously
identified decision kept its disposition. The parser itself adds one tooling
decision. There are now 1,172 runtime gaps and 7,398 tooling gaps. The
[archive checksums](decisions/inventory-appendix-720f64e.sha256) identify
the reports, test logs and source bundle.

### Consecutive simulator sync failures

On `dd19a6c`, four two-failure schedules checked the exact durable bytes
after each failed sync, the volatile bytes through the live lease and the
reopened bytes after a successful retry and another crash. Five mutations
changed the survivor base image, pending-write retention, successful-sync
cleanup, retry source or failure status. All five failed the intended test
in debug and release on both macOS arm64 and Docker Linux arm64. The same
41 ByteStore, durability and storage-decision tests passed in each run.

The [simulator ledger](decisions/simulator-survival-current.md) reconciles
its remaining 48 source records. The 32-bit conversion, coupled lease/mask
operands and physical-device checks retain their earlier qualifications.
There are 1,124 runtime and 7,398 tooling gaps. The [archive manifest](decisions/sim-survival-dd19a6c.sha256)
identifies the source bundle, reviewed inventory and 80 supervised command
receipts. Off-machine custody remains outstanding.

### Codec checksums and recovery decisions

All 46 codec tests passed on `e045521` in debug and release on macOS arm64
and Docker Linux arm64. They include both byte-for-byte golden histories,
the ECMA known vector, incremental checksums and recovery rejection of a
corrupt newest graph. Five changes to the CRC tables and streaming update
each failed the independent bit-recurrence test in all four runs. The
[archive checksums](decisions/codec-crc-e045521.sha256) cover all 20 mutation
detections, 80 command receipts, the source bundle and final reconciliation.

The [CRC and recovery audit](decisions/codec-crc-current.md) accounts for
49 records. The [remaining codec audit](decisions/codec-recovery-offsets-current.md)
accounts for 57 more, including existing partial metadata-layout pairs and
platform-specific arithmetic constraints. Reconciliation at `94c5145` leaves
zero unclassified codec records, 1,018 runtime gaps and 7,398 tooling gaps.
Compiler MC/DC and off-machine proof custody remain separate gates.

### Allocator growth and storage diagnostics

On `0f9b506`, all eight independent invalid limit configurations and four
growth mutations failed their intended tests on macOS arm64 and Docker Linux
arm64 in debug and release. The four growth controls distinguish the exact
maximum, the exact-size retry, a provider refusal and the requested fallback
size. Each profile passed the same 26 memory, five allocation, six allocator
routing and nine storage diagnostics tests. The [archive checksums](decisions/memory-growth-0f9b506.sha256)
identify 48 mutation detections, 172 command receipts and the source bundle.

The [growth ledger](decisions/memory-growth-current.md) reconciles the final
30 allocator records. The [diagnostics audit](decisions/diagnostics.md) binds
20 existing dispositions to their original immutable source, including the
two dependent context guards. Reconciliation at `9313bcb` leaves zero gaps
in either module, 968 runtime gaps overall and 7,398 tooling gaps. Compiler
MC/DC and off-machine custody remain outstanding.

### Journal replay after a later writer advances

On `ae9441f`, the coordinator committed revision 4, lost the reply and accepted
another writer at revision 5. After both databases reopened, journal check
reported the current value at revision 5, while resume confirmed the original
receipt at revision 4. The coordinator image stayed byte-identical during
check and replay. A further client restart returned revision 4 without sending
another request. The [decision table](decisions/journal-replay-current.md)
distinguishes a valid historical receipt from an inconsistent head receipt.

All nine journal unit tests and the native process lost-reply/restore test
passed in debug and release on macOS arm64 and Docker Linux arm64. All six
mutations failed the intended tests in each run. The [archive manifest](decisions/journal-replay-ae9441f.sha256)
covers 24 detections, 96 command receipts, both platform suites and the
source bundle. There are 75 unclassified journal records, 933 runtime gaps
overall and 7,398 tooling gaps. Off-machine custody remains outstanding.

### Journal write admission and durable intent

A client with a receipt at revision 4 rejects a coordinator at revision 3
before writing a new intent. It also rejects a requested revision ahead of
the coordinator. The local floor check makes one comparison in the remote
check redundant. The admission regression passed before and after removing
that comparison. It verifies the prior receipt, the entire local image on
rejection, the commands sent and the pending intent after a lost reply.
The [decision table](decisions/journal-admission-current.md) covers each case.

At `c0e3463`, all 37 ordinary coordinator tests passed in debug and release
on macOS arm64 and Docker Linux arm64. The extended maintenance and restore
tests also passed in both profiles. Linux additionally exercised journal
ENOSPC and replay on a separate 8 MiB tmpfs, bringing the totals per profile
to 39 on macOS and 40 on Linux. Each of five mutations failed its intended
test in all four runs. The [local archive manifest](decisions/journal-admission-c0e3463.sha256)
covers 20 detections, 80 supervised commands, the suites and source bundle.

Reconciliation at `470aa78` leaves 44 journal gaps, 902 runtime gaps overall
and 7,398 tooling gaps. Compiler MC/DC and off-machine proof custody remain open.

### Journal command dispatch and occupied slots

At `62e5989`, the native CLI committed through slot 1023, rejected slot 1024
and rejected attempts to initialize an occupied slot at an existing or fresh
path. Invalid actions, options and numeric arguments preserved the local file
and coordinator receipt. The new regression passed on macOS arm64 and Docker
Linux arm64 in debug and release. The [dispatch ledger](decisions/journal-cli-current.md)
records 14 decisions using these runs and the existing lost-reply test.
The [archive manifest](decisions/journal-cli-62e5989.sha256) identifies the
logs, source bundle and reconciliation. Thirty journal gaps remain, including
the all-zero identity response. There are 888 runtime gaps overall and 7,398
tooling gaps. These counts track source review; compiler MC/DC remains open.

At `2aab66f`, a scripted peer exercised seven journal initialization
exchanges. A zero identity stopped the client before the receipt query. EOF
in either exchange, invalid identity text and a noncanonical empty receipt
all left the journal path absent. A valid identity and exact empty receipt
allowed creation, and a new process reopened the expected identity and slot.
The three ordinary journal process tests passed in both profiles on macOS
arm64 and Docker Linux arm64. The [archive manifest](decisions/journal-handshake-2aab66f.sha256)
covers those runs and the updated reconciliation. Twenty-five journal records
and 883 runtime records overall remain unclassified.

### Journal output failure after a durable write

At `24f7900`, set committed revision 4 and saved its receipt before reporting
a broken stdout pipe. Check and resume also reported the native write error.
Reopening and resuming with working stdout returned the original revision 4.
The local journal bytes, coordinator value and receipt confirmed that the
retry performed no second write. Initialization with a closed stdout likewise
left a readable journal. The argument tests also verified lease errors,
connection setup errors, invalid slots and refusal to overwrite an existing
journal for another slot.

All 24 unit tests and 16 ordinary process tests passed per profile in debug
and release on macOS arm64 and Docker Linux arm64. The extended tests remained
opt-in in these runs. The [archive manifest](decisions/journal-output-24f7900.sha256)
covers all four suites, the reconciliation and the source bundle. Of the 861
unclassified runtime decisions, three are in the journal: printing the second
and third status lines, and propagating a failed initial state commit.

### Interrupted journal creation and partial status output

At `fd37a1b`, the simulator failed each I/O operation during journal
creation, then reopened after dropping or persisting the pending writes.
The two root barriers distinguish revision 0 (no collection, journal load
returns NotFound), revision 1 (empty collection, UnsupportedFormat) and
revision 2 (exact initial identity, slot and empty receipt). The
[operator guide](../apps/write-coordinator/README.md#durable-client-journal)
covers a partially initialized file. The status test bounded its output
at each line boundary and verified the exact prefix, propagated write error
and unchanged durable image. All 26 unit tests and 16 ordinary process tests
passed per profile in debug and release on macOS arm64 and Docker Linux
arm64. The [local archive manifest](decisions/journal-initial-fd37a1b.sha256)
covers these runs and source. Reconciliation at `7ea6e28` leaves zero
unclassified journal records, 858 runtime gaps and 7,398 tooling gaps.
Compiler measurement, delegated cases and off-machine custody remain open.

### Native Windows compact dispatch

The Windows job ran the same-arity unknown command and the unsupported
compact command in both debug and release. Each returned its expected error
and preserved its input. The tested merge commit `afcdb16` has the same
Git tree as PR head `c8beb0e`. The [job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45635931513/job/139281231969)
and [archive manifest](decisions/windows-admin-c8beb0e.sha256) identify the
runner logs and checkout metadata for both outcomes.
