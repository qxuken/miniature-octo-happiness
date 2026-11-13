# Production acceptance audit

This is a requirement-by-requirement acceptance review of
[production-readiness.md](production-readiness.md), not a new qualification run.
Initial audit source HEAD: `032dadf591b0a0ded4894898e8208283c50425fb`; the
subsequent requirement review includes qualification work through
`e20ceceebd6f8ba1ec0da89a7514f93d6ffd706b`, with each report retaining its named
source/profile. These qualification tranches change tests/tools, not engine
or application runtime. An old passing
run supports its recorded source/profile only. Neither a source disposition nor
an error-path test elsewhere closes an untested `E`, platform `cfg`, dependency,
macro expansion or operating-system behavior by association.

The [completed quota integration checkpoint](roadmap-status.md#completed-quota-integration-checkpoint)
verifies all ten engine jobs and bounded fuzzing green at PR head `e20cece`.
That job/step-status review complements the detailed artifact and data oracles
recorded in this audit; it does not re-audit their payloads or close the remaining
release gates.

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
| F02 Terminal transactions, snapshot lifetime, threading and lease contention | Engine/FFI ledgers, concurrent tests, native file tests; [hosted native matrix](hosted-qualification-cd7cdfe.md) including Windows | Per-handle unsafe contracts still apply. The named native CI profile now executes Windows; additional architecture/schedule/decision obligations are not closed by that passing matrix. |
| F03 `Io`/uncertain/recovery contracts and application retry IDs | Publication/recovery ledgers; [operations](operations.md); coordinator receipts; [delivered C fault recovery](delivered-c-faults.md) | Linux exact-package C checks IO/uncertainty, cleared outputs, terminal/recovery-required transactions and fresh-process request-ID reconciliation with full retained data. EIO is interposed; real ENOSPC is tmpfs-only. Other delivered language/platform fault profiles and physical failures remain separate obligations. |
| F04 Read-only full verifier and selected-root authority | `storage::inspect_file`, admin `check`; `tests/inspection.rs`; [diagnostics](verification-diagnostics.md) | Implemented. Generic file checking does not automatically prove typed index correctness; use typed `check_indexes` explicitly. |
| F05 Cheap health/headroom and actionable errors | `storage_space`, native diagnostic scope; errno categories; [native drills](native-fault-drills.md); [actual Linux EDQUOT/recovery gate](native-quota.md) | Ubuntu 24.04/ext4 append exhaustion now reports real OS 122/Quota/Write with global space available, preserves current/retained 3/2 through fresh reopen while full, and retries/reopens to 4/2. Full bytes, source/artifact hashes and cleanup are verified. Maintenance quota failures and other filesystem/OS profiles remain separate; older Linuxkit/synthetic results retain their scope. |
| F06 Build/format identity and secret-safe output | `version`, ABI identity, release manifests; diagnostics omit record/path contents | Implemented. Local: validate service log/output redaction and structured error observability as a deployed workflow; do not claim arbitrary app logging is safe. |
| F07 Verified backup/restore and new destination | `backup_file`/`restore_file`; independent CLI tests; typed `process_evolution`; [operations](operations.md) | Scoped state/history/index oracles exist. Owner sets recovery-time objective; repeated restore with actual workload is necessary for that objective. |
| F08 Recovery point and identity after fork/restore | Selected leased revision is recovery point; identity preserved; [follower backup](follower-backup-restore.md), [CRDT recovery](crdt-operations.md) | Implemented explicit recipes. No zero-data-loss promise beyond preserved acknowledged state/available backup. Actor reset and writable copies require reviewed enrollment/routing changes. |
| F09 Salvage and logical format transitions | `storage/{salvage,logical}.rs`, admin export/import, [salvage](salvage.md), [logical format](logical-format.md) | Complete-root salvage is explicit; partial arbitrary-row salvage not offered. Qualify each actual future format converter before declaring a supported upgrade. |
| F10 Headroom, history reclamation and interrupted maintenance | `storage_space`, release/compact, replacement simulator, follower cleanup/manifest compaction | Implemented quiescent workflow; no automatic background reclamation. Owner supplies maintenance thresholds; local threshold/monitoring recipe can make limits actionable. |
| F11 Near-cap append/recovery/copy space and latency | [resource envelope](resource-envelope.md), `examples/recovery_scale.rs`, cap tests | Finite cases exist. Local: joint high-breadth/shared-DAG/cache-search workload and concurrent drafts; report scratch-copy domains and process RSS, not only one budget domain. |
| F12 Safe orphan deletion and authoritative image | Verified canonical replacement and selected follower manifest; namespace fuzz and maintenance tests | Implemented scoped checks. Hostile namespace mutation is outside trusted-directory contract; real device namespace durability remains separate. |
| F13 API/ABI/disk/wire version policy | [release contract](release-contract.md), header, runtime identity, fixtures | Owner: stabilize chosen Rust/API/error/feature limits and support window. Pre-1 historical fixtures are not a blanket compatibility promise. |
| F14 Upgrade/downgrade and original preservation | [storage compatibility](storage-compatibility-21e5d26.md), core wire fixtures, typed retained migrations, actual [snapshot](snapshot-cli-upgrade.md) and [study](study-cli-upgrade.md) CLI upgrades | Named private pre-1 pairs now execute on Mac/Linux: snapshot interoperates both directions; pre-ALPN study requires coordinated upgrade and preserves offline edits. These do not establish arbitrary historical directions, changed schemas or a support window. Add each additional promised profile/version separately. |
| F15 Packaged optimized Rust/C/C++/Odin consumers and relocation | [exact-21 packages](local-qualification-21e5d26.md), source isolation and symbol checks; [hosted combined-feature packages](hosted-qualification-cd7cdfe.md) on Mac/Linux/Windows; [native Windows trace handoff](windows-artifact-trace.md) prepared | The [134-cell v2 trace](artifact-trace-validation-modes.md) exercises both validation modes, empty and retained-metadata publications through optimized Mac Rust/C/Odin and Linux ext4 Rust/C. Original v1 bytes/oracle remain preserved. Windows has native DLL C/C++ smoke and relocation; the new full C/C++ trace runner/pinned-package plan is locally checked but awaits native MSVC execution. Linux Odin, packaged Rust source and broader feature/error cases retain their scope. |
| F16 MSRV, loader, toolchain and support promises | Rust 1.99.1 qualification, pinned manifests/toolchain identities, private package instructions | The declared Rust 1.99 minimum passes full [Mac](local-baseline-ffdf506.md#declared-minimum-compiler) and [Linux ext4](local-toolchains-a56083c.md) checks at 1.99.0 with unchanged locks. The latter predates the new cutover tests. Windows/other target combinations and owner support window remain open. |
| F17 Worst-case heap/stack/disk/work/latency | Per-value/transaction/image/history caps, opt-in requested-core budget, [resource envelope](resource-envelope.md) | Local: combined/adversarial cases and encoder search work. No RSS/latency upper bound follows from the core budget or a requested worker-stack setting. |
| F18 Decoder/DATA/encoder cache searches | Decoder indexing, recovery scale, [breadth](serialization-scale.md) and [64-root history](serialization-history-scale.md) experiments with full current/retained/maintenance oracles | Distinct VALUE search is quadratic. DATA comparisons rise from 516 at eight roots to 108,036 at 64 in the bounded history lifecycle; accumulated publication work is cubic in history length, each lookup linear. Local: separately reviewed charged-index experiment and compound near-cap profiles. Owner sets acceptable budgets; no optimization or speedup is inferred. |
| F19 Fallible allocation, overflow, panic, unsafe and consumer builds | Memory/FFI ledgers, fault sweeps, Miri/Loom, abort subprocesses; [unsafe review](unsafe-review-2490631.md) | Scope remains source/ABI/config-specific. Candidate sanitizer work is separate; review downstream build flags and uninstrumented/native dependencies explicitly. |
| F20 Filesystem/device/path/ACL threat model | Native adapters and operational docs; trusted config directories; Unix private keys; Windows external ACL provisioning | Platform/device: actual target filesystem/cache/ACL behavior. No race-proof hostile namespace or dishonest-flush guarantee. |
| F21 License/security/dependencies/provenance | MIT OR Apache-2.0, notice inventory/std notices, [SECURITY](../SECURITY.md), package manifests | Owner accepts distribution terms, support response and exceptions. A [fresh immutable-database dependency review](local-baseline-ffdf506.md#locked-dependency-review) passed all twelve checkpoint locks plus the later progress fixture. Final immutable candidate bundle/attribution and owner approval remain; packaging eligibility is not production approval. |
| T01 Typed field IDs, missing/null/default/unknown semantics | Schema tests, typed models, `schema_index_migration` fuzz; [typed ledger](decisions/typed-runtime.md) | Implemented scoped evidence; generated code and changed model versions retain separate obligations. |
| T02 Atomic indexes, consistency and rebuild | `Read::check_indexes`, `Write::rebuild_indexes`, duplicate/missing/orphan tests | Implemented. Raw mutations require application ownership and explicit repair; do not silently repair on normal open. |
| T03 Bounded migration, resumable progress and retained restore | Explicit-ID batches with same-transaction progress receipt, process-evolution tests | Local: qualify a concrete app rollout/rollback plan across mixed-version readers/writers when promoted; no automatic distributed schema coordinator. |
| R01 Enrollment/restart/reseed/authentication | `weekend-replicate`, shared TLS admission, [runbook](replication-operations.md), [native credential cutover](snapshot-credential-cutover.md) | Mac/Linux stopped-endpoint leaf/pin changes now test reachable stale-pin/client rejection, interrupted startup, exact reopened receipt and retained bytes, and valid retry. Local capture remains separate from follower durability. Live reload, torn config writes and general revocation/distribution are outside this drill; Windows remains open. |
| R02 Bounded transfer/retry and lost reply | Absolute 30-second socket budget, retained same export within retry, reopen/poll reconciliation | File verification/flush can exceed socket deadline. Owner sets transfer envelope; restarted sender captures a new export, not transparent old-export replay. |
| R03 Cleanup, manifest growth and rotation | Offline cleanup/compact commands and tests | Implemented Mac/Linux; Windows explicit unsupported manifest compaction and fresh-directory reseed. Native Windows execution remains. |
| R04 Progress and lag diagnostics | `status`/`poll` report receipt/revision/digest; live server owns manifest lease | The [private JSON progress helper](replica-progress.md) now separates inspected capture and fresh follower receipt, exact/newer/behind/digest-conflict relations, unknown failures and historical prior state. Native Mac/Linux tests include lost response, reopen and refusal. No commit timestamp or time-lag guarantee exists; continuous monitoring/Windows remain unqualified. |
| R05 Mixed-version and backup recovery | Core wire compatibility, verified follower restore/reseed, credential cutover and [actual CLI upgrades](snapshot-cli-upgrade.md) | Old 315068f/current 365daf6 release CLIs pass both sender directions, persisted follower reopen, failed transfer/retry, exact receipt/digest and full retained data on Mac/Linux. Both use the same ALPN. Other promised versions/config changes and Windows require their own gate; no automatic promotion authority. |
| C01 Lawful CRDT records and reserved row protection | `Record` laws documented, encode/decode preservation, actor checks; raw API bypass explicitly documented | Application must enforce exclusive reserved-row writes; do not label runtime checks a proof of arbitrary app merge laws. Local: reusable law-test helper for app records would improve adoption without adding consensus. |
| C02 Causality/replay/resolution/bounds | Register event oracle, two-peer schedules, bounded frame/set/counter containers, repeated-sync no commits | Implemented finite models. Larger causal histories, forged-state shapes excluded from a harness and other policies need their own tests; no expansion by association. |
| C03 Evolution, actor replacement and stale backup | [policy migration](crdt-policy-migration.md), [credential cutover](study-credential-cutover.md), actual [pre-ALPN CLI upgrade](study-cli-upgrade.md) | Mac/Linux qualify fixed-roster credential replacement and a distinct binary upgrade: mixed versions reject without remote acknowledgment or lost offline edits, coordinated new peers preserve retained records and converge without repeat commits. Actor-generation/roster migration routing and other version pairs remain separate; neither drill resets actors. |
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
actually probed, including [dated 2026-11-12](local-toolchains-a56083c.md), reject
MC/DC instrumentation. Condition outcomes, manual pairs,
mutations and compiler line/branch data remain distinct evidence.

| Roadmap module requirement | Existing source/test ledger | Acceptance still open |
|---|---|---|
| Engine observations, lifecycle, retained bounds, failure atomicity | `engine.rs`, transaction/schedule/concurrency suites, engine ledger | Trace every remaining feasible condition and generic instantiation; current core-library report is not coverage of every API/profile. |
| Tree/value collisions, shape/size/scan limits and sharing | Tree/value ledgers; independent models and directed collision fixtures | Actual hash collisions are directed controls, not credited to random 64-bit collision fuzzing. Broad adversarial shape combinations remain local work. |
| Allocator counters, ownership, atomics and abort policy | Memory ledger; allocation sweeps, Miri, bounded Loom and negative controls | Models cover chosen ownership schedules, not every native execution/ABI. Candidate/architecture reruns and remaining dispositions stay visible. |
| Storage core lease, I/O progress, poison, sync and copy | Storage/publication ledgers and ByteStore fault tests | Custom adapters must meet contract. Native quota/device behavior is not implied by synthetic result mapping. |
| Codec slots/witness/selected graphs/cache and work bounds | Recovery metadata/graph ledgers, corruption fixtures, structured fuzz | Retain infeasibility proofs tied to order/bounds; no implicit older-root fallback. Compound parser work/caches still need resource experiments. |
| Native file adapters | Native ledger, Mac/Linux tests and [actual hosted Windows execution](hosted-qualification-cd7cdfe.md) | The scoped native CI Windows profile now passes; remaining condition pairs, other architectures/filesystems and actual device assumptions still require their own evidence. cfg-dispatch classification alone is insufficient. |
| Replacement/simulator | Replacement/namespace and crash oracles | Modeled flush/reorder/lease model must remain separate from kernel/device failure claims. |
| FFI/header/public exports | FFI ledger, ABI manifest checks, valid-handle fuzz, native consumers | Caller UB stays outside acceptance; exact packaged differential traces and unsupported-target paths need their own evidence. |
| Schema/typed runtime | Schema/typed ledgers; six additional public guards and mutations | Same-instantiation poisoned-write pair is qualified; do not generalize its counts to other monomorphizations or schemas. |
| Derives | Pure expansion seam, negative compilation, renamed consumers, token fuzz; [four direct expansion pairs](macro-expansion-guards-032dadf.md) now have same-process outcomes and mutation sensitivity | Earlier four zero unit-view outcomes already had downstream diagnostic tests. Direct attribution is now qualified; generated code, `?`, compiler-host execution and other explicit blind spots keep their own scope. |
| Snapshot replication | Wire/follower and CLI ledgers; native TLS and stopped-endpoint cutover cases | Wire checks, local durability barriers and remote acknowledgement are distinct. CLI binary upgrade and deployment admission are not closed by decoder or credential tests. |
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
| Sanitizer/Miri/Loom/corpus infrastructure | Calibration, saved seeds, deterministic replay, bounded atomic models, native thread tests; [complete POSIX helper runner](helper-controls.md) with frozen and [hosted Mac/Linux controls](hosted-qualification-cd7cdfe.md) | Local/CI entry points now cover all nineteen suites in both Python modes, with exact test IDs, explicit skips and bounded watchdog controls. Hosted Rust ASan/TSan workspace results are distinct from the calibrated Linux C/Rust ASan controls; timeout/capacity cancellation is not passing evidence. Windows helper portability, final candidate/platform runs and lasting retention remain separate. ASan without rebuilt std does not instrument all code; Miri/models do not prove native syscalls. |

## Crash, platform and delivery gates

| Gate from roadmap | Concrete evidence | Acceptance action |
|---|---|---|
| Fault-model publication | Storage/simulator contract, selected-root/witness semantics, [operations](operations.md) | Owner/platform: validate actual filesystem/device flush assumptions before power-loss promises. |
| Allocation and I/O failure matrix | Simulator, corruption/fault sweeps, current/retained state oracles, [native drills](native-fault-drills.md), [actual Linux append EDQUOT](native-quota.md) | The named native quota append/recovery profile now passes. Other failure phases/platforms and systematic second-failure combinations with open dispositions require their own evidence. No blanket closure follows from one recovery test. |
| Native platform/filesystem matrix | Exact-21 Mac/Docker aarch64 Linux ext4 checks plus [cd7cdfe hosted Mac arm64, Linux x86_64 and Windows MSVC x86_64](hosted-qualification-cd7cdfe.md) | Actual Windows and x86 Linux execution now exists for the named profiles. Other advertised ABI/filesystem/feature combinations and final candidate consolidation remain; neither hosted nor container success proves a deployed physical device's flush behavior. |
| Compatibility manifest and upgrade/rollback | [storage cases](storage-compatibility-21e5d26.md), logical/v2 fixtures, typed retained migration | Local: promised CLI/config directions; owner chooses supported versions. Historical pre-1 reader compatibility is not every future version. |
| Binary delivery | [private artifact/source evidence](local-qualification-21e5d26.md), relocation, symbols and notices | Shared consumer traces are now recorded in their exact artifact scope. Local: final feature-isolated/combined candidate matrix. Owner: final clean candidate, support matrix, retained evidence bundle and attribution review. |
| Long-lived soak and reproducibility | Finite soak, accelerated month and cross-OS peer rounds; corrected launcher rejects incomplete round sequences and persists failures | The [Mac/Linux 1,800-second runs](soak.md#extended-macos-and-linux-controls-at-7a9e661) now retain exact-state/resource checks and bounded cleanup; wall-clock longevity cannot be inferred from simulated calendar days. |
| Performance/scale | [SQLite comparison](sqlite-baseline.md), resource latencies, monthly workload metrics | Owner sets application budgets. Local: credible sample counts, tail latency under concurrency/maintenance, RSS/disk/restore and follower-age diagnostics; small samples do not establish p99 guarantees. |
| Support and release | Security owner/contact, private patch policy, unsafe review and scoped runbooks | Owner: supported versions, response goals, exceptions, incident/rollback drills. No production release is currently qualified merely because package tooling says candidate-eligible. |

## Highest-impact remaining local work

1. **Evaluate a VALUE identity index separately.** The serialization measurement
   establishes quadratic VALUE lookup work and complete current/retained oracles.
   A candidate must preserve emission order and exact fixed-identity simulator
   bytes, qualify rebuilding from old caches, and repeat allocation/budget/crash
   tests. The 64-root DATA measurement now exposes its separate history cost;
   include that axis when comparing candidate designs. Owner budgets still
   determine acceptable service latency and RSS.
2. **Qualify remaining delivered failure profiles and transitions.** Named
   binary/credential upgrades and both transaction validation modes now have
   native evidence. Exact-package Linux C additionally exercises uncertain-I/O
   reconciliation in a fresh process. Other language/platform failure profiles,
   compound failures, actor-generation rollout and each further promised version
   retain their separate obligations. Do not credit one profile to all surfaces.
3. **Consolidate one final candidate bundle.** Existing results retain their
   source, feature, compiler, artifact and filesystem scope. Final package,
   source-consumer, sanitizer, mutation/corpus and workload evidence must be
   reconciled against the chosen immutable candidate. The shared trace and
   progress helper add evidence, without retroactively qualifying every profile.
4. **Resolve continuous coordinator receipt policy before implementation.** The
   tested permanent 1,024-slot cap intentionally rejects overflow. The owner
   chooses whether bounded sessions suffice; otherwise specify safe request-ID
   epochs/archive and reconciliation of stale retries. Silent eviction is not
   an acceptable shortcut.

The named hosted Windows and Linux append-quota profiles now have actual
execution evidence. Additional native profiles, actual device power interruption,
production workload budgets, support promises and distribution approval remain
distinct platform/owner gates. They cannot be replaced by local models or more
simulated calendar days. Remaining feasible decision pairs and broader
workload/error traces stay visible in the module and fuzz rows above.

## Subsequent local evidence

The shared optimized artifact trace and versioned progress helper now add
cross-language state verification and separate capture/receipt observations
in their documented finite scopes. The progress helper reports
observation timestamps rather than inventing commit-time lag. The Mac/Linux
1,800-second lifecycle runs extend wall-clock evidence for the four-record
workload; they do not establish larger-cardinality or month-long uptime.
Minimum-compiler and dependency snapshots are qualified separately. Stopped
credential cutovers and named binary upgrades now pass their Mac/Linux profiles;
further promised transitions, candidate/platform/device qualification, feasible decision
pairs and declared service/support budgets remain open. This audit is updated
with evidence; it is not a replacement for those acceptance gates.
