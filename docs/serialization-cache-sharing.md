# Immutable serialization caches and reusable indexes

Baseline `36592eec0f3e64c76f5cf58dcce305e8ef2a7ff7` and candidate
`910e4904e8f13a7bc8f0ef1401092ed2fb3b11c8` run the same
[compound profile](compound-resource-envelope.md), 2026-11-14. The candidate
preserves all **17 exact serialized lifecycle/history images**, all retained
roots and the directed recovery outcomes on Mac/Linux. It avoids cache entry
copies for metadata-only publications and avoids rebuilding indexes on every
commit. Persisting the indexes increases live core memory and adds cold-open
work; finite latency/RSS observations are reported separately below.

## Ownership and publication

Each VALUE cache owns its emission-order entries and identity hash index in one
immutable, charged `Shared` allocation. DATA uses a separate immutable cache
with its entries and revision-sorted positions. Separating the two lets a new
DATA object reuse every VALUE without copying the VALUE cache.

An encoder initially borrows each acknowledged cache. A cache hit reads it
without copying. The first actual append privately forks that cache's entries
and copies the existing index; further appends use the private copy. Because
entry order and Value identities remain unchanged, existing hash slots and
sorted positions remain valid without rehashing or sorting. Capacity growth
still rehashes VALUE entries when required. DATA revisions above the sorted
maximum append without a binary insertion search; older/equal revisions retain
the existing lower-bound insertion behavior. First-match duplicate behavior and
original cache/emission order remain unchanged.

Before returning a candidate, each borrowed cache is cloned as a shared handle;
each private cache is put into a fallibly allocated shared header. Every
allocation precedes backend writes. Allocation/Limit failure drops only private
working state and preserves the acknowledged caches, including their capacity.
The candidate installs its cache handles only after append, root and witness
barriers complete. I/O uncertainty retains the original poisoning/recovery
contract. The local commit and follower durable acknowledgment remain separate.
No new unsafe path, public API, byte format, configured maximum or lock is added.

Recovery verifies the selected graph as before, then native `Storage::open`
builds the two indexes once from recovered cache order, before stabilization
sync/witness I/O. Recovered DATA entries are sorted only in the separate index.
An index/header allocation failure rejects that open, drops private state and
releases the lease; it does not select an older root. The raw graph verifier
still returns entry vectors and does not build these encoder indexes. Read-only
processes using Database open therefore pay the new index construction even if
they never commit; broader lazy-index designs remain unimplemented.

## Work measurements

Disposable source instrumentation counts copied entries, successfully copied
index positions, VALUE entries visited by index rebuilds, DATA entries supplied
to index sorting, DATA comparisons, and existing DATA positions shifted at
insertion. Both revisions are compiled with the actual archived Rust source;
patch uniqueness and complete source-bound clean references are checked.
Instrumented phase/state/live-byte/file-size records must match the corresponding
clean run. These counters do not change layouts or production code. Instrumented
latency/RSS are excluded from performance comparisons.

| Native lifecycle work | Baseline | Candidate |
|---|---:|---:|
| VALUE entries cloned | 411,664 | 269,672 |
| DATA entries cloned | 14,898 | 9,330 |
| VALUE entries visited by rebuild | 444,476 | 62,132 |
| DATA entries supplied to sorting | 14,898 | 454 |
| DATA comparisons | 87,653 | 62,213 |
| VALUE hash slots copied | 0 | 893,536 |
| DATA sorted positions copied | 0 | 9,330 |
| Existing DATA positions shifted | 192 | 192 |

VALUE entry clones decrease **34.5%**, rebuild visits **86.0%**, and DATA sort
input **97.0%**. Copying 893,536 integer hash slots is new work, about 6.82 MiB
of slot payload; it cannot be equated to the old hash/probe/refcount work by
counting elements. New-value commits still clone the whole affected VALUE cache.
This change does not make every commit constant-time or remove all cache copying.

| Other process | Work | Baseline → candidate |
|---|---|---|
| Fresh inspector | VALUE rebuild visits | 0 → 4,192 |
| Fresh inspector | DATA sort input / comparisons | 0 / 0 → 65 / 367 |
| Eight-case simulator | VALUE entry clones | 67,072 → 50,304 |
| Eight-case simulator | DATA entry clones | 1,040 → 780 |
| Eight-case simulator | VALUE rebuild visits | 67,072 → 104,800 |
| Eight-case simulator | DATA sort input / comparisons | 1,040 / 15,316 → 1,625 / 18,535 |

