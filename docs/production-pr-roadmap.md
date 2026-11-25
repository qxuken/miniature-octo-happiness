# Private production release PR roadmap

This sequence starts after [PR #27](https://github.com/qxuken/miniature-octo-happiness/pull/27),
merged as `cde6f2d`. The release scope includes the embedded engine, typed Rust,
C/C++/Odin consumers, passive snapshot replication, fixed two-actor CRDT peers,
and the write coordinator. The primary deployment is a Linux server and macOS
client. Exact supported platforms and the deployment budgets require agreement.
Cargo packages retain `publish = false`.

The [remaining-work ledger](remaining-roadmap.md) tracks requirements R1–R7
and historical evidence. [Acceptance criteria](production-acceptance-audit.md)
define the covered outcomes. Evidence keeps its original source, toolchain,
features, artifact and platform identity.

## Correctness and assurance

| Batch | PR | Gate | Completion evidence |
|---|---|---|---|
| 1 | Production support contract | R2–R5, R7 | Agree on the OS, architecture, filesystem, device, feature, topology and version support matrix. Set ceilings for data size, history, memory, latency, maintenance, restore and replication. |
| 2 | Finish allocation fault coverage | R1 | Use separate selected allocators for the simulator and engine. Exercise Global refusal at plain Rust `Vec` outputs. Migrate the remaining allocation sweeps before retiring the legacy hook. |
| 3 | Codec, cache and index decisions | R1 | Cover cache fork, growth, lookup and cold-open rebuild. Verify golden bytes, bounded work, selected-root rejection, and current and retained roots. |
| 4 | Transactions, trees and values | R1 | Close the remaining conflict, absence, scan, sharing, collision and resource decisions. Check success and failure atomicity against independent models. |
| 5 | Publication, maintenance and `ByteStore` | R1, R5 | Qualify progress, leases, flushes, poison, replacement and cleanup. Check custom backends and second failures during recovery. |
| 6 | Ownership, threading and FFI | R1 | Exercise allocator lifetime, handle ownership, panic containment, close quiescence and concurrent publication. Use directed tests, Miri, bounded atomic models and exact native consumers. |
| 7 | Typed schema and indexes | R1, R4 | Cover migration, conflicts, missing/default/unknown fields, retained schemas and atomic unique-index updates. Verify rollback and index consistency. |
| 8 | Derives and generated code | R1, R4 | Compile representative Model, Record and Resolution expansions. Check invalid declarations and pinned generated schema and wire bytes. |
| 9 | Snapshot protocol and follower decisions | R1, R2 | Verify bounds, old and duplicate offers, install, reopen and retry. Cover the follower durability barrier independently of local commit. |
| 10 | CRDT runtime decisions | R1, R2 | Check actor and policy admission, causality, exhaustion and concurrent resolutions against independent histories. Repeated sync preserves the committed result. |
| 11 | Coordinator and service decisions | R1, R2 | Qualify admission, deadlines, sequencing, receipt replay, journal recovery and shutdown. Restart returns the exact acknowledged outcome. |
| 12 | Close the R1 evidence ledger | R1 | Record evidence or a reviewed disposition for each applicable safety-critical decision. Document compiler, generated code, native delegation and tooling scopes. |

Reuse existing tests and exact-source controls. Split a batch into smaller PRs
when its review or validation needs it. The post-#27 inventory records 9,916
unresolved source records, including 2,556 runtime records. The
[allocation decision ledger](decisions/allocation-recovery-current.md)
records the current baseline.

## Deployment and release

| Batch | PR | Gate | Completion evidence |
|---|---|---|---|
| 13 | Core fuzz and recovery oracles | R1, R7 | Extend repeated crashes, compound allocation and I/O failures, adversarial graph shapes and backend schedules. Save minimized cases and run deterministic replay. |
| 14 | Distributed transport fault campaigns | R2, R7 | Exercise authenticated truncation, delay, disconnect, duplicate delivery and restart. Verify exact durable outcomes for snapshot, CRDT and coordinator profiles. |
| 15 | Linux/macOS workload and resources | R3 | Replay the simulated month and sustain a native soak on the intended setup. Check commit and queue tails, memory, disk growth, maintenance, restore and follower lag. Compare SQLite with matched durability and retained history. |
| 16 | Native Unix and device durability | R5 | Qualify the actual Linux and macOS devices, filesystems and cache/flush settings. Verify the interruption, power-loss, remount and restore outcomes required by the support contract. |
| 17 | Windows deployment qualification | R5 | Verify exact delivered artifacts on the promised NTFS profile under the intended ordinary account. Check ACLs, quota, maintenance and restore. |
| 18 | Snapshot operator acceptance | R2 | Run delivered client and follower on the intended deployment. Rehearse lost replies, backup, reseed and authority handoff. Verify both acknowledgment boundaries and both current and retained state. |
| 19 | CRDT operator acceptance | R2 | Rehearse offline edits, peer restore, credential rotation and policy generation changes with the fixed roster. Verify convergence and repeat-sync stability. |
| 20 | Coordinator operator acceptance | R2 | Rehearse two-client conflict, lost replies, slot retirement, journal restart and stale-backup restore. Confirm the receipt and authority rules with the operator. |
| 21 | Release compatibility matrix | R4 | Run exact old and candidate binaries in every promised upgrade and rollback direction for disk, logical export, C ABI, typed schemas, and distributed wire and CLI versions. Verify original bytes on rejection. |
| 22 | Dependencies, notices and support | R6 | Review the exact locks and delivered feature sets for advisories, registry state, licenses, notices and provenance. Obtain owner approval of support and security response terms. |
| 23 | Freeze and approve the private release | R7 | Bind supported packages, exact consumer traces, sanitizer and fuzz evidence, workload and restore drills, and compatibility to one immutable source. Restore the proof from off-machine custody and verify it before owner approval. |

## Stack order

The original batch numbers below identify verification obligations. Use these
groups for the remaining PRs so shared fixtures, source inventory updates and
review evidence go through CI together.

| Group | Original batches | Review checkpoint |
|---|---|---|
| Coordinator and client journal | 11, coordinator portion of 12 | Exercise admission, dispatch, durable receipt replay and client-journal recovery together. Carry forward delivered-package operator evidence with its original source. |
| Engine assurance and bounded recovery | Residual work in 2–10, 12–13 | Group decision pairs, mutation controls and fuzz oracles by the engine or native boundary they exercise. Keep any independent native follow-up tied to its platform. |
| Support contract and deployment | 1, 15–17 | Agree on the support matrix and performance budgets alongside assurance work. Combine workload, device and native evidence once the relevant deployment profile is agreed. |
| Distributed transport and operator acceptance | 14, 18–20 | Share delivered fixtures and fault schedules across snapshot replication, fixed two-peer CRDTs and the write coordinator. Use the agreed deployment profile for operator sign-off. |
| Compatibility and distribution review | 21–22 | Verify the same old and candidate versions against the agreed support window, package features, dependency locks and notices. |
| Immutable candidate qualification | 23 | Run the full release matrix on one source and verify proof restoration before approval. |

Run focused checks locally before pushing each group. Keep dependent changes in
one PR when they share evidence and can be reviewed together. The PR lane
retains its three macOS jobs and the portable Linux coverage. Release
qualification retains the full platform matrix.

The roadmap PR is the base. Batch 2 is split into the simulator allocator,
the plain Rust output boundary, public hook retirement, and the remaining
unit tests. The codec tests move first. Follow with cold-open and native
adapter checks, then tree, value, and memory tests. Delete the unit-only
counter once the remaining tests drive real allocation requests.
[Batch 2 evidence](decisions/unit-allocator-retirement.md) records that
transition. Batch 3 starts with VALUE hash probing and rehash, DATA
lookup and insertion, and the borrowed-to-owned cache transition.

Batch 5 starts with [ByteStore transfers and local publication](decisions/bytestore-publication-current.md),
[creation and reopen stabilization](decisions/storage-creation-reopen-current.md),
[verified compact copies and headroom](decisions/compact-copy-current.md),
[native namespace replacement](decisions/native-replacement-current.md),
[Unix writer leases](decisions/unix-leases-current.md),
[Unix pathname admission](decisions/unix-path-admission-current.md),
[Unix sync and I/O attribution](decisions/unix-sync-current.md),
[macOS native primitives](decisions/macos-primitives-current.md),
[Linux primitives](decisions/linux-primitives-current.md),
[Linux metadata validation](decisions/linux-metadata-current.md),
[Windows pathname preparation](decisions/windows-paths-current.md),
[Windows file and volume admission](decisions/windows-admission-current.md),
[Windows handle ownership and error routing](decisions/windows-handle-io-current.md),
[Windows file identity result handling](decisions/windows-rng-results-current.md),
[Windows file open results](decisions/windows-open-results-current.md),
[Unix scratch preparation and positioned I/O](decisions/unix-scratch-io-current.md),
[Linux architecture admission](decisions/linux-architecture-current.md),
[Unix compaction descriptor exhaustion](decisions/unix-descriptors-current.md),
[Unix metadata reads](decisions/unix-metadata-reads-current.md),
and [Unix compaction file clone failures](decisions/unix-clone-failures-current.md).
Kernel metadata errors, native Win32 decisions and deployed device
qualification remain open.

Batch 6 starts with [FFI handle accounting and allocation refusal](decisions/ffi-handle-accounting-current.md).
The [FFI boundary ledger](decisions/ffi-boundaries-current.md) adds portable
source controls for buffers, options, path admission and panic status, plus
the relocated macOS and Linux C/C++/Odin packages. The
[generated C export ledger](decisions/ffi-generated-exports-current.md)
adds validation-mode controls, delivered allocator callback checks and the
134-cell trace. The [C file-factory ledger](decisions/ffi-file-factories-current.md)
sweeps selected-allocator create and reopen failures against the exact
packaged libraries. The [foreign allocator ledger](decisions/foreign-allocator-current.md)
qualifies callback descriptor validation, context calls and zero-byte layout
dispatch. The [provider ownership ledger](decisions/provider-ownership-current.md)
qualifies allocation propagation and final release with concurrent owners.
The [C transaction and snapshot ledger](decisions/ffi-transaction-reads-current.md)
qualifies error outputs, draft deletion, pinned reads and scan ownership.
Continue with the remaining generated paths and delivered consumers per ABI.

Batch 7 starts with [typed unique-claim replacement](decisions/typed-unique-claims-current.md).
It covers stale, malformed and competing owners, paired primary and index
updates, and allocation rollback with retained roots. The
[index allocator follow-up](decisions/typed-index-allocator-current.md) routes
checking and repair scratch through the database allocator and charges held
scan keys to the live-byte cap.
The [migration recovery matrix](decisions/typed-migration-recovery-current.md)
qualifies batch admission, rollback on ID scratch refusal and resumed
conversion after interruption, with paired primary, index and progress rows.

Batch 8 starts with [derive admission](decisions/derive-admission-current.md).
It qualifies generated schema and CRDT behavior alongside named controls for
`Model`, `Record` and `Resolution` expansion. Remaining derive and generated
target paths stay in the inventory.

Batch 9 starts with [snapshot acknowledgment boundaries](decisions/snapshot-ack-current.md).
It verifies the selected image after Ready and terminal transport errors and
qualifies the sender's receipt admission and follower's retry decisions.
The [capture and saved-image ledger](decisions/snapshot-capture-current.md)
covers source and descriptor admission, destination sync and verification,
and exact current and retained roots after retry.
The [sender reply and capture ledger](decisions/snapshot-sender-current.md)
qualifies poll and ship reply dispatch, error propagation, and native
capture and saved-image factories.

Batch 10 starts with [CRDT actor and policy admission](decisions/crdt-admission-current.md).
The durable fixture checks both sync roles, retained state after reopen, and
receipt coverage before progress is reported. The
[register causality and resolution ledger](decisions/crdt-causality-current.md)
covers the two-actor event model, sequence and retirement admission, bounded
minimum and maximum resolution, and rejection before publication or receipt.
Native Windows and deployed peer qualification remain open.

Develop the support contract alongside the assurance work. The contract sets
the deployment conditions for batches 15–22. Freeze the release after those
gates and the applicable assurance batches are complete.

Record each PR and its base in its description. Retarget dependent PRs in order
when their bases merge. Record the exact evidence and remaining gates in the
[remaining-work ledger](remaining-roadmap.md).
