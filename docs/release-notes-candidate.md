# Private release candidate notes

Prepared 2026-11-16. These notes describe a qualification checkpoint, not an
approved production release or an offer of ongoing support. The project name,
Rust API and package version `0.1.0` remain provisional. All Cargo packages keep
`publish = false`. No registry publication or distribution approval is implied.
The intended scope includes the embedded engine, typed Rust, C/Odin consumers,
passive snapshot replication, fixed two-actor CRDT peers and the coordinator;
each retains its own qualification boundary.

## Source and build identities

The four-platform delivery checkpoint is the full tested merge
`7e0e44f98809b5a82a223604f05425dae3c9ab2b`, combining branch head
`8ddf502b906befe2ad0364ec2246a309d06752bf` with main
`693aaef6550867db7a8477dc88e77c26c6e44aa1`. Its Git tree is
`2a5e4065fac6d49b1943adf05fc38b4afbc9c479`; its source archive SHA-256 is
`c9b156fd4025a00d38351151e53a71358fac1ffcf682aa6c3c8b1e5325fc7d07`.
Use the tested merge identity when reproducing these packages. The branch head
and a later verifier commit are distinct source identities.

All four package lanes use Rust and Cargo 1.99.0, the declared minimum compiler.
Each lane passes debug and release workspace tests and packages feature masks
0 (base), 1 (replication), 2 (CRDT) and 3 (replication plus CRDT). The 16 packages
pass their C11, C++17 and Odin consumers and relocation checks. The combined
packages also pass the fresh Rust source consumer and exact 134-cell trace.
Ignored tests retain their recorded skips; a passing lane does not mean every
test or possible platform configuration executed.

| Executed package lane | Workspace passes / skips per debug or release profile | Retained hosted artifact |
|---|---:|---|
| macOS arm64 / APFS | 698 / 6 | Local retained lane |
| Docker Linux arm64 / ext4 | 734 / 8 | Local retained lane |
| Hosted Linux x64 / ext4 | 734 / 8 | `14216513014`, SHA-256 `9d2a83e0e5a9978b6dba3a27b43cc1f61fb799ca79d52c6d2caf86162383e5aa` |
| Hosted Windows x64 / fixed NTFS, MSVC | 562 / 6 | `14217930125`, SHA-256 `fc2ee6be4ac255bdee67e80a52dc4092d0ecad71adf3a70a104fc427a16603f1` |

