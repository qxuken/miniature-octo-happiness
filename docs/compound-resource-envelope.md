# Compound near-cap concurrency and recovery

Engine `c583ca7260207c3b6ad0e44fa60b738f172bf4cc` passes this finite combined
profile on macOS arm64/APFS and Docker Linux aarch64/ext4, 2026-11-14. It combines
real reachable data, all 64 retained roots at depth 64, concurrent drafts,
aggregate budget saturation, contended commits, readers and failure recovery.
No production source, format, durability barrier or configured engine limit was
changed. The new private harness uses public APIs and the existing simulator.

## Workload and checks

Three collections contain 48 independently allocated 896 KiB bulk values,
one evolving depth-64 value (63 singleton lists over an 8 KiB leaf), and eight
worker records with sequence/payload fields. Bulk bytes are shared by the 64
retained roots; each root has its own deep value. Retained IDs 1..64 capture
revisions 53,55,..179. Current revision is 180 before contention. The configured
requested-core cap is **96 MiB**; all other Limits remain default, including the
64 MiB image cap, 1 MiB byte-value bound and depth 64. The 65th retain rejects.

The native file starts at **63,401,984 bytes (94.48% of image capacity)**.
No synthetic padding is added. Eight actual threads prepare and hold two
distinct 1 MiB pending values each. A prepared valid candidate and additional
owned fillers saturate the shared core budget. Its commit returns `Limit`,
becomes terminal `Failed`, and a second commit returns `Closed`. Acknowledged
revision, all current/retained values and physical file length remain unchanged;
recovery is not required. Dropping drafts/fillers returns live core charge to
the exact pre-draft value, and another 1 MiB allocation succeeds.

Eight writer threads then prepare disjoint updates in four barrier-controlled
rounds. All eight drafts exist before each round's commits. Two readers start
with the writers and check both fresh snapshots and a held retained snapshot.
Every sequence/payload pair must be internally consistent; retained values stay
at sequence zero. All **32 commits** succeed with unique consecutive revisions
181..212; readers complete 256 measured snapshot/oracle iterations.

The file reaches **66,154,496 bytes (98.58%)**, leaving 954,368 bytes. Preparing
four distinct 1 MiB leaves in a list establishes a physical lower bound above
that headroom; publication rejects with `Limit`, preserving length and state.
Close/reopen and a separate fresh process repeat the complete oracle. Quiescent
backup at a 96 MiB copy budget rejects with an empty destination and source
bytes unchanged. Fresh backup, restore and compact-copy paths succeed at an
explicit **256 MiB copy budget**, with all current/retained values checked again.
Their 63,074,304-byte files retain the complete selected graph; 256 MiB is a
tested budget, not a minimum requirement. Maintenance runs after drafts close.

All 48 bulk values are compared in full in current state and every retained
root. Every retained revision, row count, deep list length, leaf byte, worker
sequence and worker payload is checked. Actual current bulk bytes, all deep
leaves, current worker payloads and retained worker sequences are fingerprinted
with FNV-1a and checked by a separate Python byte generator. Retained bulk/worker
payloads also undergo full Rust byte comparisons; they are not accepted from
summary counts alone. FNV is an accidental-error oracle, not authentication.
The coordinator rejects omitted/reordered phases, wrong bytes/revisions, missing
draft/read/commit samples, over-cap telemetry and changed rejection lengths.

## Memory observations

Requested core layouts match on both platforms:

| Phase | Live core bytes | Peak in that open domain |
|---|---:|---:|
| Reopened compound fixture | 47,933,448 | 48,440,202 |
| Eight held drafts plus prepared candidate | 64,780,404 | 64,780,916 |
| Saturated candidate rejected | 100,596,648 | 100,663,189 |
| Drafts/fillers dropped | 47,933,448 | 100,663,189 |
| After 32 commits | 50,065,480 | 100,663,189 |
| Reopened after contention | 48,484,598 | 48,978,650 |

The cap is 100,663,296 bytes: peak admission remains **107 bytes below it**.
The rejected candidate clears its pending writes before the reported live
sample. Peak is cumulative within an open Memory domain, not a per-phase peak.
Reopening reclaims obsolete serialization-cache ownership while keeping all
64 retained roots. Copy/verification APIs have separate internal domains; the
reopened destination's accounting does not expose their peak allocations.

| Process observation, KiB | macOS | Linux |
|---|---:|---:|
| RSS at saturated rejection | 110,624 | 110,528 |
| RSS after drafts/fillers drop | 110,512 | 71,536 |
| RSS after compact + reopen/oracle | 456,352 | 112,008 |
| Sampled native-lifecycle maximum | 456,352 | 173,580 |
| Separate fresh-inspector maximum | 51,712 | 51,224 |
| Separate simulator-recovery maximum | 748,592 | 376,432 |

Native, inspector and simulator workers are distinct processes. Simulator RSS
includes the input image and volatile/durable backend copies outside the core
budget. Native RSS includes caller/oracle buffers, allocator retention and copy
domains; it excludes the Python coordinator and kernel/device caches. The
macOS process retains much higher RSS after repeated copies despite low current
core charge; this is not evidence of a core ownership leak. Sampling at roughly
50 ms (plus probe cost) can miss peaks. Docker's 2 GiB containment setting and
the requested core cap are not whole-process RSS guarantees.

## Finite latency observations

Milliseconds, final v4 runs; nearest-rank summaries of the indicated samples:

| Operation | Samples | macOS min / median / p95 / max | Linux min / median / p95 / max |
|---|---:|---|---|
| Commit API under writer contention | 32 | 7.334 / 36.778 / 71.761 / 72.694 | 0.646 / 3.136 / 6.608 / 7.213 |
| Snapshot plus full current/pinned cell oracle | 256 | 0.916 / 0.931 / 1.015 / 72.793 | 0.920 / 0.944 / 1.221 / 7.459 |

Commit time includes waiting for publication synchronization and native flushes,
but excludes draft construction. Reader time includes constructing expected
payloads and comparing bytes; it is not a point-read API microbenchmark. Four
rounds do not establish a service latency distribution or p99 guarantee.

| One operation, ms | macOS | Linux |
|---|---:|---:|
| Native reopen | 173.254 | 165.352 |
| Budget-rejected backup | 231.701 | 231.151 |
| Successful backup | 581.818 | 794.812 |
| Restore | 594.825 | 769.362 |
| Compact copied file | 846.388 | 949.931 |

Operation timers exclude the subsequent independent oracle; successful copy
timers also exclude the destination reopen. File copies used to prepare the
compact input are outside its timer. OS caches are warm. Mac/Linux runs and
some helper checks overlapped, and Docker shares host resources: these are
observations, not controlled platform rankings or deployment SLOs. Native
worker lifecycles took 7.805 / 6.466 seconds; each includes full Rust oracles.

## Directed recovery under compound pressure

The separate simulator worker starts from the 98.58%-full native image. Two
additional 1 MiB drafts remain live alongside the candidate, with real owned
fillers taking core live usage to **91,025,020 bytes** before each attempt.
The successful trace supplies actual append/root/witness barrier positions.
The probe changes writer zero from sequence 4 to 5. Every reopen checks the
entire current graph and all retained roots; other writers remain at 4.

| Case | Commit result | First reopen | After retry/later acknowledged commit and another crash/reopen |
|---|---|---:|---:|
| First core allocation fails | `OutOfMemory` | 212 | 213 |
| Append sync fails, pending bytes dropped | `Io` | 212 | 213 |
| Append sync persists bytes but fails | `Io` | 212 | 213 |
| Root sync fails, root dropped | `OutcomeUnknown` | 212 | 213 |
| Root sync fails, root survives | `OutcomeUnknown` | 213 | 214 |
| Witness sync fails, pending witness dropped | `OutcomeUnknown` | 213 | 214 |
| Witness sync persists but fails | `OutcomeUnknown` | 213 | 214 |
| Commit acknowledged, pending bytes dropped | `Ok(213)` | 213 | 214 |

