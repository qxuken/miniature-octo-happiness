# Private matched durable-history baseline

On 2026-11-12, a bounded local comparison completed with both engines preserving
the same current map and four durable historical maps through reopen, backup,
restore and compaction. SQLite was faster and smaller on this small workload.
These observations establish a reproducible baseline, not representative-scale
qualification, a latency guarantee or an engine-wide performance ranking.

The private harness is [tools/sqlite-baseline](../tools/sqlite-baseline/README.md).
It extends the retained-history scenario in `tools/benchmark.rs` and the monthly
driver's full historical oracle with a matched SQLite application schema. It
does not reproduce the monthly driver's typed indexes, CRDTs or replication.
No repository, crate, result or artifact was published and no remote job ran.

## Matched application contract

Each round starts with two new durable local files on the same filesystem.
Both seed 32 binary eight-byte keys with 256-byte byte-string values. There are
64 data transactions, each replacing four keys `(epoch*4+j)%32`, followed by a
separate durable pin operation after data epochs 16, 32, 48 and 64. Setup/seed
commits are outside the timed data samples. Every pin is counted and timed;
there are 68 timed durable logical operations per engine per round, not 64.

The value contract is explicit: the first eight bytes encode the last-write
epoch, the next eight encode the key, both little-endian; remaining byte i is
`(epoch*17+key*31+i)%251`. Seed values use epoch zero. Retained IDs are exactly
1 through 4, with the four epoch mappings above. The current epoch is 64.
The end-of-run current map equals the last retained map, but the last pin remains
an independently persisted retention entry. All four entries survive maintenance.

SQLite uses a `WITHOUT ROWID` table keyed by `(binary_key, logical_epoch)` with
the same byte-string payloads, a current-epoch row and an explicit retained-root
table. A historical map selects the latest version of each key at or before its
epoch. It writes **every** replacement version; history is not represented by an
unpersisted open reader or omitted from file accounting. Before compaction there
are 288 version rows: 32 seed rows and 256 replacements. Current/retained states
require 128 distinct versions after garbage collection. Unpinned obsolete versions
remain reclaimable garbage until explicit compaction, like obsolete append records
remaining in the weekend-db file before its compactor runs.

Weekend-db uses its native collection, byte values and retained snapshots.
Collection creation and seeding establish revision 2; data commits and pins each
advance its physical revision. The retained revisions are 18, 35, 52 and 69;
current revision is 70. SQLite's logical epoch intentionally does not pretend to
be the same physical revision or storage format. The Rust driver checks these
weekend-db revisions as well as the retained-ID contract.

Both engines use a single writer with no concurrent clients. SQLite executes
`BEGIN IMMEDIATE`, version inserts, a head update and `COMMIT`; each retention
insert has its own transaction. Weekend-db begins a draft, replaces four records,
commits, and separately calls retain_snapshot on pin epochs. They implement the
same application result through different metadata/index/publication machinery.

