# Production acceptance audit

This is a requirement-by-requirement acceptance review of
[production-readiness.md](production-readiness.md), not a new qualification run.
Reviewed source HEAD: `032dadf591b0a0ded4894898e8208283c50425fb`; the working
copy also contains the explicitly source-bound qualification reports cited
below. No engine changes or executions were made for this audit. An old passing
run supports its recorded source/profile only. Neither a source disposition nor
an error-path test elsewhere closes an untested `E`, platform `cfg`, dependency,
macro expansion or operating-system behavior by association.

**Disposition vocabulary:** **Implemented/scoped evidence** means the capability
and named tests exist, not that every release gate passed. **Local** means useful
implementation or qualification can be performed on the available Mac/Linux
setup. **Platform/device** requires the named actual environment. **Owner** means
a support promise, workload or policy must be selected. **Outside profile** means
an unoffered expansion, not a defect to fix speculatively.

## Release profiles and promises

| Surface | Actual current boundary | Acceptance still needed |
|---|---|---|
| Embedded engine | One file-owning process, native exclusive lease, concurrent in-process readers/drafts; [contract](../DESIGN.md), [operations](operations.md) | Owner selects exact OS/architecture/filesystem/features and support period; execute candidate matrix. Multiple independent file writers are outside this profile. |
| Typed Rust | Matching core/macros/runtime; raw writes can violate index/catalog ownership | Keep raw-write ownership explicit; qualify supported schema transitions, not arbitrary mixed versions. |
| C/Odin | ABI 1, valid pointers/live handles/close quiescence, unwind-enabled package; [release contract](release-contract.md) | Exact artifact consumers on each advertised target; no promise that arbitrary downstream abort builds contain panics. |
| Passive follower | Verified snapshots, configured leaf pins, separate local revision and durable receipt; offline maintenance and capture require quiescence | Owner accepts stop/capture/restart workflow and transfer size/rate envelope; no automatic promotion/fencing. |
| CRDT peers | Exactly two enrolled actors, fixed policy/field bounds, lawful app-supplied `Record`, application-owned reserved rows | Owner accepts fixed roster and operator-controlled migration; authenticate transport and prevent raw writes in the embedding app. |
| Coordinator | Loopback demo, optional configured shared secret, one unsigned record, 1,024 permanent receipts | Owner must select authenticated local-only promotion or fund remote transport/authorization. No HA or per-user ACL claim. |
| Expansion | SQL, gossip/discovery, dynamic rosters, encryption, online maintenance and larger images are not offered | Outside profile until a named workload requires them. They are not prerequisites for the bounded local profile. |

## Functional and operational acceptance

