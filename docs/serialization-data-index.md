# Charged DATA revision index

This follow-up replaces encoder DATA linear lookup with a temporary sorted
index of cache positions. Baseline:
`fcfaf23e78638364cf234efdf1d44968755e163f`, which already includes the
[VALUE identity index](serialization-identity-index.md). Qualification ran on
2026-11-14. The cache, traversal and publication order stay authoritative;
sorting the index never sorts or emits DATA objects.

## Representation and resource bounds

`DataIndex` owns a `ChargedVec<usize>` sorted by `(data revision, original cache
position)`. It is rebuilt from the input cache on every encode, including
recovery's current-first ordering. Binary lower-bound lookup preserves the
original first-match rule for duplicate internal cache entries. The cached
`next_collection` check remains in force: finding the right revision never
bypasses counter validation. Zero, nonconsecutive and maximum u64 revision keys
need no arithmetic conversion or subtraction.

New entries still append to the original DATA vector after object preparation.
Only their indices are inserted into revision order. Binary search takes
O(log H) revision comparisons for H cache entries; rebuilding sorts H indices
in O(H log H) worst-case comparisons. Inserting an out-of-order revision can
move O(H) indices; appending a new largest revision moves no existing indices.
Cache cloning, sorting/rebuilding, VALUE indexing, byte-buffer/cache-vector
growth, decoding and CRC remain costs. This change removes the repeated linear
revision search; it does not establish a bound on complete commit latency.

Capacity grows geometrically through the existing fallible, budget-charged
allocator. The physical object limit bounds H to 16,384. On supported 64-bit
targets maximum index capacity is 131,072 bytes; largest old/new growth overlap
charges 196,608 bytes. At 65 entries, rounded capacity is 128 slots / 1,024 bytes.
Sorting uses the nonallocating unstable sort with the original position as a
tie-breaker. No new unsafe code, dependency, public API, file or wire format is
introduced. A tight live budget can reject preparation earlier with `Limit`.

Failed initial allocation or growth releases temporary charges and preserves
the old index. Any later preparation failure abandons the candidate before
publication I/O. The index is dropped before the candidate is published; no
index is persisted in Storage or on disk. Durability barriers, transaction
terminal/retry semantics and separate follower durability acknowledgment remain
unchanged.

## Exact bytes and retained history

The original ten-image `serialization_identity_golden_lifecycle` retains its
unchanged fingerprints. New `serialization_data_golden_history` was captured
with the linear DATA encoder before implementation and freezes seven additional
image length/CRC fingerprints. It creates all 64 retained roots, interleaves
empty commits and retention publications, reopens every eight epochs, verifies
every retained revision/value, releases IDs 1/32/64, backs up and compacts, then
reopens and commits/retains again with ID 65. Revision gaps, metadata-only DATA
reuse and current-before-retained reconstruction all occur in the fixture.

The original Git archive plus the fixed history fixture was independently
built and rerun. All **17 complete images** match the macOS candidate and Linux
candidate **byte-for-byte**, including backup/compact equality and later cache
reuse. `exact-bytes.json` records every size/SHA-256. CRC fingerprints provide
permanent regression controls; they alone are not the exact byte comparison.
Test-build DATA lookups additionally compare every result against the previous
linear first-match search, just as VALUE lookups already do.

The unchanged scaling driver independently checks complete current and retained
payloads, IDs and revisions after each epoch and after reopen/backup/restore/
compaction. Baseline/candidate breadth and history axes execute 56 clean/probe
processes. Accepted histories, file sizes and cap outcomes agree throughout.
Distinct breadth 2,048 still accepts seven epochs and rejects the eighth at the
unchanged image cap without altering acknowledged state or physical length.

## Measured DATA comparison work

The disposable launcher supports both the original `.find` predicate and the
new revision comparison function, rejecting missing, duplicated, ambiguous or
already-instrumented sites. Indexed counts include initial sorting, lookup
lower bounds, hit checks and insertion searches. Tie-break comparisons of
cache positions and shifted/copied bytes are separate work. Counters and their
accessor exist only in disposable copies; the production API is unchanged.

| Retained roots | Linear DATA comparisons | Indexed DATA comparisons |
|---:|---:|---:|
| 1 | 12 | 23 |
| 8 | 516 | 686 |
| 32 | 16,132 | 9,728 |
| 64 | 108,036 | 39,837 |

