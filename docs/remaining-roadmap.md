# Work remaining after the first review checkpoints

PRs #1 and #2 are review checkpoints. The production-readiness goal remains open.
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
| R1 | The [merged-main inventory refresh and named control batch](r1-qualification.md) records every source and an explicit per-record reconciliation result. Current engine TX-V1, admin export, typed, CRDT and snapshot controls are repeatable. PR #2's 16 checks pass, including native Windows. | **R1 remains open:** reconcile the individual G queue in the [current ledger](decisions/current.md), plus generated/compiler, delegated/native and tooling obligations. Source reference discovery and a finite guard batch do not close the full per-decision requirement. R2 tooling updates the denominator without promoting those gaps. |
| R2 | The [operational refresh](r2-qualification.md) qualifies engine `6b657e5`: 38 operator outcomes on each Mac/Linux lane, 30 authenticated Study rounds/six paired restarts, and 31 snapshot installations/32 polls with four reconciled lost replies. Native Windows operator CI is added. | **R2 remains open:** qualify restored pending journal intents and coordinator lost-reply replay, operator handoff and each promised deployment profile. Credit Windows operator execution only after its hosted run. Current/retained roots, exact receipts and repeat-sync stability remain required. Local commit and follower durable acknowledgment stay separate. |
| R3 | Qualify deployment workloads and resource budgets for the proposed Linux server/macOS client. Extend compound near-cap cases with affected-cache forks, independent/shared DAG shapes, cold-open index construction, full-width retained histories and longer contention/maintenance schedules. | Measure p50/p95/p99 response and queue latency, commit rate, RSS/stack, disk growth, maintenance pauses, restore time, recovery point and follower lag. Compare the matched SQLite durability/history profile. Agree on budgets before claiming service limits; accelerated logical months are not wall-clock soaks. |
| R4 | Select the supported API/ABI/disk/wire/CLI version window and qualify its release endpoints, including schema/index and policy migration, upgrade, rejection and rollback. | Exact historical/candidate binaries and original locked inputs pass every promised direction, preserving original files, retained schemas and identities. Existing pinned version pairs do not imply arbitrary compatibility. Future format converters require their own qualification. |
| R5 | Close remaining native deployment profiles. Exercise promised Windows administrative/service workflows under the intended ordinary account, permissions and ACLs; run each additional advertised architecture/filesystem combination. | Native exact-artifact traces, interruption/error outcomes and restoration evidence identify the actual account, filesystem and toolchain. Existing Windows NTFS, owner-quota and delivered-DLL passes retain their named scope. |
| R6 | Refresh release dependency/advisory and distribution review. Reconcile all fourteen locks, generated notices, source/artifact identities and the intended supported feature sets. | A dated audit retains advisory inputs and reviewed exceptions; every distributed package has matching licenses/notices and reproducible provenance. An unchanged lock hash is carry-forward, not a fresh advisory scan. |
| R7 | Freeze the eventual release and consolidate its final evidence, including the follow-up workloads. Establish durable off-machine proof custody, retention and restoration ownership. | One immutable release source binds its promised package/platform, sanitizer, fuzz, decision, compatibility and operational results. Fresh extraction validates the index and reruns the verifier. The local PR proof is a checkpoint, not a retention service. |

The [fuzz acceptance table](production-acceptance-audit.md#fuzz-and-dynamic-analysis-acceptance)
also retains bounded-oracle gaps: broader repeated/compound failures,
adversarial tree complexity, representative compilation of generated derives,
transport fault profiles and custom backend conformance. Longer campaigns
should follow those gaps and promote every discovered defect to a deterministic
regression. No finite campaign establishes SQLite-equivalent assurance.

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
