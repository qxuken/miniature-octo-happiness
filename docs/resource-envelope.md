# Finite resource-envelope qualification

Frozen engine `96e105f80c189a2f9de12530518c4d834f2796ea` passed four finite
resource cases locally on macOS arm64/APFS and Docker Linux arm64/overlayfs on
2026-11-12. These cases exercise 64 retained roots at value depth 64, real
64 MiB requested-core saturation, the exact physical image boundary, and a
naturally full image with 63 MiB of reachable payload. All current and retained
byte oracles passed after the tested lifecycle operations. No core limits or
APIs were expanded. These are observed cases, not general RSS, stack, latency,
deployment-device or production-lifetime guarantees.

The private [harness and commands](../tools/resource-envelope/README.md) reuse
the existing budget contracts and scenarios in `tests/live_budget.rs`,
`tests/headroom.rs`, `examples/recovery_scale.rs` and `tools/soak`, rather than
treating the small-state soak as a near-cap qualification. Source freezing and
no-overwrite helpers reuse the private SQLite-baseline runner. No publication,
remote service, hosted job or existing data file was used.

## Workload and independent oracle

The fixed workload has no unbounded command-line size parameters. Every phase
reports requested core live/peak/cap bytes, process RSS in KiB, file length,
revision, root count, timing and a digest of bytes read from actual values.
Rust checks every expected row, depth, payload byte, retained ID and revision.
An independent Python oracle reconstructs the expected bytes and checks the
observed-value digests. Payload bytes encode their epoch in the first eight
little-endian bytes; byte i thereafter is `(epoch*13+i)%251`. FNV-1a summarizes
the independently checked bytes, not a cryptographic integrity assertion.

Each final run requires exactly 20 ordered phase records and one final complete
record, 64 nested data-commit and 64 pin samples, three nested and three large
reopen samples, and the exact accepted-large-commit count. Missing phases,
wrong digests/counts and over-cap telemetry fail qualification, including with
Python optimization enabled.

| Case | Contract and result on both platforms |
| --- | --- |
| Nested/retained | One record containing 63 singleton lists over an 8 KiB byte leaf: depth exactly 64. Commit a distinct epoch and separately retain it 64 times. IDs 1..64 have revisions 2,4,..128; current revision is 129. The 65th pin and a depth-65 wrapper return Limit. Every current/retained leaf and list length is verified. |
| Requested-core pressure | Reopen under a 67,108,864-byte cap; hold 247 real drafts with distinct owned 256 KiB pending payloads, plus 33 smaller owned filler values. A previously prepared candidate must return Limit without changing acknowledged revision, visible rows or file length, and without requiring recovery. Drop pending work/drafts/fillers, successfully admit another payload, then release all 64 pins and reopen the current-only graph. |
| Synthetic exact image boundary | A separately verified copy of the 64-root graph receives an explicitly synthetic, densely written zero tail to 64 MiB minus 4096 bytes. One empty commit consumes the final region, a further commit returns Limit, and revision 130/all 64 roots survive. Offline compaction and reopen restore append headroom. This is an exact-boundary fixture, not 64 MiB of reachable data. |
| Reachable natural image limit | Under a separate explicit 256 MiB requested-core budget, append distinct 896 KiB values, retaining the first eight prefix roots. At most 80 attempts are permitted. Both runs accepted 72 values and rejected attempt 73 with Limit, preserving revision 81, current rows, all eight historical prefix maps and file length. No padding is used. Three reopens, verified backup, restore and offline compaction repeat the complete byte oracle. |

The reachable case stores **66,060,288 payload bytes (63 MiB)** in a file of
**67,014,656 bytes**, 99.86% of the unchanged 64 MiB image cap, with 94,208 bytes
of append headroom. Each retained prefix has exactly 1..8 expected rows and its
original revision 2,4,..16. The failed key is absent after rejection and all
subsequent reopens. Backup, restored and compacted files are 66,416,640 bytes and
still contain every current and retained byte. The 256 MiB heap budget is an
application configuration for this separate fixture, not an expanded image cap
or proof that these graphs can be built/recovered under a 64 MiB heap budget.

At total heap saturation, retained_ids enumeration itself needs charged scratch.
The oracle therefore uses the previously checked immutable IDs 1..64 and
allocation-free snapshot/point reads for that phase. It rechecks catalog
enumeration after releasing drafts. This avoids demanding a successful scratch
allocation from an exhausted budget; it does not bypass validity checks in core.

## Core layouts, RSS and reclamation

Core values below were identical on macOS and Linux. They are measurements of
the indicated open Memory domain, including its cache/metadata ownership, not
attributable byte counts for disjoint graph components.

