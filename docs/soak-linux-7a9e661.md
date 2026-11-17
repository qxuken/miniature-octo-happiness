# Linux native 1,800-second soak at 7a9e661

A local Linux aarch64 run of exact source
`7a9e6611eaee911ae7477149cbcafaada8a65e47` completed **1,800 seconds** of
native lifecycle work successfully. It was not shortened by the round limit.
No production source or Rust soak driver overlay was used.

| Observation | Result |
|---|---:|
| Completed rounds | 10,578 |
| Concurrent-reader observations | 5,813,826 |
| Rust elapsed time | 1,800,008 ms |
| Observer elapsed time | 1,800.212 s |
| Final database revision | 698,142 |
| Final retained roots | 8 |
| Maximum recorded live core bytes | 22,720 |
| Maximum recorded core peak bytes | 51,168 |
| Declared live core cap | 16 MiB |
| Sampled peak process RSS | 2,928 KiB |
| Maximum pre-compaction file bytes | 917,504 |
| Maximum compacted file bytes | 122,880 |

Core allocation counters are requested engine bytes, distinct from sampled RSS.
RSS sampling can miss between-sample peaks and is not a process memory cap.
Timings and counts describe this constrained, concurrently used local host;
they are not throughput or latency benchmark comparisons.

## Native workload and oracle

The unchanged Rust driver uses two writers and two concurrent readers, paired
keys per writer, immutable pinned snapshots, and 32 commits per writer each
round. Every round verifies current records and all retained roots, releases
old retained IDs after eight while checking a held snapshot remains valid,
closes/reopens the file, compacts, verifies reopening again, then makes a verified
backup and restore and checks both copies. It removes only its own temporary
backup/restore files after verification.

The shared Python `soak.execute` and `summary` functions require consecutive
round records, exact revision `2 + 65*r + max(0, r-8)`, retained count `min(r,8)`,
strictly increasing reader observations, consistent completion counts, bounded
core accounting and valid compaction lengths. A separate post-run review read
all 10,578 records and independently checked these relationships and the full
1,800-second duration. The Rust driver's full current/retained value oracles
remain the behavioral checks; consistent summary counters alone do not prove
record correctness.

## Source, tools and execution boundary

Host Mise Python 3.14.8 extracted the exact Git archive with `filter="data"`.
All 583 archived files were hashed, including original Cargo manifests and
lockfiles. Linux mounted the frozen source and dependency cache read-only and
built the unchanged soak package with `cargo build --offline --locked --release`,
`CARGO_INCREMENTAL=0`, and a fresh target on the private Docker volume. Source
hashes were checked before build, after build and after execution; post-copy
review rehashed all source files again. No lockfile was regenerated.

Immutable local image:
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
It supplied Rust 1.99.1 (`58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM
23.1.1), Python 3.11.2 and aarch64 Linux. `findmnt` confirmed **ext4** on the
private volume, backed by Docker's `/dev/vda1`. Build, temporary files and native
engine data used that volume, not a host-bind filesystem. The container had
networking disabled, 3 GiB memory, 256-process and two-CPU limits.

Installed container Python 3.11 lacks the required safe archive-extraction
support. Accordingly, a frozen private wrapper consumed the host-extracted
source and reused the archived shared `soak.execute`/`summary` and bounded
compatibility helpers. This qualifies the Linux Rust workload, observer and
oracles, **not Linux execution of the top-level soak archive launcher**. The
wrapper uses a 300-second build deadline, five-second RSS-probe deadline and
1,800-second run plus 120-second completion grace; the host orchestration has a
2,400-second limit. The complete run returned zero.

The qualification binds the engine through archived source and binary hashes;
it does not claim an additional runtime SOURCE_ID probe. The host launcher and
Linux wrapper are separately preserved and hashed. An initial wrapper attempt
passed a string where its digest helper requires a Path, failing before build or
workload. That diagnostic-only failure is retained separately under
`target/soak-7a9e661-linux-1800-initial-wrapper-failure`; it contributes no soak
time or successful evidence. The corrected fresh run is the result above.

## Artifacts and cleanup

Local evidence: `target/soak-7a9e661-linux-1800/` contains the original archive,
source inventory, wrapper and launcher identities, `docker-evidence.json`, and
`independent-verification.json`. Its `result/` contains the complete JSONL,
stdout/stderr, compiler/filesystem/build logs, executable, final live database
and `evidence.json`. Every copied result artifact was independently rehashed.
These local paths are not a permanent artifact-retention promise.

| Artifact | SHA-256 |
|---|---|
| Git archive | `94353aeeb3294c75a775234b4f2a5c8c9efcf1491538c06e57e45e398cca30f7` |
| Linux wrapper | `f4c84935f5b948694c865898a890d8f92e04111d1dbee0d490879e0e9d5a9f7e` |
| Host launcher | `aaba652b549f5913ca95c0d1d2b4d315d42c468369af1a64dff7baa8b84c5fef` |
| Native soak executable | `e57f095db880ca5439d80611ddf135d8ec8a65aa5918893a809e87aa64aba06f` |
| Linux result manifest | `c708528a8427f1409b835de896fb37a9fa14d3a03ce563831f48c92f37a5cca6` |
| Complete rounds JSONL | `4d9d83d55359757cb076da6a1df95051a3d67b96fcaed9b90f462067c7bdea07` |
| Docker/cleanup manifest | `90f48683a7b195a0865bd704989a4dd494627fb6dca07fe2495ed5a410474f2c` |

After copying results, the launcher removed only its UUID-owned container
`weekend-soak-be15fd476a1740f8a10ed6a7d556e53b` and corresponding `-data` volume.
Successful live-daemon inventory queries independently confirmed both absent.
Source stability, artifact copy verification, process exit and cleanup checks all
passed.

This extends the tested duration on this Linux configuration. It does not
qualify power loss, arbitrary native hardware/filesystems, network partitions,
replication deployment, Windows, or long-duration fleet behavior. No hosted CI,
publication, repository push or claim of full production readiness was involved.
