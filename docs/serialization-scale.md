# Bounded serialization scaling experiment

This experiment measures the existing encoder before optimization. It changes
no production source. Baseline engine:
`032dadf591b0a0ded4894898e8208283c50425fb`. New private driver files are explicitly
overlaid onto a disposable `git archive`; instrumented copies additionally
change only `src/storage/codec.rs` and `src/storage/mod.rs`.

## Workload and independent oracle

Each fresh process creates one collection with 128, 512 or 2,048 keys. Every
accepted data epoch replaces every value and retains that epoch, up to eight
separate data revisions. Values are 16 bytes: an independently constructed
big-endian epoch followed by eight `0x5a` bytes. In **distinct** mode each key
gets a separately allocated value with those bytes. In **shared** mode every key
clones one identity for that epoch. Thus both shapes have the same logical data
for the same accepted history; identity sharing, not value contents, differs.

After every commit/retain, an independent BTreeSet/key-range and full byte
oracle checks all current rows and every retained root, exact revisions and
retained IDs. The same complete oracle runs after native reopen, backup,
restored-copy reopen and in-place compaction. Copies use fresh destinations.
Clean and instrumented executions must agree on accepted history, limit outcome
and physical file sizes. This is not an encoder/decoder round-trip-only oracle.

The driver and launcher independently require exactly eight accepted epochs
without Limit except the specified distinct/2,048 case, which must accept seven
and reject attempt eight. Missing or reordered maintenance phases reject.
The 64 MiB image cap is unchanged. The 2,048-distinct case accepts seven retained
data epochs; epoch eight returns `Limit`. It verifies unchanged physical length
and complete last-accepted current/retained state before proceeding through all
maintenance. The Limit guard also proves `before_bytes + breadth * REGION > MAX_IMAGE_BYTES`
using the minimum new distinct VALUE regions: 59,301,888 + 8,388,608 exceeds
67,108,864. Recorded append headroom is 7,806,976 bytes; that native
Mac point-in-time core live/peak charge is 5,043,080/22,668,568 bytes.
These charge readings are not whole-process RSS or all-domain peaks.
Its final revision is 15; other cases finish at revision 17 with
eight retained roots. The initial exploratory driver treated that documented
limit as an unexpected failure; that run is retained separately and is not a
passing qualification. The final driver handles and reports it explicitly.

The requested core heap limit is 128 MiB; entry/write/observation limits are
4,096. These do not cap process RSS, backend buffers, caller-owned scan/oracle
outputs, allocator retention or multiple independent database/copy domains.
No image cap was increased to force the eighth distinct epoch through.

## Instrumentation and controls

Disposable instrumentation increments relaxed atomic counters inside the two
actual encoder `.find` predicates: value node identity and DATA revision.
Every examined entry counts, including a successful early hit; an empty cache
counts zero. Counters are read once and printed at process end through an
experiment-only accessor in the disposable storage module. Neither accessor
nor counter enters the production checkout or release artifact. Clean builds
have neither; their zero counter fields mean **not instrumented**, not no work.

The instruments preserve cache vectors, lookup outcomes, allocation paths and
emission order. Atomic increments perturb runtime, so timings below use the
paired **uninstrumented** runs. Both native platforms obtained identical counts.
For the complete-eight histories, four serializations (initial history,
backup, restore and compaction) give an independent arithmetic control:
`4 * N * (N - 1) / 2` distinct comparisons, where `N = 8 * breadth`;
shared comparisons are `4 * (breadth * (1 + ... + 8) - 8)`. All full-history
counts match. The limited case additionally counts abandoned preparation and
is not compared as if it completed eight epochs.

| Shape | Breadth | Accepted epochs | Value comparisons | DATA comparisons | Before / compact bytes |
|---|---:|---:|---:|---:|---:|
| distinct | 128 | 8 | 2,095,104 | 516 | 4,362,240 / 4,284,416 |
| shared | 128 | 8 | 18,400 | 516 | 200,704 / 122,880 |
| distinct | 512 | 8 | 33,546,240 | 516 | 17,043,456 / 16,965,632 |
| shared | 512 | 8 | 73,696 | 516 | 299,008 / 221,184 |
| distinct | 2,048 | 7 | 440,169,243 | 391 | 59,301,888 / 59,232,256 |
| shared | 2,048 | 8 | 294,880 | 516 | 692,224 / 614,400 |

## Observed native costs

Rust 1.99.1 (`58cf8f9c35b355199727627d07ae34a4e2846d67`), LLVM 23.1.1,
release optimization with overflow checks enabled. macOS arm64 used APFS
(`/dev/disk3s5`); Linux aarch64 used immutable Docker image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
network disabled, with actual **overlayfs** data files in the container's
private writable layer. This is not an ext4 or physical-device qualification.

Each row is one finite clean process, not a distribution or p99 claim. Commit
milliseconds sum all attempts, including the rejected eighth attempt where
applicable. Reopen is the original-file open; backup/restore/compact are timed
separately. Complete verification remains outside those operation intervals.
Whole-process time includes retained-root commits and all oracle work. RSS was
sampled externally across the entire lifecycle, including maintenance, at a
requested 20 ms interval (plus probe cost); short peaks can be missed. It is not
attributed to a particular hidden allocation domain or phase.