Both hosted minimum-compiler jobs passed and checked out the full tested merge:
[Linux x64](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43848281064/job/133233954223)
and [Windows x64](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43848281064/job/133233954359).
All 13 jobs in this Engine workflow also passed. Same-source macOS workspace
ASan and calibrated TSan each passed 698 tests with six skips; Linux x64
shared-runtime C/Rust ASan passed its calibration and three consumers.
The [retained checkpoint](private-delivery.md#four-platform-checkpoint-at-7e0e44f)
keeps diagnostic compilers, package compilers and coverage scope distinct.

The later verifier source `6eb5f1a884bd75cfd71cdd75e3fa1cf0be597e63`
adds optional retained-fuzz verification in xtask and accompanying documentation;
the changes from the tested merge do not change engine, typed runtime, macros
or application runtime source. This verifier binds the same four-platform
checkpoint to three retained macOS compaction microbenchmark samples and an
all-22-target ASan campaign. The benchmark uses Rust 1.99.1. The completed
extended campaign records 1,442,896 actual executions under per-target
100,000-run/120-second bounds, including seed initialization. The combined
report SHA-256 is
`a70f827e3e846aa7de73767f7258d01db3ab250a1fb4aabf08cd0de749c77cbc`.
The later verifier does not relabel the delivered binaries as builds of
`6eb5f1a`. See the [checkpoint and retained evidence](private-delivery.md#four-platform-checkpoint-at-7e0e44f).

Tool checkpoint `615f880b1d25176bef3122d98463889c7a0e0a95` subsequently adds
the [administrative recovery rehearsal](incident-recovery-drill.md) and
[service output regressions](service-diagnostics.md). These pass their named
macOS and Docker Linux profiles, with source overlays and executable hashes
recorded separately. The changes affect tools and tests; they do not change
the engine, typed, macro or service runtime source or relabel the four-platform
packages as builds of that later commit.

Packages expose version, source identity, physical format, ABI and feature mask.
Ordinary builds can report source `unknown`; private package manifests and
runtime queries must agree on the actual delivered identity. Fault injection
is a separate feature bit, 4, and is not one of the four package masks above.
The [release contract](release-contract.md) describes lockfiles, panic strategy,
configuration hashes, notices and artifact verification.

## Independent compatibility boundaries

| Boundary | Candidate behavior and limits |
|---|---|
| Physical storage | Format 2; no physical-format migration is introduced by the later verifier change. Images remain capped at 64 MiB with at most 64 retained roots. Unsupported formats reject; a corrupt authoritative newest graph does not authorize fallback to an older graph. Native durability depends on the documented filesystem/device assumptions. |
| Logical export/import | Logical format 1 is separate from physical format 2. Verified quiescent export preserves identities and revisions; import creates a fresh physical destination. Bounds and physical representability still apply. This does not perform application schema conversion. See [logical format](logical-format.md). |
| C ABI / Odin | ABI 1 and header/runtime identity checks; status/kind meanings and opaque-handle ownership remain separate from disk format. Qualified packages use unwinding. Valid pointers, live handles and close quiescence remain caller obligations; refcount exhaustion intentionally aborts. Generated Odin bindings must accompany their matching header/library. |
| Rust / typed schema | Pin matching core, macros and typed runtime source. No stable Rust API or arbitrary mixed-schema compatibility is promised. Raw writes can violate typed index/catalog and CRDT reserved-row ownership. Check/rebuild typed indexes explicitly when required. |
| Snapshot protocol / CLI transport | Core snapshot wire version 1 is separate from the file codec. The caller authenticates core channels. `weekend-replicate` uses completed mutual TLS, exact configured leaf pins and `weekend-snapshot/1` ALPN. Local commit/capture and follower `DurableAck` are distinct outcomes. |
| CRDT / application protocols | Exactly two enrolled actors share a collection identity, policy generation and roster. Application payload markers and merge policies are independently versioned. `study-peer` requires `study-progress/1` ALPN. A higher generation does not convert records or replace actors automatically. |
| Coordinator | The loopback service owns its reserved record/receipt layout. Legacy identified requests retain at most 1,024 permanent receipts; the opt-in sequenced profile uses 1,024 assigned slots and a durable client journal. Retirement, backup reconciliation and deployment authorization remain separate from engine transactions. |

## Qualified upgrade directions and rollback procedures

The following are named pre-1 qualification pairs and workflows. They are not
an assertion that every historical executable interoperates with `7e0e44f`.
Before promoting this checkpoint, run the actual deployment's old/new pair
and preserve its exact artifacts and state checks.

| Surface | Executed direction and operator boundary |
|---|---|
| Physical format-two fixture | `a7dc343` and `21e5d26` each wrote an image that both readers verified; unsupported-version reads preserved bytes. This small base-feature case passed macOS/APFS, Linux/overlayfs and separately Windows/NTFS. No format translation was needed. [Storage compatibility](storage-compatibility-21e5d26.md). |
| Core snapshot wire | `dcd2402` → `315068f` and `315068f` → `dcd2402` passed the retained-state, digest, retry/reopen and unsupported-poll-version oracles on macOS and Windows. This private core loopback test does not establish CLI TLS interoperability. [Snapshot compatibility](snapshot-compatibility.md). |
| Authenticated snapshot CLI | Actual release binaries `315068fc38b99d8ec616fdb5a8ee81107b471534` and `365daf6c8a6b7c85fb049fb5a0370b401dde2424` passed old/new sender-server combinations, restart, wrong-pin rejection and retry. Separate native Mac/Linux/Windows results and a macOS-client → Docker Linux/ext4-follower sequence retain their own evidence. The latter verifies both client versions with both server versions, including duplicate shipment. Physical cross-host routing and arbitrary later-version rollback remain unqualified. [CLI upgrade](snapshot-cli-upgrade.md). |
| Typed additive schema | `1ca5e5a` → `6cc8ef3` passed explicit resumable migration of a defaulted score with a fixed unique index. The old binary could read both row versions and edit a migrated row, but could not insert a new v1 row after catalog advancement. Rollback restored the pre-upgrade backup into a fresh file and reopened it with the old binary. Post-backup writes require reconciliation. This is not a general in-place schema downgrade. [Typed rollout](typed-schema-rollout.md). |
| StudyProgress CLI | `fb80989` → `365daf6` requires a coordinated stop/upgrade/restart of both peers. A mixed old/new pair rejects synchronization at the new ALPN boundary while preserving locally acknowledged edits. Both upgraded peers converge and preserve retained history. Old-reader rollback after arbitrary new application changes was not qualified. [Study upgrade](study-cli-upgrade.md). |
| CRDT policy or actor history | Freeze and reconcile both old writers; preserve both histories; review conversion; stage and verify a fresh collection/generation/roster; then change routing. Restoring a stale backup must not resume the retired actor's writes. Rollback after new-history edits requires explicit reconciliation and conversion. [CRDT operations](crdt-operations.md), [migration example](crdt-policy-migration.md). |

For an ordinary binary replacement, stop users of the database, record runtime
identity and feature mask, run read-only inspection and preserve a verified
backup before introducing the new executable. Reopen and verify the complete
current and retained application state and any typed indexes before routing
writes. Restoring a backup requires a fresh destination; retain the upgraded
source until post-backup writes have been reconciled. See [operations](operations.md).

A restored or forked primary needs a fresh snapshot stream and explicit follower
enrollment. A replacement follower for the same primary lineage can use the
existing enrollment only through the documented fresh-directory reseeding
workflow. There is no automatic promotion or fencing. Keep uncertain transfers'
immutable exports for poll/retry. After local `OutcomeUnknown`, recover and
reconcile before replaying a non-idempotent operation. A failed publication
handle can require recovery even when its result is `Io`; subsequent writes
return `RecoveryRequired`. See [replication operations](replication-operations.md)
and the [publication contract](../DESIGN.md).

## Current limitations and remaining release gates

- The local storage profile has one file-owning process and concurrent readers
  and drafts inside it. Inspection, export, backup and maintenance require the
  documented leases and quiescence. Windows in-place compaction is unavailable;
  use verified fresh-path copies and follower reseeding as documented.
- The four-platform package matrix establishes those precise native consumer
  profiles. Historical fault, upgrade, resource, mutation and sanitizer results
  retain their original source/compiler/filesystem scope. They do not become
  candidate-wide passes by association.
- The bound benchmark retains measurements and recomputes aggregation; its
  verifier does not verify the actual benchmark binary. Three microbenchmark
  samples and the bound 1,442,896-execution finite campaign do not establish deployment
  latency, throughput, memory budgets, sustained fuzzing or long-running service
  behavior. Select budgets and execute representative candidate workloads.
- Complete remaining feasible decision/failure pairs, relevant generated and
  delegated-code review, broader compound/resource traces, and the selected
  candidate's sustained sanitizer/fuzz and deployment soak evidence. Line or
  region coverage is not compiler-measured MC/DC. Accelerated calendar runs do
  not establish continuous wall-clock longevity.
- Qualify the actual advertised OS/architecture/filesystem/device stack and
  version transitions. Process kills, simulator tears, disk images and kernel
  error drills do not establish physical power-loss durability. Additional
  languages, filesystems, devices, network routes and deployment profiles need
  their own evidence.
- Before qualification, the release owner must select the immutable distributed
  revision and artifact set, preserve a reproducible evidence bundle, complete
  attribution/dependency and incident/restore review, accept operational and
  workload limits, and define supported versions and private patch handling.
  Known data-loss, memory-unsafety or authorization defects in the selected
  profile block qualification. See the [acceptance audit](production-acceptance-audit.md)
  and [readiness roadmap](production-readiness.md).

Report suspected data loss, memory unsafety or authorization defects privately
to **sec@qxuken.dev**, also the release-owner contact. Include source/build
identity, platform/filesystem, features and a minimal reproduction; omit data,
credentials and private keys from the initial report. No response or patch
deadline is guaranteed during pre-release work. [Security reporting](../SECURITY.md).
