# Bounded native soak

This private, unpublished driver exercises a real native database in one
long-lived process. It is an opt-in qualification tool, not a default test:

```sh
cargo xtask soak --output /absolute/path/to/NEW-output-directory --seconds 600
```

Git and the pinned Rust toolchain are required. The driver exercises native
file compaction on macOS and Linux.
The output directory must not exist and its parent must exist. The runner writes
only into that directory, builds offline with a lockfile and incremental builds
disabled, and never deletes caller files. Cargo's existing dependency cache is an
input. No registry publication occurs.

The runner archives committed HEAD by default. Use `--revision COMMIT` for
another frozen engine. It copies the driver's Cargo.toml, Cargo.lock and main.rs
into the archive and hashes the overlay separately. `evidence.json` identifies
the engine commit, archived files, fixture, executable, OS, architecture,
filesystem, toolchain and commands. The archive and build stay in the output.
The controller checks source and binary hashes again after execution.

Each round performs:

1. Open with a 16 MiB live core allocation limit. Two synchronized writer threads
   each commit 32 changes to their own two keys. Two concurrent readers repeatedly
   use one snapshot to assert each pair has the same counter; each counter remains
   within the independently known old/new range. The ending oracle is exactly 32
   increments per writer, with exactly four primary keys.
2. Verify a snapshot pinned before writes still has the old counters. Retain the
   new root, keep the newest eight roots, and release the oldest while a reader
   still holds it; that reader must continue to observe its original values.
3. Verify every current/retained counter, revision and retained ID, then drop all
   database handles and snapshots. Compact, reopen, and repeat the complete oracle.
4. Create a verified backup and restore it to another new file. Open both and
   repeat the same full oracle, then remove only those two harness-owned files.

The process survives across rounds; database handles deliberately close for
maintenance. Fixed keys and eight retained roots bound logical history. Each round
compacts rather than letting append history grow for the full run. The driver
asserts that compaction does not grow the file and pre-compaction length remains
below 32 MiB. It logs both lengths and live/peak core bytes each round. A successful
run reports consecutive round records and a final `complete` record. The
launcher independently checks each expected revision (`2 + 65*r + max(0,r-8)`),
retained count, strictly increasing read progress, monotonic elapsed time, core
accounting and file bounds, and matching final counts. A completion flag alone
is insufficient. A nonzero exit, malformed/inconsistent output or timeout fails
and preserves `verified: false` with terminal status and failure context. A lower round bound may finish before the requested
wall duration. When the child finishes below that round bound, the controller
requires both the final child clock and its own observed runtime to reach the
requested duration. Round duration may overshoot the deadline; the runner enforces a
120-second grace and kills a stuck child. Defaults allow at most 100,000 rounds;
accepted wall duration is 1–86,400 seconds. At four tiny records this is a bounded
small-state lifecycle/concurrency workload, not a throughput benchmark.

The driver now records durations per round for the concurrent writer/reader
group, compaction, reopening and verifying the compacted file, backup and
restore. It also records the full round duration, which includes retention
and verification of the backup and restored files. The launcher checks
that stage durations fit within their round and that round durations fit
within the driver clock. Evidence includes sample count, zero-clock ticks,
p50, p95, p99 and maximum nanoseconds for each stage using nearest rank.
The concurrent duration describes a batch of 64 commits with readers
running. Individual commit and queue times are outside this measurement.

`sampled_peak_rss_kib` is the maximum child RSS observed at approximately 200 ms
intervals through native process telemetry. The controller retains the peak
and sample count. It can miss intermediate
peaks, excludes build/launcher memory, and is a measurement, not a cap. The 16 MiB
cap applies to requested core allocation layouts, excluding backend/application
allocations, allocator overhead, foreign handles and the budget control block.
Core peak accounting resets when the database is reopened; process RSS continues
across all rounds and can expose cross-lifecycle growth. Neither a small finite
run nor flat samples prove the absence of leaks.

This exercises normal filesystem completion and concurrent in-process use. It
makes no process-interruption, device power-loss, simulated-month, multiple-process
writer, network replication, large-dataset, total-RSS-bound or production-lifetime
claim. Separate crash/fault, native interruption and platform qualification remain
necessary. Evidence stays in caller-selected output directories rather than being
committed as bulky artifacts.