| OS | Shape/breadth | Commit attempts ms | Reopen ms | Backup ms | Restore ms | Compact ms | Process s | Sampled RSS MiB |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| macOS | distinct / 128 | 359.13 | 26.38 | 170.19 | 168.19 | 196.27 | 1.576 | 19.72 |
| macOS | shared / 128 | 214.05 | 15.85 | 43.98 | 41.41 | 87.73 | 0.793 | 4.25 |
| macOS | distinct / 512 | 631.39 | 61.70 | 482.63 | 453.12 | 620.10 | 2.773 | 60.80 |
| macOS | shared / 512 | 211.93 | 19.52 | 72.40 | 47.44 | 98.25 | 0.848 | 7.09 |
| macOS | distinct / 2048 | 1341.64 | 193.47 | 1934.46 | 1749.84 | 1887.83 | 8.184 | 252.69 |
| macOS | shared / 2048 | 234.18 | 18.96 | 75.20 | 63.50 | 142.73 | 1.062 | 13.69 |
| Linux | distinct / 128 | 40.88 | 12.61 | 64.34 | 59.22 | 64.37 | 0.381 | 7.38 |
| Linux | shared / 128 | 51.92 | 3.79 | 4.52 | 4.00 | 16.28 | 0.227 | 2.70 |
| Linux | distinct / 512 | 97.00 | 58.93 | 174.22 | 166.76 | 221.39 | 0.923 | 22.95 |
| Linux | shared / 512 | 42.81 | 3.12 | 7.20 | 12.24 | 19.34 | 0.223 | 5.32 |
| Linux | distinct / 2048 | 374.66 | 160.17 | 681.02 | 604.68 | 733.02 | 3.245 | 82.43 |
| Linux | shared / 2048 | 37.45 | 6.09 | 21.94 | 20.65 | 30.03 | 0.274 | 13.32 |

The distinct 128→512 breadth increase produces approximately 16× value-cache
comparisons, versus approximately 4× for shared values. This is evidence of
quadratic identity-search work in the existing implementation. It is **not** a
16× end-to-end slowdown: native I/O, page layout, decoding and oracle work also
matter. Distinct tiny values occupy separate protected regions, explaining much
of the image amplification and why the largest history hits the cap.
DATA comparisons stay 516 for eight epochs regardless of breadth; this run does
not demonstrate a DATA-cache performance problem. It also does not measure the
64-retained-root history-scaling axis.

## Reproduction and provenance

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/serialization-scale/test_runner.py
mise exec rust@1.99.1 python@3.14.8 -- python -O tools/serialization-scale/test_runner.py
mise exec rust@1.99.1 python@3.14.8 -- python tools/serialization-scale/run.py \
  --revision 032dadf591b0a0ded4894898e8208283c50425fb --output /absolute/new/output
# Add --docker-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08 for Linux.
```

Four launcher controls pass normally and under `-O`: existing/dangling output
rejection before external work, build override neutralization, uniquely
matched instrumentation/reapplication rejection, and full phase/history outcome
validation (including rejected early Limit, wrong shape/width and skipped backup). The Rust driver passes strict
Clippy. Fresh output is required. Each workload has a 180-second timeout with
process-group kill/reap; build/probe commands have bounded waits, and macOS RSS
probes have a five-second timeout. Docker has a 900-second outer timeout and its
UUID container is removed with absence checked. These are user-space bounds,
not guarantees about a hung kernel or Docker daemon.

Known Rust/compiler/profile/preload overrides are removed. Rustup's installation
location is preserved. Cargo configuration remains a trusted input; discovered
configuration hashes, toolchain identity, archive hash, explicit driver overlays,
all clean/instrumented source hashes and binary hashes are recorded. Locked,
offline builds are used. Current runner SHA256:
`31f3f17b337446be1bcdf2d521b43f7d39e4538a80c6ea80def34e6e2c485609`.
Instrumented codec SHA256:
`d7b7971d216efa91cd9988cbc6df3c58ccb1be17e00604391dddba312bb1d009`;
instrumented storage module:
`4a34025974c910291be2d9fd69c293965375214989191e4ef59476825157b1da`.

| Evidence | SHA256 |
|---|---|
| `target/serialization-scale-mac-strict/evidence.json` | `f7eb96dea5a1634e92262c3be697c1ccbc7627179fef3e47edb5496bc260b57f` |
| `target/serialization-scale-linux-strict/evidence.json` | `b699699ff7fbc956b0c5b38072599546a2cf6d631df9ea0a1f975e89c1d8f63f` |

Each top-level evidence file binds its `run/evidence.json`, whose phase events,
RSS samples, binary and source hashes cover all twelve clean/instrumented
processes. `target/serialization-scale-summary-strict.json` retains the extracted table
and arithmetic checks. The earlier Mac/Linux paired runs retain their original hashes and weaker
outcome-admission checks. The strict fresh runs above replace them for acceptance;
counts match, but no old result is relabeled as testing the strengthened driver.

## Decision before optimization

A bounded charged VALUE identity index is worth a separately reviewed experiment.
Preserve cache vectors and emission order, qualify index rebuild from prior
caches, and compare exact fixed-identity simulator image bytes with this baseline.
Revisit allocation-failure atomicity and aggregate core-budget boundaries because
index storage introduces new fallible allocations and peak charges. Repeat the
clean operation/retained-state oracles, paired timings, crash tests and compatible
format fixtures. DATA indexing needs a separate rationale or history-scaling
measurement; its low count here is not evidence that it must change. No optimizer,
new public production API, durability barrier or file-format change is included
in this experiment.

The later [history-axis experiment](serialization-history-scale.md) supplies
that separate DATA measurement through 64 retained roots, including complete
current/retained and maintenance oracles. Its 108,036 lifecycle comparisons
at 64 roots expose history cost; no production index or speedup is inferred.

The subsequent [charged VALUE index](serialization-identity-index.md) is now
implemented and qualified separately. It preserves exact baseline bytes and
reruns both axes, allocation/budget/crash and retained-root checks. This report
continues to describe the original linear encoder and its original evidence.