Preparation OOM does not poison the database. I/O/uncertainty leaves the
in-process acknowledged head at 212, requires recovery and blocks the other
pending drafts with `RecoveryRequired`; no follower acknowledgment is involved.
Interrupted append bytes may remain as unreachable tail, without publishing a
new root. A surviving selected root is fully verified, not replaced by an older
root to hide damage. The eight cases are directed scenarios, not an exhaustive
allocation/I/O or second-failure sweep. Native fresh-process reopen is real;
the crash/persistence policies above are simulated, not native SIGKILL or device
power-loss evidence.

## Provenance, checks and remaining scope

Both final v4 runs archive the explicit engine commit and overlay six named
private harness files. Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), locked/offline
release, overflow checks, source ID and feature bits 4 (fault injection) are
verified. macOS 27.0.1 build 26A434 uses arm64/APFS; Linux aarch64 uses a private
ext4 volume on `/dev/vda1`, kernel 7.0.14-linuxkit, immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
Docker has no network, two CPUs and 256 PID slots. Host Python is Mise 3.14.8;
the native Docker observer uses image-pinned Python 3.11.2. Cargo configuration
hashes and removed ambient build-override names are recorded; configuration is
trusted. Binaries, frozen sources and copied evidence are rehashed after use.
Docker container and volume absence and native owned-group retirement pass.

Five new Python controls pass normally and under `-O`; the complete POSIX
registry passes **328 tests plus four registered skips**, 332 reported across
44 workers. Legacy observer controls remain passing. The same eleven focused controls
also pass on Linux with Mise Python 3.14.8 in normal and optimized modes
(20 passes plus two explicit optional-evidence skips across four processes). Mac targeted library,
allocator, budget, durability, compaction, headroom, retention and concurrency
regressions pass **183 tests in each debug/release profile**. Driver all-target
release Clippy with warnings denied, formatting and whitespace checks pass.
All 19 Cargo packages remain unpublished. No production source changed.

Review strengthened the shared observer: it accepts a profile validator,
rejects logs reaching 8 MiB before JSON parsing, and retires descendants even
after leader exit. `waitid(WNOWAIT)` keeps the owned leader PID reserved until
group retirement, avoiding a PID-reuse signal race. On this macOS kernel a
zombie-only group returned `EPERM`; the observer accepts that only after proving
leader exit and no live group members with a bounded process-table query.
Timeouts still retire the group. Controls exercise actual descendant exit,
unreaped leader ownership and exact/overflow output limits. Earlier trial and
v1/v2/v3 runs retain their original scopes/failures; only v4 binds the final
supervision code. Each worker has a 180-second deadline; Docker has a separate
600-second outer deadline, with partial logs copied before cleanup. Periodic
output checks are not a hard disk-write ceiling, and no watchdog bounds a hung
kernel. Failed outputs are retained; fresh paths are required for retries.

Final evidence is local/ignored under `target/compound-{mac,linux}-final-v4/`.
The compact index `target/compound-qualification-index.json` binds both reports,
source/binary/artifact hashes and control/regression logs; SHA-256:
`37dce35c96f60796dba301fd4e911ae35dff909fc672fa3e68079fee66f839b1`.
This is not permanent hosted evidence, an automatic native CI workload gate,
or a final immutable release candidate. The helper controls are registered for
CI; execution of this full workload remains explicit.

This closes one combined finite resource profile. Remaining work includes
longer/larger concurrent schedules, other breadth/DAG/transaction bounds,
systematic compound allocation and repeated-recovery faults, interrupted
near-cap maintenance, delivered/network/Windows profiles, cache-copy growth,
owner latency/RSS budgets and deployment device assumptions. Physical power
loss and Linux server x86_64 qualification are not inferred from this Docker
aarch64 run. The [roadmap](production-acceptance-audit.md) retains those gates.

The subsequent [buffer-growth comparison](serialization-buffer-growth.md)
reruns this same compound workload on fe4d057 and 742079c, preserving exact
lifecycle/history bytes and all roots/recovery outcomes. It measures allocator
work and copy-domain peaks alongside clean latency/RSS, including the charged
cache-slack tradeoff. Figures above remain the original c583ca7 checkpoint.

The [near-cap interruption/recovery follow-up](near-cap-interruption-recovery.md)
reuses this fixture for 52 native maintenance kills and 108 repeated-recovery
cases per Mac/Linux platform. It verifies complete current/retained bytes and
later successful retry in fresh processes. Every source/target sync is covered;
large I/O is sampled. Earlier eight pressure cases, new composed failures and
namespace/device persistence assumptions retain their separate scopes.

## Repeated near-cap maintenance

```sh
cargo xtask resource compound-cycles --revision af91f05 --output target/resource-cycles
```

Use `--docker-image` with the recorded immutable Linux image to repeat the
Docker ext4 profile. The controller first runs the original compound workload,
independent reopening and simulator cases. A new process then copies the
verified compacted fixture and continues for 24 cycles. Each cycle prepares
eight simultaneous drafts in four rounds, checks 256 current and pinned
reader observations, rejects a commit larger than remaining append headroom,
rejects a backup under the 96 MiB copy budget, and retries backup, restore
and in-place compaction with the 256 MiB copy budget.

All 768 contended commits received consecutive revisions. Each rejected
commit preserved physical length, and each failed backup preserved the full
source bytes and left an empty destination. The worker verified all current
and 64 retained roots in every phase and reopened each recovered or compacted
copy. The independent controller verifies all seven phase records for each
cycle, including revision arithmetic, byte digests and sample counts. It
also checks continuity with the previous compacted image and verifies that
only the canonical file remains. Both runs finish at revision 980, writer
sequence 100, and 63,074,304 bytes. Post-write images reach 65,826,816 bytes
every cycle, about 98.1% of the 64 MiB cap.

| Observed metric | macOS arm64/APFS | Docker Linux arm64/ext4 |
|---|---:|---:|
| Cycle worker elapsed | 106.6 s | 95.1 s |
| Sampled peak process RSS | 1,548,256 KiB | 192,216 KiB |
| Peak requested core bytes | 57,794,708 | 57,794,708 |
| Commit p50 / p95 / p99, 768 samples | 44.5 / 79.8 / 210.3 ms | 2.77 / 4.59 / 5.25 ms |
| Reader p50 / p95 / p99, 6,144 samples | 0.934 / 2.10 / 79.3 ms | 0.907 / 0.940 / 4.91 ms |
| Reopen p50 / max, 24 samples | 158.4 / 196.5 ms | 146.9 / 162.9 ms |
| Backup p50 / max, 24 samples | 517.9 / 534.1 ms | 496.2 / 519.9 ms |
| Restore p50 / max, 24 samples | 519.5 / 553.8 ms | 488.4 / 515.0 ms |
| Compact p50 / max, 24 samples | 690.4 / 719.5 ms | 628.9 / 648.6 ms |

RSS encompasses the whole worker, including the file-comparison buffers and
copy domains. The core column measures the opened database's allocation
domain at the recorded phases. macOS RSS rose and fell during the run. The
post-compaction live core charge was 48,744,902 bytes in all 24 cycles on
both hosts. The allocator and copy contributions to the macOS peak remain an
open measurement. Timings are finite observations in the stated environments.

| Platform | Controller proof SHA-256 | Worker proof SHA-256 |
|---|---|---|
| macOS | `cb3490625ee7c74f050a7d2c45b96950fe75c716a2f806b6cb1a1bad84c5dfb3` | `390766bd4779e4fa9928e5ba43e4aec8b6e5847837e6138c14fd0bb54178421e` |
| Linux | `2891c5036f0490614ad8f1808f8137252fa00e42e1f899f53b595ce205fe3271` | `ce5730ccfbeddfb18b1557edfcf8f1d813ec62dbf47e0decb4565fd73370aaee` |