SQLite settings are explicitly set, queried back and recorded: WAL,
`synchronous=FULL`, `fullfsync=ON`, `checkpoint_fullfsync=ON`, page size 4096,
cache size -2000, mmap size 0 and autocheckpoint 1000 pages. SQLite documents a
WAL sync per FULL commit; its fullfsync option selects F_FULLFSYNC on macOS.
The weekend-db native adapter uses its normal F_FULLFSYNC publication protocol.
These are durable configurations conditional on the OS/filesystem/device contracts,
not matched counts of sync calls or measured power-loss survival.
[SQLite PRAGMA documentation](https://www.sqlite.org/pragma.html#pragma_synchronous),
[fullfsync](https://www.sqlite.org/pragma.html#pragma_fullfsync).

## Measurements and oracle

Three fresh-file rounds alternate execution order: weekend/SQLite,
SQLite/weekend, weekend/SQLite. No OS cache eviction occurs. Reopen samples run
after a clean close with a warm OS cache; they are not crash recovery or cold
storage measurements. Process startup, Rust compilation and payload generation
are outside the transaction samples. The clocks measure elapsed wall time.

| Metric | Exact boundary |
| --- | --- |
| commit | Durable commit API/SQL COMMIT call to return; transaction preparation is outside this interval. |
| transaction | Begin, prepare/apply the pre-generated writes, and durable commit through return. Allocation/binding and API overhead are included. |
| pin | Full separate durable retention operation, including its preparation. |
| reopen | Open/configure the database and acquire current/retention metadata. Weekend-db's open eagerly recovers its graph; SQLite initializes its pager/schema and reads metadata through queries. |
| reopen + full oracle | Same start as reopen, ending after every current and retained key/value is checked. This also exposes SQLite's deferred read cost. Clean close occurs after the interval. |
| verified backup/restore | Source full logical verification, fresh-destination copy API, destination full logical verification; SQLite additionally runs integrity_check and explicit destination file/directory sync. |
| verified compact | Native offline compact plus full logical verification, versus SQLite latest-version-per-live-root GC transaction, VACUUM, truncate checkpoint, close, integrity_check/full oracle and explicit destination sync. |

Rust independently calculates each expected key's last write by backwards search;
Python replays updates forwards and saves complete expected maps. Every key and
payload byte, row count, retained ID/epoch and weekend-db physical revision is
checked after normal execution, on every reopen, and after backup, restore and
compaction. Complete-state FNV-1a digests must match across both engines and the
Python oracle: `14964336404429242912`. The digest summarizes the checked bytes;
the byte-for-byte oracle checks, rather than hash collision resistance, establish
the assertion. SQLite integrity_check additionally runs on copies and compacted
files. Checks fail the run; they are not silently excluded from the result.

File sizes are logical file lengths from stat, not physical allocated blocks,
write amplification or device traffic. SQLite main, WAL and SHM lengths are
captured while the writer is open, after an explicitly timed truncate checkpoint,
and after close. The checkpoint result must be `(0,0,0)`. WAL is a persistent
part of the database state while present, so main-file-only live sizes are not
used as total storage. SQLite also documents that checkpointing adds a distinct
operation to WAL use. [SQLite WAL documentation](https://www.sqlite.org/wal.html).

Backup/restore timings are API diagnostics, not strict speed comparisons:
weekend-db's copy API verifies and compacts reachable current/retained graphs;
SQLite's backup API copies the version table including obsolete rows, which are
only pruned during compact. The formats, checks and garbage handling differ.
Both backups and restored files must expose the same current and pinned states.
Copies are quiescent; online concurrency is not tested. SQLite uses its backup
API rather than copying a live WAL main file. VACUUM is used for in-place
compaction after explicit application-history GC.
Weekend-db's restore API preserves native database identity/revisions/retained
IDs for replacement of the source lineage. SQLite copies the application epoch
and retention catalog without an equivalent weekend-db native identity. This
workload tests retained IDs and readable states; it does not test replicated
fork identity or concurrent operation of restored copies.
[SQLite backup API](https://www.sqlite.org/backup.html),
[VACUUM](https://www.sqlite.org/lang_vacuum.html).

## Observed local results

Final evidence: `target/sqlite-baseline-2490631-reviewed/evidence.json`, SHA-256
`e3d318a8d4033555c557069d1f7fab9bc0c1d41172f4ef9ebbb0bcb3093bdd7c`.
It reports passed=true, raw samples and source/toolchain/options bindings.
Earlier smoke/exploratory runs are separate artifacts and are not pooled below.
Host: macOS 27.0.1 arm64, Mac15,7, 38,654,705,664 bytes RAM, 12 logical CPUs;
both engines ran on the same local APFS solid-state volume `/dev/disk3s5`.
Host hardware/filesystem observations were read separately with sysctl/diskutil;
platform/machine identifiers are also in the evidence. Other local work was not
excluded or CPU-isolated.

Quantiles use nearest rank on pooled raw samples. Times below are milliseconds.
The 12 pin observations and 96 reopen observations provide very limited tail
resolution: p99 is the maximum for those sample counts. There are no confidence
intervals or independence/stationarity claims.

| Metric | Samples/engine | Weekend p50 / p95 / p99 | SQLite p50 / p95 / p99 |
| --- | ---: | --- | --- |
| commit | 192 | 9.055 / 10.071 / 10.986 | 2.988 / 3.918 / 3.992 |
| transaction | 192 | 9.069 / 10.088 / 10.997 | 3.014 / 3.941 / 4.012 |
| separate pin | 12 | 8.928 / 9.485 / 9.485 | 3.000 / 3.740 / 3.740 |
| reopen | 96 | 5.803 / 7.503 / 10.712 | 0.324 / 0.723 / 1.314 |
| reopen + full oracle | 96 | 6.008 / 7.777 / 10.934 | 3.050 / 5.254 / 8.787 |

Per-round upper-middle commit samples for weekend-db were 9.002, 9.077 and
9.886 ms; SQLite's were 3.014, 3.067 and 2.866 ms. That variability is retained
in the evidence and prevents treating these results as stable service bounds.

Sizes were identical across the three rounds:

| Stage | Weekend bytes | SQLite main / WAL / SHM bytes |
| --- | ---: | --- |
| Writer open after writes/pins | 1,765,376 | 4,096 / 1,491,472 / 32,768 |
| After SQLite truncate checkpoint | not a separate weekend operation | 114,688 / 0 / 32,768 |
| Closed | 1,765,376 | 114,688 / 0 / 0 |
| Backup and restored file, each | 565,248 | 114,688 / 0 / 0 |
| Compacted, four retained entries | 565,248 | 53,248 / 0 / 0 |

SQLite's live main+WAL+SHM sum is 1,528,336 bytes. Every final file keeps four
retention entries; no history was released to produce the compact sizes.
SQLite checkpoint durations were 4.266, 3.377 and 3.022 ms, measured separately
from commit. They remain part of the workload's maintenance cost.

Single maintenance samples per round, in execution-round order:

| Operation | Weekend ms | SQLite ms |
| --- | --- | --- |
| Verified backup | 28.974 / 27.924 / 30.894 | 22.000 / 21.232 / 22.030 |
| Verified restore | 28.905 / 27.019 / 30.087 | 20.087 / 20.672 / 18.743 |
| Verified compact | 41.819 / 48.049 / 42.993 | 19.850 / 21.141 / 21.238 |

## Reproduction and binding

Use the README command with a new output path; existing files, directories and
dangling symlinks are rejected before any SQLite open. Invalid workload bounds
reject before creating output. Existing backup destinations are never overwritten.
Failed runs leave their newly created private output directory for inspection.
The five Python tests exercise these rejection/no-write cases, a historical
GC/VACUUM/reopen/backup/restore oracle with uneven key cycles, and negative
controls for malformed short/extended keys, corrupted payloads and a wrong
retained epoch. All five passed both normally and under Python `-O`;
strict offline/locked Clippy passed for the isolated Rust harness.
Production checks use explicit failures, so optimization cannot remove either
the oracle checks or PRAGMA/checkpoint side effects. Independent review found
and closed the malformed-key normalization gap before the final run: the SQLite
oracle now requires eight-byte keys and unique decoded keys, matching Rust's
key-shape invariant. Exact live-version count is also checked after history GC.

The final engine is runtime `2490631dab43678279f8ba2c56458c8ea908c1b4`, compiled
offline from a Git archive with SHA-256
`8060e8e4178b2aa34459b05df8a722f7af977b7fb2d4f8d0d2649b8f7c4fba8d`.
The driver asserts runtime source ID equality and feature bits zero. It uses
Mise Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), release with overflow checks enabled
as in the engine's release profile, and an isolated Cargo.lock/target directory.
No Cargo registry dependencies are used by this base-feature harness. Ambient
Rust flags/wrappers/profile overrides are removed from the child build environment.
The current harness is copied into the archive and hashed separately; it is not
claimed to have existed in the older engine revision.

Python was Mise 3.14.8 with its actual linked SQLite 3.53.1, source ID
`2026-05-05 10:34:17 c88b22011a54b4f6fbd149e9f8e4de77658ce58143a1af0e3785e4e6475127e9`.
SQLite compile options and queried PRAGMAs are recorded per round. Python's
sqlite3 binding overhead and Rust's native API overhead are included at their
respective timing boundaries; this is not an equal-language microbenchmark.

| Bound artifact | SHA-256 |
| --- | --- |
| Harness Cargo.toml | `de3045b1ac4196655e790323c55f555e6b062fcb8d7c4089c7e70105b7e3af34` |
| Harness Cargo.lock | `45950780752628779054ec611b9aaa8797b5c3a8c223aef2579512f26053d48b` |
| Rust driver | `67c8d662b082aa11b5d34658f83c7bcc2f79c2477c1950e5480a86ff19225855` |
| Python coordinator | `0c3ee03fbc840ace8e5c6feecb0cff2ca54b366a5f01362d084461ce616efa87` |
| Python tests | `9130044f9e3dedf7a17203d7108e3ba92d018308e681ec755d2f57d14484e287` |
| Rust executable | `88ee8ab28238e9f6b6b312e7c310ad3e6ccbdde834df199e3a2dfc5aa18b7305` |

Independent root-agent comparability review accepted the application protocol
subject to exact epoch/ID checks, persisted obsolete-version cost, separate pin
operations, same-host sync settings, full independent oracle, WAL/SHM accounting
and the limitations above. A separate reviewing agent then inspected the actual
driver/coordinator, reproduced the malformed-key issue, reviewed its fix, ran all
five tests with Python `-O`, and completed an independent cross-engine `-O` smoke
with eight commits, five keys, batch three, three pins and two reopens per engine.
Its `target/sqlite-baseline-independent-review/evidence.json` reports passed=true
and digest `11366097594411976088`; the reviewer found no remaining actionable
finding. That independent smoke has its own workload/source bindings and is not
pooled into the performance samples. This report does not close larger-state/history,
concurrency, cold-cache, sustained throughput, long-duration, other-platform,
crash/power-loss, alternative SQLite-schema/tuning or actual storage-device gates.
