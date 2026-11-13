# Charged VALUE identity index

This production change replaces encoder VALUE linear lookup with an ephemeral
open-addressed identity index. The baseline is
`92c1ba73193c0d88f9b6c86098441fe1ba9f0118`; candidate source is that archive plus
the three source overlays below. Qualification ran on 2026-11-14. The earlier
[breadth](serialization-scale.md) and [history](serialization-history-scale.md)
experiments remain historical measurements of the linear encoder.

## Representation, bounds and failure behavior

The emission-order `CacheEntry` vector remains authoritative. The index contains
only vector indices plus one (zero is empty), uses mixed allocation addresses
to choose buckets, and confirms hits with `same_node`. Equal contents in distinct
allocations remain distinct objects. Cache-owned Value clones keep all indexed
allocations alive; integer addresses are never converted back to pointers.
No new unsafe code is introduced.

The index is rebuilt from the input cache on every encode, including caches
reconstructed by recovery. Its layout never determines VALUE traversal, offsets,
birth revisions, sharing or output order. Initial rebuilding preserves the old
first-match behavior even for redundant internal cache entries. After a miss,
children serialize in their existing order; the new VALUE is indexed only after
its children and object bytes have been prepared. The candidate cache retains
the original append order. No index is stored on disk or retained by Storage.

Power-of-two tables stay at most half full, grow geometrically, and use linear
probing with wraparound. The format bounds the cache to 16,384 entries. On the
supported 64-bit targets the maximum table is 32,768 slots / 262,144 requested
bytes; simultaneous old/new tables at the largest growth occupy 393,216 bytes.
All table storage uses `ChargedVec` and the existing fallible `Memory::reserve`.
Failure leaves the prior index intact, releases temporary charges, and abandons
preparation before any publication I/O. Existing transaction terminal/retry rules
apply. A tight existing budget may now reject preparation earlier with `Limit`.
The temporary table is released before the prepared candidate is published.

Expected lookup work is constant with linear rebuild work; adversarial bucket
clustering can still make probes linear in table size. The empty-slot invariant
bounds termination, not a hard latency SLO. Cache cloning, exact-capacity cache
vector growth, CRC, tree reconstruction, I/O and DATA revision searches remain
separate costs. In particular this does not solve accumulated cubic DATA search
work across growing retained histories.

## Exact bytes and independent state checks

`serialization_identity_golden_lifecycle` fixes store identity and operations:
three generations of separately allocated equal bytes, shared nested descendants,
NaN bits, commits, retains, reopen between generations, backup, compaction and
a later append reusing a retained value. Its ten image length/CRC fingerprints
were captured before the index edit and remain frozen. An optional fresh
`WDB_IDENTITY_IMAGES` directory exports images without bypassing those checks.

For this qualification the identical test was overlaid onto the original Git
archive and rerun. Every image was compared **byte-for-byte** against both the
macOS candidate and Linux candidate; all ten match. SHA-256 inventories are in
`exact-bytes.json`, with baseline test provenance in `byte-fixture-source.json`.
The frozen CRC test is a regression fingerprint, not an independent byte equality
proof by itself. Every test-build encoder lookup additionally checks its result
against the original linear identity search.

The unchanged serialization driver checks all current rows and every retained
root against independently constructed payloads, IDs and revisions after each
epoch and after reopen/backup/restore/compaction. Both breadth and history axes
were rerun against baseline and candidate: 56 clean/instrumented processes.
Accepted epochs, file sizes, limit outcomes and DATA comparison counts agree.
The 2,048-distinct case still accepts seven epochs and rejects the eighth at the
unchanged physical image cap, with unchanged acknowledged state and file length.

## Lookup measurements

Disposable instrumentation still counts actual `same_node` comparisons. For
the index this includes occupied-bucket comparisons during queries, insertion,
initial rebuild and growth; empty slots do not compare identities. The baseline
counts linear-search predicates. Candidate counts depend on process allocation
addresses and are observations, not fixed expected constants.

| Breadth / shape | Linear comparisons | Indexed comparisons |
|---|---:|---:|
| 128 distinct | 2,095,104 | 10,961 |
| 512 distinct | 33,546,240 | 40,596 |
| 2,048 distinct, seven epochs | 440,169,243 | 147,132 |
| 128 shared | 18,400 | 5,261 |
| 512 shared | 73,696 | 25,103 |
| 2,048 shared | 294,880 | 100,354 |