| Phase | Live core bytes | Peak core bytes | Retained roots |
| --- | ---: | ---: | ---: |
| First depth-64 current/pinned root | 34,582 | 587,556 | 1 |
| Built 64 roots | 2,175,734 | 2,861,458 | 64 |
| Reopened 64-root graph | 2,184,632 | 2,603,290 | 64 |
| 247 held drafts | 66,979,099 | 66,979,099 | 64 |
| Saturated candidate rejected | 67,108,747 | 67,108,859 | 64 |
| Drafts/fillers dropped | 2,184,632 | 67,108,859 | 64 |
| Pins released, still open | 2,146,398 | 67,108,859 | 0 |
| Reopened current-only graph | 34,110 | 322,800 | 0 |
| Built near-full reachable graph | 66,095,472 | 67,955,952 | 8 |
| Reopened near-full graph | 66,103,992 | 67,152,824 | 8 |

The saturated live count is only 117 bytes below the 64 MiB budget; its peak is
five bytes below it. Rejection leaves the current/pinned oracle intact and draft
drop returns to the exact pre-draft live count. Pin release immediately frees
some catalog/head ownership, while the native identity cache deliberately keeps
committed Values until close. Close/reopen after release reclaims that cache and
reduces live core bytes to 34,110. Thus pin release alone is not claimed to
immediately reclaim all formerly pinned payloads. Subsequent 256 KiB/1 MiB
admission probes succeed after the corresponding releases.

RSS is deliberately separate:

| Observation | macOS RSS KiB | Linux RSS KiB |
| --- | ---: | ---: |
| Reopened 64-root graph, first sample | 28,448 | 7,264 |
| Saturated 64 MiB core domain | 90,256 | 68,544 |
| After draft/filler drop | 90,256 | 5,564 |
| After pin release and current-only reopen | 91,856 | 6,724 |
| Near-full graph, first reopen | 121,472 | 70,728 |
| Sampled main-process maximum over the run | 381,344 | 163,472 |

There were 135 macOS and 99 Linux main-process samples at approximately 50 ms
plus sampling overhead. These can miss transient peaks. The parent sampler does
not include the separate stack subprocess; that process reports its own phase
RSS. Core accounting excludes application/backend/output buffers, allocator
rounding/metadata, stacks and other documented exclusions. Inspection/copy/open
operations have independent Memory domains; the core peak shown after a copy is
the newly opened destination domain's peak, not the maximum across hidden copy
domains. Process RSS includes those operations and allocator retention across
lifecycles. The macOS RSS remaining high after core release is an observation,
not a demonstrated core leak or a general upper bound.

The Docker driver ran 5.301 seconds; the macOS driver ran 8.164 seconds. These
elapsed values exclude compilation and the launcher's later independent Python
digest validation. Linux data was on the container's measured writable
**overlayfs**, not a host bind mount or a qualified deployment ext4 device.
Docker's 2 GiB memory limit and 256-process limit are containment settings,
not engine RSS guarantees.

## Latency and file observations

Commit samples time the commit API after draft preparation; pins are separately
timed durable calls. All reopens have warm OS caches, without cache eviction.
Reopen-only and reopen-plus-full-Rust-byte-oracle times are distinct. The extra
phase digest and independent Python check are outside these timings. Quantiles
below are nearest rank over each finite sample set, in milliseconds.

| Operation | Samples/platform | macOS p50 / p95 / p99 | Linux p50 / p95 / p99 |
| --- | ---: | --- | --- |
| Nested data commit | 64 | 10.978 / 13.796 / 13.966 | 1.171 / 2.088 / 12.970 |
| Separate nested pin | 64 | 8.984 / 9.589 / 17.999 | 0.295 / 0.709 / 3.282 |
| Large reachable data commit | 72 | 10.178 / 13.208 / 18.174 | 4.213 / 5.895 / 7.156 |

The three reopen samples and single maintenance samples are reported directly,
not turned into claims about tail latency:

| Operation | macOS ms | Linux ms |
| --- | --- | --- |
| Nested open | 53.265 / 51.991 / 54.681 | 46.737 / 44.419 / 44.247 |
| Nested open + full oracle | 53.955 / 52.716 / 55.422 | 47.435 / 45.125 / 44.969 |
| Reachable open | 162.026 / 160.133 / 165.245 | 154.761 / 158.372 / 160.053 |
| Reachable open + full oracle | 246.903 / 244.887 / 249.568 | 240.099 / 245.061 / 245.094 |
| Reachable verified backup + open/oracle | 833.287 | 807.350 |
| Reachable verified restore + open/oracle | 832.804 | 781.118 |
| Reachable offline compact + reopen/oracle | 1,017.955 | 936.621 |
| Synthetic exact-cap compact + reopen/oracle | 297.277 | 242.857 |

The nested file was 18,124,800 bytes before pin release. Releasing pins appended
metadata and grew it to 18,386,944 bytes; release does not physically compact.
The independent synthetic file reached exactly 67,108,864 bytes and compacted to
17,588,224 bytes while retaining all 64 roots. File sizes are stat lengths, not
allocated-block/device-write measurements. Neither these timings nor Docker
fsync are a substitute for deployment filesystem/controller/power-loss evidence.
The Mac and Linux runs also ran concurrently, so this is not a controlled
cross-platform speed comparison.

## Deliberately bounded worker stack