The source-overlay inventories match, all 28 worker proof artifacts per
platform were independently rehashed, and Docker resources were confirmed
absent. The Rust controller tests reject omitted work, altered file length,
incorrect revisions and stale state digests.

A full repeat with the final controller passed again on both hosts, reaching
the same revision, retained state, file lengths and core peak. The macOS
worker's sampled RSS peak rose to 3,683,632 KiB (3.51 GiB). The Linux repeat
peaked at 191,060 KiB. This spread makes the macOS process-memory observation
a separate deployment gate from the configured core budget.

| Repeat platform | Controller proof SHA-256 | Worker proof SHA-256 |
|---|---|---|
| macOS | `e0d2fabf1de6dec72b6db7debc1445e46753c3d6b0ea6f81a261e148e114f030` | `6ec3f9adc9a9939927b9cc8c5cbf6936fc12dc8971d645ca765d92d0510aad29` |
| Linux | `c4f5317a76b5fd1172511b0c11b7420b0d27d408d30727acacb977d4646fbe17` | `0e9030f594e467c38baf17c679155775ec51babb0080ab377fdc5cdd6878bd37` |

### macOS allocator diagnosis and bounded verification buffers

A disposable copy of the frozen driver sampled Darwin allocator statistics
at the same phase boundaries. An 8 MiB test allocation increased the live
counter and releasing it returned the counter to baseline. The diagnostic
run matched the original revision, sequence, full-state digests and physical
length for all 168 phase rows. At open/compact boundaries the malloc in-use
counter stayed between 48,922,848 and 48,924,432 bytes while phase RSS
ranged from 84,592 to 1,480,848 KiB. A live `vmmap -summary` snapshot
recorded 706.9 MiB resident in `Malloc Large (empty)` regions and 997.0 MiB
total resident. This identifies empty allocator regions as the largest
component of that snapshot's RSS.

The maintained driver now compares the exact bytes across a failed backup
using an owned pre-failure copy and two 64 KiB buffers. It removes the copy
before retry. Both platforms again passed all 24 cycles, with unchanged
final state, physical length and recorded core peak. Sampled process RSS
was 3,499,504 KiB on macOS and 173,100 KiB on Linux. The macOS peak persisted
with the smaller verification buffers. Allocator retention under this
maintenance workload remains a deployment measurement.

| Artifact | SHA-256 |
|---|---|
| Diagnostic input inventory | `a477b035d492ad64ecb6514ea60eb84054212ea9f17c83d0e39d7edaf3518e04` |
| Diagnostic phase rows | `d9f2ec4bf2e9807c4118ecc2204ce2fed79f7595a01dd5eb8b7a5158a76c9577` |
| Diagnostic VM map | `728b8d8e1f103e59edf3bd73b524dfbf1692e9221d865c328701dcda85de89eb` |
| macOS bounded-buffer controller proof | `ee0233d4fac61d9887f1a3d06d43be044782acc5834fdc35cffb80367bebea52` |
| Linux bounded-buffer controller proof | `e0089a80b5ade00fd017b1d9e5bd8b3ca2f1906d6d0a9f9c71d48f3453b585b9` |

### Offline copy graph lifetime

Offline backup and restore now release the source serialization caches before
encoding the destination. Once the destination is durably published, they
release the input graph and the destination's cached graph references. The
independent destination recovery runs while both leases are held and checks
the complete current and retained graphs, identity, revision, committed end
and witness. A simulator regression observes the first destination header
read, checks the source lease is still exclusive, and checks the old graph's
allocations have been released. The previous implementation failed that
control with 1,129 charged bytes still live on the small fixture.

A disposable macOS probe instrumented the two implementations on the same
66,154,496-byte input from the bounded-buffer workload. The copy budget
was 256 MiB. All four backup and restore outputs were byte-identical and
63,074,304 bytes. Inspection confirmed the same identity, revision and 64
retained entries. Under the 96 MiB budget, both implementations returned
`Limit` and left an empty destination. The source SHA-256 was unchanged.

| Single backup + restore probe | Original lifetime | Shorter lifetime |
|---|---:|---:|
| Core live bytes before each destination verification | 49,010,118 | 0 |
| Core peak bytes across encoding and verification | 150,179,254 | 149,915,550 |
| Peak process RSS, macOS `/usr/bin/time -l` | 230,998,016 bytes | 185,876,480 bytes |

The encoding stage still sets the core peak for this input. The process RSS
figures are single-run observations, including the probe's inspections.
Input SHA-256 `691d199057cdf32493def408c3beeb03bc647200c1d2847adee1a812ead4a527`.
Output SHA-256 `4ef4eb245148021c8e243a0f03f60d746f9cdf42ad1004b6e894d5e82c0375c3`.

The all-feature engine tests passed in debug and release on macOS arm64
(311 per profile) and Docker Linux arm64 (315 per profile). Both profiles
report one worker fixture ignored by libtest and invoked by the native
process tests. Strict all-target Clippy passed on macOS and for the Windows
MSVC cross-target.

The exact `55e35ad` engine passed all 24 cycles on both platforms. Comparing
all 168 phase records against the bounded-buffer baseline matched the logical
digests, revisions, root counts and file lengths. Each fresh workload creates
a random store identity. For the exact byte check, release admin binaries
from `af91f05` and `55e35ad` copied the same frozen macOS source fixture. They
produced the same backup, restored bytes and independent inspection output.
The fixture retained its original SHA-256.

| 24-cycle workload | macOS arm64/APFS | Docker Linux arm64/ext4 |
|---|---:|---:|
| Worker elapsed | 107.2 s | 95.8 s |
| Sampled peak process RSS | 1,413,968 KiB | 127,044 KiB |
| Peak requested core bytes in the opened database | 57,794,708 | 57,794,708 |
| Backup p50 / max | 521.0 / 569.3 ms | 491.4 / 531.0 ms |
| Restore p50 / max | 530.3 / 562.3 ms | 478.4 / 573.1 ms |

The smaller RSS observation is consistent with reduced overlap. The previous
macOS repeats varied substantially, so a deployment-level process-memory
bound remains open. Independent review rehashed all 28 worker artifacts per
platform and confirmed Docker resource retirement.

| Platform | Controller proof SHA-256 | Worker proof SHA-256 |
|---|---|---|
| macOS | `5c96a51a32723ea03d0575e45cf3b1a129d8ffbfae41d4e2a19d23bef4db4ef9` | `08b75558ad99248f75908a2acb20cdaddfdb34ca76c0e37d42688f30dbb8648a` |
| Linux | `ada746730dc115c919616afb81fdfae8616dffc777e20436bd3fae4ee67057c4` | `206b859bd9d3ffd262f08f209b52559c79265e4b10237a22b222e004e9d0325c` |

### In-place compaction graph lifetime

In-place compaction releases the source serialization caches before encoding.
After the destination is written, it releases the temporary source graph and
the destination serialization caches before the first verification read. Both
file leases stay held. Verification runs before metadata copy and again
after metadata sync, before namespace exchange. A live `compact_to` caller
retains its source state for subsequent transactions. The compact destination
also releases its serialization caches before verification.

The native regression uses a 512 KiB value shared by the current and retained
views. The baseline reached the first target verification with 525,757
bytes still charged. The new test passes at both first verification reads
with less than the payload size charged, and probes both file leases at each
read. The regression exercises the same file compaction path as the public
API.