Repeated recovery adds index work in the fault worker. This is an intentional
shift from per-publication rebuilds toward retained indexes and cold-open cost,
not evidence that recovery work has decreased. Raw counters, domains and source/
binary hashes are retained in the local proof index.

## Compound memory, latency and state

The same 48 large values and 64 depth-64 retained roots survive eight held
drafts, aggregate 96 MiB saturation, 32 commits from eight writers, two readers,
256 reader/oracle iterations and a physical image reaching **98.58%** of 64 MiB.
File sizes stay 63,401,984 before contention and 66,154,496 afterward. Rejected
oversized publication preserves state and physical length. Each fresh-process
oracle checks complete current/retained bytes and independent fingerprints.

| Requested core bytes, both OSes | Baseline | Candidate |
|---|---:|---:|
| Reopened fixture | 48,062,216 | 48,194,008 |
| Eight held drafts and candidate | 64,909,172 | 65,040,964 |
| Saturated rejection, live | 100,596,360 | 100,596,760 |
| Saturated domain peak | 100,663,260 | 100,663,269 |
| Drafts/fillers released | 48,062,216 | 48,194,008 |
| After contention | 50,191,944 | 50,324,248 |
| Reopened after contention | 48,612,598 | 48,744,902 |

The 100,663,296-byte limit holds, with **27 bytes** of peak headroom. Dropping
pending work restores the exact pre-draft charge. Persisted indexes/headers
increase reopened live core by **131,792 bytes (128.7 KiB)** initially and
132,304 bytes afterward. Tight budget thresholds can change; every owned
capacity/header remains charged.

The three 256 MiB copy domains peak at baseline **150,179,254** bytes each;
candidate backup/restore remain the same, while compact-copy reaches
**150,311,558** bytes. Clean exact-rlib backup checks at **160 MiB** succeed on
both revisions/platforms, with source hashes unchanged. Candidate destinations
are reopened in a separate process and checked in full. The ordinary 96 MiB
copy rejection and 256 MiB backup/restore/compact success checks still pass.

| Platform / revision | Primary commit median / p95 ms | Repeat median / p95 ms | Primary / repeat peak RSS KiB |
|---|---|---|---|
| macOS baseline | 44.766 / 85.788 | 33.788 / 64.858 | 464,064 / 458,272 |
| macOS candidate | 35.824 / 72.756 | 40.099 / 77.153 | 461,088 / 459,680 |
| Linux baseline | 3.234 / 5.610 | 2.893 / 4.704 | 165,828 / 168,472 |
| Linux candidate | 10.413 / 20.784 | 3.712 / 6.385 | 163,620 / 149,588 |

A further isolated Linux block runs baseline/candidate/candidate/baseline
sequentially without other qualification workers. Commit median / p95 is
2.977 / 4.989, 2.789 / 4.669, 2.694 / 4.619, 2.764 / 4.787 ms
respectively. Corresponding sampled RSS is 166,056 / 173,620 / 153,652 / 154,672 KiB.
The initial Linux candidate is substantially slower; that difference does not
repeat in the isolated block. The timing ranges overlap, and repeat Mac results
reverse the initial ordering. No consistent throughput/RSS win is established.


Each commit summary uses 32 contended commits; median / nearest-rank p95 is in
milliseconds. RSS is sampled native-worker-only KiB, includes caller/oracle,
allocator and copy domains, and excludes coordinator/device caches. Primary
Mac/Linux runs share the host and overlap other qualification work. Repeat
workers run sequentially per platform. Warm-cache samples and roughly 50 ms
RSS sampling do not establish causal attribution, a platform ranking, worst
case or service SLO. Slower samples remain recorded; counters are not a general
throughput or process-memory speedup claim.

## Failure cases and decision controls

All eight compound simulator cases retain two additional drafts at over 90%
core pressure. Allocation, append-drop/persist and root-drop first reopen **212**;
surviving root, either witness failure and acknowledged publication reopen
**213**. Retry/later acknowledged publication survives another crash/reopen as
**213/214**, with every retained root intact. These are directed simulated
persistence failures plus native fresh-process reopening, not device power loss
or an exhaustive second-failure/maintenance sweep.