On each platform, a dedicated subprocess starts one thread with requested stack
size **1,048,576 bytes**. That thread constructs depth 64, rejects a depth-65
wrapper, commits/retains, exports logical format, rejects importing that valid
depth-64 image under max_depth=63, imports it under max_depth=64, verifies both
decoded current/retained values, and reopens/verifies the native file. Both
subprocesses completed normally. Phase RSS was 4,432 KiB on Mac and 2,476 KiB on
Linux; those numbers are not stack sizes.

This qualifies those value/parser/native-open workloads at a deliberately
requested worker-stack setting. OS minimums/rounding/guard pages remain platform
behavior. No stack high-water mark, minimum required stack, all-input bound,
smaller-stack failure boundary or whole-engine stack guarantee was measured.

## Provenance, guards and remaining scope

Both builds archive explicit engine revision 96e105f, archive SHA-256
`64b04352c995246b1f3f1a528f1d7a16539f67f53aa04a13031794a601892528`.
The current six harness files are overlaid explicitly and separately hashed;
they are not attributed to the older engine commit. Runtime source ID and base
feature bits are asserted. Host compilation uses Mise Rust 1.99.1
(`58cf8f9c3`, LLVM 23.1.1), offline/locked release with overflow checks, and a
private target directory. The immutable Docker image reports the same Rust
compiler on aarch64 Linux. Host Python is Mise 3.14.8; the in-container observer
uses installed Python 3.11.2. The observed Docker VM kernel was Linux
7.0.14-linuxkit aarch64. macOS was 27.0.1 arm64 on the local APFS volume.

| Artifact | SHA-256 |
| --- | --- |
| `target/resource-envelope-96e105f-mac-bounded-final/evidence.json` | `2cb9d4a0e3166f898ed9483ee6c47d6bbf58e4d7b098f9f1e702c915ab79ff3f` |
| `target/resource-envelope-96e105f-linux-bounded-final/evidence.json` | `a9a5cd6e821b062b003389d6d2794d5c402932301e2233c156ed4566b8d8a4db` |
| Rust driver source | `075cfb94d5e37877aa9c460a5970d83da8df2818b1b981d64230efbbe7450ffb` |
| Python coordinator | `0162000591e5213a5a52edb2645dc60ddcbf7d1b4ec96aff349e84c6887822cf` |
| Python controls | `5a6dca93c84be85dbd76d02def40fc744e4b22537251dc8fd581ebd0575b35b7` |
| macOS executable | `4097121e64e3518d76b9fc4091574e3a01b288acc104a6f88066140d92965b2b` |
| Linux executable | `f0cc972e087364e7e226a44f8b63ae72e3f3a97df43f1b9407494c570ea2a9d2` |

Docker image ID:
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
Network was disabled; source was read-only; data/builds used a fresh writable
container layer. Artifact copies and bounded removal are recorded, with
successful daemon-live absence verification. Existing output paths, including
dangling symlinks, are rejected by the reused helper. The observer also rejects
existing data/report/log paths. It kills a timed-out native process group,
including descendants, and preserves terminal failure metadata. Production
checks and side effects survive Python `-O`. Local Cargo configuration remains
a trusted build input; known ambient Rust flags/wrappers/profile overrides are
scrubbed, while exact compiler/command/source/binary identities are retained.
The macOS ps sampler has a five-second subprocess timeout; its TimeoutExpired
path records failed terminal evidence and kills/reaps the controlled worker
process group. Setup probes/builds use a local 120-second default timeout;
Docker retains its separate outer/copy/removal limits. These bound the stated
subprocess waits under normal OS scheduling, not arbitrary kernel/filesystem
operations or overall kernel liveness. Both final platform runs were repeated
after this supervision fix; older final-named artifacts are historical only.

Strict driver Clippy/formatting passed. Six launcher/oracle controls passed
normally and with Python `-O`: no-overwrite, malformed-output failure metadata,
real descendant timeout cleanup, macOS sampler timeout/real-child reaping, a
fixed small digest, and frozen positive plus
corrupted digest/count/cap/root/missing-phase cases. An independent reviewing
agent inspected Rust/Python/cleanup semantics, reran all six optimized controls,
and independently rehashed all archived source files and six live overlays for
both final artifacts; no actionable finding remained. A failed earlier trial
that attempted charged catalog enumeration at saturation remains separate and
is not counted as a passing run.

Unqualified combinations remain: maximum reachable payload plus all 64 roots
each with distinct maximum-depth data simultaneously, breadth/entry/node limits,
all allocator/interleaving/failure paths, adversarial parser inputs, sustained
concurrency/load, cold-cache behavior, service budgets, Windows and other ABIs,
device interruption/power loss, and production-lifetime memory/stack guarantees.
This record closes these finite near-limit cases only.

Root review inspected the four distinct workloads, independent observed-byte
oracles and cache-versus-draft reclamation semantics. It also identified the
sampler supervision gap corrected before the final runs. Source-bound manifests
and independent-review/control logs are preserved in
`target/local-qualification-resource-envelope/index.json`.