| ID / roadmap requirement | Concrete implementation/evidence | Remaining acceptance or useful local work |
|---|---|---|
| F01 Isolation, observations and conflict rules | `src/engine.rs`; `tests/transactions.rs`, `tests/schedules.rs`; [engine ledger](decisions/engine.md) | Implemented/scoped evidence. Keep absence/scan/catalog/ABA pairs separately mapped; run exact consumer traces below. |
| F02 Terminal transactions, snapshot lifetime, threading and lease contention | Engine/FFI ledgers, concurrent tests, native file tests | Per-handle unsafe contracts still apply. Native Windows and architecture-specific concurrency qualification remain; another profile's success does not close them. |
| F03 `Io`/uncertain/recovery contracts and application retry IDs | Publication/recovery ledgers; [operations](operations.md); coordinator atomic durable receipts | Core contracts implemented. Local: replay the same uncertainty/reconciliation trace through delivered Rust/C/Odin surfaces, rather than only Rust-linked FFI calls. |
| F04 Read-only full verifier and selected-root authority | `storage::inspect_file`, admin `check`; `tests/inspection.rs`; [diagnostics](verification-diagnostics.md) | Implemented. Generic file checking does not automatically prove typed index correctness; use typed `check_indexes` explicitly. |
| F05 Cheap health/headroom and actionable errors | `storage_space`, native diagnostic scope; errno categories; [native drills](native-fault-drills.md) | Real Linux ENOSPC/EACCES evidence exists at its frozen source. Actual quota exhaustion is not established by an enum/mapping test; local disposable quota fixture if kernel support permits. |
| F06 Build/format identity and secret-safe output | `version`, ABI identity, release manifests; diagnostics omit record/path contents | Implemented. Local: validate service log/output redaction and structured error observability as a deployed workflow; do not claim arbitrary app logging is safe. |
| F07 Verified backup/restore and new destination | `backup_file`/`restore_file`; independent CLI tests; typed `process_evolution`; [operations](operations.md) | Scoped state/history/index oracles exist. Owner sets recovery-time objective; repeated restore with actual workload is necessary for that objective. |
| F08 Recovery point and identity after fork/restore | Selected leased revision is recovery point; identity preserved; [follower backup](follower-backup-restore.md), [CRDT recovery](crdt-operations.md) | Implemented explicit recipes. No zero-data-loss promise beyond preserved acknowledged state/available backup. Actor reset and writable copies require reviewed enrollment/routing changes. |
| F09 Salvage and logical format transitions | `storage/{salvage,logical}.rs`, admin export/import, [salvage](salvage.md), [logical format](logical-format.md) | Complete-root salvage is explicit; partial arbitrary-row salvage not offered. Qualify each actual future format converter before declaring a supported upgrade. |
| F10 Headroom, history reclamation and interrupted maintenance | `storage_space`, release/compact, replacement simulator, follower cleanup/manifest compaction | Implemented quiescent workflow; no automatic background reclamation. Owner supplies maintenance thresholds; local threshold/monitoring recipe can make limits actionable. |
| F11 Near-cap append/recovery/copy space and latency | [resource envelope](resource-envelope.md), `examples/recovery_scale.rs`, cap tests | Finite cases exist. Local: joint high-breadth/shared-DAG/cache-search workload and concurrent drafts; report scratch-copy domains and process RSS, not only one budget domain. |
| F12 Safe orphan deletion and authoritative image | Verified canonical replacement and selected follower manifest; namespace fuzz and maintenance tests | Implemented scoped checks. Hostile namespace mutation is outside trusted-directory contract; real device namespace durability remains separate. |
| F13 API/ABI/disk/wire version policy | [release contract](release-contract.md), header, runtime identity, fixtures | Owner: stabilize chosen Rust/API/error/feature limits and support window. Pre-1 historical fixtures are not a blanket compatibility promise. |
| F14 Upgrade/downgrade and original preservation | [storage compatibility](storage-compatibility-21e5d26.md), snapshot wire fixtures, typed retained migration tests | Local: add any *promised* profile/version direction not represented. Core wire interoperability is not mixed-version CLI TLS/ALPN interoperability. |
| F15 Packaged optimized Rust/C/C++/Odin consumers and relocation | [exact-21 packages](local-qualification-21e5d26.md), source isolation and symbol checks | Mac/Linux source-bound evidence. Local: a shared deterministic stateful trace through exact packaged languages; Windows native consumers still a gate. |
| F16 MSRV, loader, toolchain and support promises | Rust 1.99.1 qualification, pinned manifests/toolchain identities, private package instructions | Tested compiler is not independently established lowest MSRV. Owner declares minimum/support window; local build checks can enforce the declared minimum afterward. |
| F17 Worst-case heap/stack/disk/work/latency | Per-value/transaction/image/history caps, opt-in requested-core budget, [resource envelope](resource-envelope.md) | Local: combined/adversarial cases and encoder search work. No RSS/latency upper bound follows from the core budget or a requested worker-stack setting. |
| F18 Decoder/DATA/encoder cache searches | Decoder indexing and recovery-scale example; the [serialization experiment](serialization-scale.md) now measures distinct/shared VALUE and eight-revision DATA searches, with complete maintenance oracles and RSS | Distinct breadth 128→512 gives approximately 16× comparisons; shared gives approximately 4×. DATA totals stay 516 for full histories. Local: separately reviewed VALUE-index experiment and 64-root history axis; owner sets acceptable latency/resource budgets. No optimization or overall speedup is inferred. |
| F19 Fallible allocation, overflow, panic, unsafe and consumer builds | Memory/FFI ledgers, fault sweeps, Miri/Loom, abort subprocesses; [unsafe review](unsafe-review-2490631.md) | Scope remains source/ABI/config-specific. Candidate sanitizer work is separate; review downstream build flags and uninstrumented/native dependencies explicitly. |
| F20 Filesystem/device/path/ACL threat model | Native adapters and operational docs; trusted config directories; Unix private keys; Windows external ACL provisioning | Platform/device: actual target filesystem/cache/ACL behavior. No race-proof hostile namespace or dishonest-flush guarantee. |
| F21 License/security/dependencies/provenance | MIT OR Apache-2.0, notice inventory/std notices, [SECURITY](../SECURITY.md), package manifests | Owner accepts distribution terms, support response and exceptions. Local: fresh locked dependency review and final immutable candidate bundle; packaging eligibility is not production approval. |
| T01 Typed field IDs, missing/null/default/unknown semantics | Schema tests, typed models, `schema_index_migration` fuzz; [typed ledger](decisions/typed-runtime.md) | Implemented scoped evidence; generated code and changed model versions retain separate obligations. |
| T02 Atomic indexes, consistency and rebuild | `Read::check_indexes`, `Write::rebuild_indexes`, duplicate/missing/orphan tests | Implemented. Raw mutations require application ownership and explicit repair; do not silently repair on normal open. |
| T03 Bounded migration, resumable progress and retained restore | Explicit-ID batches with same-transaction progress receipt, process-evolution tests | Local: qualify a concrete app rollout/rollback plan across mixed-version readers/writers when promoted; no automatic distributed schema coordinator. |
| R01 Enrollment/restart/reseed/authentication | `weekend-replicate`, shared TLS pin/ALPN/admission controls, [runbook](replication-operations.md) | Implemented for configured private endpoints. Local: coordinated credential replacement/revocation drill with old-pin rejection and unchanged/reopened state. |
| R02 Bounded transfer/retry and lost reply | Absolute 30-second socket budget, retained same export within retry, reopen/poll reconciliation | File verification/flush can exceed socket deadline. Owner sets transfer envelope; restarted sender captures a new export, not transparent old-export replay. |
| R03 Cleanup, manifest growth and rotation | Offline cleanup/compact commands and tests | Implemented Mac/Linux; Windows explicit unsupported manifest compaction and fresh-directory reseed. Native Windows execution remains. |
| R04 Progress and lag diagnostics | `status`/`poll` report receipt/revision/digest; live server owns manifest lease | Local functional tooling gap: machine-readable capture/poll comparison with separate local/remote observations and lag/age, including unavailable/uncertain states. Revision difference is not elapsed-time lag. |
| R05 Mixed-version and backup recovery | Core wire compatibility and verified follower restore/reseed cases | Local: supported CLI TLS/config upgrade and credential rotation cases. No automatic failover or promotion authority. |
| C01 Lawful CRDT records and reserved row protection | `Record` laws documented, encode/decode preservation, actor checks; raw API bypass explicitly documented | Application must enforce exclusive reserved-row writes; do not label runtime checks a proof of arbitrary app merge laws. Local: reusable law-test helper for app records would improve adoption without adding consensus. |
| C02 Causality/replay/resolution/bounds | Register event oracle, two-peer schedules, bounded frame/set/counter containers, repeated-sync no commits | Implemented finite models. Larger causal histories, forged-state shapes excluded from a harness and other policies need their own tests; no expansion by association. |
| C03 Evolution, actor replacement and stale backup | [policy migration](crdt-policy-migration.md) has fresh identities, staging interruption and retained-state tests | Routing/quiescence/reconciliation are application responsibilities. Local: operational cutover rehearsal with stale old binary/credential attempts, not a silent in-place actor reset. |
| C04 Membership/gossip/cross-record constraints | No offered dynamic membership/discovery/atomic distributed constraints | Outside profile. Do not introduce coordinator/CRDT abstractions solely to tick a roadmap row. |
| W01 Coordinator auth, authorization, admission/backpressure | Loopback/optional shared secret, bounded line/frame, serial handling, native lease | Local-only trust boundary explicit. Remote TLS and per-principal authorization are missing only if owner promotes a remote coordinator. Public unauthenticated service is not supported. |
| W02 Preconditions, durable receipts, restart/lost reply | Atomic record+receipt; [capacity case](../apps/write-coordinator/README.md#finite-native-receipt-capacity-qualification) | Finite 1,024 receipt limit is real: no eviction/expiry. Owner decides bounded-session acceptability; if continuous service is required, design safe epoch/archive/reconciliation before adding reclamation. |
| W03 HA leadership/fencing | Not implemented or promised | Outside selected single-owner profile. Do not infer safe failover from snapshot durability or the file lease on another host. |

## Decision inventory acceptance

The inventory currently classifies 2,561 explicit source records in 51 files,
with 316 macro blind spots. This is a classification denominator, not 2,561
proved MC/DC decisions. See [inventory](decision-inventory.md),
[ledger index](decisions/README.md), [coverage correction](historical-coverage-audit.md)
and [per-executable coverage](coverage-executables.md). The compiler configurations
actually probed reject MC/DC instrumentation. Condition outcomes, manual pairs,
mutations and compiler line/branch data remain distinct evidence.

| Roadmap module requirement | Existing source/test ledger | Acceptance still open |
|---|---|---|
| Engine observations, lifecycle, retained bounds, failure atomicity | `engine.rs`, transaction/schedule/concurrency suites, engine ledger | Trace every remaining feasible condition and generic instantiation; current core-library report is not coverage of every API/profile. |
| Tree/value collisions, shape/size/scan limits and sharing | Tree/value ledgers; independent models and directed collision fixtures | Actual hash collisions are directed controls, not credited to random 64-bit collision fuzzing. Broad adversarial shape combinations remain local work. |
| Allocator counters, ownership, atomics and abort policy | Memory ledger; allocation sweeps, Miri, bounded Loom and negative controls | Models cover chosen ownership schedules, not every native execution/ABI. Candidate/architecture reruns and remaining dispositions stay visible. |
| Storage core lease, I/O progress, poison, sync and copy | Storage/publication ledgers and ByteStore fault tests | Custom adapters must meet contract. Native quota/device behavior is not implied by synthetic result mapping. |
| Codec slots/witness/selected graphs/cache and work bounds | Recovery metadata/graph ledgers, corruption fixtures, structured fuzz | Retain infeasibility proofs tied to order/bounds; no implicit older-root fallback. Compound parser work/caches still need resource experiments. |
| Native file adapters | Native ledger and Mac/Linux native tests | Windows/NTFS current candidate, other architectures and real device assumptions require execution; cfg-dispatch classification alone is insufficient. |
| Replacement/simulator | Replacement/namespace and crash oracles | Modeled flush/reorder/lease model must remain separate from kernel/device failure claims. |
| FFI/header/public exports | FFI ledger, ABI manifest checks, valid-handle fuzz, native consumers | Caller UB stays outside acceptance; exact packaged differential traces and unsupported-target paths need their own evidence. |
| Schema/typed runtime | Schema/typed ledgers; six additional public guards and mutations | Same-instantiation poisoned-write pair is qualified; do not generalize its counts to other monomorphizations or schemas. |
| Derives | Pure expansion seam, negative compilation, renamed consumers, token fuzz; [four direct expansion pairs](macro-expansion-guards-032dadf.md) now have same-process outcomes and mutation sensitivity | Earlier four zero unit-view outcomes already had downstream diagnostic tests. Direct attribution is now qualified; generated code, `?`, compiler-host execution and other explicit blind spots keep their own scope. |
| Snapshot replication | Wire/follower and CLI ledgers; native TLS cases | Wire checks, local durability barriers and remote acknowledgement are distinct. CLI upgrade/rotation and deployment admission are not closed by decoder tests. |
| CRDT core/containers | CRDT ledger and register causality/event oracle | Arbitrary application merge laws and roster changes are not proved by shipped-container convergence. |
| Apps/examples/tools | Distributed/snapshot ledgers, package consumers, independent tool controls | Tool decisions are not all part of the core source denominator. Parent-only coverage deliberately excludes spawned-app coverage; keep it separate. |

## Fuzz and dynamic-analysis acceptance

The [exact-21 campaign](fuzz-qualification-21e5d26.md) completed 22 targets and
1,431,321 actual runs, with 242 unchanged seeds and independent 25-test replay.
This is finite source-bound evidence, not a declaration that every row below
has its full requested oracle. The [fuzz inventory](../fuzz/README.md) is the
harness-level scope reference.

| Roadmap target family | Implemented coverage | Local or external residual |
|---|---|---|
| Raw/structured image open | Both targets, repaired checksums, fixture-driven allowed results | Keep parser acceptance/work/allocation separate; arbitrary bytes alone rarely reach deep valid graphs. |
| Image plus operations | `image_and_ops` with logical state checks | Broaden graph/operation combinations and retain minimized regressions; no native interruption inference. |
| Values/adversarial trees | Independent BTreeMap/Vec models and long/binary/prefix/list cases | Directed collision and adversarial complexity tests remain necessary in addition to fuzzing. |
| Transaction programs/schedules | `txn_schedule`; planned `txn_program` is documented overlap, not a missing second executable | Exact compatible-success as well as conflict oracle matters. Additional programs only if they add behavior not already generated. |
| Commit/history/compact crash | Commit and retention/compaction targets with simulated persistence oracles | Native physical loss and all compound failures remain outside model. |
| Replacement/backup | Namespace model and backup/restore oracles | Real namespaces/devices/ACL behavior require separate native checks. |
| ByteStore contract | Progress/lease/fault scripts and independent byte model | Custom backend implementations still need their own conformance run. |
| FFI operations | Valid live handles/buffers/foreign stores through actual exported functions | Rust-linked FFI harness is not a substitute for exact packaged C/Odin consumer execution or caller UB testing. |
| Schema/index/migration | Primary-derived index oracle, missing/default/unknown and migration cases | Selected schemas do not cover all app conversion functions; retain process/rollback tests. |
| Derive tokens | Exact expansion functions, compile-fail tests and stable generated examples | Token expansion success is not compilation of every generated output; bounded representative compile oracle remains explicit. |
| Snapshot frames/install | Structured frames and manifest/image faults | Authentication/TLS/native transfer timing belongs to separate suites. |
| CRDT frames/schedules/registers | Fixed two-actor model plus independent register dots/events and policy commits | Excluded forged histories/exhaustion shapes require directed tests; larger rosters not offered. |
| Coordinator/peer transport | Exact production parsers, bounded deterministic transport seams | Not a general TLS/network-fault fuzzer; authenticated native tests and operational credential changes remain separate. |
| Sanitizer/Miri/Loom/corpus infrastructure | Calibration, saved seeds, deterministic replay, bounded atomic models, native thread tests | Candidate/platform-specific runs and lasting artifact retention are separate gates. ASan without rebuilt std does not instrument all code; Miri/models do not prove native syscalls. |

## Crash, platform and delivery gates

| Gate from roadmap | Concrete evidence | Acceptance action |
|---|---|---|
| Fault-model publication | Storage/simulator contract, selected-root/witness semantics, [operations](operations.md) | Owner/platform: validate actual filesystem/device flush assumptions before power-loss promises. |
| Allocation and I/O failure matrix | Simulator, corruption/fault sweeps, current/retained state oracles, [native drills](native-fault-drills.md) | Local: actual quota if feasible; systematic second-failure combinations where a reviewed disposition remains open. No blanket closure from one recovery test. |
| Native platform/filesystem matrix | Exact-21 Mac and private Docker aarch64 Linux/ext4 full checks | Platform: current native Windows/NTFS and every other advertised ABI/filesystem. Container success is not x86 Linux or a deployed physical disk. |
| Compatibility manifest and upgrade/rollback | [storage cases](storage-compatibility-21e5d26.md), logical/v2 fixtures, typed retained migration | Local: promised CLI/config directions; owner chooses supported versions. Historical pre-1 reader compatibility is not every future version. |
| Binary delivery | [private artifact/source evidence](local-qualification-21e5d26.md), relocation, symbols and notices | Local: feature-isolated/combined candidate matrix plus shared consumer traces. Owner: final clean candidate, support matrix, retained evidence bundle and attribution review. |
| Long-lived soak and reproducibility | Finite soak, accelerated month and cross-OS peer rounds; corrected launcher rejects incomplete round sequences and persists failures | Root is extending this separately. Require exact-state/resource checks and bounded cleanup; wall-clock longevity cannot be inferred from simulated calendar days. |
| Performance/scale | [SQLite comparison](sqlite-baseline.md), resource latencies, monthly workload metrics | Owner sets application budgets. Local: credible sample counts, tail latency under concurrency/maintenance, RSS/disk/restore and follower-age diagnostics; small samples do not establish p99 guarantees. |
| Support and release | Security owner/contact, private patch policy, unsafe review and scoped runbooks | Owner: supported versions, response goals, exceptions, incident/rollback drills. No production release is currently qualified merely because package tooling says candidate-eligible. |

## Highest-impact local next work

1. **Measure adversarial serialization work before changing it.** Add a bounded
   driver with independent complete current/retained byte oracles, distinct tiny
   objects versus maximally shared objects, multiple DATA revisions and breadth
   sweeps. Record encoder node-identity and DATA-cache comparisons alongside
   elapsed time, peak process RSS and copy/restore scratch-domain usage. Current
   `Encoder::value`/`data` linearly search vectors; this motivates measurement,
   not an unsupported claim of exploitable slowdown. If quadratic work is
   reproduced within the accepted envelope, replace only that lookup with a
   charged bounded index and rerun allocation/crash/retained-state tests.
2. **Replay one independent operation trace across delivered APIs.** Generate a
   small versioned trace/expected-state file containing nested values, absent
   reads/scans, conflicting writes, retained reads, reopen and documented error
   outputs. Have packaged Rust, compiled C and Odin consumers interpret the same
   trace, without invoking Rust's private test helpers. Compare independent
   digests/revisions and failure statuses after relocation. This closes a real
   gap between rich Rust-linked models and fixed ABI smoke examples.
3. **Qualify a coordinated credential change with real persistent peers.** Stop
   endpoints, replace configured leaves/pins in trusted private directories,
   restart with matching identities, and test old/one-sided/wrong pins plus
   interrupted operator cutover. Reopen data and compare exact revisions,
   retained state and receipts; a rejected TLS attempt must not mutate storage.
   Apply separately to snapshot and study ALPN profiles. No live reload, CA
   service, automatic revocation or topology change need be invented.
4. **Make replica progress operationally observable.** Add a small machine-readable
   capture/poll status tool or stable CLI output carrying source identity,
   captured local revision, exact digest, confirmed follower revision/digest,
   observation timestamps and explicit unknown/error state. Test newer follower
   acknowledgments, lost reply, restart and stale saved observation. Never call a
   local export a follower acknowledgment, or a revision delta a time guarantee.
5. **Resolve long-lived coordinator receipt policy before implementation.** The
   tested permanent 1,024-slot cap is correct and intentionally rejects overflow.
   For a continuous production coordinator this is an owner-visible operational
   stop. First specify whether bounded sessions are enough. If not, define safe
   request-ID epochs/archive and reconciliation of stale retries; silent eviction
   is not an acceptable implementation shortcut.

The initial serialization measurement in item 1 is now recorded in
[serialization-scale.md](serialization-scale.md); it narrows the remaining work
to a separately reviewed VALUE-index candidate, a larger history axis and
owner-selected performance acceptance. It does not justify DATA optimization.

The first four are local engineering/qualification opportunities. They should
be sized against the selected release profile rather than made unconditional
feature demands. Native Windows execution, actual device power interruption,
production workload budgets, support promises and distribution authorization
cannot be replaced by more local simulated tests.
