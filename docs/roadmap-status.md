# Production qualification status

Release scope includes the local store, typed Rust, snapshot replication,
CRDT peers and coordinator, as requested on 2026-11-09. The project uses
`MIT OR Apache-2.0`. Cargo packages stay private with `publish = false`.
The project name is provisional. Baseline `1e883c8` checkpoints the prior
decision tests and first four fuzz targets.

| Gate | Progress and remaining evidence |
|---|---|
| Decision ledger | Scoped metadata, graph, transaction, memory, FFI, I/O, simulator and replacement pairs exist. Initial inventory recorded 2,380 decisions/macro blind spots. Engine141 and schema176 source audits plus allocation pairs added; other module classifications and delegated obligations remain open. |
| Fuzzing | Twenty-one targets and deterministic seeds exist. Remaining inventory is in the roadmap. The 17-target checkpoint campaign passed Linux ASan CI; new targets await the next matrix. |
| Inspection | Read-only graph verification and CLI implemented. Typed index checking is available. Native error context and cheap append-space queries implemented. Corruption offsets remain. |
| Backup/restore | Verified create-new copy, retained roots and process restore tested. Typed restore across processes and old retained schemas is tested. Deployment restore-time budgets remain. |
| Migration/salvage | Logical export/import format 1 implemented with frozen fixture. Explicit complete-root salvage is tested. Future-version conversion remains. |
| Storage lifecycle | Compaction exists. Append headroom and near-cap recovery/compaction tests implemented. Indexed recovery measured near the cap. Cleanup policy and device qualification remain. |
| Resources | Per-value/transaction limits exist. Opt-in aggregate core layout budget for pinned/draft allocations implemented and independently reviewed. Application/backend/RSS envelopes and sustained soak remain. |
| Packaging | Runtime identity and private artifact checks added. Private identity/notice checks existed at a7dc343; later relocation testing exposed and fixed its temporary loader-path defect. Windows compiled consumers passed at 1ca5e5ab. Windows private packaging and optional macOS Odin exact-artifact checks added; fresh combined CI is pending. |
| Compatibility | v2 format rejection exists. Archived v2 and logical-v1 fixtures exist. a7dc343 reads 1ca5e5ab files in tested pre1 cases. Broader version/feature qualification remains. |
| Native matrix | macOS and Docker aarch64 Linux executed locally. Native Windows/macOS/Ubuntu passed 1ca5e5ab; simulated month also passed Linux ext4 server/macOS APFS client. New candidate/device qualification remains. |
| Unsafe/concurrency | Thread/process/sanitizer tests exist. A bounded Loom model, Miri and abort subprocess checks passed. Charged allocation/realloc paths independently reviewed and tested under Miri. Additional model workloads and full unsafe review remain. |
| Typed Rust | Models/migrations/indexes exist. Verification/rebuild implemented and fault-tested. Explicit bounded migration with atomic progress tested. Mixed-version qualification remains. |
| Snapshot replication | Verified snapshots and separate acknowledgment exist. Manifest bounds, cleanup, compaction and explicit reseed are tested. Authenticated deployment and mixed-version qualification remain. |
| CRDT peers | Fixed two-actor TLS demo exists. Policy evolution, stale backup identity, reserved-row ownership and resource/soak gates remain. |
| Coordinator | Conditional writes, durable receipts and configured loopback authentication tested. Deployment qualification remains. |
| Release operations | Support/security policy, provenance, workload budgets, restore drill and final evidence bundle remain. |

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
They establish execution on those configurations. Windows native and
device power-loss evidence remain outstanding.

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
The 30-day simulated workload passed on an actual Linux server/macOS client pair. Measurements and concrete ext4/APFS execution are recorded; sustained wall-clock
soak and physical-device qualification remain open.

Checkpoint `a7dc343` passed native macOS/Ubuntu, AddressSanitizer,
ThreadSanitizer and coverage in GitHub Actions run `42525776563`. Windows
Rust and ctypes passed; its compiled-consumer command incorrectly treated
the import library as C/C++ source. The `/link` fix has a portable command
regression and awaits a fresh native run. Private artifact creation from
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