A focused macOS arm64/APFS run used Rust 1.99.1 release builds from Git
archives of `d7aa7eb` and `438d9b2`. Each process compacted an independent
copy of the same revision-213 input 12 times. Both final images have
63,074,304 bytes, the same identity and 64 retained roots, and the exact
same SHA-256 `3d6d987a9e411d6b3e8e5a3e5000d5471e5940150996ae0425c6d6b74818bc35`.

| 12-compaction observation | Baseline `d7aa7eb` | Candidate `438d9b2` |
|---|---:|---:|
| Compactions elapsed | 9.856 s | 9.703 s |
| macOS `time -l` maximum RSS | 889,176,064 bytes | 838,680,576 bytes |

The immutable input SHA-256 is
`12463c2d6f37877731e6c33ea273fb2a135b702b18f7cc885c2f1c0851bbdbc0`.
The RSS difference is a single paired observation on this host.

The same release builds each ran all 24 maintenance cycles from
a copy of one compacted revision-212 seed with SHA-256
`c50285d97a005ee7690781b2e081ba83662f1a5ee59bc1f49a99901fb59f21d6`.
Independent replay checked all 168 phase digests per build, all 768 commit
samples, all 6,144 reader samples, the open-database allocation budget,
and the file lengths at each phase. Both reached revision 980, sequence 100
and 63,074,304 bytes. The final raw images differed only in the eight
workers' last-write revisions and their DATA object's checksum. Independent
decoding identified the same revision set, 973 through 980, assigned in
different orders. Three same-build single-cycle controls also produced
distinct hashes under contention.

| 24-cycle observation | Baseline `d7aa7eb` | Candidate `438d9b2` |
|---|---:|---:|
| Worker elapsed | 126.03 s | 125.27 s |
| macOS `time -l` maximum RSS | 1,226,555,392 bytes | 1,307,787,264 bytes |
| Peak charged bytes in the opened database | 57,794,708 | 57,794,708 |
| In-place compact p50 / max, 24 samples | 765.0 / 900.7 ms | 748.6 / 1,145.5 ms |

Peak RSS was higher in the mixed candidate run. The verification lifetime
test measures charged allocations at a precise boundary. The mixed process
RSS remains a separate deployment measurement.

To repeat the paired cycles, build each exact checkout and use the same
verified compacted revision-212 image as the seed for both workers.
Choose a fresh output directory each time.

```sh
revision=$(git rev-parse HEAD)
WEEKEND_DB_SOURCE_ID="$revision" cargo build --offline --locked --release \
  --manifest-path tools/resource-envelope/Cargo.toml --features compound \
  --bin weekend-compound-resource --target-dir target/compaction-cycles-build
/usr/bin/time -l target/compaction-cycles-build/release/weekend-compound-resource \
  target/compaction-cycles-fresh "$seed" cycles
```

On macOS, 182 all-feature library, compaction, durability, inspection,
replication and retention tests passed, with one registered fixture ignore.
Strict Clippy passed for macOS and the Windows MSVC cross-target.
The hosted Ubuntu x64/ext4 job passed the data, slot and witness maintenance
writeback checks on tested merge `a67f5330a6fd719aaa270c34f894d1a5d99398ad`.
The exact fixture rehashed, and both final archived images for each barrier
matched their recorded hashes and 64 retained-root counts.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43757596487/job/132944986947), [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43757596487/artifacts/14179131667)

Local evidence is under `target/compaction-lifetime-validation/`.
The cycle audit SHA-256 is
`6681d82da4a9b7cf60c20a94ad0ad19cfdd9200cf8eaab7b17d5f7043f10236c`.

### Short-lived maintenance processes

Run the paired workload from the exact source on macOS or Linux. To use
the recorded Linux arm64/ext4 setup, add `--docker-image` with the Rust
1.99.0 image ID `sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.

```sh
cargo xtask resource compound-isolation --revision b377d37 \
  --output target/resource-compound-isolation
```

The controller builds one release binary, creates one compacted
revision-212 seed, and runs 24 ordinary cycles followed by 24 cycles
that start a fresh copy of that same binary for each backup, restore or
compaction. This includes the intentionally rejected backup. Both modes
call the same operations with the same budgets of 96 MiB for the rejected
backup and 256 MiB for successful maintenance. Reopening, contention and
graph verification in the workload stay in the main process. The controller
checks every phase, rehashes the seed and checks that each run leaves only
its final `cycles.wdb`.

The main worker RSS is sampled by the supervisor about every 50 ms.
Each short-lived helper reports its own `RUSAGE_SELF` RSS high-water mark
after the operation. The two measurements cover different processes.
The paired measurement below adds aggregate process-group RSS. Deployment
memory limits remain open.

Each run passed all 168 state observations, 768 contended commits and
6,144 reader checks. Both finished with revision 980, sequence 100, 64
retained roots, a 63,074,304-byte image and a 57,794,708-byte core peak in the
opened database. An independent byte comparison allowed only the eight keys'
last-write revision fields and the current DATA checksum to differ. The
eight revisions remained the set 973 through 980 in both runs.

| macOS arm64/APFS, Rust 1.99.1 | In process | Short-lived helpers |
|---|---:|---:|
| Worker wall time | 105.56 s | 106.86 s |
| Main worker sampled peak RSS | 1,446,816 KiB | 73,216 KiB |
| Main worker RSS sample count | 1,624 | 1,649 |
| Maximum of 96 helper RSS high-water marks | — | 120,569,856 bytes |
| In-place compaction median / max, 24 samples | 686.0 / 726.0 ms | 703.3 / 717.7 ms |

| Docker Linux arm64/ext4, Rust 1.99.0 | In process | Short-lived helpers |
|---|---:|---:|
| Worker wall time | 94.00 s | 93.65 s |
| Main worker sampled peak RSS | 127,032 KiB | 69,300 KiB |
| Main worker RSS sample count | 1,658 | 1,646 |
| Maximum of 96 helper RSS high-water marks | — | 117,440,512 bytes |
| In-place compaction median / max, 24 samples | 624.0 / 640.0 ms | 633.3 / 642.9 ms |

The macOS seed SHA-256 is
`07dc128d21484d3a2acf5f7c53377fd106f51691e2704288209b82e6dda99fab`.
The release fixture SHA-256 is
`c5f6a013e4aed174b25298a384380b6146090e73267b85ddb627be2b8b15b75c`.
The checked worker evidence in `target/compound-isolation-macos/run/evidence.json`
has SHA-256 `cb273f886158af21e17af874e9e1966b6e0e0ccdf545c972fce5e1f7b38a9eca`.

The Linux seed SHA-256 is
`1e65af2a71a821e7ae03d0fecdf6fb75b886eb4dcd651a859cf269c044fd90ea`.
The release fixture SHA-256 is
`30478b6d45dea6dd4e57d7cc40d5e4ce5665a8e3baf7d286c9bc2fa0f77ae9a8`.
The checked worker evidence in `target/compound-isolation-ext4/run/evidence.json`
has SHA-256 `1b7658acc880a1feb730e8088bbeda7ec2afbc84768adba4de43fa24ab745e71`.
The controller verified removal of its container and volume.

The production `weekend-admin compact` uses the existing offline API.
The native subprocess test holds the source lease, verifies `Busy` and
unchanged input bytes, releases the lease, and compares the CLI output
image byte for byte to the API's output from identical input. It reopens
to compare current and retained roots. The all-feature admin and
compaction suites passed 12 tests on macOS and 13 on Docker Linux. The
14 resource-controller and oracle tests passed on macOS. On Windows the
command reports `UnsupportedPlatform` and `backup` produces a compacted copy.

### Paired process-group RSS during maintenance

Run the same 24-cycle comparison with the process-group observer at
`9b234a7b818eeecd08f6659a25b174728b9c3fa0`.

```sh
cargo xtask resource compound-isolation --revision 9b234a7 \
  --output target/compound-group-rss
