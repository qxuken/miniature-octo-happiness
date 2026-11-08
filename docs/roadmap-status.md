# Production qualification status

Release scope includes the local store, typed Rust, snapshot replication,
CRDT peers and coordinator, as requested on 2026-11-09. The project uses
`MIT OR Apache-2.0`. Cargo packages stay private with `publish = false`.
The project name is provisional. Baseline `1e883c8` checkpoints the prior
decision tests and first four fuzz targets.

| Gate | Progress and remaining evidence |
|---|---|
| Decision ledger | Scoped metadata, graph, transaction, memory, FFI, I/O, simulator and replacement pairs exist. Complete module denominator and remaining module decisions are open. |
| Fuzzing | Twelve targets and deterministic seeds exist. Remaining inventory is in the roadmap. Scheduled CI needs execution after integration. |
| Inspection | Read-only graph verification and CLI implemented. Typed index checking is available. Detailed errors and cheap space queries remain. |
| Backup/restore | Verified create-new copy, retained roots and process restore tested. Typed restore and recovery-time budgets remain. |
| Migration/salvage | Logical export/import format 1 implemented with frozen fixture. Explicit complete-root salvage is tested. Future-version conversion remains. |
| Storage lifecycle | Compaction exists. Headroom, near-cap measurements, cleanup policy and native replacement qualification remain. |
| Resources | Per-value/transaction limits exist. Aggregate pinned/draft budget and adversarial latency/soak remain. |
| Packaging | Runtime identity and private artifact checks added. Clean candidate, Windows compiled consumers and dependency/license inventory remain. |
| Compatibility | v2 format rejection exists. Archived v2 and logical-v1 fixtures exist. Historical-reader/new-writer qualification remains. |
| Native matrix | macOS and Docker aarch64 Linux executed locally. Windows native and same-candidate matrix/device qualification remain. |
| Unsafe/concurrency | Thread/process/sanitizer tests exist. A bounded Loom model, Miri and abort subprocess checks passed. Additional model workloads and full unsafe review remain. |
| Typed Rust | Models/migrations/indexes exist. Verification/rebuild implemented and fault-tested. Bounded migration and mixed-version qualification remain. |
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

The target workload/latency/restore budgets and named security/release
owner have been requested from the user. Those inputs remain required
for operational sign-off. The implementation work continues independently.