Distinct history cases at 1/8/32/64 roots drop from
112/8,064/130,560/523,264 to 16/606/6,647/14,437 comparisons.
DATA comparisons remain 12/516/16,132/108,036. Instrumentation and counters are
confined to disposable copies; no new production API is added.

Four additional sequential, uninstrumented 2,048-distinct lifecycles ran in
baseline/candidate/candidate/baseline order after the other local checks finished.
Total commit-attempt time was 347.22/305.15/306.79/342.89 ms respectively.
Backup, restore and compaction timings overlap broadly: compaction was
834.18/844.35/842.66/879.27 ms. Sampled whole-lifecycle RSS was
264,093,696/264,798,208/264,470,528/264,126,464 bytes. These are two observations
per variant, not latency distributions, per-phase memory attribution or resource
ceilings. Earlier paired measurements overlapped other checks and are not used
to claim a runtime speedup. Fewer identity comparisons do not imply the same
factor of end-to-end improvement.

## Executed checks and provenance

- macOS arm64/APFS, Mise Rust 1.99.1 / Python 3.14.8: all-feature core package
  tests pass in debug and release, **299 passed per profile**, one intentionally
  ignored subprocess worker invoked by its bounded parent tests. Includes the
  existing allocation sweeps, budget boundaries, 14 durability tests, ten
  compaction tests, retained roots, compatibility, inspection and replication.
- Linux aarch64, Rust 1.99.1, immutable image
  `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`:
  **303 passed per debug/release profile**, the same one helper ignored per
  profile, plus the separate exported-image test. Native temporary files use
  the private **ext4** volume (`/dev/vda1`). Network disabled, two CPUs, 3 GiB
  container memory, 256 PID limit; locked/offline builds use a read-only host
  registry cache. The first attempt lacked cached sha2 and failed before tests;
  its log is retained separately. Both attempts' owned containers/volumes are
  removed and queried absent. This is not physical-device qualification.
- Two new directed tests cover bucket collisions/wraparound, clone hits,
  same-content misses, reversed/duplicate cache rebuilding, growth OOM rollback,
  exact table budget admission/reclamation and bounds before allocation. Both
  pass under Miri, nightly-2026-11-12. The initial invocation encountered a Mise
  shim recursion/missing component; the final run uses the installed pinned
  toolchain directly and is the credited result.
- All 25 deterministic fuzz replay tests pass. Six serialization launcher
  controls pass normally and under Python `-O`. Strict workspace/all-feature/
  all-target Clippy, formatting and whitespace checks pass. All 19 tracked
  Cargo package manifests retain `publish = false`.
- Fresh AST inventory: 2,571 records / 51 files, 317 macro blind spots and no
  parse failures. This replaces neither the historical 2,561-record disposition
  table nor a compiler MC/DC denominator; see the [codec follow-up](decisions/codec.md#value-identity-index-follow-up).

| Candidate source | SHA-256 |
|---|---|
| `src/memory.rs` | `b2d3c2ab7fc6bbbafdb98ededff9b0f5d0f483bb564d76bb37a339a0bc6cc324` |
| `src/value.rs` | `254aaa0b5e56fa7cdee7e164b93d655311e0585aee86f40494301688662b663d` |
| `src/storage/codec.rs` | `1cc8dcf5151edf1b2f198c3f76e90b8a001d4cecccab51ced1a816cfeca40b6c` |

Local evidence root: `target/identity-index-qualification/`. `summary.json`
binds source overlays, paired counters, test-log hashes, exact byte inventories,
timings, inventory and Linux cleanup; SHA-256
`f51842690fa6038ef647b706bbb09bcdcec93bdb097ca48f61138191a74e104d`.
These ignored local files are not permanent hosted evidence. Earlier exact-DLL
Windows results apply to their original source; this new runtime change needs
its own Windows/candidate qualification. Publication barriers, separate follower
durable acknowledgment, file/wire formats and public APIs are unchanged. No
crate is published.

Reproduce the core checks with:

```sh
mise exec rust@1.99.1 -- cargo test --offline --locked -p weekend-db --all-features
mise exec rust@1.99.1 -- cargo test --offline --locked -p weekend-db --all-features --release
mise exec rust@1.99.1 -- cargo test --offline --locked --manifest-path fuzz/Cargo.toml --lib
mise exec rust@1.99.1 python@3.14.8 -- python tools/serialization-scale/run.py --revision HEAD --output target/index-breadth-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/serialization-scale/run.py --revision HEAD --axis history --output target/index-history-fresh
```