```

The supervisor records each leader's RSS and the sum of resident memory
reported by the live processes in its owned group in the same polling
iteration. Maintenance helpers stay in that group. Polls are scheduled
50 ms after the previous read, with additional time for process inventory
and scheduling. macOS uses `ps` and Linux reads `/proc/PID/stat`. The
observer records the group member count, both RSS totals and the number
of samples where another member contributes RSS. It excludes the observer
and other groups. RSS can count shared pages more than once and the sampling
can miss short peaks. The result is a sampled process-group total. It leaves
physical footprint, kernel cache memory and deployment limits open.
[Linux process accounting](https://www.kernel.org/doc/html/latest/filesystems/proc.html)

Both modes again verify each current and retained state after all 24 cycles.
The runs on a platform start from one seed and finish at revision 980 with
64 retained roots. The controller checks sample pairing and recomputes the
peak and overlap summaries before accepting the evidence.

| macOS arm64/APFS, Rust 1.99.1 | In process | Short-lived helpers |
|---|---:|---:|
| Group sampled peak RSS | 3,600,880 KiB | 181,552 KiB |
| Leader sampled peak RSS | 3,600,880 KiB | 64,096 KiB |
| Group samples with other resident memory | 1 of 1,386 | 628 of 1,396 |
| Worker wall time | 106.01 s | 106.59 s |
| In-place compact median, 24 samples | 699.0 ms | 700.9 ms |

The macOS isolated peak sample includes 63,808 KiB from the leader and
117,744 KiB from a second group member. The observed in-process peak varies
across runs. The earlier comparison's leader sampled peak was 1,446,816 KiB.

| Docker Linux arm64/ext4, Rust 1.99.0 | In process | Short-lived helpers |
|---|---:|---:|
| Group sampled peak RSS | 126,232 KiB | 176,532 KiB |
| Leader sampled peak RSS | 126,232 KiB | 64,628 KiB |
| Group samples with other resident memory | 0 of 1,654 | 777 of 1,671 |
| Worker wall time | 94.43 s | 95.04 s |
| In-place compact median, 24 samples | 628.8 ms | 635.5 ms |

The Linux isolated peak sample includes 61,852 KiB from the leader and
114,680 KiB from a second member. Short-lived helpers lowered the leader
peak and raised the group peak in this Linux pair.

Both platforms finished at sequence 100 with a 63,074,304-byte image.
Independent comparison identified only the contending keys' last-write
revision fields and the current DATA checksum as byte differences within
each pair. Each run verified 768 contended commits and 6,144 reader checks.
Independent checks rehashed both source archives and the 32 payloads per
worker. Docker retired the container and its volume. Strict Clippy passed
on both platforms. The macOS xtask suite passed 190 tests with one registered
ignore. The 15 resource tests and 13 supervisor tests passed on Linux.

| Evidence under `target/group-rss-validation/` | macOS SHA-256 | Docker Linux SHA-256 |
|---|---|---|
| Identical seed within each pair | `ff28a871a014ffef5941d3de9faf6577cb4d231eb43149793139d19a31a780b4` | `a916394d55d98553dc3a8f318b2a582bba44328554f2dc8cede43a9de5be46be` |
| Exact release fixture | `62358fd5be434fcc65f011fceb423a27cc80c68020609818d10707c4945a78ab` | `375e0174db70b26b1b95bb42b7c469b74d1f6449a61f71912fb9335fbf8b7b85` |
| Worker evidence | `3ce93e31213c6ecf718616e8d939bb90fe5ab27bb6919ecf24b77c07f8ee8df2` | `b7b84a8db7b9b035244ed9c1d47f6c6988ad710961d82b1e1fef1b6729f84961` |
| Independent audit | `78807cf4dee2c353ae3ec46202d7202eb2ae0f2c5b0bf7e158c1b90e42d02b2e` | `eaa6ba4999ca56c9725d16c315135c06f792de501151a08e985f005204047f17` |

## Default width, depth and write count near the image cap

The `compound-breadth` profile fills one collection with 4,096 keys in four
1,024-write commits. One record is depth 64. Its bottom list references the
same 4,096-entry map twice, and every map entry references one shared scalar.
Nine full update batches and 55 single-record updates produce 64 distinct
retained DATA roots. Each retains the same deep DAG alongside a different
record state. The current revision is 133, with DATA revision 132.

The fixture checks the 4,097th map entry, 65th nesting level, 1,025th staged
write, 4,097th collection entry and 65th retained root. Each returns `Limit`.
Under the 32 MiB logical draft budget, 203 references to the shared value
can be staged. The next reference returns `Limit` and a scalar at the same
key still fits. A separate 1,024-write batch of new scalars stages in the
near-cap store, then returns `Limit` during commit. The failed transaction is
terminal, the handle stays healthy and the file matches its pre-attempt
bytes. A fresh open verifies the same state.

On both platforms the source is **63,574,016 bytes** (94.73% of the 64 MiB
format cap). Backup, restore and in-place compaction each produce
**58,523,648 bytes**, with byte-for-byte agreement within each run and the
same store identity. The rejected batch fits after compaction, reaching
62,840,832 bytes. Using the compacted bytes as the seed for each simulation,
the fixture observes three sync points from the successful publication and
injects an error at either the slot or witness sync. The crash discards
pending writes.

| Simulated publication | Commit result | Inspected revision | Witness on inspection | Revision after reopening |
|---|---|---:|---|---:|
| All syncs succeed | Acknowledged revision 134 | 134 | Present | 134 |
| Slot sync fails, pending slot discarded | `OutcomeUnknown` | 133 | Present | 133 |
| Witness sync fails, pending witness discarded | `OutcomeUnknown` | 134 | Absent | 134 |

Both failed syncs require recovery before more writes. Reopening installs the
missing witness in the last row. A second read-only inspection matches the
selected metadata with the witness now present. At each of the nine reported
phases the fixture checks all 266,240 records across the current and retained
views and 532,480 nested map leaf references. The Rust controller
independently calculates the expected digest for both revisions, verifies
file lengths and copy hashes, binds the build and artifacts to the source
archive, and checks that the owned worker group has retired. Docker independently
checks the owned container and volume are absent. The macOS xtask suite and
the 16 Linux resource controls passed. Strict Clippy passed for the controller
and the fixture on both platforms.

| Single operation observation | macOS arm64/APFS | Docker Linux arm64/ext4 |
|---|---:|---:|
| Create and fill source | 1,636.0 ms | 297.4 ms |
| Stage and reject full batch | 11.3 ms | 11.8 ms |
| Reopen source | 232.8 ms | 240.7 ms |
| Verified backup | 709.6 ms | 719.6 ms |
| Verified restore | 715.7 ms | 697.1 ms |
| In-place compaction | 1,010.2 ms | 969.8 ms |
| Sampled peak worker RSS, including simulator | 479,440 KiB | 295,696 KiB |

Peak requested core memory was 20,451,448 bytes for the source writer
and 79,338,568 bytes for each native reopen, including verification reads
through that handle. The maximum of the simulated reopen samples was
80,611,848 bytes. The source and reopened handles used a 96 MiB core
budget. Each maintenance API call used a 256 MiB budget. Operation times
exclude the fixture's subsequent full-state checks. Compaction timing
starts after creating the disposable source copy. RSS includes simulator
images and allocator overhead. The table records individual observations.
The follow-up exercises concurrent full-width drafts and additional simulated
tears. Distinct deep DAGs combined with large payloads, owner service limits and
device interruption remain open. [Concurrent workload and results](#full-width-drafts-during-a-native-commit)

Run with a clean checkout at `c4e437b`. The local macOS run used Rust 1.99.1.
The Docker image uses Rust 1.99.0 and an ext4 volume.

```sh
cargo xtask resource compound-breadth --revision c4e437b \
  --output target/compound-breadth-qualified-macos
