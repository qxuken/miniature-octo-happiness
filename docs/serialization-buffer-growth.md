# Charged serialization-buffer growth

On 2026-11-14, baseline `fe4d057c7432c7e19f0352d4ee7a86edffffcfd2`
and candidate `742079cad1cd7683395f027d2ca60f75b7da31a8` pass the same
[compound workload](compound-resource-envelope.md) on macOS arm64/APFS and
Docker Linux aarch64/ext4. Seventeen baseline lifecycle/history images remain
byte-for-byte identical. Copy-domain requested-layout peak decreases from
**166.76 MiB to 143.22 MiB**; a verified backup now succeeds at a **160 MiB**
budget where the baseline rejects. Reopened cache capacity costs about 126 KiB
more live core memory. These are workload-specific results.

## Policy and limits

`Memory::reserve_geometric` selects the next power-of-two capacity, clamped to
an explicit maximum, through the existing fallible `ChargedVec` allocator.
Checked length arithmetic and the maximum precede allocation. Existing spare
capacity consumes no allocation claim. The full new layout is admitted while
the old layout remains charged, including unused capacity. Failed admission or
allocation preserves the original buffer and rolls back new ownership. A
rounded request returning `Limit` retries the exact needed capacity if smaller;
`OutOfMemory` propagates without a smaller retry. Previously retained slack
can still make a tight budget reject; fallback does not guarantee identical
admission to the old implementation for every workload.

The codec uses this for incremental Writer fields, the aggregate encoded byte
vector, encoder VALUE/DATA cache copies and appends, and decoder VALUE/DATA
appends. Cache maxima remain 16,384 entries; aggregate bytes remain bounded by
the remaining portion of the 64 MiB image. A Writer chunk larger than its
current logical length reserves its known size exactly. Such a bulk append
already more than doubles length; rounding it adds unused large-payload capacity.
Known-size final DATA projection remains exact. VALUE hash indexing, DATA sort/
search, cache cloning and rebuild algorithms are unchanged.

No new unsafe allocation path, public API, disk/wire format or configured limit
is introduced. Cache order, emission order, offsets, padding, checksums and root
selection stay unchanged. Publication barriers and the separate local-commit
and follower-durability acknowledgments retain their existing contracts.

## Same compound state and failures

Both revisions retain all 64 depth-64 roots over 48 independently allocated
896 KiB values. Eight threads hold two 1 MiB pending values each, then a candidate
and owned fillers saturate a 96 MiB domain. Publication rejects atomically;
dropping pending work returns exactly to the pre-draft charge. Eight writers
commit 32 updates with two concurrent readers and 256 measured current/pinned
oracle iterations. The image grows from 63,401,984 to 66,154,496 bytes (98.58%
of capacity). Oversized publication preserves physical length and logical state.

Every current/retained value passes the existing full Rust byte oracle and
independent Python fingerprints after native fresh-process reopen, backup,
restore and compact-copy. The ordinary 96 MiB copy rejects with an empty target;
256 MiB succeeds with a 63,074,304-byte image. A separate clean consumer linked
to each measured binary's exact rlib checks **160 MiB**: baseline `Limit`/empty
target; candidate success/63,074,304 bytes, followed by another full inspector
process. Both source files are hashed before/after and unchanged, on both OSes.
160 MiB is a demonstrated successful budget, not the minimum required budget.

The eight existing compound simulator cases pass with two extra drafts and
over 90% live-core pressure. Allocation, append-drop, append-persist and
root-drop first reopen revision **212**; surviving root, either witness failure
and an acknowledged commit reopen **213**. Retry/later acknowledged publication
survives another crash/reopen as **213/214**, preserving every retained root.
Existing allocation sweeps, copy/verification failures, retained-catalog faults,
native process interruption and follower-ack recovery tests also pass. This is
directed simulated persistence plus native process evidence, not device power
loss or exhaustive compound double-failure qualification.

## Requested memory and allocator work

Core live bytes match across both platforms:

