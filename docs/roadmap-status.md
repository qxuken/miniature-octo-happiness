# Production qualification status

Release scope includes the local store, typed Rust, snapshot replication,
CRDT peers and coordinator, as requested on 2026-11-09. The project uses
`MIT OR Apache-2.0`. Cargo packages stay private with `publish = false`.
The project name is provisional. Baseline `1e883c8` checkpoints the prior
decision tests and first four fuzz targets.

| Gate | Progress and remaining evidence |
|---|---|
| Credential cutover | Stopped-endpoint [snapshot](snapshot-credential-cutover.md) and [StudyProgress](study-credential-cutover.md) leaf/pin replacement passed native macOS, Linux and Windows debug/release. The tests verify stale identities, preserved local progress and retained roots, and repeat sync. Live reload, actor migration and other versions remain separate. |
| Decision ledger | Scoped metadata, graph, transaction, memory, FFI, I/O, simulator and replacement pairs exist. The historical [source audits](decisions/README.md) classify 2,561 explicit records across 51 files, including shared TLS separately for each binary profile. Later VALUE/DATA index and [buffer-growth controls](serialization-buffer-growth.md) have separate mappings; fresh [shared-cache inventory](serialization-cache-sharing.md) has 2,604 records and 319 macro blind spots. Historical mappings, supplemental FFI/derive mappings and current inventory counts remain distinct. Signed alternate leaves are rejected by both exact-pin checks. Directed [handshake pairs](tls-handshake-decisions.md) now establish unfinished-handshake/pending-write independence at both endpoints; four guard omissions are detected. Delegated failures, generated-code inventory and compiler MC/DC measurement remain separate obligations. |
| Fuzzing | All 22 targets passed the [exact-21e5d26 extended ASan campaign](fuzz-qualification-21e5d26.md), totaling 1,431,321 actual executions under per-target 100,000-run/120-second bounds, including 31,034 register-causality executions. Independent stable replay passed 25 tests; source, both locks and 242 seeds were verified. This supersedes the limited post-fix register count evidence for this checkpoint. Longer campaigns and broader oracles remain. |
| Inspection | Read-only graph verification and CLI implemented. Typed index checking is available. Native error context and cheap append-space queries implemented. Verification stage/structure-start offsets implemented and fault-tested. |
| Backup/restore | Verified create-new copy, retained roots and process restore tested. Typed restore across processes and old retained schemas is tested. Deployment restore-time budgets remain. |
| Migration/salvage | Logical export/import format 1 implemented with frozen fixture. Explicit complete-root salvage is tested. Future-version conversion remains. |
| Storage lifecycle | Compaction exists. Append headroom and near-cap recovery/compaction tests implemented. Indexed recovery measured near the cap. Cleanup policy and device qualification remain. |
| Resources | Per-value/transaction limits and opt-in aggregate core layout admission exist. A [four-axis native qualification](resource-envelope.md) passed macOS/APFS and Docker Linux/overlayfs: 64 depth-64 retained roots, near-exact 64 MiB core saturation/recovered draft admission, the synthetic exact image boundary, and a naturally full image with 63 MiB reachable payload. A requested 1 MiB worker stack completed its bounded parser/value workload. RSS is measured separately, not capped by core accounting. The later [compound profile](compound-resource-envelope.md) combines large payloads, 64 depth-64 roots, eight held drafts, 96 MiB saturation and 98.58% image occupancy, with contention/RSS and directed recovery on Mac/Linux. Other feasible combinations, longer load and native Windows remain. |
| Packaging | [V2 cross-language traces](artifact-trace-validation-modes.md) now compare both validation modes through exact relocated Mac Rust/C/Odin and Linux ext4 Rust/C against one independent 134-cell oracle, preserving v1 bytes. [Exact 21e5d26](local-qualification-21e5d26.md) private Mac/Linux artifacts and copied-source Rust consumers passed, including relocation and independent hash checks. Runtime identity and private artifact checks added. Private identity/notice checks existed at a7dc343; later relocation testing exposed and fixed its temporary loader-path defect. Windows compiled consumers passed at 1ca5e5ab. Windows private packaging passed 6266359; macOS Odin relocation passed locally. Clean 4203e0a source profile passed native and isolated Linux builds; registry inputs remain external. |
| Compatibility | The [typed rollout and rollback](typed-schema-rollout.md) passes on macOS/APFS, Docker Linux/ext4 and Windows/NTFS. [Frozen a7dc343 ↔ 21e5d26 storage cases](storage-compatibility-21e5d26.md) pass both writers/readers on macOS/APFS, Docker Linux/overlayfs and Windows/NTFS with original locks, retained roots and byte-preserving unsupported rejection. The [dcd2402 ↔ 315068f core wire pair](snapshot-compatibility.md#windows-ntfs-result) passes both directions on macOS and Windows private loopback. The separate [snapshot](snapshot-cli-upgrade.md) and [study](study-cli-upgrade.md) release CLI upgrade pairs pass on Mac/Linux. Additional schema transitions, version pairs, mixed-OS traffic and Windows authenticated CLI upgrades remain open. |
| Native matrix | Hosted PR checks at `6cc8ef3` and `bc56084` passed Mac arm64, Linux x86_64 and Windows MSVC x86_64, including the native package jobs, Rust/C sanitizers, Linux quota and library coverage. Native Windows at `af91f05` also passed follower backup/reseed, credential cutover and all 20 exact-DLL cases. Local Mac/Linux C fault and Odin evidence retain their own scope. Candidate consolidation and device power-loss gates remain. |
| Unsafe/concurrency | [Fresh exact-21e5d26 sanitizer checks](sanitizer-qualification-21e5d26.md) passed Mac workspace Rust ASan/TSan and calibrated shared-runtime C/Rust ASan on Mac and Docker Linux. Thread/process/sanitizer tests exist. Bounded ownership/admission Loom models, negative controls, Miri and abort checks passed. C-side ASan passed macOS and Linux with calibration at 4203e0a and 2490631. A fresh independent [core unsafe-boundary review](unsafe-review-2490631.md) found no actionable defect under the documented FFI and quiescence contracts; 13 focused Miri tests passed. The review covers the named source files, not dependencies or every target ABI. Additional model workloads, native Windows and x86 Linux qualification remain. |
| Typed Rust | Models/migrations/indexes exist. Verification/rebuild implemented and fault-tested. Explicit bounded migration with atomic progress tested. A [mixed-version rollout and rollback](typed-schema-rollout.md) passed on macOS and Docker Linux for an additive default and fixed unique layout. Additional schema/platform profiles and the final candidate remain. |
| Snapshot replication | Verified snapshots and separate acknowledgment exist. Manifest bounds, cleanup, compaction and explicit reseed are tested. Offline CLI cleanup and macOS/Linux manifest compaction now preserve the exact receipt and retained graph; Windows compaction rejects before opening files and uses explicit fresh-directory reseeding. The 14 focused app tests passed macOS debug/release and Linux debug, with macOS strict Clippy; local Windows cross-compilation was blocked by missing SDK headers. The private [weekend-replicate CLI](replication-operations.md#authenticated-native-cli) adds exact-leaf mTLS, distinct ALPN, bounded socket deadlines and same-export retry. Exact tree `a0aad9f9ff95a2998ca623b654c452ae02658b6b` passed macOS and Linux/ext4 nine app plus one CLI test in debug/release, macOS strict Clippy/full Rust/C/Odin checks, and authenticated Mac→Linux ship/restart/poll: follower revision 13 retained root 1@11 and exact digest survived primary advancement to 14. That historical qualification predates subsequent audit tests and maintenance commands. Optional Linux Clippy was unavailable. Separate native macOS [dcd2402 ↔ 315068f core wire-v1 qualification](snapshot-compatibility.md) passed both directions using original locked dependencies: durable 4/retained 2, independent export digest, reopen/retry, and mutated v255 **poll header** rejection with follower files unchanged. This does not qualify upload faults or mixed-version CLI TLS/trust rotation. Frozen `65d142e` also passed the native macOS [verified follower backup/restore and fresh-stream reseed](follower-backup-restore.md): current 4/retained 2, exact former/new receipts, wrong-enrollment status and authenticated-upload rejection, and all original files unchanged. This requires maintained quiescence and does not implement automatic fencing/failover. Broader deployment, checkpoint/OS combinations and physical power-loss qualification remain. |
| CRDT peers | Fixed two-actor TLS demo exists. Register admission rejects unwitnessed retirement and incompatible cyclic histories, with bounded causal-event/frame oracles. The [explicit migration example](crdt-policy-migration.md) qualifies reviewed schema/policy conversion, fresh identities, stale-backup recovery and repeat sync. Native macOS/Linux process tests now verify both old writer leases, six staging interruptions, independent destination reopening and complete old current/retained preservation. Serial/default debug/release pass after correcting a child-output fixture bug. Raw reserved-row ownership, distributed quiescence/routing, operational cutover and broader policy/deployment qualification remain application gates. |
| Performance | Original [serialization scaling](serialization-scale.md) and [history](serialization-history-scale.md) reports preserve the linear-search baselines. The [VALUE index](serialization-identity-index.md) and [DATA index](serialization-data-index.md) now pass exact-byte and allocation/budget/crash/retained-root checks on Mac/Linux. DATA comparison work at 64 roots drops from 108,036 to 39,837 including sorting/searches; small histories use more comparisons. [Charged buffer growth](serialization-buffer-growth.md) passes the same compound workload on Mac/Linux, reduces measured copy-domain peak from 166.76 to 143.22 MiB and preserves 17 exact images; live cache slack increases and clean timing/RSS outcomes are mixed. [Shared cache/index reuse](serialization-cache-sharing.md) now reduces entry clones and per-publication rebuild/sort work while adding retained index memory and cold-open work. Affected-cache forks and additional resource profiles remain; no general latency bound is claimed. A [matched durable-history SQLite baseline](sqlite-baseline.md) passed full current/retained byte oracles across 192 data commits, 12 separate pins and 96 warm-cache reopens per engine on macOS APFS, including verified backup/restore/compaction. Three finite small-state rounds are a baseline, not representative scale, service budgets, stack/RSS bounds or other-platform performance qualification. |
| Authenticated peer workload | Frozen ed8f9f9 passed [30 real CLI offline/sync rounds](study-peer-soak.md) between macOS/APFS and Docker Linux/ext4 with exact pins. All six paired post-acknowledgment SIGKILL restarts preserved exact local revisions and displayed values; repeated covering exchanges added no commits after convergence. The final Python-O launcher and independent review passed. This is a bounded accelerated scenario, not a wall-clock month, long-duration soak, new-candidate or native Windows qualification. |
| Coordinator | The [legacy capacity case](../apps/write-coordinator/README.md#finite-native-receipt-capacity-qualification) passed 1,024 permanent receipts on macOS, Linux and Windows. The [opt-in sequenced mode](../apps/write-coordinator/README.md#sequenced-requests-for-long-running-writers) stores one receipt per slot, rejects retired requests and passed 1,152 native writes with repeated maintenance on macOS and Linux. Client journal policy and deployment gates remain. |
| Release operations | [Minimum-compiler/dependency review](local-baseline-ffdf506.md) passed full Mac checks at declared Rust 1.99.0 and all twelve checkpoint locks plus the new progress fixture against 1,490 frozen advisories. [Linux ext4 minimum-compiler checks](local-toolchains-a56083c.md) also pass at 1.99.0. Quota provisioning stopped before engine execution on the available kernel. Support/security policy and provenance recorded; capped cross-OS month and restore passed. [1,800-second Mac/Linux soaks](soak.md#extended-macos-and-linux-controls-at-7a9e661) passed the tiny-record lifecycle workload; longer deployment soak, latency guarantees and final bundle remain. |

The full acceptance inventory is in [production-readiness.md](production-readiness.md).
Rows stay open until evidence is recorded. Physical power-loss qualification
needs the declared OS/filesystem/device stack. Configured CI and executed
qualification are recorded separately.


Exact `2490631` [local qualification](local-qualification-2490631.md) passed the
listed macOS/Docker Linux, sanitizer, package, feature-profile, model and bounded
fuzz gates. Windows linked 47 test executables but executed none; exact-head
native Windows/NTFS and C/C++ package execution remain open. Hosted jobs were
billing-blocked, and line/branch metrics are not measured MC/DC.

## Recorded combined execution

The 2026-11-09 local worktree passed `python3 tools/check.py` in debug and
release on macOS 27.0.1 arm64 with rustc 1.99.1, including Rust/C/Odin
consumers. A source snapshot passed `python3 tools/check.py --skip-odin`
on aarch64 Linux 7.0.14-linuxkit using the local Docker `rust:1-bookworm`
image and overlayfs. That run includes native Rust/C file, process and
thread tests. Snapshot manifest SHA-256 is
`48b4e7ed925d9d2e26a647ca376d9887b87bd2a413b6ce561ac875acdbd258bc`.
The source tar SHA-256 is
`a18d2956c8a632eba76194567f728b77633a5f6f63bc1987392f94fa889dc11a`.
These runs precede subsequent distributed fuzz/coverage additions.
They establish execution on those configurations. Later checkpoints supply native Windows evidence below; physical-device
power-loss qualification remains outstanding.

The allocator null-result test passed natively and under Miri. It replaces
only a thread-local allocation result and checks input destruction and
retry, without exhausting system memory.

Dependency review on 2026-11-09 resolved 76 workspace packages. `cargo audit
--json` reported no vulnerabilities or warnings using RustSec database
commit `309392b18f6b17022f82c701f300433126901068` (1,422 advisories).
This is a database scan of the current lockfile, with no ignored advisories.
License identifiers and notices are being included in private artifacts.

The owner selected a Linux server and macOS client with a simulated month
of stored workload, and `sec@qxuken.dev` as security contact/release owner.
The 30-day simulated workload passed on an actual Linux server/macOS client pair. Measurements and concrete ext4/APFS execution are recorded. The later macOS
600-second soak adds finite wall-clock evidence; longer deployment soak and
physical-device qualification remain open.

Checkpoint `a7dc343` passed native macOS/Ubuntu, AddressSanitizer,
ThreadSanitizer and coverage in GitHub Actions run `42525776563`. Windows
Rust and ctypes passed; its compiled-consumer command incorrectly treated
the import library as C/C++ source. The `/link` fix has a portable command
regression; native compiled consumers subsequently passed at 1ca5e5ab, and
Windows private packaging passed at 6266359. Private artifact creation from
that clean checkpoint passed runtime identity, C/C++, symbol and dependency
notice checks. No registry publication was performed.

The subsequent batch adds indexed recovery (160 ms median for 16,000 scalar
records in a 66,093,056-byte image on this host), append-space diagnostics,
scoped native error context, bounded typed migrations and five macro/typed/operational
fuzz targets. A native 240-edit lifecycle run measured commit/reopen/copy
latencies near the image cap with 64 retained roots. See DESIGN.md for scoped tests and limitations.

The full workspace `tools/check.py` passed again after the indexed recovery,
headroom, diagnostics and typed migration changes, in debug and release on
arm64 macOS, including Rust, C and Odin consumers.

Checkpoint 83954e8 passed all native jobs, AddressSanitizer, ThreadSanitizer,
coverage and Linux bounded fuzzing. The 17 fuzz targets completed 16,409 actual
executions; one hit its configured time budget. This precedes the subsequent
live-budget, source-audit, application/tree fuzz and packaging additions.

## Integrated audit checkpoint (2026-11-12)

The [source inventory](decision-inventory.md) and [module ledgers](decisions/README.md)
classified 2,415 records at checkpoint 8556dc1 with explicit residuals. The later
snapshot CLI audit adds 115 records without changing that historical scope.
Generated Model::patch now has a
direct type-ID acceptance/rejection test. The four focused register guard/oracle
tests passed Miri; the bounded honest-history oracle enumerates 5,461 prefixes.
The current 22-target corpus passed 25 stable replay tests. Historical frozen
4203e0a extended fuzzing completed 1,384,848 executions across 21 targets with
100,000-run / 120-second per-target limits, seed 2; ten reached the run limit
and eleven the time bound. The raw extended cache expired. DESIGN.md records
the observed per-target counts, toolchain and limits. The register target
passed 1,000 ASan runs before the guard fix; the post-fix rerun exited successfully
but its surviving summary does not retain an independently recountable run count. Neither campaign qualifies later arbitrary edits.

The [600-second macOS soak](soak.md) completed 924 rounds and 59,136 writer
commits with concurrent readers, retained roots, reopening and compaction under
a 16 MiB core budget. This adds finite wall-clock evidence, not production-scale
or physical power-loss assurance. Native macOS/Ubuntu/Windows and sanitizer CI
passed checkpoint 4203e0a; newly added tests/CRDT guards need fresh candidate CI.


Audit checkpoint `8556dc1`: fuzz run `43248620063` passed all 22 targets
with 21,031 actual executions. Engine checks run `43248619666` passed native
Ubuntu/Windows, C ASan and Rust ASan.
Hosted macOS, coverage and TSan jobs were cancelled due to runner capacity,
not recorded as passes. This checkpoint excludes the new authenticated CLI;
its independent exact-tree evidence is linked from the replication runbook.

Before offline maintenance, checkpoint `939d683` passed all executed jobs in
Engine run `43258936460` attempt 2 and fuzz run `43258936624`. Native macOS job
`131350167575` was blocked before runner execution by GitHub account billing/
spending limits. Exact archived-source local macOS full checks and TSan passed;
this is not a hosted macOS pass and no workflow change was made.

Shared private CLI TLS hardening now passes 33 focused tests per debug/release
profile on both macOS and Docker Linux with strict Clippy. Study requires
coordinated peer upgrades for `study-progress/1`; snapshot retains its profile.
Five Windows test executables cross-link with Mise-pinned Rust/cargo-xwin;
native Windows behavior remains unexecuted. Source-decision ledgers now bind
2,557 records/51 files/316 macro blind spots, with shared TLS counted per target.
See [TLS evidence and remaining independence obligations](shared-app-tls.md).

The following local tranche adds the directed handshake pairs, explicit CRDT
migration reference, matched SQLite baseline and authenticated cross-OS peer
workload linked above. Whole-workspace all-feature/all-target strict Clippy and
workspace formatting passed after integration; the isolated baseline driver's
formatting also passed. Runtime evidence retains each report's exact frozen
source or explicit overlays. It is not one newly qualified release candidate,
and no repository visibility, remote branch or Cargo publication changed.

## Native limits and coverage-tool qualification

The next tranche adds native migration interruptions, actual Linux ENOSPC during
backup/restore/compaction, and the four distinct resource cases linked above.
The [native error drill](native-fault-drills.md) preserves source/backup bytes,
reopens acknowledged and retained state, and verifies fresh retries after freeing
only bounded private tmpfs ballast. Quotas, other filesystems and device failure
remain separate. Root and independent reviews found and corrected test-marker
and supervisor-timeout weaknesses; no core publication change was required.

[Coverage calibration](coverage-calibration-96e105f.md) now distinguishes full
branch outcomes from independent condition effects, rejects malformed/zero
denominators and unexpected source scope, and binds its fixture/runner. A frozen
library-only condition-mode run passed 124 tests and measured 953/1,133 branch
outcomes across 29 repository files. MC/DC instrumentation is rejected by both
tested compiler builds and is explicitly unsupported, not inferred from branch
results or LLVM's zero-denominator column. Existing manual pairs retain their
own source/condition scope. The initial historical 2490631 check established
only absence of remapped-sysroot leakage. The later
[diagnostic audit](historical-coverage-audit.md) found incompatible mappings and
withdraws both aggregates; those earlier raw totals are not qualified coverage.

Whole-workspace strict Clippy and formatting passed after this integration;
the resource driver also passed standalone formatting/Clippy and six normal and
optimized Python controls. All proof remains tied to the reports' source
checkpoints/overlays. Native Windows execution, combined/adversarial limit cases,
declared deployment budgets, longer soak and real device power-loss gates remain
open. Nothing was pushed or published.


## Local candidate and assurance corrections (2026-11-13)

Exact [21e5d26 qualification](local-qualification-21e5d26.md) passed complete
macOS and Docker Linux/ext4 checks, private relocated artifact consumers and
copied Rust-source isolation. The 652 Mac and 47 Linux packaged artifacts were
independently rehashed against their package manifests; both 568-file source
inventories matched the frozen commit. No crate or repository was published, no remote branch was updated,
and hosted CI was not run.

Coverage investigation identified an assurance defect: successful LLVM exit
status hid profile/mapping mismatches. The corrected driver rejects those
warnings and preserves diagnostics. Historical workspace aggregates at 1ca5e5a
and 2490631 are withdrawn; exact-21e5d26 stable line and nightly condition
aggregates also reject. The explicitly labeled core-library line command passed
124 tests with one executable and no LLVM warnings. The configured future job
uses that scope; full-workspace functional checks remain separate. The
[separate-executable companion](coverage-executables.md) produced 36 clean
independent parent reports in its expanded run, while twelve empty test/source
scopes and all children/build profiles stayed unqualified. These percentages
cannot be pooled into a workspace score. Compiler MC/DC remains unsupported.

The [fresh sanitizer qualification](sanitizer-qualification-21e5d26.md) uses
unchanged exact-21e5d26 sources. Mac workspace ASan and TSan each passed 405
reported tests with rebuilt instrumented standard library. Calibrated shared
C/Rust ASan passed memory, threaded and native-file consumers on Mac and Docker
Linux; both deliberate language-specific controls were detected on both platforms.
This Linux result is the C/Rust consumer profile, not a full Linux workspace
TSan run. Source, compiler, runtime, consumer and log hashes bind each result.

Six [typed guards](typed-decision-gaps-21e5d26.md) now have direct public-API
controls: poisoned Write, duplicate primary ID, absent deletion, corrupt unique
claim ownership, nonfresh initialization and exact format-marker admission.
Eighteen tests pass Mac/Linux debug and release; six isolated guard mutations
are detected. Warning-free same-object condition reports show both outcomes,
including the same closure instantiation for the poisoned Write. The source
ledger remains 2,561 records/51 files/316 macro blind spots; these outcomes
refine dispositions without changing production source or its denominator.

Native Windows/NTFS remains a release gate: current Rust/process tests,
compiled C/C++/Odin consumers and private artifact relocation, authenticated
replication/follower maintenance, migration interruptions, receipt capacity,
credential ACL provisioning and resource workloads need actual execution.
MSVC cross-linking establishes build compatibility only. Physical power-loss
qualification, combined/adversarial bounds, deployment latency/restore budgets
and longer representative soak also remain open.

The root review preserved 37 compact local proof records at
`target/local-assurance-review-21e5d26/index.json`, SHA-256
`721c111fa60798d31ea5fc1badf6ce0d4feec9910d593da535cb66c65c777a01`.
Copies retain exact original hashes, separate source/overlay identities and
explicit coverage supersessions; no private credentials or binaries were copied.
Package manifests, sanitizer artifacts and full coverage mappings remain in
their separately recorded local directories. These ignored artifacts are not
a permanent retention or release promise. Final workspace formatting, strict
all-feature/all-target Clippy and whitespace checks passed after source freeze.


## Requirement audit, operational observations and longer native runs

The [acceptance audit](production-acceptance-audit.md) now maps each functional,
surface, module, fuzz and delivery requirement to evidence and unresolved work.
It does not close delegated or unmeasured conditions by association. Four
[direct macro expansion pairs](macro-expansion-guards-032dadf.md) add same-process
attribution and four detected guard mutations for diagnostics already covered
by downstream compilation. Production source spans and inventory counts remain
unchanged.

The [serialization experiment](serialization-scale.md) compares identical logical
content with distinct versus shared value identities across three breadths and
eight requested epochs. Distinct 128→512 breadth produces approximately 16×
cache comparisons. The largest distinct case correctly admits seven epochs,
rejects the eighth before publication, then preserves every current/retained
byte through reopen, backup, restore and compaction. Its required new VALUE
regions exceed physical headroom independently of the observed error. The strict
launcher validates the full phase sequence; early limits or omitted maintenance
cannot qualify. Whole-process sampled RSS and clean phase timings remain finite
observations; index design remains separate; the later history-axis report below extends to 64 roots.

The [shared artifact trace](artifact-trace.md) now exercises optimized source
Rust, C and Odin consumers on Mac, and Rust/C on a private Linux ext4 volume.
Every 59-step transcript matches an independent model of complete nested values,
transaction conflicts/terminal states, overlays, current/held/retained roots and
reopening. Exact packages identify 21e5d26. Root review corrected dangling output
admission and strengthened buffer-length/sentinel/error-output checks before
final qualification. An earlier host-bind rustc SIGBUS occurred before driver
execution; its cause remains unestablished, and the final ext4 result is recorded
separately. This does not qualify all public API/error variants or Windows.

The [replica progress helper](replica-progress.md) emits versioned private JSON
with separately inspected local capture, fresh authenticated follower receipt,
exact/newer/behind/different-digest relation and historical prior metadata.
Failed transfers never turn a local export or old saved receipt into a fresh
remote acknowledgment. Fourteen controls including native TLS lost response,
restart, refusal, repeat poll, stale capture and digest conflict pass normally
and with Python -O on Mac/Linux. Review also closed bounded-output, child cleanup
and malformed-input gaps. No core wire, publication barrier or application API
changed; timestamps describe observations, not elapsed replication lag.

[Mac](soak.md#extended-macos-and-linux-controls-at-7a9e661) and
[Linux ext4](soak-linux-7a9e661.md) each passed the full 1,800-second tiny-record
native lifecycle workload at 7a9e661. Mac completed 1,941 rounds/124,224 writer
commits; Linux completed 10,578 rounds/676,992 writer commits. Both preserved
all current/retained/maintenance oracles, bounded accounting and exact summary
arithmetic. These extend duration on the measured configurations, without
claiming month-long uptime, larger cardinalities or device power loss.

The [baseline review](local-baseline-ffdf506.md) separately verifies the declared
minimum compiler and immutable advisory scan. Actual EDQUOT remains untested:
the attempted quota fixture could not mount before any engine operation, and
the available kernel lacks the relevant quota formats. Native Windows, device
failure, further feasible decision pairs, final candidate consolidation and owner
service/support budgets remain open; stopped credential cutover is recorded below. All changes and
proof are local; repository and crates remain private, with no push/publication.

A compact local review index at
`target/local-assurance-review-3cd621c/index.json` preserves copies of fourteen
selected evidence indices and three independent reviews. Original artifacts
retain their source/platform scopes; this is not a final release bundle.
Completed Docker scratch containers/volumes were removed only after checking
the exported evidence. Host artifacts and all three tool/runtime images remain.

## Stopped credential cutover and maximum retained-history axis

New [snapshot](snapshot-credential-cutover.md) and
[StudyProgress](study-credential-cutover.md) process tests pass Mac/Linux debug,
release and strict targeted Clippy from frozen a56083c plus their separate test
overlays. Reachable stale credential/pin attempts reject, an interrupted cutover
preserves acknowledged state, and coordinated fresh credentials succeed. Snapshot
local capture 4 remains distinct from follower receipt 3 until a valid upload;
full retained rows, export digests and unchanged poll/reopen manifests are checked.
Study peers preserve offline edits and both retained records, converge complete
causal state, and add no commit during repeat sync. These drills do not qualify
torn configuration writes, live reload, actor migration or binary upgrades.

The [history axis](serialization-history-scale.md) passes 32 paired processes
across Mac/Linux at 1/8/32/64 roots. DATA comparisons reach 108,036 at 64 roots,
matching independent arithmetic; complete current/retained bytes survive reopen,
backup, restore and compact. No production optimization follows from these small
finite timings. Six controls include cleanup failure and requested compiler
selection; review fixes were rerun under frozen v3 sources.

[Linux Rust 1.99.0](local-toolchains-a56083c.md) now passes the complete existing
Rust/C checker on ext4, complementing the Mac minimum check. It covers exact
a56083c before the new test overlays. The dated 2026-11-12 compiler still rejects
MC/DC instrumentation. The [artifact trace selector correction](artifact-trace.md#compiler-selector-correction-and-fresh-replay)
also passes nine controls and fresh optimized Mac Rust/C/Odin and Linux Rust/C
replays, all five transcripts unchanged. Prior proofs retain their original
compiler/source scope. All steps remain local/private; native Windows, device
and quota-capable execution, broader mixed-version/decision cases, final
candidate consolidation and owner service/support choices remain open.

Root integration review at a8e8122 additionally passes workspace formatting,
strict all-feature/all-target Clippy and all-feature debug workspace tests on
Mac Rust 1.99.1. The 57 successful summaries report 422 tests, including doctests;
these are not a coverage denominator. All 165 selected tracked code/Cargo inputs
stay unchanged. The current combined debug run complements the separate native
debug/release cutover qualifications; it is not a new packaged release matrix.
Proof: `target/roadmap-cutover-review-a8e8122/{evidence.json,summary.json}`.

The compact local index `target/local-assurance-review-a8e8122/index.json`
preserves eighteen selected proof/review records and checks all nineteen
package manifests remain private. It retains the distinct source/compiler
scopes; this is not the final release bundle. Runtime production sources remain
unchanged from 21e5d26, and no remote or registry action occurred.

## Actual binary upgrades and delivered publication errors

Named [snapshot](snapshot-cli-upgrade.md) and [study](study-cli-upgrade.md) release
CLI upgrades now pass native Mac/Linux gates using separate original archives
and lockfiles. Snapshot 315068f/365daf6 interoperate in both directions with
exact durable receipts and full retained bytes. Pre-ALPN study fb80989/current
365daf6 reject a mixed deployment without losing local commits, then converge
after coordinated upgrade with both retained records intact. Repeat/restart
sync adds no commits. Neither pair establishes arbitrary version compatibility.

The [134-cell delivered trace](artifact-trace-validation-modes.md) now compares
Records and WholeDatabase under data, empty and retention-metadata publications
through five optimized Rust/C/Odin or Rust/C consumers. All transcripts match
an independent model; original v1 bytes/oracle and thirty additional actual
driver controls remain separate. Packages still identify exact 21e5d26.

The [Linux delivered-C drill](delivered-c-faults.md) separately verifies cleared
commit outputs, terminal/recovery-required states and request-ID reconciliation
in six fresh recovery processes. IO and uncertainty reopen complete revision
3 or 4 as expected; all final states are 4 with retained revision 2. Injected
syscall EIO and actual tmpfs ENOSPC are explicitly distinct from device power
loss. Five helper controls pass normal/-O on Mac and Linux; 91 copied artifacts
and both owned-resource absences were verified.

Review corrected descendant cleanup, copied-proof/source verification and
independent Docker cleanup. Actual negative controls detect exited-leader
leaks, corrupted copied artifacts and cleanup failures. Fresh v2 upgrade proofs
bind the fixed tools; older passing runs keep their original narrower scope.
Runtime production sources remain unchanged. Further delivered language/platform
failure profiles, actor-generation rollout, native Windows, device/quota
execution, remaining feasible decision pairs, final immutable candidate and
owner service/support decisions remain open. All work is local/private.

Root review reran forty helper/oracle controls normally and under Python `-O`
(eighty reported executions), including actual descendant cleanup and strict
two-process fault-result schemas. Formatting and whitespace checks pass.
`target/assurance-review-a35e5dc/index.json` retains their logs/hashes, copies of
four final gate indices and the nineteen private package-manifest checks.
This compact checkpoint preserves distinct evidence scopes rather than creating
a unified release bundle or coverage denominator.
The [continuous helper runner](helper-controls.md) closes that local wiring gap:
all nineteen suites run in separate normal/`-O` workers, with exact test IDs,
explicit skips, source stability and bounded process/output cleanup. Frozen
Mac/Linux runs each pass 268 tests plus four registered skips across 38 workers;
seventeen runner controls include the detached-child/grandchild regressions found
during review. The workflow now schedules the same Mise-pinned command on both
POSIX platforms. Their [hosted executions](hosted-qualification-cd7cdfe.md) now
pass separately; the local results retain their own scope. Native Windows
helper portability is not inferred.

## Hosted native platform execution

The [cd7cdfe gate](hosted-qualification-cd7cdfe.md) passes actual native Mac arm64,
Linux x86_64 and Windows MSVC x86_64 checks, including compiled Windows C/C++
clients and package relocation. All six native package manifests/payloads are
checked, with raw Windows hashes and checkout line-ending conversion recorded
explicitly. The previously unexecuted native Windows profile now has scoped
evidence. Packaged Rust source, Odin and broader delivered state/error traces
retain their separate profiles.

All 22 fuzz targets, hosted helper controls, Linux C/Rust ASan/atomic models,
Rust ASan and a complete PR TSan run pass. The duplicate push TSan run timed out
near the final capacity test; it receives no passing credit. Measured complete
and incomplete runs support increasing only TSan's budget to 40 minutes, keeping
all 1,024 receipts, restarts, replays and maintenance checks and adding uncaptured
phase progress. PR/main-push/manual engine triggers avoid duplicate feature-branch
matrices. Runner-capacity cancellation and its scoped retry remain explicit.

The library-only line/region coverage report has clean LLVM diagnostics and
stable source. Branch/MC/DC denominators are unmeasured; no withdrawn pooled
percentage is restored. Physical durability, actual quota, remaining feasible
decision pairs, complete advertised feature/ABI profiles, final immutable
candidate and owner service/support choices remain open. Crates remain private.

## Native Linux quota exhaustion and recovery

The [Linux quota controller and fixture](native-quota.md) are implemented and
wired into a dedicated Ubuntu 24.04 job. Local checks include twelve normal/`-O`
controls and Linux ext4 semantic/fresh-process/no-quota negative controls. The
complete helper runner now covers twenty suites, forty workers, 292 passes and
four registered skips per run. The hosted job now passes at `5e665f1`: actual
errno 122 is distinct from global disk exhaustion, current/retained revisions
3/2 survive failure and fresh reopen while full, and retry/reopen reaches 4/2
after freeing ballast. Complete row bytes, source/tool identities, uploaded
artifact digest and all 56 evidence payloads are rechecked; mount and loop
absence are proven. This closes the named F05 native append quota profile.
Other quota failure phases/filesystems and physical power-loss assumptions
remain separate obligations. No crates are published.

## Completed quota integration checkpoint

PR head `e20ceceebd6f8ba1ec0da89a7514f93d6ffd706b` has completed green
[Engine checks](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43484984404)
and [bounded fuzzing](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43484984417).
All ten engine jobs passed: native Mac/Linux/Windows checks and packages,
Mac/Linux helper controls, native Linux quota recovery, Rust ASan/TSan,
Linux C/Rust ASan with atomic models, and core-library coverage. Fuzz helper
controls, committed-seed/model replay and all manifest targets also passed.
The Windows native job executed its compiled C/C++ consumers; that conditional
step is intentionally skipped on Mac/Linux. No other engine steps were skipped.

This closes the pending full-CI integration check for the quota work. The
source-bound detailed quota artifact audit remains at `5e665f1`, and earlier
package/coverage audits retain their named revisions. This update verifies job
and step outcomes rather than claiming a new artifact-payload audit or complete
MC/DC/device qualification. The API status snapshot is preserved in
`target/ci-progress-e20cece/index.json`, SHA-256
`9fe9b542238b147773ef5df9356eaf6b5ecbf123a8482d3675327630db3e6669`.
Remaining release-candidate, device, broader failure-profile and owner gates
remain open. Crates remain unpublished.

## Windows delivered trace prepared for native testing

The [Windows C/C++ handoff](windows-artifact-trace.md) now includes an executable
runner, exact reference package/manifest/DLL pins, native-agent commands and
acceptance criteria. It reuses the existing 134-cell model/common consumer,
checks process-local DLL path/hash/identity before and after execution, and adds
compatibility, input-rejection and loader-mismatch controls in both languages.

Local controls and cross-compilation pass; the complete helper runner now has
21 suites, 42 workers, 316 passes and four registered skips. Native MSVC and
Windows execution are explicitly pending. The existing Windows smoke/relocation
qualification is not relabeled as a full-state trace pass. No new automatic
native trace CI gate or production runtime change is introduced by this handoff.

## Native Windows delivered trace completed

The [native result record](windows-artifact-trace.md#native-results-2026-11-13)
qualifies clean harness `80eb0c2` with the exact pinned reference DLL source
`7a490e5ce24910d5d8ec96abe0fb9d218ea9e870`. All twenty C11/C++17 cases pass
on native Windows 11 Pro build 26300, fixed local C: NTFS, x64 MSVC toolset
14.51.36231 (compiler 19.51.36247.0), SDK 10.0.26100.0 and Mise Python 3.14.8.
Twelve controls pass normally and under `-O`. Assertions remain enabled in both
optimized builds; the delivered header/import library and unchanged oracle
are used. No engine or harness fix was required.

Every successful transcript matches exactly, loader proofs bind actual
path/hash/source/ABI/format/features/PID before and after execution, and every
rejection exits 3 without a database. All 23 command trees are cleaned up;
package and harness hashes remain stable. Evidence and its complete hash
inventory are retained locally at
`C:\Users\21asg\AppData\Local\weekend-trace-80eb0c2-20261113\result-initial`;
`evidence.json` SHA-256 is
`ceab27169583071ffa533a4987b6e54af8d47b6898ec7c4d148469cea46f175e`.
This closes the named F15 Windows full-state trace profile, independently of
earlier preparation checks. Other feature/ABI/platform profiles, device and
failure-injection qualification, final candidate and owner gates remain open.
Evidence is local, and no automatic Windows trace CI gate is added. No crates
are published.

## VALUE identity index implemented and qualified locally

The [serialization index report](serialization-identity-index.md) records the
production change from linear VALUE identity search to a bounded, charged,
temporary hash index. Emission-order caches remain authoritative; rebuilding
from recovered caches, equal-content distinct nodes and shared descendants
preserve their original offsets and bytes. Ten fixed-identity images match the
linear baseline byte-for-byte on both Mac and Linux, including retained roots,
reopen, backup, compaction and a subsequent append. No disk/wire/public API or
durability barrier changes; follower durable acknowledgment remains separate.

Both breadth/history axes pass independent complete-state checks in 56 paired
processes. The largest distinct case drops from 440,169,243 to 147,132 identity
comparisons including index rebuild/growth, with unchanged accepted history,
file sizes and cap rejection. DATA comparison counts remain unchanged through
64 retained roots; its accumulated history cost remains open. Four sequential
clean runs show modest commit-time improvement and broadly overlapping
maintenance costs, not a latency distribution or general speedup guarantee.

All-feature core tests pass Mac debug/release (299 per profile) and Linux
aarch64/ext4 Docker debug/release (303 per profile), each with one parent-invoked
helper intentionally ignored. These runs include allocation/budget sweeps,
crash publication/recovery, retained roots and compaction. New collision,
wraparound, cache rebuild, OOM rollback and budget controls also pass Miri.
All 25 fuzz replays, strict workspace Clippy, formatting and six launcher
controls in normal/optimized Python pass. Fresh inventory has 2,571 source
records / 317 macro blind spots and no parse failures; the codec follow-up keeps
the old disposition table's historical scope. All 19 Cargo packages remain
unpublished. Source hashes and local evidence hashes are recorded in the report.

Next: evaluate remaining DATA/cache-vector work and compound resource profiles,
continue feasible decision/failure-profile qualification, then reconcile one
immutable candidate's full platform/artifact matrix. Native Windows has not
executed this new runtime change. The owner cleaned its earlier worktree:
the twenty-case exact-DLL pass remains a recorded historical result, but its raw
proof ZIP is unavailable here. Repeat and retain that gate for the final
candidate; do not claim surviving proof storage from the old paths.

## DATA revision index implemented and qualified locally

The [DATA index report](serialization-data-index.md) adds a temporary charged
revision-sorted vector of cache positions and binary lookup. DATA cache/emission
order, first-match behavior, next-collection validation and publication barriers
remain unchanged. Sorting never selects emission order; rebuilding supports
current-first recovered caches. New index allocation/growth is fallible and
charged, and the index is discarded before publication. Follower durability
acknowledgment remains separate; Cargo packages remain unpublished.

All seventeen fixed-identity images match the baseline byte-for-byte on Mac
and Linux: the original VALUE lifecycle plus seven images covering 64 retained
roots, metadata/empty commits, revision gaps, reopen, release, backup/compaction
and a later append/retain. Existing allocation/budget/crash/recovery and retained
root suites pass in all-feature core debug/release: 302 tests per profile on
Mac, 306 on Linux aarch64/private ext4 Docker, with one intentionally ignored
parent-invoked helper per profile. Linux's two separate image-export tests pass
and owned container/volume absence is verified. Both new index controls pass
pinned Miri, all 25 fuzz replays pass, and strict workspace Clippy/formatting
passes. The helper registry passes 318 tests plus four registered skips across
42 workers; all 19 Cargo manifests still disable publication.

The unchanged independent full-state oracle passes both scaling axes against
baseline and candidate in 56 clean/probe processes. DATA comparisons at
1/8/32/64 roots change from 12/516/16,132/108,036 to
23/686/9,728/39,837, including index sorting and searches. Small histories do
more comparison work; the result is not a universal latency speedup. VALUE
lookup and all disk/wire/public APIs remain unchanged. Fresh inventory has
2,577 records / 318 macro blind spots and no parse failures; changed controls
are documented separately from historical disposition tables.

Next: combined near-cap/concurrent resource profiles and remaining index-rebuild,
cache-cloning/vector-growth costs, feasible decision/failure-profile gates, and
one immutable candidate's complete platform/artifact matrix. This runtime change
still needs native Windows/candidate qualification. The historical Windows pass
and unavailable proof ZIP remain recorded without claiming a new DLL test.

## Compound near-cap drafts, readers and recovery qualified

The [compound profile](compound-resource-envelope.md) qualifies exact engine
c583ca7 using a private frozen Rust/Python harness on Mac arm64/APFS and Docker
Linux aarch64/ext4. It combines 48 large values and all 64 distinct depth-64
retained roots. Physical image occupancy is 94.48% before concurrency and
98.58% after 32 successful commits by eight writer threads, with two concurrent
readers checking current/pinned values. Eight additional held drafts plus real
fillers saturate a 96 MiB requested-core domain; failed publication is terminal,
preserves revision/bytes, and dropping pending work restores the exact baseline
charge. Peak admission remains 107 bytes below the cap. A naturally oversized
candidate rejects without changing the image.

Complete current/retained byte oracles pass after budget failures, fresh-process
reopen and quiescent backup/restore/compaction. A 96 MiB copy budget rejects
before destination writes; 256 MiB succeeds. This is a tested budget pair, not
a minimum copy-memory formula. Native lifecycle RSS, fresh-inspector RSS and
simulator RSS are measured in separate processes; allocation domains and RSS
are explicitly distinct. The report records finite commit/reader samples and
single reopen/copy latencies without converting them into deployment SLOs.

Eight directed failures retain two more drafts and over 90% core pressure.
Allocation/append or dropped-root failures reopen 212; surviving root/witness
or an acknowledged commit reopen 213. Retry/later commit survives another
crash/reopen as 213/214, with all 64 retained roots intact. These faults model
persistence; native fresh-process reopen does not establish device power loss.

Targeted existing library/allocation/budget/durability/compaction/headroom/
retention/concurrency tests pass 183 per Mac debug/release profile. Five new
Python controls are wired into the continuous helper registry, now 44 workers,
328 passes and four registered skips. Focused controls also pass Linux normally
and under optimized Python. Review fixed exact-output-limit handling and
retiring descendants while keeping the leader PID reserved through cleanup;
final v4 platform runs bind those fixes. Strict driver Clippy/formatting and
source/binary/copied-proof hash checks pass. Owned Docker containers/volumes
and runtime groups are verified retired. Production sources and durability/
follower acknowledgment semantics are unchanged; all 19 Cargo packages remain
unpublished.

This closes one combined finite profile. Next remain cache-copy/vector-growth
costs, additional feasible near-cap shapes and longer schedules, systematic
compound/recovery/maintenance faults, delivered/network/native Windows profiles,
owner budgets and one immutable candidate. Earlier Windows results retain their
original source and unavailable-proof status; no new Windows pass is inferred.

## Cache and byte-vector growth qualified

The [growth comparison](serialization-buffer-growth.md) binds baseline fe4d057
and engine 742079c on Mac arm64/APFS and Docker Linux aarch64/ext4. Charged
bounded geometric capacity replaces exact-per-append codec growth; dominating
Writer bulk appends stay exact, and only Limit permits an exact fallback.
Existing allocator/unsafe code and all serialization/publication/acknowledgment
contracts remain unchanged. All 17 golden images remain exactly equal.

The complete compound profile passes again: 64 depth-64 roots, eight held
drafts, 96 MiB saturation, 98.58% image occupancy, 32 commits/two readers,
fresh-process oracles and eight pressured recovery/retry cases. Native grow
calls fall 423,138 → 231,839; measured copy-domain peak falls 166.76 → 143.22 MiB.
A clean exact-library copy rejects at 160 MiB before the change and succeeds
afterward on both OSes. Reopened cache slack adds about 126 KiB of charged live
memory. All capacity remains charged; arbitrary old budget thresholds are not
guaranteed unchanged. Clean latency/RSS comparisons include mixed results and
a slower Mac repeat; no universal speedup or service bound is claimed.

Mac core-package debug/release pass 305 tests each, Linux 309 each, plus golden
exports; three new tests pass Miri. All 25 fuzz replays, strict Clippy/formatting
and 330 helper controls plus four registered skips pass. Fresh inventory has
2,584 records/51 files and 318 macro blind spots, with zero parse failures;
new memory/codec mappings retain explicit MC/DC residuals. Owned Docker/groups
are verified retired. All 19 package manifests stay unpublished.

Next: cache cloning and index rebuild/sort/out-of-order insertion, additional
feasible near-cap combinations and longer schedules, systematic compound/
maintenance/second failures, owner service budgets and a final immutable
platform/artifact candidate. Historical Windows evidence retains its original
source and unavailable-proof status; a fresh native Windows test is still due.

## Cache cloning and index rebuilding qualified

The [cache-sharing comparison](serialization-cache-sharing.md) binds baseline
36592ee and engine 910e490. Immutable VALUE/DATA owners permit metadata-only
reuse and separate affected-cache forks. Index copying avoids per-encode hash
rebuilds and DATA sorting, while native open builds indexes once before
stabilization. No new unsafe/public/format path, barrier or follower-ack contract
is introduced. All 17 exact images, current/retained byte oracles and the eight
compound crash/retry outcomes pass again on Mac/Linux.

Native VALUE clones decrease 411,664 → 269,672; rebuild visits decrease
444,476 → 62,132; DATA sort input decreases 14,898 → 454. Hash-slot copying is
new work. Reopened core usage increases about 129 KiB and repeated recovery
performs more index work; clean timing/RSS observations retain mixed results.
The 96 MiB compound cap, all 64 depth-64 roots, eight drafts, 32 contended
commits/two readers, copy limits and 98.58% image occupancy remain qualified.
Both revisions retain 160 MiB clean-copy success. No universal speedup is claimed.

The named remaining growth MC/DC case is closed: hold rounded admission's
Limit fixed and toggle `capacity != needed` with exact versus rounded requests.
Native debug/release, Miri and Linux execute the false arm and existing true arm.
Two new cache tests cover immutable reuse, entry/index/header failures, exact
rollback and full candidate allocation sweeps. Mac core profiles pass 307 tests
each and Linux 311 each; remaining Mac workspace packages pass 126 tests.
All 25 fuzz replays and strict Clippy/format checks pass. Fresh inventory has
2,604 records/51 files, 319 macro blind spots and no parse failures. Supplemental
memory/codec mappings keep compiler MC/DC and generated/delegated code separate.
All 19 packages remain unpublished.

Next: interrupted near-cap maintenance, second failures during recovery and
longer compound schedules, then reconcile one immutable platform/artifact
candidate. Affected-cache forks/cold-open work and additional shapes remain
measured optimization opportunities, guided by owner latency/memory budgets.
Historical Windows results remain source-bound; this runtime still needs native
Windows qualification.

## Interrupted near-cap maintenance and repeated recovery qualified

The [new finite matrix](near-cap-interruption-recovery.md) at `3136017` passes
Mac arm64/APFS and Docker Linux aarch64/ext4. Fifty-two native SIGKILL cases per
OS cover every source/target durability barrier, metadata/namespace edges and
sampled large I/O. Each canonical image equals the original or compacted control,
then passes a fresh-process complete current/all-64-retained-root oracle. Retry
with scratch names still present succeeds and passes another fresh checker.
Original bytes remain unchanged; only private test directories are retired.
Kernel/device power loss and automatic production orphan cleanup remain separate.

Each platform also passes 108 near-cap recovery cases: six prior commit states
× nine failing opens × two crash policies. Another witness-sync failure follows
without a successful handle on the actual medium between failed opens. Only a
visible nondurable root can fall from 213 to 212; a durable/acknowledged 213 is
preserved. Full current/retained values, lease release and later acknowledged
213/214 fresh-process reopening pass. A 40 MiB core limit rejects without a
handle; publication barriers and follower-durable acknowledgment stay unchanged.

A 42-case smaller regression runs in the regular durability target. Mac core
profiles pass 308 tests each, Linux 312 each, with strict core/private-driver
Clippy and formatting. New helper controls validate complete matrices, source
identity, numeric/Boolean types, kill/exchange positions, calibration and cleanup;
the continuous registry includes them. The report records final evidence hashes
and exact platform scopes. No production code/format or AST denominator changes.
All 19 packages remain unpublished.

Next: additional shapes and longer schedules; maintenance ENOSPC/quota/device
failures and broader torn/all-offset recovery sequences; remaining delivered,
network and native Windows profiles; one immutable candidate's complete
artifact/sanitizer/fuzz/compatibility matrix and owner service budgets. These
finite results advance composed failures without closing those distinct gates.

## Near-cap maintenance ENOSPC and quota recovery qualified

The [new space-exhaustion drill](maintenance-space-exhaustion.md) freezes
`74fd4e8`. Backup, restore and in-place compaction pass actual Linux tmpfs
ENOSPC and hosted Ubuntu ext4 EDQUOT with the existing 98.58%-full image and
all 64 depth-64 retained roots. Quota exhaustion occurs with ample global space.
Each failure produces an incomplete copy, preserves source/backup bytes, and
permits fresh-process full current/retained verification while still full.

After freeing owned ballast, same-UID write/fsync succeeds and all three
maintenance operations retry successfully. Partial images reject; backup and
restore refuse existing destinations; compaction retries with its partial
scratch present. Successful output matches the good backup exactly. Maintenance
keeps revision 212; a later acknowledged commit reopens 213 with all retained
roots unchanged. Both profiles have 32 distinct fixture processes, negative
controls and proven cleanup. Exact uploaded quota binary, artifact digest,
source-tree equivalence and all copied payload hashes pass root review.

The original append-quota gate passes again, as do 34 targeted Mac regressions
per debug/release profile, new/legacy quota controls in both Python modes,
strict private-driver Clippy, formatting and workflow lint. No production
engine/format/AST or follower-ack contract changes. All 19 packages remain
unpublished. This closes the named near-cap copy-phase ENOSPC/EDQUOT gate.

The final complete helper run passes 344 controls plus four registered skips
across 52 workers. Two earlier local attempts failed in the existing compound
output-limit helper with an incomplete observer report. Diagnostics now retain
the original assertion instead of masking it with KeyError; the intermittent
cause still needs reproduction. Full results and failed-attempt paths are in
the report; CI helper checks pass separately on Mac/Linux at the checkpoint.

Next: other maintenance error phases (including delayed writeback and namespace
operations), device and other filesystem/OS profiles; additional shapes/longer
schedules and broader torn/all-offset recovery; remaining delivered/network and
native Windows profiles; one immutable candidate's full artifact/sanitizer/fuzz/
compatibility matrix and owner service budgets. Finite kernel-error results do
not establish physical power-loss durability or automatic orphan collection.

## Incomplete helper observer reports fixed

The [observer finalization fix](helper-observer-finalization.md) reproduces the
missing `failure` field deterministically: a failing cleanup process-table proof
used to jump past the terminal report write. An outer finalizer now records
startup, cleanup, wait and cancellation errors before they propagate, preserving
an earlier observation error separately from cleanup failure. Failed cleanup
still rejects qualification; group ownership, reaping, output/time bounds and
EPERM proof requirements are unchanged. The output-limit test again requires
the actual terminal report and successful cleanup.

Five new controls reject the old observer in normal/optimized Python and pass
against the fix. Full Mac arm64 and Docker Linux aarch64 registries each pass
354 controls plus four registered optional skips (358 tests, 52 workers), with
Mise-pinned Python 3.14.8, unchanged input hashes and verified process cleanup.
An additional 300 real output-limit iterations pass; copied Linux payload hashes
and private container/volume absence are reviewed. The spontaneous historical
trigger did not recur; the report distinguishes that unknown trigger from the
reproduced and fixed finalization defect. No production engine, format,
publication/follower-ack contract or Rust decision denominator changes.
All 19 Cargo manifests remain unpublished. The maintenance/failure/candidate
roadmap above continues; this closes the incomplete-report helper defect.

## Private Rust xtask migration, first batch

The [private xtask](xtask.md) moves check, build, package and cleanup orchestration
to Rust with a clap-derived CLI. The original six check-command plans remain
golden controls; debug/release tests, native consumers, Odin/wgpu, exact-artifact
checks, attribution, relocation and source/configuration gates are retained.
Python ABI/platform-policy/attribution/source validators remain leaf checks;
legacy check/release scripts forward to xtask. CI uses the new entry points.

Cleanup keeps the running tool cache and saved qualification outputs unless
`--proofs` is explicit. Dry-run, source/self/symlink protection and nested-cache
controls accompany real failed-subprocess tests. Compiler-compatible Windows
paths use dunce. Linux validation exposed and reproduced a pre-existing short
checkout-root false positive in the delivered-source oracle; absolute path-token
matching now distinguishes `/src` from `/registry/src`, preserving real checkout
and undelivered-sibling rejection. Both Python modes cover the regression.

Engine/wire/ABI behavior and separate follower durability acknowledgment remain
unchanged. All 20 manifests stay private; no publication command is added.
Product coverage keeps tooling separate, and no xtask MC/DC score is claimed.
Next: bounded process/fault-runner migration with existing ownership/deadline/
output-bound controls, then independent validation before retiring Python leaf
policies/oracles. Native Windows and immutable-candidate qualification retain
their own source/platform gates.

Frozen `964efef` passes full Mac/Linux check/build pipelines: actual logs preserve
all 44/36 original commands and arguments. Each platform's helper registry passes
356 controls and four registered skips across 52 workers. Follow-up `c17cd76`
closes an empty-image source-isolation bypass and requires explicit profile
success. Eleven latest Rust controls pass debug/release on Mac; hardened clean
package profiles pass again on Mac/Linux, with C/C++ relocation, native Rust
source, and Mac Odin plus Docker source isolation. All source/artifact hashes
are independently reviewed in the [qualified results](xtask.md#verified-first-batch-results).
Dirty/existing-output/selected-abort/empty-image/wrong-ID controls reject as
required. Actual cleanup retains proofs/runner and reduces the main target to
about 572 MiB. Native Windows remains a CI gate for these new entry points;
no crate is published and no production-release closure is inferred.


## Rust supervision and fault-runner migration (2026-11-15)

The [bounded supervision batch](xtask-supervision.md) moves POSIX process ownership,
deadlines, capped pipes, RSS, process journals, cancellation and terminal reports
into the private Rust xtask. Compatibility, quota, resource and helper execution
adapters share it. Native permission/ENOSPC and exact-delivery fault parent/worker
orchestration now live in Rust; independent Python byte/state/package oracles and
privileged OS/profile scheduling retain separate scope. New Docker tool snapshots
preserve the workspace graph and locked dependencies independently of the engine.

The deadline/output/cleanup regressions cross the Rust supervisor interface.
Former Python OS mocks move to Rust injection controls with real child retirement;
adapter tests preserve observation/cleanup errors and refuse the oracle on failure.
Detached/reparented helpers, exact 8 MiB limits, inherited file limits, cancellation,
no-overwrite, disabled injection and fail-closed Docker absence queries remain.
Both full helper registries pass 352 checks plus four registered skips across
52 workers per platform, using Python 3.14.8 on Mac arm64 and Docker Linux aarch64.
The native tmpfs campaign and all six exact-package C fault modes pass; recovery
runs in fresh processes without the shim, validates retained revision 2 and retry
to revision 4. The compound native/inspect/eight-fault profile passes with its
independent byte/retained-root oracle. All owned containers/volumes are absent.

Engine bytes, durability barriers and separate follower acknowledgment remain
unchanged; no product MC/DC claim is added. Crates remain unpublished. Next:
remaining Python profile/network/OS orchestration where useful, independent leaf
oracle migration only with cross-checks, and the roadmap's remaining platform,
maintenance-phase/device and immutable-candidate qualification gates. This tooling
step does not close the production-readiness roadmap.

Final reviewed Rust controls pass 27 tests per debug/release profile on Mac and
Docker Linux. Strict all-target Clippy, formatting and Windows MSVC cross-target
Clippy pass. A transport-import finalization edge case and isolated legacy-CLI
import path are reproduced and fixed during review. Thirty-two additional real
exact-limit/overflow iterations pass without retaining their large temporary
payloads. Maintenance ENOSPC also passes through the native Rust supervisor:
32 fixture processes, 30 phase records, complete retained roots, full-space
reopening and successful maintenance retry, with independent owned-resource
absence. Proof review hashes and the precise split between Rust execution and
retained Python policies/oracles are in the linked report. Native Windows runtime
and production/candidate qualification are not inferred from these tooling checks.

Final local proof-review index SHA-256:
`9b05901a864a22d429a53c128b967f95cf649430bdab79776b0afc830bf9bcfb`.
All 20 manifests remain unpublished; no crates.io operation is performed.

## Hosted quota controller inventory fix (2026-11-15)

CI on `61df2be` exposes a false tool-stability rejection: `supervision.py` is in
the pre-run inventory but absent from both post-run inventories. One shared
inventory function now feeds all three checks. A regression reaches the end of
both native and maintenance `main` paths with mocked OS provisioning, accepts
stable tools, and requires an actual supervision-file mutation to fail closed
with cleanup evidence. Both positive cases fail against the previous code with
the hosted error. The inherited helper file-size limit remains enforced; no
privileged provisioning is performed by the control. See the
[diagnosis and scope](xtask-supervision.md#hosted-quota-inventory-regression).
Engine bytes, durability barriers, follower acknowledgment and crate privacy
remain unchanged. Native hosted EDQUOT validation remains required after pushing
this fix.

Both local full helper registries pass **354 checks + four registered skips,
52 workers** on Mac arm64 and Docker Linux aarch64 (Python 3.14.8). Source
stability, all worker retirements and owned Docker container/volume absence are
verified. Local review index SHA-256 is
`5912160693a98ed58610ac4867032bb6ef939f70421953f9b70a6c5c602a1ec8`;
its evidence stays under ignored `target/ci-quota-inventory/`.

The quota fix `95206ed` passes hosted native Linux quota and maintenance EDQUOT,
plus native Mac/Linux/Windows, helper, coverage, fuzz and address-sanitizer jobs.
Its remaining TSan failure is the xtask cancellation fixture's ten-second setup
allowance, reproduced locally. Instrumented runner hashing alone takes 13.794 s
for a 32,375,608-byte local binary. An explicit minimal Git/Docker fixture removes
checkout-growth and Python startup from the control; a finite sixty-second setup
allowance covers provenance, while the fifteen-second cancellation/cleanup bound
and production deadlines stay intact. All five supervisor integration controls
pass with nightly TSan and instrumented std in 18.65 s. No product TSan exclusion
or race waiver is added; details are in the supervisor report.

## Complete xtask conversion in progress (2026-11-15)

The user's [complete conversion scope](xtask-conversion.md) includes all maintained
Python controllers, independent validators and tests before the roadmap resumes.
Packaging, policy, attribution, raw Odin generation, dynamic C-ABI smoke and
delivered Rust-source validation now run directly in Rust. Clean `06d0a15` packages
qualify on native Mac and Docker Linux, including relocation and delivered-source
consumers. Hosted Windows at `e3ff5ad` passes after a CRLF-checkout regression is
fixed without weakening declaration checks. `065dbdb` also passes hosted TSan;
its setup correction preserves the separate cancellation deadline.

The next verified tooling batch replaces workspace sanitizers, shared-runtime
C/Rust ASan, atomic models and fuzz orchestration with bounded Rust campaigns.
Local Mac workspace ASan, all 22 fuzz targets (21,391 actual executions), Mac/Linux
C/Rust calibration plus real consumers, model baselines and both fail-required
mutants pass. Runner controls pass debug/release on Mac and debug on Docker Linux,
with strict native/MSVC cross-target Clippy. The remaining Mac helper registry
passes 314 checks plus four registered skips across 44 workers. Frozen legacy
controls remain accounted for: 20/179 map to Rust tests, with 159 still pending;
56 maintained Python files remain. Coverage/executable attribution, mutations,
the probe, independent delivery/fault oracles and the remaining qualifications are
explicitly open. Detailed source IDs, hashes, failed Apple-runtime calibration
and platform limitations remain in the conversion ledger. No crate is published.

Clean `cfa08d1` subsequently passes all hosted engine/fuzz jobs, including native
Windows and TSan. The next local batch ports both coverage drivers and preserves
18 more legacy controls, taking the mapping to 38/179 and leaving 52 Python files.
Mac old/new line totals match exactly; corrected Docker Linux line, calibration
and independent parent reports pass after replacing assumptions about `.cargo`
and `.rustup` installation paths with exact configured/selected paths. Outside
sources, LLVM warnings, empty metrics and unsupported MC/DC still reject. Mac
debug/release controls and Linux debug pass; 278 remaining helper checks plus four
registered skips pass. Exact counts, source scopes and proof hashes are in the
[conversion ledger](xtask-conversion.md#coverage-conversion-checkpoint).

The Rust [mutation runner](mutations.md) then preserves all 56 reviewed changes
and passes all 13 baselines/56 detections on both native Mac and offline Docker
Linux. All 680 copied inputs are restored, root inputs stay stable, and owned
resources are retired. The final predicate rejects compiler errors, quoted
markers, empty tests and supervision failures. No separate Python suite is
removed in this step: 38/179 legacy mappings remain, with 51 Python files left.

The Mac packaging experiment is also Rust-only: `cargo xtask probe` passes all 29
original cases, including the two expected static-Rust/wgpu linkage failures and
both successful shared-Rust/wgpu profiles. Debug/release runner controls pass on
Mac/Linux; the experiment itself remains Mac-only. The remaining conversion is
50 Python files for independent delivery/fault/state oracles and distributed,
workload, quota and helper qualification. Clean `60f03c0` passes hosted engine and
fuzz runs, including Rust coverage, both sanitizers and native private packaging.

Fault qualification then becomes Rust-only, including the independent delivered
package/phase oracle. Real six-mode C exercise/recovery, disabled-injector control,
native permission/ENOSPC maintenance/retry and owned-resource checks pass. Separate
phase receipts remain mandatory. The ledger records exact old-package/new-runner
identities; 41/179 controls now map to Rust, 46 Python files remain, and the
remaining helper registry passes 272 checks plus four registered skips.

The POSIX delivered-artifact trace is now Rust-only, including its independent
model and offline Docker worker. Both immutable trace formats/transcripts remain
byte-identical. Real optimized Mac Rust/C/Odin and isolated Linux/ext4 Rust/C
consumers match all 134 lines against exact `06d0a15` packages. Cancellation stays
latched between commands while explicit retirement remains available; stream,
file, deadline, package, source, copied-evidence and owned-resource gates remain.
Mac debug/release runner controls and native/MSVC cross-target Clippy pass. The
conversion now accounts for 55/179 legacy controls, with 43 Python files remaining;
the residual helper registry passes 244 checks plus four registered skips. The
Windows trace port and remaining state/distributed/workload/quota/helper tools
remain open. Exact provenance and proof hashes are in the
[conversion ledger](xtask-conversion.md#independent-posix-delivered-trace-conversion).

Hosted `fec3bcf` then passed the Mac/Linux native and helper jobs, address gates,
quota and coverage; Windows caught CRLF-converted oracle fixtures. Explicit LF
attributes and a real `core.autocrlf=true` checkout regression preserve the exact
frozen hashes. All nine focused model controls pass locally; the Windows CI
rerun remains pending rather than being counted as a native pass.

The Windows fixture rerun at `cad7154` passes native validation, full checks,
C/C++ consumers and private packaging. The next Rust Windows trace port is locally
verified by portable controls and strict MSVC cross-compilation; the workflow now
requires 20 actual C/C++ executions with exact DLL attestations. Native process
ownership and private MSVC-discovery controls still require that hosted Windows
run. Legacy Windows tooling remains until it passes; no retirement count or
production qualification is advanced ahead of execution.

The Rust Windows trace passed all 20 exact-DLL C/C++ executions at `b0e0d73`'s
native PR job. The [Windows report](windows-artifact-trace.md#rust-runner-and-native-ci-2026-11-15)
records the actual merge-checkout package source, manifest/DLL/proof hashes and
workflow evidence. Twelve controls move to Rust; the old Windows controller,
model, suite and compiler-environment transport are retired. The conversion
now maps 67/179 controls, with 39 maintained Python files remaining. Crates
remain unpublished.

The release benchmark and paired serialization experiments now have Rust entry
points. Three Mac release samples with compaction pass. Mac breadth/history and
Linux/ext4 history pass against frozen `0f46c2a`; a Linux/ext4 breadth replay
reproduces all original `032dadf` linear comparison counts and image sizes. No
performance guarantee is inferred from the tooling check. Seven more controls
map to Rust (74/179; 37 Python files remain). The archived serialization Python
source remains an exact dependency of the pending compound/resilience conversion.
See the [benchmark and serialization checkpoint](xtask-conversion.md#benchmark-and-paired-serialization-conversion).

The `885f347` hosted Windows run exited during coordinator receipt-capacity
qualification after 512/1,024 commits, with socket error 10060 in the CLI client.
The test now uses an authenticated loopback client with a 30-second absolute
reply deadline, a 128-byte cap, and no retries for failed observations. The
product's three-second network deadlines remain unchanged. macOS debug/release
passes all six writer tests, including 1,024 receipts, eight lost replies and
restarts, three full replays, and revision 1,026. Native and MSVC cross-target
Clippy pass; the new native Windows CI run is pending.

The hosted Windows job for `7d45f48` passed the receipt-capacity test and all
20 exact-DLL traces. The engine and fuzz workflows are green at that commit.

The Rust quota controller and its shared maintenance oracle now run the Docker
ENOSPC drill. All three partial-copy errors used errno 28 and passed reopen and
retry with 64 retained roots. The native EDQUOT job now uses the Rust entry point.
Legacy quota files remain until the native job verifies the replacement.
[Quota migration evidence](xtask-conversion.md#linux-quota-controller-and-maintenance-enospc)

Hosted Linux verified both Rust quota profiles at `d4c8fff`. The append test
observed errno 122 and reopened revisions 3 and 4. Near-cap backup, restore and
compaction each reported errno 122 after 8,392,704 bytes of partial output.
Both profiles verified cleanup. All 16 legacy quota controls map to Linux Rust
tests. The migration now accounts for 90/179 frozen controls, with 31 Python
files remaining. [Native quota evidence](xtask-conversion.md#linux-quota-controller-and-maintenance-enospc)

The Rust resource controller passed the standalone envelope on macOS/APFS and
Docker Linux/overlayfs, the combined workload on macOS/APFS, and the full
interruption matrix on Docker Linux/ext4. It verified 52 maintenance kills and
108 repeated recovery cases, with the source image unchanged. The growth and
cache probes reproduced the macOS clean state and byte observations. Eleven
Python files were retired, bringing the conversion to 112/179 frozen controls
and 20 maintained Python files. [Resource evidence](xtask-conversion.md#resource-workloads-and-instrumentation)

The Rust SQLite comparator passed three alternating rounds on macOS and Docker
Linux against historical engine source `2490631`. Both engines matched the
original digest across current and retained states through reopen and
maintenance. Five legacy controls map to Rust tests, bringing the conversion
to 117/179 and 18 maintained Python files.
[SQLite verification](xtask-conversion.md#sqlite-retained-history-comparison)

The Rust storage and wire-v1 controller passed both directions on macOS and
Docker Linux against `738e924`, using the original endpoint lockfiles. Storage
read and unsupported-rejection images stayed byte-identical. Snapshot durable
revision 4 and retained revision 2 survived reopen and duplicate shipment
while the primary advanced to 5. The migration now accounts for 124/179
controls and 17 Python files.
[Compatibility verification](xtask-conversion.md#frozen-native-and-wire-compatibility)

`cargo xtask soak` passed a 30-second macOS/APFS run and the 12-round cap on
Docker Linux/ext4 against frozen engine `7a9e661`. The controller validated
every round, verified process cleanup and bounded RSS telemetry to the peak
and sample count. The conversion now covers 132/179 controls and 15 Python
files. [Native lifecycle verification](xtask-conversion.md#native-lifecycle-soak)

The Rust follower backup and reseed controller passed on macOS/APFS and Docker
Linux/ext4 at `65d142e`. The new enrollment reopened at durable revision 4 with
retained revision 2. Wrong-stream upload preserved the old durable receipt
and all original files. Both runs verified process retirement and removed
their private keys. Fourteen Python files remain.
[Reseed verification](xtask-conversion.md#authenticated-follower-backup-and-reseed)

The Rust mixed-version snapshot CLI qualification passed on macOS and Docker
Linux/ext4 with durable receipts at 3, 4 and 5. Wrong-pin failure left the
follower at 3 while the local capture was 4. Retry advanced follower
durability and repeat polls at 5 preserved exact files. The conversion now
covers 139/179 controls and 12 maintained Python files.
[Snapshot upgrade verification](xtask-conversion.md#mixed-version-authenticated-snapshot-cli)

The Rust replica progress observer passed native TLS response-loss and restart
checks on macOS and Docker Linux/ext4. Fresh polling verified durable revision
3 after a lost response. The final source and follower reopened at 4 with
retained revision 2. The conversion now covers 153/179 legacy controls and
10 maintained Python files.
[Progress observer verification](xtask-conversion.md#replica-progress-observations)

The Rust accelerated month runner passed the full 30-day schedule with a
native macOS client and Docker Linux/ext4 server using the 16 MiB core budget
profile. It also passed the prebuilt local pair with an explicit server
argv. Eight interrupted receipts resumed, every repeat sync stayed at zero
commits, and each restored database verified 30 retained roots. Eight Python
files remain. [Month workload verification](xtask-conversion.md#accelerated-month-workload)

The Rust StudyProgress upgrade controller passed the historical old/new
release pair on macOS and Docker Linux/ext4. Mixed-version ALPN rejection
preserved local commits with zero replies. Coordinated upgrade converged
with both retained roots, and repeat sync and restart kept both revisions.
The migration covers 160/179 controls with six Python files remaining.
[Study CLI upgrade verification](xtask-conversion.md#coordinated-studyprogress-cli-upgrade)

The Rust StudyProgress peer soak verified all 30 rounds and six paired SIGKILL
restarts between macOS/APFS and Docker Linux/ext4, using frozen source
`ed8f9f9`. Both peers finished at revision 63 and repeat covering exchanges
stayed stable. The migration covers 162/179 controls with four remaining
Python files. [StudyProgress peer soak verification](study-peer-soak.md#rust-controller-result)

The final Rust helper controller passed debug and release on macOS/APFS
(184 tests per profile) and Docker Linux/ext4 (201 per profile), with one
explicit separately qualified TLS skip per profile. All 179 frozen controls
now map to Rust tests and zero maintained Python files remain. CI uses the
same Rust helper command. Crates stay private.
[Final xtask retirement](xtask-conversion.md#final-helper-retirement)

Hosted macOS ThreadSanitizer runs `43700106330` and `43700707072` failed
the replica-progress failed-ship fixture. Both retained reports show that
the fixture returned `Timeout` rather than the expected `CliFailure` with
the former three-second aggregate test budget. The test now gives its real
subprocesses 90 seconds. The separate timeout control still uses 1.5 seconds,
and both tests pass on macOS debug, release and with instrumented std on
nightly `2026-11-12`. This evidence concerns the fixture. Hosted rerun
results are tracked separately.

The typed release-binary rollout passed all 12 phases on macOS/APFS and
Docker Linux/ext4. A precommit interruption left the exact file unchanged.
The historical binary preserved fields and indexes on a compatible edit,
and the pre-upgrade backup restored an independently writable v1 database
with its retained checkpoint. The existing storage and snapshot compatibility
profiles also passed on macOS after extending the controller.
[Typed rollout evidence](typed-schema-rollout.md)

## Rust-only exact-package qualification

Clean `bc5608447a1830587b7fcd32902e1fc075e2425c` passed the private package
profiles on macOS/APFS and Docker Linux/ext4. Both manifests bind the same 666
source inputs and report verified relocation and complete attribution. macOS
ran exact C/C++/Odin consumers plus the copied Rust source consumer and its
isolated Linux rebuild. The Docker Linux package ran exact C/C++ consumers and
an ext4 copied-source Rust build.

| Proof | SHA-256 |
|---|---|
| macOS package manifest | `f8d47cecec4ade56027a99a2fc8f352154391a4e8d2c2addfb78ff4f714aa5f5` |
| Linux package manifest | `0bb683cc87929403a7fd9ab4217d75aa651f8ef4570f5bc2e75aaf6597de798f` |
| macOS 134-cell trace | `3426415a60fb88e48f4cfd9f379ec6969a50cc0ab05c070943c1d6d4cc503584` |
| Linux 134-cell trace | `7362e6c77d43f5285a2c9c398ff36322c090706b001d51a8b52fdc4cee07ac09` |
| Linux delivered C fault sweep | `98e88fd90fe003f75b7d0730caf9d39a89dc881ff65f1e5da349722ac67aa57c` |

All five optimized Rust/C/Odin transcripts matched the independent 134-cell
oracle. The exact Linux shared library also passed all six delivered C fault
modes. Fresh recovery selected revisions 4/3/4/4/3/3 in mode order and every
case finished at revision 4 with retained revision 2. Kernel ENOSPC was errno
28 on private tmpfs. The disabled injector failed the required C status
assertion. An independent rehash verified both manifests' files, both trace
inventories and the delivered fault worker inventory. All owned Docker
containers and volumes were confirmed absent. These are packaging and
exact-artifact checks on the named profiles.

A separate clean macOS run of `cargo xtask check --skip-odin` at `6cc8ef3`
passed the full debug/release checks and native Rust/C consumers. Its log
SHA-256 is `15132f5bfece2b4bf6d6287ac0dfe80dd95186b8ea39a58b7dff10e33e198c07`.
The whole xtask test suite at that checkpoint also passed locally under
ThreadSanitizer with instrumented std, with 180 passes and one registered
fixture skip. Its log SHA-256 is
`2ccee77ba8c6f2422329ab045c21cc3d30d1f56e3d891921be5cd7b40dd9e4e9`.

The [native macOS delivered C profile](delivered-c-faults.md#native-macos-delivered-c-faults)
passes five cases against the exact `bc56084` dylib, including append, root and
witness full-sync injection and root-write rejection. Fresh process recovery
selects 4/3/4/4/3, then reaches 4 with retained revision 2 in all five cases.
The disabled injector fails the required assertion. The shared Linux runner
was requalified on the exact `bc56084` shared library across all six cases.

The [portable follower backup and reseed regression](follower-backup-restore.md#portable-native-regression)
passed debug and release on macOS/APFS and Docker Linux/ext4. The old follower
rejects the new stream without changing any files. The fresh follower reopens
the restored revision 4 and retained revision 2 with its own durable receipt.
The Windows-native result is pending.

The hosted `6cc8ef3` matrix passed all ten engine jobs and bounded fuzzing.
The macOS ThreadSanitizer artifact confirms that the failed-ship fixture and
its separate timeout control passed with instrumented std. The Rust-only
helper artifacts verify debug/release controls on both hosted POSIX platforms.
The workflow checked out PR merge `8cdb0df862c4a641d5746a1e0989feda6cdf983a`.
[Engine checks](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43703164759)
[Bounded fuzzing](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43703164764)

Downloaded Windows proof from the same run contains all 20 successful
C/C++ case dispositions and the 134-cell trace. Independent review verified
all listed package and trace file hashes and byte-matched the packaged DLL
to the relocated and both consumer DLL copies. The DLL SHA-256 is
`3a84afa47ce45bf772161f0f8e5f9ccfb09762efb62b1e61dd232b9eb8739f62`.
The Windows trace proof SHA-256 is
`5f2fc9543bac32f316917262e5de216c843531a64308d212d9424e83dd899608`.

The [supervision ownership regression](xtask-supervision.md#descendant-discovery-after-reaping)
covers new detached descendants below known parents and ignores reuse of a
reaped worker's PID. The macOS debug/release suites, Docker Linux helper
registry and native macOS delivered fault cases passed. The original
intermittent hosted Linux cleanup report remains separately recorded.

The [snapshot credential cutover regression](snapshot-credential-cutover.md#portable-digest-verification)
now computes the export digest portably and is enabled in the Windows-native
suite. Debug/release passed on macOS/APFS and Docker Linux/ext4. The Windows
result is pending.

The [CRDT record law helper](crdt-operations.md#check-an-applications-record-laws)
checks all pairs and triples of seven reachable StudyProgress states, including
two offline writers, an observed removal, two policy resolutions and a later
edit. A left-biased record fails the helper. All 13 integration tests passed
in debug and release on macOS and Docker Linux.

Native Windows passed the follower backup and reseed case in debug and
release at `5be73db`. The separate Windows cutover test initially invoked the
Unix-only `keygen` command. Its fixture now shares credential provisioning with
the Windows CLI tests. The revised CLI and cutover suites pass on macOS and
Docker Linux, with hosted Windows rerunning.
[Windows follower job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43705255065/job/132784834134)

The [repeated near-cap workload](compound-resource-envelope.md#repeated-near-cap-maintenance)
passed 24 cycles per platform on macOS and Docker Linux. All 768 contended
commits per run, repeated image-limit and backup-budget failures, verified
backup/restore/compaction and 64 retained roots passed through revision 980.
Both runs held the same 57,794,708-byte recorded core peak. Sampled worker RSS
reached 1,548,256 KiB on macOS and 192,216 KiB on Linux. The higher macOS RSS
remains a resource investigation.

Windows also passed the portable stopped-endpoint credential cutover and
follower backup/reseed in both profiles at PR head `af91f05`. All 20 exact
DLL cases passed against merge `fc31b9a3e0eafbf3c4387f37f1fde8b6cb432d4f`.
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43706610324/job/132788972396)

The repeated near-cap workload passed a second time on both platforms.
The macOS sampled RSS reached 3.51 GiB with the same recorded core peak and
final physical file size. The repeat Linux RSS stayed about 187 MiB.

The [macOS allocator diagnostic](compound-resource-envelope.md#macos-allocator-diagnosis-and-bounded-verification-buffers)
found 706.9 MiB resident in empty large-allocation regions in a 997.0 MiB
VM-map snapshot. The opened allocator's in-use count stayed near 49 MB at
phase boundaries. The workload now compares exact failed-backup bytes with
two 64 KiB buffers. The full profile passed again on both platforms, and
the macOS RSS peak persisted at 3,499,504 KiB.

The [Linux process inventory](xtask-supervision.md#linux-process-inventory-under-churn)
now reads bounded procfs rows and handles a process reaped during inspection.
Mac xtask passed 183 tests per profile. Docker Linux passed 206 tests per
profile, rehashed all 70 helper artifacts, and passed 120 focused repetitions
while two shell loops launched short-lived children. The earlier Ubuntu log
records a parse overflow without preserving the offending process row.
[Hosted Ubuntu helper at `d43f544`](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43709335373/job/132797279967)

The [offline copy lifetime change](compound-resource-envelope.md#offline-copy-graph-lifetime)
releases the source graph and destination caches before independently
verifying the destination with both leases held. Frozen release binaries
produce the same bytes from a shared near-cap input. All 24 repeated cycles
passed on macOS and Docker Linux, retaining all 64 roots through revision
980. Sampled process RSS was 1,413,968 KiB and 127,044 KiB respectively.
The measured core peak of the opened database was unchanged.

## Private qualification checkpoint `025b87f`

Clean macOS arm64 and Docker Linux arm64 packages each bind the same 668
tracked source files. Both passed notice, unwind, ABI, C/C++ relocation and
copied-source Rust consumer checks. macOS also passed Odin relocation and an
isolated Linux build of its copied source. Independent hash checks covered
every file listed by both package manifests. The optimized artifact oracle
matched all 134 cells for Rust/C/Odin on macOS and Rust/C on Linux. All five
macOS and six Linux delivered C fault cases passed, preserving retained
revision 2 and reopening the selected revisions. Each retry reopened at 4.

The bounded [30-day Linux server/macOS client workload](monthly-workload.md)
passed all 432 phase records, eight interruptions and 120 sync sessions.
Both restored databases verified all 30 retained roots. The separate
[authenticated CLI soak](study-peer-soak.md) passed 30 offline-edit/sync
rounds and six paired process interruptions. Both peers reached revision 62
with 1,395 practice minutes, weekly goal 230, session limit 10 and lessons
2–61. Repeat covering syncs stabilized without another local commit.

The authenticated [replica observer](replica-progress.md) and all 23 release
`weekend-replicate` tests passed on both platforms. When the response was
dropped, the observer reported the follower as unconfirmed. After restart
a fresh poll verified durable revision 3 and the exact export digest. A later
shipment advanced the follower to 4. Local capture and the follower's durable
receipt remain separate.

The [near-cap resilience profile](near-cap-interruption-recovery.md) passed
52 native maintenance interruptions and 108 composed recovery cases per
platform. The current and all 64 retained roots matched the oracle through
fresh-process retries. Both workers retired cleanly and each proof's 50 listed
artifacts rehashed successfully.

Native Windows x86_64 on fixed NTFS checked PR merge
`4de0880b575fe2941e2d38cf83ab09adf8cd02ec`.
Its Git tree matches `025b87f`. The native job passed StudyProgress credential
cutover and coordinator capacity in debug and release. All 20 C/C++ cases
passed the delivered-artifact trace. Independent verification rehashed all
52 package and 170 trace entries and byte-matched the package DLL to the
relocated, C and C++ copies. DLL SHA-256
`fcc830646ce57fbd4b7b51f734db04edf77f76fbf9741ae178e51691fead60ce`.
[Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43710316144/job/132800236604)

Local macOS AddressSanitizer and ThreadSanitizer rebuilt the standard library
with nightly `2026-11-12`. Each passed 621 workspace cases with three registered
ignores. The passing cases included xtask supervision and the StudyProgress
cutover. Documentation edits marked the AddressSanitizer checkout dirty.
Independent verification matched all 285 recorded source hashes for both runs
to `025b87f`.
The hosted Ubuntu native, quota, helper and C AddressSanitizer jobs passed,
as did the bounded fuzz workflow.
[Engine job matrix](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43710316144)
[Bounded fuzzing](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43710316153)

| Checkpoint proof | SHA-256 |
|---|---|
| macOS package manifest | `f2bfa18b511dd626d0011347fd2ff4b96c0dfcfab53f61e352331315d579b0a6` |
| Linux package manifest | `41b7dae70b7dd1cd6f5e427c04bdbec305659a0c391f565920dc5d70ed69d4ec` |
| macOS trace evidence | `24e84b62e5f1569976a5218d65c20eb5d584bc526eb7061c340a12cc8dbdf8ad` |
| Linux trace evidence | `0deaaa4e9082d3ebccf532db791cb038635a2c02cc718ace68c5b4bf7e76e90f` |
| macOS delivered fault evidence | `cfbd8f82c6ffee0ac31aa610462bc6b3f920604ccb538b8a91daabd114b09e0e` |
| Linux delivered fault evidence | `96622427bebabe6c55fe45959de5e356edddaa48a4d3a1ca2de0f34ca3b67793` |
| macOS near-cap resilience worker | `2d039eeb20d661231fe4a81354f2139323ac3fd8581b1562170988212b6142ba` |
| Linux near-cap resilience worker | `59dacf65cb6839d82794117db775cfaa7e5faae2919095e6cf102e4864aa7111` |
| Windows package manifest | `988f3e54fc83f2aca3ad1f775b0cdb16b21e996a47f5c9237945f5b950521585` |
| Windows trace evidence | `9392437074ab2be196fb7ed8caee428abb1a5bea41a48da67dd50f4e2e7fb347` |
| macOS workspace AddressSanitizer evidence | `f40a4cb3ba9c0dcf276bc89e69413a408285fbfa0d5d86d2c1c90dc13497ef47` |
| macOS workspace ThreadSanitizer evidence | `0e49f8d0f8cc49fde66aa11860a42eb0e33fc8153e248654a61b86dfe95922b5` |
| Accelerated month evidence | `44381a3ce8bb9bc4fdacee43dd40a11754dd03d16996175aa27913ad82425271` |
| Authenticated StudyProgress soak evidence | `6dabd65147e87dd86f6d3810eee6e1514abc3c04f569291ed0e895787001b843` |
| Native macOS replica observer log | `296663fbb22f9e498269fd636263db3d3efa3db28539325890b0aa1191f50f99` |
| Native Linux replica observer and app log | `3d468f43dd64dea3957c8da8f6d86ca3fb4f1529c3b1378d3b5b00e335475a60` |

## Torn metadata prefix qualification

[PUB-V2](../DESIGN.md#pub-v2-persisted-prefixes-in-reused-root-and-witness-regions)
adds 280 publication cases covering every distinct persisted prefix of each
chosen reused root or witness region, and expands the compound recovery test
to 156 cases. Both slot parities, exact durable bytes, read-only witness
status, the reopened revision and both retained snapshots are checked.
The existing near-cap qualification retains its separate 108 recovery cases
per platform with 64 retained roots.

Run `cargo test --offline --locked -p weekend-db --all-features --test durability -- --nocapture --test-threads=4`
in debug and with `--release` for the optimized profile. Each passed 16 tests
on macOS arm64 and Docker Linux arm64. Linux used immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
macOS ASan and TSan rebuilt std and passed all 16 tests. Each of two
single-change mutations failed the new test on its intended assertion.

| Evidence on base `af29db7` | SHA-256 |
|---|---|
| Durability test source | `f8a3aabef596062eb8c9a8f9ce93fd6325409c2b2d1771c62eaf82407913e109` |
| macOS debug | `cde0bc95ae23a76d025f5ba19b904f75b491ab53e1151347c39ddab31aa17e56` |
| macOS release | `d081ad36d6b866f37c71aeac0e71e03a8df4048ee91a6433573df41e63d5bae5` |
| Linux debug, release and Clippy | `947ed81f427514f7df94f60335a098a8c8d4cb95dc5626c9f4604791ff32161d` |
| macOS ASan | `46da9b939bf7ec403023d3c37c30c5b6c03743ed08263f2e7697e8963fb6cecb` |
| macOS TSan | `4618f511d71a3f3fb48c80bd581f29d4edbc297575e24d1ca13701d35322af80` |
| Witness-report mutation | `2d2d52d3b1f83d4056fdc516e1dce3e3f8ea4a0d3bf5b68a5d31c045d388024d` |
| Checksum-rejection mutation | `d0bfc1439315a057252f2dd66120a5eac84ed2606a46ba056cfdc4a2dcc26586` |

## Package feature identity qualification

At `025b87f`, a clean `--features crdt` build with
`RUSTFLAGS='--cfg feature="fault-injection"'` produced a manifest marked
verified and candidate-eligible. An independent C process linked against that
packaged dylib returned `6` from `wdb_features`, while the Cargo profile was
CRDT only. The original manifest SHA-256 is
`d46a498b0f7c45b5edb5653a00bb948a231fe4d7364ad8e856158b2c0990f8e3`.

`2e7ca9a` now checks the mask reported by the delivered and relocated library
against the features Cargo selected for its engine library artifact. The
packager passes the expectation to `abi-smoke` alongside the source ID.
On macOS and Docker Linux, the injected feature build now rejects with
`actual 6, expected 2` and retains an ineligible, unverified manifest.
Requesting both features from a CRDT-only artifact also rejects with
`actual 2, expected 3`. [Package contract](release-contract.md)

Both platforms built from a clean `2e7ca9a` checkout with Rust 1.99.1 and
locked cached dependencies. Each package passed the C/C++ consumers,
relocation and exact mask checks below. The CRDT-only check used the
qualified Cargo spelling `weekend-db/crdt`. Independent verification
matched every listed file hash and confirmed Docker resource retirement.
Strict macOS and Windows MSVC cross-target Clippy passed. The fresh native
Windows combined package subsequently verified Cargo and runtime mask `3`.
Its published CI artifact was rehashed and byte-matched to the DLL used by
all 20 C/C++ cases. The trace checked 134 cells. The checks used merge
`a57dae6d287bb142dacfff8183a699359ea1df81` at PR head `5142906`.
[Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43713372642/job/132809582790)

| Build | Runtime mask | macOS manifest SHA-256 | Linux manifest SHA-256 |
|---|---:|---|---|
| Default | 0 | `13b8d2119b6c5b38b3206ddf031a99dbb9b2c1ab6c7a367842cb578e5035b2eb` | `eddfe2fa59c667f35b1df609d0a912c61b21ad70516394ee0746d51afcb93042` |
| Replication | 1 | `6647b4c294066b85aebb1b60c70105a768ad0d90de889855f7c855ef2eb7534f` | `077ba1cc0b73e2816113fc2592168c4d6921e3da5a7c00a5739c6b26a869e50e` |
| CRDT | 2 | `a7e938497f8304e37921ed3a31e1ee15d5a217c8c0dab47e8f1d77f7897a45a2` | `48556087f5c1e2c7dfb58c3671a507abeb1405dbbb4a8938caf07229819e51f2` |
| CRDT and replication | 3 | `747f0ba36eeac6855f322b094f406adb71b8fdf0f523b209cebeae2251e4004e` | `312669d0fd36a1f73ef464729e9ddbdc705cc93570024f74ebce75619c74c7e7` |
| Explicit fault-injection profile | 4 | `9f0db490bd11d3de16d0243b254e495a1d262d1769c4714e8eab2e1f27e40b4a` | `1041d74e09e8bb38b670959a9ca20132cede4b4a89290d8d987eb6161a85f368` |
| Rejected extra feature | 6 | `fb7d9d73782f36652f239b334f05b282e7c9379b9221aa55a98a50ca4154700c` | `9e4b4c164bb870ac7b9344331cf924df5a8e2a316a9df630ebb5dfd8281bfa1c` |

Windows manifest `1c42306aea5c7e81f4d7993b07d9c71f560ee1cf7be7d29d790be0dae5a10103`,
DLL `bec84ae64df14dd925ccff68dab923210513abbf3445bea6622a783ca3dd84b6`,
and trace evidence `26ed1b182f68c4a334f560944bcf3f0872b729657deb89131aa39ba2d6d992bc`.

## Sequenced coordinator receipt retirement

The [opt-in local profile](../apps/write-coordinator/README.md#sequenced-requests-for-long-running-writers)
accepts contiguous sequences in 1,024 explicitly assigned slots. A successful
write replaces the latest receipt atomically with its effect. A client must
journal the latest acknowledgment before advancing its slot. Older sequence
numbers report the retirement watermark. The existing permanent-ID profile
preserves original receipts and uses its own startup command.

The configured native process test covers a lost reply, server kill and
restart, two competing clients, an invalid replay, and resuming on a verified
restore. The added fault matrix reopens the old value and receipt or their
successors together for every I/O error in the retirement commit. The bounded
protocol fuzzer compares a four-slot model through 32 operations, including
reopens, and completed 2,000 ASan runs at seed 7. Four sequenced checks passed
under both macOS ASan and TSan with rebuilt std at nightly `2026-11-12`.
The app suite passed 15 tests in each debug and release profile on macOS and
Linux. The extended native maintenance workload ran separately. Two disposable
mutations failed as expected when they returned an acknowledgment for a retired
sequence or published the value without its receipt.

A native release workload passed 1,152 commits and nine verified copies on
macOS/APFS and Docker Linux/ext4. Both ended at revision 1,155 with two live
receipt rows. Each compact copy occupied 45,056 bytes. The first batch grew
to 2,158,592 bytes and each later batch to 2,142,208 bytes. The default native
Windows job for `bbf5b7c` passed the sequenced restart and restore test in
debug and release. The extended nine-copy workload remains scoped to macOS
and Linux. [Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43715144047/job/132814939237)

| Evidence | SHA-256 |
|---|---|
| Sequenced implementation | `b3d6e9790dc99f7e57a5c34a9203b507e9f24126fc0053456bbd4ed4c719a82f` |
| Native process test source | `ab507a3aa23b96dbfe36802a2f905eeaa98e66767c70542c6e922e201022b5f0` |
| Fuzz source | `2311ff37db87b250e977a20961e6ff1a6eee1bfe4a0b3234165def6af44dbc2f` |
| macOS default debug tests | `5448da85a9b8e88aaa382de9f7104e8a12c95097f67336de391f21c76064dd5c` |
| macOS default release tests | `83eb7df9b87e18df069f473d770261e84304d57e5bbcc1903b50388ae838996b` |
| Linux default tests and Clippy | `0dcf63e23036dcd0689f4cbafd18fbe4c209d2b8b938cfc39ffa3bbef5cbf035` |
| macOS native maintenance log | `ca7ea0ddd7c76ebee6db3d30ba9563d30d6b66bd118fd01326702a1f6c7d8b77` |
| Linux native maintenance log | `b2203c0820cf475b3141612d32d756f39c59880be27efa43cd539f81f205ef38` |
| Bounded native ASan fuzz log | `db284535f2eee4e4091c30daad448fed8670f8db45433ce356e98fffee25e5fe` |
| Retired receipt acknowledgment mutation | `5e8f2376cded9e2e5ccf94dd1ddfd717383d5ee63de895fac603028cf53eae00` |
| Missing receipt mutation | `9bb1d28b00f9444b7e81b18677b0aa270d1f75f7672fdd587e6165a51ee852c6` |
| Native Windows default profile job log | `357672e57193f728dcf15c13c041fd2f7dce0001108d0d0eff5f32f721b373f4` |

## Durable coordinator client journal

The [journal CLI](../apps/write-coordinator/README.md#durable-client-journal)
stages the exact intent before sending and saves the reply before returning a
successful exit or advancing the slot. The coordinator commits the effect
and its receipt together. Its identity identifies the database lineage, and
its last receipt guards fresh writes against a fork at the same revision.
The previous receipt and pending intent live in one fixed-size journal record.
The journal uses the native file lease and ordinary database publication path.

| Scenario | Result after reopen |
|---|---|
| I/O error during any traced operation staging the intent | Reopens idle or pending according to persistence of the root. Only the read-only preflight can have reached the coordinator. |
| I/O error saving the local acknowledgment after server revision 4 | Reopens pending or with receipt 4. A pending retry gets the exact original receipt, while an already saved reply is read locally. Both keep the server at revision 4. |
| Client process killed after the coordinator commits and before reply delivery, then coordinator restarted | Client reopens pending and recovers revision 4 twice. The server effect stays at revision 4. |
| Other lineage or a same-lineage copy with a divergent prior receipt | Client retains its pending request. An isolated incompatible endpoint retains its local data. Retrying on the correct history allows the next write at revision 5. |
| Confirmed newer precondition conflict after the predecessor check | Client clears pending and can reuse that sequence with a revised precondition. Malformed, retired and uncertain responses preserve pending. |

The simulator uses both `DropAll` and `PersistAll` at every traced operation
in each client commit. The native process oracle forwards the exact CLI
request and withholds its reply after observing the server's durable response.
The augmented protocol fuzz model covers correct, foreign and incompatible
guards and reopens periodically. It passed 2,000 ASan runs at seed 9.

macOS arm64/APFS and Docker Linux arm64/ext4 each passed 20 coordinator tests
in debug and release, with the extended workload excluded. The storage
decision suite passed nine tests on both, including lineage preserved through
a verified compact copy. Five journal checks passed under each of macOS ASan
and TSan with rebuilt std at `nightly-2026-11-12`. Strict Clippy passed on both
host profiles and for cross-target `x86_64-pc-windows-msvc`. The default
native jobs for `c6d6cf7` passed on hosted Ubuntu and Windows x64. Both job
logs show the client kill, lost reply and divergent-history test and the local
journal fault checks passing in debug and release. Deployment-specific
budgets, authoritative routing after a stale restore, and operator acceptance
of slot assignment and reconciliation remain open.
[Ubuntu job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43716518091/job/132819139859),
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43716518091/job/132819139976)

```sh
cargo test --offline --locked -p write-coordinator
cargo test --offline --locked --release -p write-coordinator
cargo test --offline --locked -p weekend-db --all-features --test storage_decisions
cargo test --offline --locked --manifest-path fuzz/Cargo.toml --lib replay_coordinator
```

| Evidence | SHA-256 |
|---|---|
| Client journal source | `d2999afd8a649577d9f5bfa4a2c656c7bf2e52b0a08b44cd1d5569847855117d` |
| Server protocol source | `ec13c699bf69bd4d0a67c3a788b05523e94fd45d65626e7c9df0db93cbbcbd73` |
| Native process test source | `2a6b76a38dd208d37a96558f48551e97b5ce20751421d4768e507f3eabfdbd8e` |
| Fuzz source | `c8ff43db3848b629ffe0fd02e2d09523c4894ce74e6e0962adadbe2951ad6480` |
| macOS debug log | `fff67b43f1ac97f1ea60c234e8fe9ce5e3d008eb2bf1eb9a02336b801809cf04` |
| macOS release log | `1f860d3af8103efe8da702884d14001abc258184233d9d3d642b0dbe87aee8c5` |
| macOS storage decisions log | `42458dfdfa1f4b5f7b23bf6de006b818e07300e71c60b018494a285ae098fc95` |
| macOS ASan journal log | `64e6d6ae6612afc5ebe1c0751ef108c8cbf2a73be7eb810d1b6da6b2bf491137` |
| macOS TSan journal log | `301c46035b5fd5cb99abb0d6a675c6f9468839d4cffba2e715af9cc859288d9e` |
| Docker Linux debug, release, storage decisions and Clippy log | `08ce1bd7970241233d914ca220d446447e3fd6592a572e7b70edf6ef2296fdd4` |
| Docker Linux ext4 mount record | `40ab1c448ee0a848b68e89ad6f74c25d07f36ec6932a602da9050e95743f05fb` |
| Windows cross-target Clippy log | `0c3cea29467528063362871133c41a5585265376542b7800b48cd2588f07d0ab` |
| Bounded ASan fuzz log | `29af6942d7a42404f8f1fe1cb1af90b734bb126e4d69828eb9a88e95dcde3d07` |
| Hosted Ubuntu journal job log | `26998d0d1996a7a02b55c3db6c065e8b2703050f965d3393a4256d9041f5ea05` |
| Hosted Windows journal job log | `c3f89ad609d12d3d322947408f79f63fedae4020075710597d18dbffda5d6c41` |

The macOS all-feature workspace run at `c6d6cf7` passed 633 tests, including
the explicit separation of local commit and follower durability over TCP.
Four extended checks were ignored by default. The workspace log SHA-256 is
`5bbac7696aab370a4c0c271da91e55e8315580e31bf0baa008d0151e79a8cd25`.

## Coordinator and journal lifecycle

The release workload alternates 1,152 writes between two actual journal
clients against the authenticated local coordinator. Every 128 writes it
stops the server, verifies and backs up the server file and both journals,
restores all three to fresh paths and compares each backup and restore byte
for byte. It preserves the original paths through verification. The oracle
reopens every file, checks the selected revision and live row counts, and
compares the server identity, counter, both receipts and both journals with
the workload's independent expected values. Repeating each journal's `resume`
preserves the server revision during all nine cutovers. Continued fresh writes
advance through the verified copies.

Both hosts finished at server revision 1,155 with counter 1,152 and two
sequence-576 receipts. Each journal finished at its local revision 1,154 with
one journal row and no pending intent. Each compact server image was 45,056
bytes and each compact journal was 32,768 bytes. The first batch grew to
2,158,592 server bytes and 1,622,016 per journal. Each later batch grew to
2,142,208 server bytes and 1,605,632 per journal.

| Rust 1.99.1 release runs | Write p50 / p95 / max | Inspect, copy, restore and verify all files | Stop to restored service and receipt validation |
|---|---|---|---|
| macOS 27.0.1 arm64, APFS | 35.91 / 40.29 / 46.32 ms | 55.67–96.75 ms | 102.93–166.34 ms |
| Docker Linux arm64, private ext4 volume | 5.89 / 9.87 / 16.46 ms | 6.93–15.19 ms | 13.38–23.21 ms |

Writes include CLI process startup, guarded reads, journal staging, server
publication and local receipt persistence. The table records finite samples
under the hosts' observed load and storage paths. Deployment latency and
restore objectives remain open. The workload uses coordinated, current backups.
The [operator guide](../apps/write-coordinator/README.md#back-up-and-restore-a-coordinator-with-active-journals)
covers stale-restore reconciliation separately.

```sh
cargo test --offline --locked --release -p write-coordinator --test writers \
  native_journal_workload_resumes_after_verified_restore -- --ignored --exact --nocapture
```

Docker used the offline `linux/arm64` image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`
and ran both the toolchain build and native files on the private ext4 volume.

| Evidence | SHA-256 |
|---|---|
| Native workload source | `03f2083e19fea6cbddb913caab55fdc812a4d99dec3fca1dbbe17aa6ff2ccd4e` |
| macOS lifecycle log | `a1ebe07d706297e484798e766df12f74e529ab662964aa2c8865f540e91c7fb5` |
| Linux lifecycle and Clippy log | `143030575b646875c72a9f479dc177bfaae1d051755b10f2ff70a667600ae11a` |

The hosted Windows job for PR head `c6d6cf7` also ran all 20 exact-DLL
trace cases on x64 fixed NTFS. Its package identifies the actual merge
checkout `a2b25dcc4f8bbcd631253d566a091785b7d9600a` and feature mask 3.
The package and trace artifacts have matching manifest and DLL bytes, and
independent rehashing passed for all 52 package and 170 trace entries.
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43716518091/job/132819139976)

| Hosted Windows evidence | SHA-256 |
|---|---|
| Package manifest | `5d33f2af5d66309ef5b76f6f34d9e3a0be46ebdac6a2e52b9504c078b957c512` |
| Exact DLL | `7cc2cbf30f29e0380f6747834322469a7064e9b0669f2fc7ef5af31f4a2f0bb6` |
| Trace | `b0a5702caa1e0b07ef485e80f1a401792d17860fbc0f68e3a35ef002318bd81e` |

## Private artifact checkpoint f70e405

Clean source `f70e405174e419ca802b7fc8853d3c1db04e7b1b` produced native
macOS and Docker Linux packages and one relocatable Rust source package.
All three passed package verification with combined CRDT and replication
feature mask 3 from both the Cargo artifact and the loaded library. The
Rust source consumer passed on macOS and in Docker without the checkout.
Independent manifest rehashing checked all 849 listed files across the
three packages.

The [134-cell trace](artifact-trace-validation-modes.md) reproduced the full
oracle transcript on macOS arm64 with Rust, C and Odin, and in Docker Linux
arm64 with Rust and C. All five stdout files matched the expected transcript
byte for byte. The Linux trace used a private ext4 volume for the build and
runtime, verified the input seed and removed its owned resources. The trace
hash and expected transcript hash are unchanged from the original oracle.

The [delivered C failure checks](delivered-c-faults.md) used byte-identical
copies of the same two libraries as the traces. In mode order, macOS APFS
reopened revisions `4/3/4/4/3`. Docker Linux reopened `4/3/4/4/3/3`,
including real kernel ENOSPC on a private tmpfs in the last case. A fresh
process finished at revision 4 with retained revision 2 in every case. Both
disabled-injector controls failed as expected. Linux removed and checked
absence of its owned Docker container and volume.

Use `cargo xtask package --features crdt,replication` on each native host
with a fresh output path. Add `--odin` to the macOS package and use
`--rust-source --rust-source-isolated` to qualify the source package.
Pass those packages to `cargo xtask artifact-trace` and
`cargo xtask faults delivered`. The Linux runs used image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
These are private source-bound results for the named consumers and fault
mechanisms. Final distribution approval and physical device qualification
remain open.

| Evidence in `target/qualification-f70e405` | SHA-256 |
|---|---|
| macOS Odin-enabled package manifest | `d7a7466c9e18d7ca6b45904db0da41e176641a7941747ad6509b3b4a5420360f` |
| macOS exact dylib | `a252a792fd0f95444c50b2f7de6fae418fd6cebcbd4e0cc86dfeebfdcf318ab4` |
| Linux package manifest | `1f2a0b44b0d1ffdecc56e09e2cecd83ab9fe3dd255b1ad824dd6dda3fd71837e` |
| Linux exact shared library | `629863a2bdf417633873ea5db0e22d236b99a4d3047e721e25cf55f0cc667598` |
| Relocatable Rust source package manifest | `e2f7c06c06a5bede8c98afeaa48215ef0c5dee3ffd76fe723e1b16ef22902993` |
| macOS trace | `4c693737b8aae214f85643f370daf5e80807b5cdcaf3f5afaadf60edf10e2259` |
| Linux trace controller | `6c2d59fbff288ba64fb0a03d48c9b6b207f2bda0d876df5184100d7a7c231c5b` |
| Linux trace worker | `0d056db2c6bb2b913ac739e3af36628828933a1108fe5e080441446510c6f74b` |
| macOS delivered faults | `caafc20b75c52936c76546610f946b202303f2d8704a027cd4cbcc5e37496061` |
| Linux delivered faults controller | `d180f762df945f3e8ee6b3291705f9db96d9b093cb131fe40eea9ce9a67027d0` |
| Linux delivered faults worker | `5e6594f96bd7ca82afc12564a9164bdf78a677cd3d09f0f79a6f678c361e79f7` |

## Near-cap native namespace failures

The [namespace error campaign](maintenance-space-exhaustion.md#injected-namespace-and-sync-errors)
passed seven syscall error cases per host on macOS/APFS and Docker
Linux/ext4. Every case returned `OutcomeUnknown` with the injected errno and
native operation. When exchange failed before invocation, the canonical path
held the exact original bytes. After a successful exchange, including the
case that returned EIO after the namespace changed, the canonical path held
the exact compacted bytes. Both paths reopened revision 212 with all 64
retained roots intact. Every retry produced the same compacted image and
preserved any preexisting scratch file byte for byte.

An uninjected calibration covered all six actual syscalls. The disabled
injector failed the expected native error check on each platform. All 15
fresh-image checks per host passed and the supervisors verified cleanup.
This adds post-copy native error handling to the existing SIGKILL,
simulated namespace and kernel copy-phase exhaustion evidence. Actual
namespace exhaustion and device power-loss qualification remain open.

## Restartable snapshot capture

The [capture workflow](replication-operations.md#restartable-captures)
commits a local descriptor only after the complete export is durable. A
later sender verifies the image's exact length, SHA-256, enrollment,
revision, witness and current and retained graphs under one lease. The
descriptor contains local capture state. The follower independently reports
its durable revision and digest through the existing authenticated protocol.

The native CLI test stopped the sender after it connected and while TLS
was waiting for the peer. It confirmed the follower was unchanged, advanced
the primary from revision 4 to 5, and sent the saved revision 4 from a new
process. The follower reopened revision 4 with current value 2 and retained
revision 2 with value 1. Replaying the same capture preserved every follower
file byte for byte. Once revision 5 was installed, resending revision 4
reported the newer receipt while preserving the saved pair and follower
files. Changed image bytes, an incomplete descriptor both before collection
creation and before row publication, and wrong enrollment all failed before
network I/O.

The native TLS fault test discards the terminal acknowledgment after
installation, then reopens the follower and reconstructs the sender from its
saved descriptor. Retry recovers the exact original digest without manifest
growth. The simulator verifies the lease, rejects a digest-matching image
with no witness, compares actual outbound bytes and permits only reads and
length queries on the reopened export.

macOS arm64/APFS and Docker Linux arm64/ext4 each passed 15 replication unit
checks and 24 application checks in both debug and release, plus six core
replication integration checks in debug. Strict Clippy passed for the app
on both hosts and for the all-feature core on macOS. macOS ASan and TSan at
`nightly-2026-11-12` each passed all 24 application checks with rebuilt std.
The MSVC cross-target core check passed. The native Windows job for
`6155aa1` also passed the saved-image verifier, lost-terminal-reply and
sender interruption checks in debug and release.
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43719561073/job/132828376063)

```sh
cargo test --offline --locked -p weekend-db --features replication --lib replication
cargo test --offline --locked -p weekend-db --features replication --test replication
cargo test --offline --locked -p weekend-replicate --all-targets
```

| Evidence | SHA-256 |
|---|---|
| Descriptor source | `f377522e6acb1a7009532677c1f65acf6755f3421d2c935b79cb05ad199cca22` |
| Core replication source | `915114528fe5b410933d522848e84c7870f66676c25fa8598e40428347a0c365` |
| Native CLI test source | `6352e13de67e6d2f83073b320b9e248e75c337c405c50caa98013e2db52f43cc` |
| macOS application debug log | `78d75abe82e47da11c90c11230a4c9a625627c94372de86bd435effcce34835c` |
| macOS application release log | `bd2b04073763710d9f71e1a410334328f77b04bd9d6cc42ffa1a1b88a3167b68` |
| macOS ASan application log | `de3812036fd902102967fbd30dc13d92f8785fde0053425df33c5da8d127d3d4` |
| macOS TSan application log | `3a7648a7e447119ee03c5103377bd24efd910e93e23567d35cf0622d7126402a` |
| Docker Linux checks and Clippy | `e4a22ec4e785698b1396a24c6a67bd523b93c47bba8135f4b7b7a08acace751e` |
| Docker Linux core release replication log | `c83585697fc9c7c5824bdb2b8f73f761e7f82da0a617e1660780804a9228faf3` |
| MSVC cross-target core check | `db7de6234672b0169b84aad4d7e6befddcb3dc2d48bfc8046b68f74882f2fd3f` |
| Native Windows job log | `519c55d41dc922af64bd3d604691470a6c1125af2bb7b425376912c703db9c2b` |

## Native StudyProgress roster cutover

The [staged cutover](crdt-operations.md#staged-roster-cutover) adds explicit
versioned policy files, `stage-roster`, `run-policy`, and
`verify-roster`. Existing default enrollment still uses the original
commands. The staged migration requires matching old causal payloads
under both native leases. It seeds the visible state on new actor A only.
New actor B obtains that state over the authenticated staging link while
application edits are disabled. The verification command reopens both
files and checks exact new causal payload equality.

| Native process case | Verified result |
|---|---|
| Divergent old histories, active lease, or reused collection/generation/actor ID | Rejects before creating either destination. |
| Second destination already exists | Reports `AlreadyExists`, preserves that file, and leaves the first destination empty at revision 0. A fresh target pair stages successfully. |
| Fresh pair before authenticated staging sync | The verifier reports `Conflict`. |
| Old actor and fresh actor deliberately exchanged using working TLS pins | Both receivers reported `ForeignStore`. Both senders recorded zero covering acknowledgments, and both revisions stayed fixed. |
| Fresh TLS pins and new roster, then one staging peer killed | The reopened pair verified revision 3 on both files with 12 practice minutes, completed topics 1 and 2, planned topic 4, goal 40 and limit 50. |
| New independent edits after verification | Both actors converged to 19 minutes, completed topics 1, 2, 3 and 5, planned topic 4, goal 50 and limit 40. Repeated covering exchanges advanced neither revision. |

The tests compare both archived old files byte for byte, including the
retained roots. They also verify a removed planned topic stays absent after
conversion. macOS arm64/APFS and Docker Linux arm64/ext4 passed all 17
`study-peer` tests per debug and release profile, including the existing
credential interruption test. Strict Clippy passed on both hosts. The
roster process test passed on macOS ASan and TSan with rebuilt std at
`nightly-2026-11-12`. Routing authority and stopping copies on remote hosts
remain operator responsibilities.

The table below records the initial `9aab470` implementation. That head
also cross-linked both apps' seven Windows MSVC test executables in debug
and release with `cargo-xwin 0.23.1`. The hosted Ubuntu qualification caught
the roster scenario entering the historical upgrade helper extraction.
The follow-up keeps both scenarios outside the shared helper prefix and
isolates explicit policy parsing from the frozen legacy protocol source.
The existing upgrade seam tests catch omissions or extra executable seams.

```sh
cargo test --offline --locked -p study-peer --test credential_rotation native_roster_cutover
cargo test --offline --locked --release -p study-peer --all-targets
```

| Evidence | SHA-256 |
|---|---|
| Roster command source | `37016be4a18fccec6655046bfa69d8074be99bf90a5e55ffcd8d7879489ff649` |
| Native process test source | `2d56b6980f9c39f49517f90ecb00509fea63f70e098911a4ca02b58c6101ce0a` |
| macOS debug log | `a95e85be415f2b60f82da1353cd74cc4c3394126c5bf687f876cd2b61b826643` |
| macOS release log | `027b5a9206ec661d3252e9a3fa6d5fa0c9bddd4a801fe746ca7c6f68b1de3e8b` |
| macOS ASan log | `96cf2fdef71d59b59d1909bde8281cc2d00734ef14d56fe2e8d9427024b44e62` |
| macOS TSan log | `b92bb332f370275c2115da1fba94bdf0ab8ab48fec72b79a8d1f0bd46d6a580a` |
| Docker Linux debug, release and Clippy log | `3fea25135c6ffb70f5ce7fa15a21f6ff98dc0fbae77532859051fb3405d9e61d` |
| Docker Linux subprocess event archive | `643d597814c5f1024d1e67ccb3fe3c11c7204dc436e90a7267b40a7c1b5c29e7` |
| Windows application debug cross-link log | `709d46d9a3e44abc56886cb8ad9cb0a79fa74ae609e7b25538a1f459fadc25d1` |
| Windows application release cross-link log | `8348ea7078e4fbd39c5077662ca36c8c8549e84b26d4e5b20c5974b102983994` |

The Mac workspace rerun after the fixture boundary fix passed 637 tests
with five default ignores. A later parallel test exposed that the native
fixture's timestamp-based directory names could collide at one clock tick.
The fixture now adds a per-process sequence. A directed test creates two
directories at the exact same timestamp. The corrected macOS app suite
passed all 18 tests and strict Clippy.

The exact historical pair `fb80989d98bd977743168ee10418a434a67a0261` and
`ebc1eca779d7827e1a19a4a8742cd047ff856e49` passed the
[legacy upgrade scenario](study-cli-upgrade.md) on macOS/APFS and Docker
Linux/ext4. Both generated fixtures passed strict release Clippy. Mixed
endpoints reached each other and rejected at ALPN with zero covering
acknowledgments. The coordinated upgrade preserved both retained roots,
converged to 36 minutes and made no new commits during repeated sync and
restart. This exercises legacy routing on the new app build separately from
the explicit fresh-roster workflow.

| Additional evidence | SHA-256 |
|---|---|
| macOS workspace log after the boundary fix | `3e7d06bdc1d1f4c94dafae2d3b454cfb5b5c8f415759a9f3dc33944d57c0ed5e` |
| macOS legacy-upgrade controller | `353dddd738122d63cbbe853df09e3fa90ba9b69190b63afb4902b0c91257c77e` |
| macOS legacy-upgrade worker | `f466e899531b9ae4a5225b1fe90d684249add61436526282e249e6ad7e46b798` |
| Docker Linux legacy-upgrade controller | `333b570d031522d9a0ae4c75cc18116052ec4ddc785259d07813c8914bad5e03` |
| Docker Linux legacy-upgrade worker | `c41a73015ffa80fcb51be4e694f898291f4df696f6ca67ab283c154f1d9713cd` |

The [Windows job for `85853c7`](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43721267623/job/132833559162)
passed the roster cutover and the fixture-directory collision regression in
debug and release. A later coordinator journal case timed out while the
1,024-receipt workload was running. The two tests now run separately within
the suite and journal failures report the command. The CLI deadline stays at
three seconds. The Windows job log SHA-256 is
`0172854294e86a9b84193cd057deb1fcf8ba414aea0a306c8aad86a95ffcfbd1`.

The separated journal and capacity tests both passed in debug and release
in the [Windows job for `86ff6c6`](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43722762171/job/132838097838).
Its overall failure came from the new fault consumer's compiler check. The
20-case normal C/C++ DLL trace passed. The job log SHA-256 is
`7939197f9012c121519011f8a37fa2627008d06c2e8332a678a754180ce77bca`.

## Windows exact-DLL publication faults

`3efda17` adds the [six-case Windows exact-DLL gate](delivered-c-faults.md#native-windows-delivered-c-gate).
An optimized C11 consumer injects write and flush errors into the loaded
DLL. A fresh process verifies the reopened revision, reconciles request 42
and reads retained revision 2. The CI job saves this evidence before the
longer workspace suite and enforces the fault result at the end of the job.

The Windows C11 fault consumer, its disabled-injection control and the
normal C11/C++17 trace consumers all cross-linked against the Windows import
library. The xtask Windows test binary also cross-linked, and strict Clippy
passed on the host and Windows targets. Native MSVC caught shadowed fixture
variables in the first run. `47915ea` corrected them and passed all six native
Windows cases against DLL source `e4be226`. The independent archive check
verified the DLL, compiler and consumer evidence. The Windows job passed
the normal 20-case DLL trace using the same artifact. The coordinator journal
and capacity tests passed in debug and release. The shared fault consumer
also passed all five macOS/APFS and all six Docker Linux/ext4 cases against
the exact `f70e405` packages, including the Linux tmpfs exhaustion case.
[Windows results and evidence](delivered-c-faults.md#native-windows-delivered-c-gate)

| Evidence | SHA-256 |
|---|---|
| macOS delivered C regression report | `6038e0eb0106496759ad83f193b26298ce98489bfe0831f7865e1e63f6ffb8ff` |
| Linux delivered C regression report | `221691fe548a9eb27b1908a31741fe5089ec8e0deaa440d636d95111d18b0656` |
| Native Windows delivered C report | `ba515c5b949a3013771c05f1cf9f9a61d123d40dd6210e35f5019e949628f26b` |
| macOS coordinator debug and release | `baa44b2d3d292b885d502dddbdafdca8c3674762f01379fbf2134c8ea256c4e7`, `7d164cda671a4dfeb23667d6943c2d1da1ead9fb03c1c73269ba9195f149a7ee` |
| Docker Linux/ext4 coordinator release | `67d72baea332f29b6ab2a93f87cc9651956d685be15f9dddf0c2cdadf7d111ac` |

## Windows NTFS append exhaustion

The [native exhaustion gate](delivered-c-faults.md#actual-ntfs-disk-exhaustion)
passed at `ebfd099` against the manifest-pinned DLL from PR merge `ac1bc1a`.
The disposable fixed 64 MiB VHD reached zero free bytes. The DLL observed
Windows error 112 on append with no injected failure, and a fresh process
reopened current revision 3 and retained revision 2 while the volume was
still full. After freeing space, a third process retried request 42 and
reopened revisions 4 and 2. The verified volume and file cleanup passed.

Strict host and Windows-target xtask Clippy passed. Windows cross-linking
and the native MSVC consumers passed. The macOS and Docker Linux exact
`f70e405` package regressions passed five and six cases respectively with
the updated shared C fixture. The scope of the new evidence is append
exhaustion. NTFS maintenance copies were tested separately below. NTFS
quota remains open.

| Evidence | SHA-256 |
|---|---|
| Native Windows NTFS exhaustion report | `eb4f24e9e0b11c4c2499e3c1be593548088c72d738bddfe788dc9ad50325a11e` |
| macOS delivered C regression report | `b154ef1230cfa872540edef877793f79226d14264f2cb58c7b4da7c38af47358` |
| Linux delivered C regression report | `3d1279e974e17fbc0a7c38743394f9734831f19f0a056c759c9909c0874d3241` |

## Windows near-cap maintenance exhaustion

The [Windows maintenance gate](maintenance-space-exhaustion.md#native-windows-near-cap-copy-profile)
passed at `cd3d14a` with PR merge source `3572572`. Backup, restore and
`compact_to` each encountered native write error 112 at an 8 MiB partial
copy. Fresh processes reopened revision 212 and all 64 retained roots in
both the source and verified backup at zero free bytes. Retrying at fresh
paths preserved each partial destination and produced copies identical to
the verified backup. A later source commit reopened at 213. The isolated
256 MiB VHD detached and was removed. The macOS seed and Linux maintenance
regressions passed with the same fixture. NTFS maintenance quota, namespace
faults and physical recovery remain separate gates.

## macOS near-cap maintenance exhaustion

The [APFS disk-image gate](maintenance-space-exhaustion.md#native-macos-near-cap-copy-profile)
passed locally on arm64 at `df61d55`. Backup, restore and in-place compaction
all returned actual `ENOSPC` after an 8 MiB partial copy. The source and
good backup reopened at revision 212 with all 64 retained roots while
4 KiB writes still failed with `ENOSPC`. APFS reported 5,849,088 bytes
available at that boundary. All three retries matched the verified backup.
The source subsequently reopened at revision 213, and the bounded 256 MiB
disk image was detached and removed. The hosted macOS job now includes the
same gate. Actual namespace exhaustion and physical power loss remain open.

## macOS APFS append exhaustion against the delivered dylib

The [C ABI append gate](delivered-c-faults.md#actual-apfs-append-exhaustion-against-the-dylib)
passed all six cases against the clean release dylib at `6053e8c`. The
actual append received kernel `ENOSPC` and returned `IO` before any root
write, witness write or publication flush. A fresh process reopened revision
3 and verified retained revision 2 while the image still rejected writes.
The failed commit and full-space reopen preserved the seeded image hash.
After freeing space, retry of request 42 reopened revision 4 with retained
revision 2. The disk image detached and was removed. The shared C fixture
also passed the Linux regression against `f70e405`. The macOS workflow
archives the combined package fault results.

## Windows NTFS owner quota and append recovery

At `fbb114e` the [hosted NTFS owner-quota gate](native-quota.md#native-windows-ntfs-owner-quota)
observed real error 112 with 52,576,256 bytes globally free under a 1 MiB
hard quota. The database and ballast both belonged to the verified Local
Service SID on the owned VHD. A fresh process reopened current revision 3
and retained revision 2 with identical bytes. Raising the quota to 4 MiB
with the ballast still present allowed retry to revision 4 and retained
revision 2. The archive, five process records and volume cleanup were
verified. Strict host and Windows-target Clippy passed, and Docker Linux
passed 18 quota controls and built the same fixture.

## Windows NTFS typed schema rollout

At `5818ba5` the [frozen typed rollout](typed-schema-rollout.md#windows-result-on-2026-11-15)
passed all 12 phases on native Windows x64/NTFS with the original lockfile
bytes. Exit 70 after staging a migration left current and retained state
unchanged on fresh reopen. The historical application preserved the new
score and opaque field 99 during its edit. It reopened a new indexed v1
insert after restoring the pre-upgrade backup. All 75 downloaded artifact
hashes and 32 child cleanup records were verified. The macOS rerun passed
all 12 phases with Git's checkout conversion enabled in the environment.
This records T03 for the pinned schema pair on all three platforms.

## Windows NTFS frozen storage and core wire compatibility

At `91b7519` the [storage pair](storage-compatibility-21e5d26.md#windows-ntfs-result)
passed two writes, four reads and four unsupported-version rejections. Each
read and rejection preserved the image bytes. The [core wire pair](snapshot-compatibility.md#windows-ntfs-result)
reopened durable revision 4 and retained revision 2 in both directions while
the primary locally reached revision 5. Duplicate shipment returned the same
receipt, and wire version 255 poll rejection left the follower files unchanged.
The two reports bind 144 verified artifacts and 54 command and job-cleanup
records. Both profiles reran on macOS with identical frozen archives and
original lockfiles. This advances F14 and the core wire part of R05.