cargo xtask resource compound-breadth --revision c4e437b \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/compound-breadth-qualified-linux
```

| Evidence in the corresponding output directory | macOS SHA-256 | Docker Linux SHA-256 |
|---|---|---|
| Release driver | `8c7217ce8178dd9cb6ac2f80fe86577134afc85a03adeee1e84e0e0f51a8c0f5` | `5f8a833947bee40b5494f9a51153a750c46bf2a567c3a66bd2b568bada13e955` |
| Controller evidence | `1efe64f7f854357d2c831baf26e9d85fb34a139213e3d9e567166d20e387517e` | `142f162416abadac7b080b0a1b80c5deee8d880b9fbbd01f3189155c4e84561b` |
| Worker evidence | `b98b2aaccfb89c95be2690fb7a54026f245107c6bd3bfc5ef79c66eacec71cdf` | `803331f7ec9aa4d3dfcf9eecb3ccc38a2ec7e37eba17fe529d128af5c8e648ab` |
| Byte-identical backup, restore and compact images within the run | `70ffb984ef17576affbb745dbafa7b5489e53fe007c1117d738ed97a94f70ec5` | `5096d4bc66651ceefdcd379cbfa1a72c01a7c8658ac79e661ecff3d0b104d281` |

## Full-width drafts during a native commit

At `791164f`, three drafts each stage 1,024 writes against revision 133 on a
copy of the 58,523,648-byte compacted seed. The test opens the file normally,
then arms a wrapper that pauses the first writer after the real append sync.
While the writer holds the publication mutex, a second writer enters the
commit path and the reader checks all records and nested leaves in the 65
pre-acquired views. In this revision, fresh snapshot acquisition uses the
publication mutex. [Fresh snapshot follow-up](#fresh-snapshots-during-durable-publication)

The first writer receives local acknowledgment for revision 134. The
overlapping writer returns `Conflict` and aborts. The disjoint draft stays
live across the first commit. Its original point observations still match,
but a second full batch of new scalar objects would exceed the format cap by
49,152 bytes. Its commit returns `Limit`, the transaction is terminal, and
the 62,840,832-byte image stays byte-for-byte intact with a healthy handle.
A new transaction updates those same 1,024 keys using one shared scalar.
It adds 126,976 bytes and acknowledges revision 135. The original pinned
view still reads revision 133. Reopening the 62,967,808-byte file verifies
revision 135 and all 64 retained roots.

The simulator uses the same compacted seed and first 1,024-write batch. Each
case injects a failed sync and persists the indicated prefix of its pending
write. The crash then discards the other pending bytes. The version 2 slot
and witness checksums occupy bytes 64 through 71. The metadata padding
after byte 71 is already zero.

| Failed sync | Persisted prefix | Commit result | Revision reopened | Witness before reopening |
|---|---:|---|---:|---|
| Append | 4,096 bytes | `Io` | 133 | Present |
| Slot | 64 bytes | `OutcomeUnknown` | 133 | Present |
| Slot | 72 bytes | `OutcomeUnknown` | 134 | Absent |
| Witness | 64 bytes | `OutcomeUnknown` | 134 | Absent |
| Witness | 72 bytes | `OutcomeUnknown` | 134 | Present |

In each case the write handle requires recovery, read-only inspection
selects the revision shown, and a subsequent open verifies the full data
and stabilizes the witness. The original acknowledged and dropped
slot/witness cases still run. The independent oracle checks the expected
state at all 18 phases, including the pre-acknowledgment read, after
rejection, and after the later acknowledged commit.

Reproduce from a clean checkout at `791164f`.

```sh
cargo xtask resource compound-breadth --revision 791164f \
  --output target/compound-breadth-concurrent-macos
cargo xtask resource compound-breadth --revision 791164f \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/compound-breadth-concurrent-linux
```

Both runs agree on the revision, logical digest, file length and witness
state of every phase. The three drafts increase requested core memory by
688,128 bytes before the writer starts. The concurrent native handle peaks
at 93,685,728 bytes on both platforms under its 96 MiB limit.

| Single observation | macOS arm64/APFS | Docker Linux arm64/ext4 |
|---|---:|---:|
| Verify all pinned views during the pause | 55.5 ms | 68.9 ms |
| First commit, including the pause | 77.7 ms | 82.8 ms |
| Reject disjoint batch at the image cap | 10.3 ms | 10.2 ms |
| Stage and commit the shared-value retry | 10.8 ms | 4.4 ms |
| Open revision 135 | 242.7 ms | 234.3 ms |
| Sampled peak worker RSS, including simulator | 525,856 KiB | 314,880 KiB |

The fixed schedule measures pinned reads and selected failure prefixes.
Fresh snapshot latency, sustained full-width contention and other tear
offsets remain open. Docker verifies that its container and volume are
absent after copyback. The macOS xtask suite passed 198 tests with one
registered ignore. Docker Linux passed the 16 resource controls. Strict
Clippy passed on both platforms.

## Retry after a segmented append tears near the image cap

At `d2cc402` the full-width workload first calibrates the actual append
writes of the 1,024-key transaction. It stops inside the write that
contains the selected cutoff and persists the completed segments plus
the final partial segment. The failed transaction returns `Io`, aborts,
and requires recovery. The exact old image and the surviving append
prefix are compared with the successful control. Read-only inspection
and a new handle select witnessed revision 133 with all 64 retained
roots. The committed end stays at 58,523,648 bytes, and the next append
starts at the physical end rounded up to a 4,096-byte boundary.

| Surviving physical length | Next append offset | Outcome of the same transaction on a new handle | State after recovery |
|---:|---:|---|---|
| 62,791,679 bytes | 62,791,680 | Fits, then a torn witness gives `OutcomeUnknown` | Revision 134 at 67,108,864 bytes, with the witness repaired |
| 62,791,680 bytes | 62,791,680 | Fits, then a torn witness gives `OutcomeUnknown` | Revision 134 at 67,108,864 bytes, with the witness repaired |
| 62,791,681 bytes | 62,795,776 | `Limit` before any write or sync | Revision 133 and the torn image are unchanged |

For the first two cases, the second fault occurs after a durable root
slot. The failed retry leaves the live handle at revision 133. The
abandoned bytes survive exactly, and the one-byte alignment gap in the
first case is zero. After another crash, read-only inspection finds
revision 134 unwitnessed. Opening it verifies the full graph and writes
the missing witness before exposing the handle.

For the one-byte-over case, the rejected transaction leaves the handle
usable and the complete image unchanged. A verified compact copy leaves
that source intact and reproduces the exact clean seed. The same
1,024-key write acknowledges revision 134 on the copy and reopens it
witnessed at 62,840,832 bytes.

The controller now verifies all 24 states, including exact digests,
physical lengths and witness status in the six added rows. Its negative
controls reject changed lengths, revisions, digests and witness state.
An engine mutation that starts the retry from the committed end fails
at the new append-offset assertion. It writes at 58,523,648 instead of
62,791,680. The fixture emits its original 18 state records before this
control fails.

```sh
cargo xtask resource compound-breadth --revision d2cc402 \
  --output target/compound-breadth-torn-tail-mac