| Phase | Baseline bytes | Candidate bytes |
|---|---:|---:|
| Reopened fixture | 47,933,448 | 48,062,216 |
| Eight held drafts and candidate | 64,780,404 | 64,909,172 |
| Saturated rejection, live | 100,596,648 | 100,596,360 |
| Peak in saturated domain | 100,663,189 | 100,663,260 |
| Drafts/fillers dropped | 47,933,448 | 48,062,216 |
| After 32 commits | 50,065,480 | 50,191,944 |
| Reopened after contention | 48,484,598 | 48,612,598 |

The limit is 100,663,296 bytes; candidate peak stays **36 bytes below it**.
Slack is charged and released with its owner. The extra reopened live capacity
is 128,768 bytes initially and 128,000 bytes after contention. Copy APIs use
separate domains, so their peaks need separate observation.

Disposable source instrumentation counts successful `ChargedVec` grow calls,
sums their prior allocated layout sizes and requested new layout sizes, and
reports domain peaks at final destruction. It changes no fields, layouts,
allocation policy or production source. Instrumented runs must reproduce clean
phase/state/live-byte/file-size records. The helper first validates full clean
profiles, source ID and source hashes; truncated references reject. Their latency/RSS are excluded below.

| Process | Successful grows, baseline → candidate | Sum of prior layouts, bytes | Sum of new requests, bytes |
|---|---|---|---|
| Native lifecycle | 423,138 → 231,839 | 693,104,277,968 → 270,348,360 | 695,255,603,203 → 2,482,364,603 |
| Fresh inspector | 16,511 → 12,276 | 281,198,592 → 268,208 | 390,288,710 → 109,489,350 |
| Eight-case simulator worker | 421,059 → 308,244 | 7,045,538,748 → 6,796,144 | 10,119,581,074 → 3,082,565,622 |

The prior-layout sum measures potential allocator preservation/copy extent;
**it is not measured bytes copied** because realloc may extend in place. New
request sums are cumulative work, not simultaneous heap usage or RSS. Counters
cover all `ChargedVec` buffers, not every shared/core/backend/std allocation.
Native grow calls decrease 45.2%. Each of the three 256 MiB copy domains peaks
at **174,858,038 → 150,179,254 bytes**, a 23.54 MiB (14.1%) reduction.

## Finite uninstrumented latency and RSS

Each primary and repeat uses a fresh native worker and the same fixture,
32 commits and 256 reader/oracle samples. Commit milliseconds include waiting
for synchronization and native flushes, and exclude draft construction.
Each table entry is median / nearest-rank p95. RSS is sampled worker-only KiB;
it includes caller/oracle buffers, allocator retention and copy domains.

| Platform / revision | Primary commit ms | Repeat commit ms | Primary / repeat native peak RSS KiB |
|---|---|---|---|
| macOS baseline | 36.763 / 71.917 | 36.796 / 72.149 | 456,080 / 458,384 |
| macOS candidate | 37.797 / 71.873 | 45.466 / 91.069 | 462,544 / 466,736 |
| Linux baseline | 3.267 / 5.650 | 3.039 / 5.158 | 175,632 / 175,264 |
| Linux candidate | 3.162 / 5.360 | 2.903 / 4.817 | 165,120 / 158,472 |

Warm-cache observations overlap some other qualification work; Docker shares
the host. The macOS repeat is slower and has higher RSS; Linux samples improve.
A further sequential Mac block runs baseline/candidate/candidate/baseline
without other qualification workers: commit median / p95 is 39.829 / 79.721,
30.871 / 60.633, 34.675 / 65.533 and 28.075 / 50.864 ms respectively. Sampled
RSS peaks are 454,256 / 459,488 / 459,536 / 452,448 KiB. The timing ranges
overlap and shift within a revision, while candidate Mac RSS remains higher.
These samples do not establish a consistent throughput/RSS benefit, causal
attribution, a platform ranking or a service latency distribution. A 50 ms RSS
sampler can miss peaks, and core admission does not cap process RSS.

| One primary operation, ms | Mac baseline → candidate | Linux baseline → candidate |
|---|---|---|
| Reopen | 158.044 → 171.735 | 148.613 → 149.339 |
| Backup at 256 MiB | 542.077 → 534.404 | 498.620 → 490.607 |
| Restore | 554.943 → 540.801 | 505.795 → 498.009 |
| Compact copied file | 858.419 → 821.996 | 693.430 → 659.802 |