## Recheck extracted timed evidence

Preserve the output directory, including its archived source and
`build/release/weekend-native-soak`. After a fresh extraction, run:

```sh
cargo xtask soak-verify /absolute/path/to/extracted-evidence
```

The verifier checks the extracted files, executable, allowed process
outcome and both completion clocks. It recomputes the timing percentiles
from the raw JSONL records and compares both the timing and completion
summaries to the evidence. Proofs produced before per-round timing do
not have the fields required by this command. Check the archive's
recorded SHA-256 before extraction to bind it to its original evidence.

## Recorded qualification

On macOS 27.0.1 arm64, rustc 1.99.1 / LLVM 23.1.1, frozen engine
`4203e0aab9b48ed371165adaca694400f38f84cc` passed a 600-second requested run.
The process completed 924 rounds in 600.402 seconds, 59,136 paired-key writer
commits, 448,149 reader iterations, and ended at revision 60,978. Every round
checked current and all retained roots after native compaction, backup and restore;
916 rounds also released a retained root while a pinned reader survived.
The runner exited zero with `verified: true`, and stderr was empty.

After reaching eight retained roots, measured post-round live core bytes stayed
at 22,720; per-open peaks ranged from 50,288 to 51,248 bytes (concurrent allocation
interleavings vary). The pre-compaction file stayed at 917,504 bytes and compacted
file at 122,880 bytes. Sampled process peak RSS was 4,096 KiB. These are observed
bounds for this finite workload, not general capacity or leak guarantees.

The driver source SHA-256 was
`566cd6367494efaec1c81955f5c0d1ca8ecc7f5dd0088895f32a493cdf7efb12`;
the exact tested binary SHA-256 was
`80310d24bdaa22def45f56455a4471cba8760b38403589c48e6283c6b6496e63`.
Local full evidence: `/tmp/weekend-soak-600s-4203e0a/evidence.json` and adjacent
`rounds.jsonl`, `stderr.log`, `build.log` and archived source. The earlier final
12-round smoke passed in 11.376 seconds with 19,920 reader iterations. Driver
Clippy passed with warnings denied; launcher existing-directory and invalid-duration
guards preserved the existing sentinel and did not create invalid output.


## Launcher corrections and short control at 032dadf

Independent review reproduced three weaknesses in the original launcher:
a dangling output symlink redirected creation, a zero-exit child printing only
a completion marker was accepted, and malformed output lost terminal failure
metadata. The corrected launcher rejects existing final components before
resolving parents, persists failure metadata from setup onward, and checks the
independently predictable sequence above. These corrections do not alter the
Rust workload or database implementation.

The launcher reuses the reviewed compatibility helper for exclusive private
output creation, bounded commands and process-group cleanup. Setup/probes wait
at most 30 seconds, the build 300 seconds, and RSS probes five seconds. Runtime
retains its requested duration plus 120-second grace. Failure terminates the
live child process group and reaps the direct child with a bounded wait. Known
ambient build overrides are removed; compiler/Cargo configuration remain trusted
inputs. Builds are locked/offline. The retained archive, all five overlay hashes,
source snapshot, binary and output-log hashes bind successful evidence. There
is no hosted CI or publication action.

Eight controls pass normally and under Python -O: valid short/retention-release
summaries, inconsistent/type-invalid records, existing/dangling outputs, setup
failure metadata, a real complete child, real malformed/incomplete/failed
children, RSS failure and RSS timeout cleanup. Independent whole-main controls
also reject the original two zero-exit cases, admit a valid sequence and reject
a changed archived source. These use harmless temporary fixtures, not production
runtime changes.

A real 12-round macOS control against frozen runtime
`032dadf591b0a0ded4894898e8208283c50425fb` plus explicit launcher overlays passed:
768 paired-key writer commits, 9,445 reader iterations, verified revision 786,
and all current/retained/backup/restore oracles. Wall time was 7.406 seconds,
sampled RSS peak 3,664 KiB. Evidence: `target/soak-harness-032dadf-smoke/evidence.json`
and adjacent logs/source. These observations qualify the corrected launcher
for this short case, not a long-duration or different-platform soak.