cargo xtask resource compound-breadth --revision d2cc402 \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/compound-breadth-torn-tail-linux
```

The macOS arm64/APFS run used Rust 1.99.1. The Docker Linux arm64/ext4
run used Rust 1.99.0. They agree on the six added revisions, digests,
lengths and witness states. Worker groups retired on both platforms,
and the Linux container and volume were removed. Nine resource-oracle
tests and 33 durability, retention and compaction tests passed on
macOS. Strict Clippy passed for the xtask and the minimum-Rust fixture.

| Evidence SHA-256 | macOS arm64 | Docker Linux arm64 |
|---|---|---|
| Controller report | `e0e9502db2cf4aceb377daf676a0c23b5c59bfbdf723c70d9f120952aaca4013` | `703dbcda4659b852337ce9586a51d1b557ee542aec2dc54bf7caa2d8cf97903c` |
| Worker report | `1fa37d2290ab33ca9cd8e478126ebf76a34bf401465bb676d457a602a57c922e` | `0b82f9dc56d54a2fe38c4b4e779741ba33c126dd226ae3f511626233b4100d03` |

The local archive `target/archived-proof/full-width-torn-tail-d2cc402.tar.zst`
contains the source, both reports with all 36 listed worker artifacts, and
the mutation patch and output. Both worker inventories matched after
extraction. The archive SHA-256 is
`95d3fc8dc43531791187411544ccf9a24475fe5fec5c08af68dfa556fa3df9a8`.

These cases exercise three neighboring cutoffs and one particular
second failure in the simulator. Other offsets and persistence orders,
native Windows and physical interruption retain their own qualification
gates.

| Evidence in the corresponding output directory | macOS SHA-256 | Docker Linux SHA-256 |
|---|---|---|
| Release driver | `3b091ace373c04bef64c1ac2c6af70236611af68410c8701b4ee88cc0ccd9503` | `2c4ac967a87fd041329a33549b7e502eaa1defec7c6221b595c32e57644fa4e9` |
| Controller evidence | `f5d08cb0756c987615eaf5fb3f8e9bd29d5bbb90200ab8b8b4e484c76707f115` | `8031208b2e387f27e0b2e62e02318a506927e1a53e4e5b3a1af1c108557b78ac` |
| Worker evidence | `a4dcc4474d50116708d06e1a93bd0720b31ad34b6e4b73733e465694f2ab9e33` | `27a166ce9e266dcc765797c09a35484188e10cfa0109c94b0ecf02ea43e04a22` |
| Final concurrent image | `30c94db33ba5c33927ad95c3551ef99166811462bbd6116aab34683b64498915` | `ecedae08e95b22a28553d60f45a9d6918e4d9bffa7806c5f7fed01b249e670cd` |

## Fresh snapshots during durable publication

At `17f7c47`, the concurrent profile acquires the current snapshot and all
64 retained views after the first writer pauses inside the native append
sync call. The reader finishes acquiring and checking all 65 views before
the writer resumes. The revision it sees is 133. The first commit still
acknowledges 134, the competing draft conflicts, the disjoint batch leaves
the 62,840,832-byte file intact at the image cap, and the shared-value batch
reopens at 135 with all retained roots.

The 18 phases match the earlier runs' recorded digests, revisions, file
lengths, witness states and requested core peaks on each platform. The
existing serialization controls independently compare ten lifecycle and
seven DATA history images captured at `b131f84` and `17f7c47`. All 17 pairs
match byte for byte. The binaries use different source revisions and the
fixture uses fixed lineage identities for these byte comparisons.

| Single observation | macOS arm64/APFS | Docker Linux arm64/ext4 |
|---|---:|---:|
| Acquire current snapshot, retained IDs and 64 retained snapshots | 2.8 µs | 26.1 µs |
| Acquire and verify all 65 views during the pause | 56.2 ms | 71.6 ms |
| Requested core peak around the concurrent limit rejection | 93,685,728 B | 93,685,728 B |
| Sampled peak worker RSS, including simulator | 525,984 KiB | 418,076 KiB |

These are single observations. The follow-up samples longer publication
windows under load. Direct installation-lock attribution and
deployment-level latency bounds remain open.
[Paired reader and writer measurements](#sustained-fresh-snapshot-acquisition)
The exact lock ordering and acknowledgment point are documented in the
[design](../DESIGN.md).

```sh
cargo xtask resource compound-breadth --revision 17f7c47 \
  --output target/fresh-snapshot-qualified-macos
cargo xtask resource compound-breadth --revision 17f7c47 \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/fresh-snapshot-qualified-linux
```

| Evidence in the corresponding output directory | macOS SHA-256 | Docker Linux SHA-256 |
|---|---|---|
| Release driver | `b4d8ec26675b5fe8c1db84e4bc432f1a2143378042b5a3bd6d8fe9a76c3ccc9b` | `f13d82d87964a9fede946684e660d8f26cb35993dd2546c96050af2228fb7dc9` |
| Controller evidence | `559bb31f62733d43e77a97e7b1ce9e5a1cba16db7fde30d44a144dcbda11ec86` | `149dd3613afa854b4f7e197da9badb3d2e5254ec7dd9c09ee08b905fa2daf264` |
| Worker evidence | `02d9bc43ec8f423036d12971752636e821f118caa618a440a892ee079267c64b` | `23743e9cde407d98b6fc879262b83ad1bfdb60f523f39792f0514c4276ca601b` |

## Sustained fresh snapshot acquisition

The `compound-breadth-sustained` profile starts from the same fixed-identity,
58,523,648-byte compacted seed in all runs. It uses the same release driver
source against `b131f84` before the lock split and `48e2e6d` after it.
On each platform the baseline and candidate ran sequentially.

Each run spans eight write-and-compact cycles. Three writer threads stage
their first drafts before a shared starting barrier, then each commits 16
full batches per cycle. Each batch assigns one shared scalar to 1,024
disjoint keys. Four reader threads acquire fresh snapshots with a requested
500 µs delay between completed iterations. They check both ends of every
writer's key range on each read and scan every record and nested map on
each newly observed revision. The independent controller replays the
acknowledged writer order and checks every sampled reader state against it.

The 384 commits advance from revision 133 to 517. Each cycle verifies the
current and all 64 retained views before and after in-place compaction.
The active file grows to 64,618,496 bytes in the first cycle and 64,749,568
bytes in the next seven, then compacts to 58,654,720 bytes each time.
The core limit is 96 MiB. Compaction runs after the worker threads finish
and the active handle closes. It has a separate 256 MiB budget.

Snapshot timings cover acquisition alone. The statistics exclude each
reader's pre-barrier sample. The transition subset contains acquisitions
that observed a revision newer than that reader's previous sample.
Commit timings include waiting for other writers and all durability
barriers. Percentiles use the nearest rank. Raw samples with zero
elapsed clock ticks are counted and retained. The reader loop submits
its next request after the previous iteration, so the candidate can
complete substantially more reads and verification work in each window.
The fixed-arrival follow-up measures queueing at one offered rate.
Direct lock-installation timing and deployment thresholds remain open.
[Fixed-arrival measurements](#fixed-arrival-snapshot-queueing)

Reproduce from the driver checkout at `48e2e6d`.

```sh
cargo xtask resource compound-breadth-sustained --revision b131f84 \
  --output target/sustained-breadth-macos-baseline
cargo xtask resource compound-breadth-sustained --revision 48e2e6d \
  --output target/sustained-breadth-macos-candidate
