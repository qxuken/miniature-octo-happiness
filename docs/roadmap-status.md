# Production qualification status

Release scope includes the local store, typed Rust, snapshot replication,
CRDT peers and coordinator, as requested on 2026-11-09. The project uses
`MIT OR Apache-2.0`. Cargo packages stay private with `publish = false`.
The project name is provisional. Baseline `1e883c8` checkpoints the prior
decision tests and first four fuzz targets.

| Gate | Progress and remaining evidence |
|---|---|
| Decision ledger | Scoped metadata, graph, transaction, memory, FFI, I/O, simulator and replacement pairs exist. The [source audits](decisions/README.md) classify all 2,415 explicit records across 47 files at checkpoint 8556dc1, plus supplemental FFI/derive mappings. There are 284 macro blind spots; delegated failures, generated-code inventory and compiler MC/DC measurement remain separate obligations. |
| Fuzzing | Twenty-two targets and deterministic seeds exist; stable replay passed 25 tests. The frozen 4203e0a campaign completed 1,384,848 executions across 21 targets. The register target has a pre-fix 1,000-run ASan smoke campaign and a successful post-fix rerun (surviving summary lacks execution count). Longer campaigns across the current candidate remain. |
| Inspection | Read-only graph verification and CLI implemented. Typed index checking is available. Native error context and cheap append-space queries implemented. Verification stage/structure-start offsets implemented and fault-tested. |
| Backup/restore | Verified create-new copy, retained roots and process restore tested. Typed restore across processes and old retained schemas is tested. Deployment restore-time budgets remain. |
| Migration/salvage | Logical export/import format 1 implemented with frozen fixture. Explicit complete-root salvage is tested. Future-version conversion remains. |
| Storage lifecycle | Compaction exists. Append headroom and near-cap recovery/compaction tests implemented. Indexed recovery measured near the cap. Cleanup policy and device qualification remain. |
| Resources | Per-value/transaction limits exist. Opt-in aggregate core layout budget for pinned/draft allocations implemented and independently reviewed. A 600-second macOS soak passed; application/backend/RSS envelopes and longer representative soaks remain. |
| Packaging | Runtime identity and private artifact checks added. Private identity/notice checks existed at a7dc343; later relocation testing exposed and fixed its temporary loader-path defect. Windows compiled consumers passed at 1ca5e5ab. Windows private packaging passed 6266359; macOS Odin relocation passed locally. Clean 4203e0a source profile passed native and isolated Linux builds; registry inputs remain external. |
| Compatibility | v2 format rejection exists. Archived v2 and logical-v1 fixtures exist. a7dc343 reads 1ca5e5ab files in tested pre1 cases. Broader version/feature qualification remains. |
| Native matrix | macOS and Docker aarch64 Linux executed locally. Native Windows/macOS/Ubuntu, ASan and TSan passed 4203e0a; simulated month also passed Linux ext4 server/macOS APFS client. New candidate/device qualification remains. |
| Unsafe/concurrency | Thread/process/sanitizer tests exist. Bounded ownership/admission Loom models, negative controls, Miri and abort checks passed. C-side ASan passed macOS and Linux with calibration at 4203e0a. Charged allocation/realloc paths independently reviewed and tested under Miri. Additional model workloads and full unsafe review remain. |
| Typed Rust | Models/migrations/indexes exist. Verification/rebuild implemented and fault-tested. Explicit bounded migration with atomic progress tested. Mixed-version qualification remains. |
| Snapshot replication | Verified snapshots and separate acknowledgment exist. Manifest bounds, cleanup, compaction and explicit reseed are tested. The private [weekend-replicate CLI](replication-operations.md#authenticated-native-cli) adds exact-leaf mTLS, distinct ALPN, bounded socket deadlines and same-export retry. Exact tree `a0aad9f9ff95a2998ca623b654c452ae02658b6b` passed macOS and Linux/ext4 nine app plus one CLI test in debug/release, macOS strict Clippy/full Rust/C/Odin checks, and authenticated Mac→Linux ship/restart/poll: follower revision 13 retained root 1@11 and exact digest survived primary advancement to 14. Code remains byte-identical after documentation updates. Optional Linux Clippy was unavailable; broader deployment, mixed-version and physical power-loss qualification remain. |
| CRDT peers | Fixed two-actor TLS demo exists. Register admission now rejects unwitnessed retirement and incompatible cyclic histories, with bounded causal-event and frame oracles. Policy evolution, stale backup identity, reserved-row ownership and representative deployment/soak gates remain. |
| Coordinator | Conditional writes, durable receipts and configured loopback authentication tested. Deployment qualification remains. |
| Release operations | Support/security policy and provenance recorded; capped cross-OS month and restore passed. A 600-second local soak passed; longer deployment soak, latency guarantees and final bundle remain. |

The full acceptance inventory is in [production-readiness.md](production-readiness.md).
Rows stay open until evidence is recorded. Physical power-loss qualification
needs the declared OS/filesystem/device stack. Configured CI and executed
qualification are recorded separately.

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
classify 2,415 records with explicit residuals. Generated Model::patch now has a
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