Timers exclude subsequent independent oracles and successful-copy destination
reopening. Individual maintenance samples are not percentile estimates. Primary
fresh-inspector RSS is Mac 51,728 → 51,504 KiB and Linux 51,232 → 51,164 KiB;
simulator RSS is Mac 746,656 → 745,280 KiB and Linux 369,256 → 352,000 KiB.
Simulator backend copies lie outside core accounting.

## Checks and provenance

- Rust 1.99.1, locked/offline: **305** core-package tests per Mac debug/release
  profile and **309** per Linux profile. Each has one intentionally ignored
  subprocess helper invoked by its parent. Linux separately exports two golden
  fixtures. All 17 images compare exactly against the baseline on both OSes.
- Three new growth/bulk tests cover 4,096 incremental bytes using 13 claims,
  contents, clamp/spare/ZST/overflow guards, exact fallback at tight budgets,
  both-buffer peak, rollback, injected allocation failure and actual allocator
  null without retry. All three pass Miri `nightly-2026-11-12`.
- All **25** deterministic fuzz replays pass. This is replay, not a new extended
  fuzz campaign. The helper registry passes **330** tests plus four registered
  optional-evidence skips across **46** normal/optimized Python workers.
- Strict workspace/all-feature/all-target Clippy, formatting and whitespace
  checks pass. Fresh AST inventory: **2,584 records / 51 files**, 318 macro blind
  spots, zero parse failures. [Memory](decisions/memory.md#charged-geometric-growth-follow-up)
  and [codec](decisions/codec.md#buffer-growth-follow-up) follow-ups map the new
  guards; historical tables remain scoped. No compiler MC/DC score is claimed.
- macOS arm64/APFS and Linux aarch64/private ext4 `/dev/vda1`, immutable image
  `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
  Docker uses no network, two CPUs, 256 PIDs, 2 GiB for compound workers and
  3 GiB for full tests, with a read-only registry cache for tests. Owned groups,
  containers and volumes are verified retired. Host Python is Mise 3.14.8;
  the native Docker observer is image-pinned Python 3.11.2.

The main policy checkpoint is `638654e`; `742079c` adds exact Writer bulk
reservation and its regression. All final candidate figures above use
`742079c`. Local proof index: `target/growth-qualification/summary.json`; its
SHA-256 is
`0cd870cee6f79f9057d099a0c1ca16d51bd49dee5db816e576b2ffa74f99820f`.
It binds both clean platform archives/binaries/artifacts, copy consumers, disposable instrumentation, golden
images, test/helper logs and the current source inventory. These ignored local
proofs are not a permanent hosted release bundle. The source inventory remains
an audit denominator; generated/dependency code retains its separate scope.

## Reproduction and remaining work

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/resource-envelope/compound.py --revision fe4d057 --output target/growth-old-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/resource-envelope/compound.py --revision 742079c --output target/growth-new-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/resource-envelope/growth_probe.py --source target/growth-new-fresh/source --reference target/growth-new-fresh/run/evidence.json --revision 742079cad1cd7683395f027d2ca60f75b7da31a8 --output target/growth-work-fresh
mise exec rust@1.99.1 -- cargo test --offline --locked -p weekend-db --all-features
mise exec rust@1.99.1 -- cargo test --offline --locked -p weekend-db --all-features --release
mise exec rust@1.99.1 -- cargo test --offline --locked --manifest-path fuzz/Cargo.toml --lib
```

Add `--docker-image` with the immutable image above to a compound invocation
for Linux. For a clean 160 MiB copy check, compile `growth_copy.rs` with the
matching pinned rustc, edition 2024, opt-level 3, overflow-checks=yes,
`--extern weekend_db=.../run/build/release/deps/libweekend_db.rlib` and
`-L dependency=.../run/build/release/deps`; pass source, fresh destination,
`limit`/`ok` and the full source ID. Use the same compound binary's `inspect`
mode to check the successful destination in a fresh process.

Remaining work includes cache cloning/index rebuild/sort and out-of-order
insertion, other feasible resource shapes, longer schedules, compound second
failures/interrupted maintenance, owner RSS/latency budgets and one immutable
candidate's full platform/artifact matrix. Earlier Windows results retain their
original source/unavailable-proof status; this change needs native Windows
qualification. All **19** package manifests retain `publish = false`;
no crates are published.