cargo xtask resource compound-breadth-sustained --revision b131f84 \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/sustained-breadth-linux-baseline
cargo xtask resource compound-breadth-sustained --revision 48e2e6d \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/sustained-breadth-linux-candidate
```

| Metric | macOS baseline | macOS candidate | Docker Linux baseline | Docker Linux candidate |
|---|---:|---:|---:|---:|
| Fresh acquisitions | 746 | 19,378 | 1,086 | 6,386 |
| Acquisition p99 | 33.924 ms | 0.417 µs | 65.972 ms | 1.292 µs |
| Maximum acquisition | 36.655 ms | 15.917 µs | 72.604 ms | 13.042 µs |
| Revision-transition acquisitions | 735 | 1,536 | 739 | 1,477 |
| Revision-transition acquisition p99 | 33.924 ms | 0.791 µs | 67.224 ms | 1.208 µs |
| Distinct intermediate revisions observed, of 376 | 191 | 376 | 235 | 371 |
| Durable commit p50 | 22.659 ms | 42.985 ms | 5.981 ms | 24.199 ms |
| Durable commit p99 | 37.422 ms | 47.008 ms | 73.725 ms | 91.139 ms |
| Compaction median, eight cycles | 982.7 ms | 999.1 ms | 1,021.3 ms | 1,024.8 ms |
| Reopen median, eight cycles | 233.1 ms | 232.6 ms | 227.5 ms | 224.3 ms |
| Largest requested core peak during load | 83,207,568 B | 83,460,236 B | 83,168,616 B | 83,480,072 B |
| Sampled peak worker RSS across the run | 552,832 KiB | 497,696 KiB | 190,336 KiB | 186,548 KiB |

The candidate completed more reads and full scans, and durable commit
latency increased in both pairs. The candidate retained 2,015 zero-tick
acquisitions on macOS and one on Linux. These distributions describe the
four finite runs. The macOS arm64/APFS host used Rust 1.99.1. Docker Linux
used Rust 1.99.0, arm64/ext4 and a two-CPU, 2 GiB container.

All four seed files match byte for byte. The eight sequence checkpoints
agree on revision, digest and file lengths. The controller hashes the
bounded raw sample log and replays it after copyback. Both Docker runs
removed their owned container and volume. The 17 resource controls and
strict Clippy pass on macOS and Docker Linux.

| Output directory under `target/` | Raw `run/run.jsonl` SHA-256 | Worker `run/evidence.json` SHA-256 |
|---|---|---|
| `sustained-breadth-macos-baseline` | `b5fc6beda9f5b1f51d61a8811b0f80912302a9f4af87d4e648a97357b95dfb0a` | `f036b527ca81cdb8d726e76c03ac4f4f1f7d2d5c055008ae803842cd474c57a5` |
| `sustained-breadth-macos-candidate` | `faf9d69c677a63a554cc244b1c61d3566ddea71ded9c11b91ae1f49d17520625` | `1aed01a471f8b007fdb704e556e2b1c06dad5a0a07fdc8b334ff7788a6a2fcc1` |
| `sustained-breadth-linux-baseline` | `3c2f31953991516d4c98df96876b502a32110da5ab7cd9f29f31d2a75b8ce150` | `4fab816fae907e9660dd6bd6fda36d7dc5023870dad72f2a35febb73ca9ccda2` |
| `sustained-breadth-linux-candidate` | `2ce071185070ea4a15632584f72aac89574bfeef62afb02b915b22f745582ccb` | `8e6c2125d3dfa4fa2b999847f8dba8dad07e14d79d97beeb0ab66350edfb2b3a` |

## Fixed-arrival snapshot queueing

The `compound-breadth-arrivals` profile pairs `b131f84` with `ece552a` on
macOS arm64/APFS and Docker Linux arm64/ext4. All four runs use the same
release driver source and the same 58,523,648-byte seed. Each run keeps
the eight cycles, three writer threads and 384 full-width commits from
the completion-paced workload. It reopens and checks the current and all
64 retained views after each compaction.

Four reader threads each receive 1,536 scheduled requests per cycle.
Their arrivals are 500 µs apart from the common start barrier, for a
nominal 8,000 arrivals per second over each 768 ms window. A reader handles
its own requests in order. The schedule models a finite per-reader queue.
Each request keeps its original arrival time through any backlog. All
49,152 requests per run must complete. Each request checks both ends of
every writer's key range. The first request and every 128 requests after
it scan the entire current view, for 384 full read checks per run.

The raw trace records each request's start, completion, acquisition time
and observed revision. The earliest eligible start is the later of its
scheduled arrival and the preceding request's completion in that reader
thread. Queue wait runs from arrival to eligibility, dispatch delay from
eligibility to actual start, service from start to completion, and response
from arrival to completion. Service includes the read checks. The controller
rejects incomplete schedules, inconsistent timing and any reader state that
disagrees with replay of the acknowledged write order. It keeps the bounded
raw log and verifies the summary from the copied log and its hash.

Reproduce from the driver checkout at `ece552a`.

```sh
cargo xtask resource compound-breadth-arrivals --revision b131f84 \
  --output target/breadth-arrivals-macos-baseline
cargo xtask resource compound-breadth-arrivals --revision ece552a \
  --output target/breadth-arrivals-macos-candidate
cargo xtask resource compound-breadth-arrivals --revision b131f84 \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/breadth-arrivals-linux-baseline
cargo xtask resource compound-breadth-arrivals --revision ece552a \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/breadth-arrivals-linux-candidate
```

| Metric | macOS baseline | macOS candidate | Docker Linux baseline | Docker Linux candidate |
|---|---:|---:|---:|---:|
| Scheduled requests completed | 49,152 | 49,152 | 49,152 | 49,152 |
| Requests that waited behind earlier requests | 39,034 | 4,058 | 20,701 | 16,456 |
| Queue wait p99 | 133.504 ms | 5.090 ms | 18.414 ms | 8.899 ms |
| Dispatch delay p99 | 120.833 µs | 125.250 µs | 663.916 µs | 665.500 µs |
| Snapshot acquisition p99 | 11.901 ms | 0.584 µs | 2.033 ms | 1.292 µs |
| Request service p99 | 12.140 ms | 9.125 µs | 3.572 ms | 11.666 µs |
| Full response p99 | 134.004 ms | 5.590 ms | 18.914 ms | 9.399 ms |
| Longest full response | 165.999 ms | 9.703 ms | 34.543 ms | 34.097 ms |
| Requests completed after their offered window | 0 | 0 | 11 | 2 |
| Durable commit p99 | 37.825 ms | 35.940 ms | 28.911 ms | 32.565 ms |
| Sampled peak worker RSS | 495,760 KiB | 495,840 KiB | 185,784 KiB | 185,888 KiB |

The lock split lowers acquisition and full-response p99 in both pairs.
Queueing still contributes to request latency after the split. The readers
in each candidate run collectively observe all 376 intermediate revisions.
Commit p99 falls on macOS and rises on Docker Linux at this fixed read
rate. Multiple offered rates, direct installation timing, and deployment
thresholds remain open.

The four seed files match byte for byte. All cycle checkpoints agree on
revision, digest and file lengths. The final open verifies revision 517
and its 64 retained roots. Docker verifies absence of the owned
containers and volumes after copyback.
The 18 resource controls and strict Clippy pass on macOS and Docker Linux.
The completion-paced profile also passes end to end on macOS.

| Output directory under `target/` | Raw `run/run.jsonl` SHA-256 | Worker `run/evidence.json` SHA-256 |
|---|---|---|
| `breadth-arrivals-macos-baseline` | `cf0cb4645070dec92bf8bf3d431ea45c35d7a71e4ff48ee882b33871d80e085d` | `f31a3f9635ea830f6c31d9bbc9c248fd6e649939a51c0e18bdae62218ab855c7` |
| `breadth-arrivals-macos-candidate` | `5f9af7fd9939e05eff02bc941d9f6b1618c9527ea05ea18f3538dccf8c18a014` | `d15feaa730840450f90348617356c5222c1e8bd7e4fcdd1897a6a40ab378010a` |
| `breadth-arrivals-linux-baseline` | `c6f672ff255606b9c5ec737422711302cafe36709a54a1c4ff67fd24741ee4d3` | `354c4a2a30789b87450da6aea1f08365c4301022482f23c4f80ce6c0bd973c93` |
| `breadth-arrivals-linux-candidate` | `9fdd44e021c3db4d10d4519d85c45f0f744ee945e0368b43ad9e04a518880815` | `c32eafaa2e28617567ed5c808815176d8620187e76e137a833c97e88095cd807` |