The maintained equivalents run with `cargo test --offline --locked -p xtask
--bin xtask soak` and the shared supervisor's sampler-failure controls.


## Extended macOS and Linux controls at 7a9e661

The corrected committed launcher and unchanged Rust workload also passed a full
1,800-second requested macOS/APFS run at exact
`7a9e6611eaee911ae7477149cbcafaada8a65e47`. It completed 1,941 rounds,
124,224 paired-key writer commits and 1,982,416 reader observations in
1,800.433 seconds of Rust time (1,800.872 seconds observed). Final revision was
128,100, with eight retained roots. Every round performed the complete native
current/retained, compact/reopen and backup/restore oracle.

Independent post-run review checked every round, exact publication arithmetic,
all 583 archived source hashes and complete log hashes. Maximum recorded live
core bytes were 22,720 and core peak 51,248. Sampled process RSS peaked at
4,032 KiB. Pre-compaction/compacted lengths stayed within 917,504/122,880 bytes.
These are finite tiny-record workload observations, not generic resource caps,
throughput comparisons or absence-of-leak proof. Concurrent development work
also used the same host.

Evidence: `target/soak-7a9e661-mac-1800/evidence.json`, SHA-256
`fb48cbcab5be8660e8372d8282cacc02389df14508e9f01d6c3acf436dc31f7b`;
full JSONL SHA-256
`8cfd669174ddcf61786a987b710f2ac670a201814eb129d0df6ff66345479f83`.
The adjacent independent-verification.json records all observed maxima and
counter/source checks. Source/tool stability and terminal exit zero passed;
stderr was empty. Mise Rust 1.99.1 and Python 3.14.8 were used on macOS 27.0.1
arm64. This run was not shortened by the 100,000-round limit.

A separate [Linux ext4 1,800-second run](soak-linux-7a9e661.md) used the same
unchanged archived Rust driver and shared observer, completing 10,578 rounds,
676,992 writer commits and 5,813,826 reader observations. Its build, temporary
files and native databases were on a private Docker ext4 volume. The Linux
profile explicitly uses host-safe extraction rather than running the top-level
archive launcher under the container's older Python. Both records preserve
their own tool, filesystem and resource boundaries. Neither qualifies Windows,
physical-device power loss, network replication, larger cardinality or a month
of wall-clock production operation.

## Workload checkpoint at 5f49a59

Two 600-second runs used Rust 1.100.0 and the same committed controller,
driver and engine (`5f49a590bd7048e5ba6dea79c5544eb7e7264b70`). They ran
concurrently on one development host. Each round checked the current and
retained views after compaction, reopen, backup and restore.

| Observation | macOS arm64 | Docker Linux aarch64, ext4 volume |
| --- | ---: | ---: |
| Driver / observer elapsed seconds | 600.738 / 601.054 | 600.007 / 600.023 |
| Completed rounds | 701 | 3,202 |
| Paired-key writer commits | 44,864 | 204,928 |
| Reader observations | 1,788,323,587 | 2,131,891,361 |
| Final revision / retained roots | 46,260 / 8 | 211,326 / 8 |
| Maximum live / peak core bytes | 32,504 / 73,616 | 32,504 / 73,424 |
| Sampled peak RSS (KiB) | 3,264 | 2,880 |
| Maximum before / after compaction (bytes) | 917,504 / 122,880 | 917,504 / 122,880 |

Independent review of each freshly extracted local proof checked both clocks,
the full sequence of revisions, retention and resource bounds, all 858 source
hashes, 38 output hashes and the separately saved executable. The SHA-256 of
`soak-macos-5f49a59.tar.gz` is
`d67c154dbf28b1ce1c4884ae977c474bfd9ef3d28f7e4700fe089e0e22fac733`.
For `soak-linux-5f49a59.tar.gz` it is
`7896913439fa829d7438771198eee592966b8ea8bd8e590f26ba01c78175ea86`.

The controller now applies the two-clock duration check on future runs.
A short zero-exit child with consistent round records and a child claiming
60 seconds of work both failed the new controls. A real two-second macOS
run passed below the round limit. The deployment-device soak and agreed
operating budgets remain open.
