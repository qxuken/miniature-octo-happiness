# Production qualification status

Release scope includes the local store, typed Rust, snapshot replication,
CRDT peers and coordinator, as requested on 2026-11-09. The project uses
`MIT OR Apache-2.0`. Cargo packages stay private with `publish = false`.
The project name is provisional. Baseline `1e883c8` checkpoints the prior
decision tests and first four fuzz targets.

| Gate | Progress and remaining evidence |
|---|---|
| Credential cutover | Stopped-endpoint [snapshot](snapshot-credential-cutover.md) and [StudyProgress](study-credential-cutover.md) leaf/pin replacement pass native Mac/Linux debug/release. Stale identities reject, interrupted offline state and retained roots survive, and successful retry/repeat sync preserve acknowledgment boundaries. This is fixed-roster operator cutover, not live reload, actor migration or binary upgrade. |
| Decision ledger | Scoped metadata, graph, transaction, memory, FFI, I/O, simulator and replacement pairs exist. The historical [source audits](decisions/README.md) classify 2,561 explicit records across 51 files, including shared TLS separately for each binary profile. Later VALUE/DATA index and [buffer-growth controls](serialization-buffer-growth.md) have separate mappings; fresh [shared-cache inventory](serialization-cache-sharing.md) has 2,604 records and 319 macro blind spots. Historical mappings, supplemental FFI/derive mappings and current inventory counts remain distinct. Signed alternate leaves are rejected by both exact-pin checks. Directed [handshake pairs](tls-handshake-decisions.md) now establish unfinished-handshake/pending-write independence at both endpoints; four guard omissions are detected. Delegated failures, generated-code inventory and compiler MC/DC measurement remain separate obligations. |
| Fuzzing | All 22 targets passed the [exact-21e5d26 extended ASan campaign](fuzz-qualification-21e5d26.md), totaling 1,431,321 actual executions under per-target 100,000-run/120-second bounds, including 31,034 register-causality executions. Independent stable replay passed 25 tests; source, both locks and 242 seeds were verified. This supersedes the limited post-fix register count evidence for this checkpoint. Longer campaigns and broader oracles remain. |
| Inspection | Read-only graph verification and CLI implemented. Typed index checking is available. Native error context and cheap append-space queries implemented. Verification stage/structure-start offsets implemented and fault-tested. |
| Backup/restore | Verified create-new copy, retained roots and process restore tested. Typed restore across processes and old retained schemas is tested. Deployment restore-time budgets remain. |
| Migration/salvage | Logical export/import format 1 implemented with frozen fixture. Explicit complete-root salvage is tested. Future-version conversion remains. |
| Storage lifecycle | Compaction exists. Append headroom and near-cap recovery/compaction tests implemented. Indexed recovery measured near the cap. Cleanup policy and device qualification remain. |
| Resources | Per-value/transaction limits and opt-in aggregate core layout admission exist. A [four-axis native qualification](resource-envelope.md) passed macOS/APFS and Docker Linux/overlayfs: 64 depth-64 retained roots, near-exact 64 MiB core saturation/recovered draft admission, the synthetic exact image boundary, and a naturally full image with 63 MiB reachable payload. A requested 1 MiB worker stack completed its bounded parser/value workload. RSS is measured separately, not capped by core accounting. The later [compound profile](compound-resource-envelope.md) combines large payloads, 64 depth-64 roots, eight held drafts, 96 MiB saturation and 98.58% image occupancy, with contention/RSS and directed recovery on Mac/Linux. Other feasible combinations, longer load and native Windows remain. |
| Packaging | [V2 cross-language traces](artifact-trace-validation-modes.md) now compare both validation modes through exact relocated Mac Rust/C/Odin and Linux ext4 Rust/C against one independent 134-cell oracle, preserving v1 bytes. [Exact 21e5d26](local-qualification-21e5d26.md) private Mac/Linux artifacts and copied-source Rust consumers passed, including relocation and independent hash checks. Runtime identity and private artifact checks added. Private identity/notice checks existed at a7dc343; later relocation testing exposed and fixed its temporary loader-path defect. Windows compiled consumers passed at 1ca5e5ab. Windows private packaging passed 6266359; macOS Odin relocation passed locally. Clean 4203e0a source profile passed native and isolated Linux builds; registry inputs remain external. |
| Compatibility | Actual [snapshot](snapshot-cli-upgrade.md) and [study](study-cli-upgrade.md) release CLI upgrade pairs now pass their separate Mac/Linux profiles. Archived v2 and logical-v1 fixtures exist. [Frozen a7dc343 ↔ 21e5d26 storage cases](storage-compatibility-21e5d26.md) passed both writers/readers on macOS/APFS and Docker Linux/overlayfs with original locks, native build identity, retained roots and byte-preserving unsupported rejection. The runner now honors --current, rejects dangling outputs and retains checks under Python -O. Snapshot dcd2402 ↔ 21e5d26 passed native Mac loopback with separate receipts and unchanged follower bytes on bad poll version. Additional shapes, promised version pairs/typed migrations and Windows remain. |
| Native matrix | [Exact 21e5d26](local-qualification-21e5d26.md) full Mac and private Docker Linux/ext4 checks passed locally; current native Windows remains unexecuted. Earlier macOS and Docker aarch64 Linux executed locally. Native Windows/macOS/Ubuntu, ASan and TSan passed 4203e0a; simulated month also passed Linux ext4 server/macOS APFS client. Separately, `b3761af` Windows job `131310796175` passed workspace/C/C++/private packaging; Ubuntu, C-ASan and fuzz passed. Hosted macOS matrix was still pending at this observation. These checkpoint results do not qualify the new backup harness on Windows. New candidate/device qualification remains. |
| Unsafe/concurrency | [Fresh exact-21e5d26 sanitizer checks](sanitizer-qualification-21e5d26.md) passed Mac workspace Rust ASan/TSan and calibrated shared-runtime C/Rust ASan on Mac and Docker Linux. Thread/process/sanitizer tests exist. Bounded ownership/admission Loom models, negative controls, Miri and abort checks passed. C-side ASan passed macOS and Linux with calibration at 4203e0a and 2490631. A fresh independent [core unsafe-boundary review](unsafe-review-2490631.md) found no actionable defect under the documented FFI and quiescence contracts; 13 focused Miri tests passed. The review covers the named source files, not dependencies or every target ABI. Additional model workloads, native Windows and x86 Linux qualification remain. |
| Typed Rust | Models/migrations/indexes exist. Verification/rebuild implemented and fault-tested. Explicit bounded migration with atomic progress tested. Mixed-version qualification remains. |
| Snapshot replication | Verified snapshots and separate acknowledgment exist. Manifest bounds, cleanup, compaction and explicit reseed are tested. Offline CLI cleanup and macOS/Linux manifest compaction now preserve the exact receipt and retained graph; Windows compaction rejects before opening files and uses explicit fresh-directory reseeding. The 14 focused app tests passed macOS debug/release and Linux debug, with macOS strict Clippy; local Windows cross-compilation was blocked by missing SDK headers. The private [weekend-replicate CLI](replication-operations.md#authenticated-native-cli) adds exact-leaf mTLS, distinct ALPN, bounded socket deadlines and same-export retry. Exact tree `a0aad9f9ff95a2998ca623b654c452ae02658b6b` passed macOS and Linux/ext4 nine app plus one CLI test in debug/release, macOS strict Clippy/full Rust/C/Odin checks, and authenticated Mac→Linux ship/restart/poll: follower revision 13 retained root 1@11 and exact digest survived primary advancement to 14. That historical qualification predates subsequent audit tests and maintenance commands. Optional Linux Clippy was unavailable. Separate native macOS [dcd2402 ↔ 315068f core wire-v1 qualification](snapshot-compatibility.md) passed both directions using original locked dependencies: durable 4/retained 2, independent export digest, reopen/retry, and mutated v255 **poll header** rejection with follower files unchanged. This does not qualify upload faults or mixed-version CLI TLS/trust rotation. Frozen `65d142e` also passed the native macOS [verified follower backup/restore and fresh-stream reseed](follower-backup-restore.md): current 4/retained 2, exact former/new receipts, wrong-enrollment status and authenticated-upload rejection, and all original files unchanged. This requires maintained quiescence and does not implement automatic fencing/failover. Broader deployment, checkpoint/OS combinations and physical power-loss qualification remain. |
| CRDT peers | Fixed two-actor TLS demo exists. Register admission rejects unwitnessed retirement and incompatible cyclic histories, with bounded causal-event/frame oracles. The [explicit migration example](crdt-policy-migration.md) qualifies reviewed schema/policy conversion, fresh identities, stale-backup recovery and repeat sync. Native macOS/Linux process tests now verify both old writer leases, six staging interruptions, independent destination reopening and complete old current/retained preservation. Serial/default debug/release pass after correcting a child-output fixture bug. Raw reserved-row ownership, distributed quiescence/routing, operational cutover and broader policy/deployment qualification remain application gates. |
| Performance | Original [serialization scaling](serialization-scale.md) and [history](serialization-history-scale.md) reports preserve the linear-search baselines. The [VALUE index](serialization-identity-index.md) and [DATA index](serialization-data-index.md) now pass exact-byte and allocation/budget/crash/retained-root checks on Mac/Linux. DATA comparison work at 64 roots drops from 108,036 to 39,837 including sorting/searches; small histories use more comparisons. [Charged buffer growth](serialization-buffer-growth.md) passes the same compound workload on Mac/Linux, reduces measured copy-domain peak from 166.76 to 143.22 MiB and preserves 17 exact images; live cache slack increases and clean timing/RSS outcomes are mixed. [Shared cache/index reuse](serialization-cache-sharing.md) now reduces entry clones and per-publication rebuild/sort work while adding retained index memory and cold-open work. Affected-cache forks and additional resource profiles remain; no general latency bound is claimed. A [matched durable-history SQLite baseline](sqlite-baseline.md) passed full current/retained byte oracles across 192 data commits, 12 separate pins and 96 warm-cache reopens per engine on macOS APFS, including verified backup/restore/compaction. Three finite small-state rounds are a baseline, not representative scale, service budgets, stack/RSS bounds or other-platform performance qualification. |
| Authenticated peer workload | Frozen ed8f9f9 passed [30 real CLI offline/sync rounds](study-peer-soak.md) between macOS/APFS and Docker Linux/ext4 with exact pins. All six paired post-acknowledgment SIGKILL restarts preserved exact local revisions and displayed values; repeated covering exchanges added no commits after convergence. The final Python-O launcher and independent review passed. This is a bounded accelerated scenario, not a wall-clock month, long-duration soak, new-candidate or native Windows qualification. |
| Coordinator | Conditional writes, durable receipts and configured loopback authentication tested. The [native capacity case](../apps/write-coordinator/README.md#finite-native-receipt-capacity-qualification) fills all 1,024 receipts, retries lost replies and restarts, rejects overflow/changed payloads, verifies backup/restore and Mac/Linux compaction, and preserves original acknowledgment revisions. Native Windows and broader deployment qualification remain. |
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