Both distinct and shared shapes produce these same DATA counts. The eight-epoch
breadth cases also move from 516 to 686, and the limited seven-epoch case from
391 to 549. Small histories perform more comparisons; the growing-history
workload benefits as repeated linear scans become more expensive. No threshold
or second lookup strategy is added to hide this cost. VALUE implementation is
unchanged; its observed probe counts can vary with allocation addresses.

These are finite source/compiler-bound work measurements, not latency
percentiles or a universal speedup. Clean timings and sampled whole-process RSS
remain in each worker's evidence. Runs overlapped other verification, so they
are not used here to attribute end-to-end timing changes to the DATA index.

## Executed checks

- macOS arm64/APFS, Mise Rust 1.99.1 / Python 3.14.8: all-feature core package
  debug and release each pass **302 tests**, with one deliberately ignored
  subprocess worker invoked by its bounded parent tests. Existing allocation
  sweeps, budget boundaries, 14 durability tests, ten compaction tests, retained
  roots, compatibility, inspection, native process interruption and replication
  are included.
- Linux aarch64, Rust 1.99.1, immutable image
  `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`:
  **306 tests per debug/release profile**, one parent-invoked helper ignored per
  profile, plus the two separately exported-image tests. Temporary native files
  use the private **ext4** volume on `/dev/vda1`. Network disabled, two CPUs,
  3 GiB memory, 256 PID limit, locked/offline builds and a read-only host registry
  cache. Owned container and volume removal and absence are verified. This is
  not physical-device or native Windows qualification.
- Two directed index tests pass under Miri nightly-2026-11-12. They cover empty
  and unsorted caches, duplicates and first-match counters, revision gaps and
  u64 boundaries, front/middle/end insertion, existing capacity reuse, initial
  and growth OOM rollback, exact initial/growth budget boundaries, maximum table
  admission and above-bound rejection before allocation.
- All 25 deterministic fuzz replay tests pass. Seven launcher controls pass
  normally and under Python `-O`, including legacy/current instrumentation and
  ambiguous/missing/duplicate-site rejection. The complete POSIX helper runner
  passes **318 tests with four registered skips**, 322 reported across 42 workers.
- Strict workspace/all-feature/all-target Clippy, formatting and whitespace
  checks pass. All 19 tracked package manifests retain `publish = false`.
- Fresh AST inventory: **2,577 records / 51 files**, 318 macro blind spots, zero
  parse failures. The [codec follow-up](decisions/codec.md#data-revision-index-follow-up)
  records changed guards and delegated algorithms. Historical tables keep
  their source scopes; no compiler MC/DC percentage or new mutation score is
  claimed.

## Provenance and reproduction

Candidate source is the baseline archive plus the following overlays. The
history fixture used for the baseline has a separately retained codec hash in
`source.json`; it changes tests, not the baseline encoder.

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `c6f5e0cb5eb7867528cea77d3762e44945745204412d1732a70c6e900be59b67` |
| `tools/serialization-scale/run.py` | `1ef637e8a3d13cfbeb4796ed0c82635060ac37331d6de56a0125dbfaaab20075` |
| `tools/serialization-scale/test_runner.py` | `428f52d003b445ffc57035ec00d42307a2590a3ff2a3eca83e4a17e0ad8fa9ae` |

Local evidence: `target/data-index-qualification/summary.json`, SHA-256
`e0e382770e895dd9809473970ef8552e8846dc13112f1f3870fbc3e5edb9edd5`.
It binds source overlays, test logs, exact-byte inventories, paired counters,
helper/inventory evidence and Linux cleanup. Frozen sources and all eight
measurement binaries are rehashed after execution. These ignored local files
are not permanent hosted evidence or a final release bundle. Earlier Windows
DLL results qualify their original library only; repeat native/candidate
qualification for this runtime change. No crates are published.

```sh
mise exec rust@1.99.1 -- cargo test --offline --locked -p weekend-db --all-features
mise exec rust@1.99.1 -- cargo test --offline --locked -p weekend-db --all-features --release
mise exec rust@1.99.1 -- cargo test --offline --locked --manifest-path fuzz/Cargo.toml --lib
mise exec rust@1.99.1 python@3.14.8 -- python tools/serialization-scale/run.py --revision HEAD --axis history --output target/data-history-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/serialization-scale/run.py --revision HEAD --output target/data-breadth-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/check_helpers.py --output target/data-helpers-fresh
```
