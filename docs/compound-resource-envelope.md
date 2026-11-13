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