Two new cache tests verify allocation-free shared handles, both entry/index
allocations and the final shared-header allocation, exact first-match index
copies, complete rollback/reclamation at each injected failure, metadata reuse,
DATA-only forking and every allocation in a new-value candidate. Existing
allocation/budget, retained-root, copy, native process interruption, publication,
recovery and follower-ack tests are rerun. Cache/header failure is before I/O;
uncertain publication still requires reopen.

The growth guard's remaining **`Err(Limit)` with `capacity == needed`** case is
closed in `geometric_growth_exact_fallback_preserves_budget_and_failure_atomicity`.
Both pair members start with length/capacity four, live limit nine and zero
allocation claims. Needing eight yields an exact rounded size eight: admission
returns Limit and the guard is false; no fallback occurs, bytes/capacity/live
charge stay unchanged. Needing five rounds to eight and returns the same first
Limit, but the guard is true: exact size five is admitted and its disabled
allocation returns OutOfMemory. The peak is nine and the old live charge stays
four. Re-enabling allocation allows that exact fallback to succeed. Thus the
matched error is held fixed while the guard changes the selected branch.
This closes the named manual decision pair, not a compiler MC/DC percentage.

## Validation, provenance and remaining scope

- Rust 1.99.1, locked/offline: **307** core-package tests per Mac debug/release
  profile; **311** per Linux profile, plus two separate golden-image exports.
  Each profile has one intentionally ignored parent-invoked subprocess helper.
  All 17 candidate images compare exactly to the baseline on both platforms.
- Remaining Mac workspace packages pass **126** tests, including typed/macro,
  CRDT, authenticated replication, coordinator capacity/restart and app cases.
  All **25** deterministic fuzz replays pass; this is not a new extended campaign.
- The completed growth pair and both cache regressions pass **Miri
  nightly-2026-11-12**. Strict workspace/all-feature/all-target Clippy,
  formatting and whitespace checks pass. Two cache-probe controls run normally
  and optimized; the full helper registry passes **334 tests + four registered
  optional-evidence skips**, 338 reported across **48 workers**.
- Fresh AST inventory: **2,604 records / 51 files**, 319 macro blind spots and
  zero parse failures. Historical mappings, source inventory and compiler MC/DC
  remain separate. The new unreachable macro has a documented conversion
  invariant.
- macOS arm64/APFS; Docker Linux aarch64/private ext4 `/dev/vda1`, immutable
  image `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
  Network disabled, two CPUs, 256 PIDs, 2 GiB for compound workers / 3 GiB for
  full tests, read-only registry cache. Host Python is Mise 3.14.8 and the native
  Docker observer is image-pinned Python 3.11.2. Docker containers/volumes and
  native owned process groups are verified retired.


Baseline/final engine archives, copied sources, binaries, physical artifacts,
clean and instrumented reports, exact images, copy consumers and checks are
rehashed in `target/cache-qualification/summary.json`, SHA-256 `9ec06a0f026653ccadb3ef3d83108e18636d978605bbc2d51544c11746fc4007`.
These ignored local proofs are not a permanently hosted release bundle.
The [memory](decisions/memory.md#growth-equality-limit-pair-closed) and
[codec](decisions/codec.md#immutable-cache-sharing-follow-up) ledgers retain
historical audit scopes and map this follow-up.

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/resource-envelope/compound.py --revision 36592ee --output target/cache-old-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/resource-envelope/compound.py --revision 910e490 --output target/cache-new-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/resource-envelope/cache_probe.py --source target/cache-new-fresh/source --reference target/cache-new-fresh/run/evidence.json --revision 910e4904e8f13a7bc8f0ef1401092ed2fb3b11c8 --output target/cache-work-fresh
```

Use the immutable Docker image in the compound report via `--docker-image`
for Linux. The cache probe adds private counters only in its fresh source copy;
`test_cache_probe.py` runs current/synthetic legacy patch controls and malformed/
regressing evidence rejection normally and under optimized Python. Actual old
and new instrumented Rust binaries are compiled in the qualification runs.

Remaining costs include affected-cache forks, copying large sparse hash tables,
cold-open index construction, out-of-order insertion and broader DAG/history
shapes. Next assurance work includes interrupted near-cap maintenance and second
failures during recovery, longer schedules, owner memory/latency budgets and
one fixed platform/artifact candidate. No new native Windows qualification is
inferred from Mac/Linux results or the historical Windows pass. All **19**
package manifests retain `publish = false`; no crates are published.
