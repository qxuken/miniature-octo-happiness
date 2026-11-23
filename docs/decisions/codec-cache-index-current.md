# Codec cache and index decisions

VALUE probing uses node identity. The directed fixture places four distinct
nodes with the same bytes in bucket 7 of an eight-slot table. Probing crosses
slot zero, finds the original shared node, and misses a distinct identity.
Rehash preserves the first matching cache entry. A half-full table reuses its
slots. Both indexes independently admit the exact object bound and refuse a
larger count before allocating.

DATA rebuild keeps emission order and sorts a separate index by revision and
original position. It selects the earliest entry for duplicate recovered
revisions. The insertion test also controls the lower-bound path when an
inserted revision equals the final revision. The comparison controls exercise
missing successors, exact matches, tie-breaking, and empty and populated order.
The standard library owns sorting, partitioning, and slice rotation.

The borrowed cache can reject a fork, retry and move to a private copy. The
original remains readable throughout. The owned draft stays at the same charge
on repeated access. Both typed caches are swept through two fork-buffer
requests and one shared-header request. Cold-open index or header refusal
preserves the unwitnessed image and releases the lease. A successful open
stabilizes revision 4 and preserves retained revision 2.

## Verification at 8150f48

- macOS arm64 passed 170 library tests per profile. Docker Linux arm64/ext4
  passed 177 library, 12 compaction, 23 durability and eight retention tests
  per profile. Both ran debug and release with Rust 1.100.0.
- Exported files from both golden histories match byte for byte across all four
  lanes. The 17-file SHA-256 manifest digest is `fea361f3bb2f84c4543030d16fa16ab7aaff99f783a9a48ed5b2f9b09121f009`.
  Set `WDB_IDENTITY_IMAGES` and `WDB_DATA_IMAGES` to fresh directories and run
  `cargo test --lib storage::codec::tests::serialization_` to reproduce a lane.
- Miri passed five directed cache and index tests. Strict Clippy passed on
  macOS and for Windows MSVC. The ten mutation-runner tests passed.
- All 29 named controls detected their intended change in both
  macOS profiles. The eight clean-source reports account for 248 verified
  command retirements. Reproduce this campaign at that revision with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-cache-` and use
  `--release` for the second profile.

The production inventories match the parent. The table records ten directed
source pairs, thirteen propagation rows, four scoped execution rows and one
coupled row. The two outer cold-open header sites use the same calibrated
fixture as index rebuild. The index-level calls in Storage::open already
have their own retained-root controls. The unreachable borrowed arm after a successful fork is an
invariant of `make_mut`'s preceding transition. Compiler coverage and the
delegated comparison and allocator internals retain their own scope.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `593b1dc249ceca770b436133c894b41aad0596c5f20be79abf96cd69644205ee` |
| `src/storage/mod.rs` | `30f5689a39d4953c8274f4deee56481edc6dc7066a65586bdb955e18099ef6f1` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| P | weekend-db::weekend_db | src/storage/codec.rs:207:9– | 6976..6981 | while | position | Probe a colliding occupied bucket and the empty bucket after wrapping. |
| P | weekend-db::weekend_db | src/storage/codec.rs:209:13– | 7070..7072 | if | position | Probe the same node and a distinct node in the same bucket. |
| P | weekend-db::weekend_db | src/storage/codec.rs:217:9– | 7306..7308 | if | get | An empty table misses and the populated table returns the stored node. |
| P | weekend-db::weekend_db | src/storage/codec.rs:223:9– | 7539..7541 | if | reserve | MAX_OBJECTS reaches the provider and MAX_OBJECTS + 1 rejects first. |
| P | weekend-db::weekend_db | src/storage/codec.rs:226:9– | 7622..7624 | if | reserve | Four entries reuse eight slots and five require growth. |
| E | weekend-db::weekend_db | src/storage/codec.rs:231:43– | 7835..7836 | try_propagation | reserve | A real refusal leaves the old slots and charge intact. Retry rebuilds. |
| E | weekend-db::weekend_db | src/storage/codec.rs:233:9– | 7882..7885 | for_iteration | reserve | Build from empty input and visit every established identity when growing. |
| P | weekend-db::weekend_db | src/storage/codec.rs:237:13– | 8146..8148 | if | reserve | Insert the first copy and skip the repeated node during rehash. |
| E | weekend-db::weekend_db | src/storage/codec.rs:271:40– | 9119..9120 | try_propagation | from_cache | Refuse the order buffer for the arbitrary-order fixture and verify cleanup. |
| P | weekend-db::weekend_db | src/storage/codec.rs:289:9– | 9836..9838 | if | reserve | MAX_OBJECTS reaches the provider and MAX_OBJECTS + 1 rejects first. |
| P | weekend-db::weekend_db | src/storage/codec.rs:292:9– | 9919..9921 | if | reserve | Build an empty order and reserve for the first entry under the independent cap. |
| E | weekend-db::weekend_db | src/storage/codec.rs:296:48– | 10224..10225 | try_propagation | reserve | At nine entries the refused grow leaves the existing eight slots and charge. |
| P | weekend-db::weekend_db | src/storage/codec.rs:301:9– | 10343..10345 | if | insert | Append above the last revision or insert at the lower bound. Equal last revision takes the latter. |
| E | weekend-db::weekend_db | src/storage/codec.rs:329:52– | 11277..11278 | try_propagation | from_entries | VALUE index refusal unwinds the selected cold-open graph before stabilization. |
| E | weekend-db::weekend_db | src/storage/codec.rs:333:64– | 11436..11437 | try_propagation | fork | Refuse the first VALUE fork buffer with the published cache still live. |
| E | weekend-db::weekend_db | src/storage/codec.rs:334:70– | 11508..11509 | try_propagation | fork | Refuse the VALUE slot copy after entry allocation and reclaim the partial fork. |
| E | weekend-db::weekend_db | src/storage/codec.rs:353:57– | 11999..12000 | try_propagation | from_entries | DATA index refusal unwinds the selected cold-open graph before stabilization. |
| E | weekend-db::weekend_db | src/storage/codec.rs:357:64– | 12158..12159 | try_propagation | fork | Refuse the first DATA fork buffer with the published cache still live. |
| E | weekend-db::weekend_db | src/storage/codec.rs:358:66– | 12226..12227 | try_propagation | fork | Refuse the DATA order copy after entry allocation and reclaim the partial fork. |
| E | weekend-db::weekend_db | src/storage/codec.rs:367:57– | 12511..12512 | try_propagation | fork_vec | Geometric reserve refusal propagates from the private copy. |
| E | weekend-db::weekend_db | src/storage/codec.rs:380:9– | 12897..12902 | match | deref | Borrowed dereference sees the prior bytes and owned dereference sees the draft. |
| C | weekend-db::weekend_db | src/storage/codec.rs:388:9– | 13128..13130 | if | make_mut | The enclosing if shares the borrowed pattern outcome. |
| P | weekend-db::weekend_db | src/storage/codec.rs:388:12– | 13131..13134 | pattern_test | make_mut | The borrowed path invokes the fork. The owned path reuses the draft without a fork. |
| E | weekend-db::weekend_db | src/storage/codec.rs:389:42– | 13205..13206 | try_propagation | make_mut | Refused fork keeps the same borrowed object, exact charge and provider bytes. Retry owns a draft. |
| E | weekend-db::weekend_db | src/storage/codec.rs:391:9– | 13227..13232 | match | make_mut | After a successful fork and on a later call, the owned arm returns the same draft. |
| E | weekend-db::weekend_db | src/storage/codec.rs:397:9– | 13415..13420 | match | finish | Borrowed returns the same shared identity. Owned allocates a header and propagates refusal. |
| E | weekend-db::weekend_db | src/storage/mod.rs:249:80– | 8955..8956 | try_propagation | open | VALUE cache header refusal releases the lease before stabilization. |
| E | weekend-db::weekend_db | src/storage/mod.rs:250:89– | 9046..9047 | try_propagation | open | DATA cache header refusal releases the lease before stabilization. |
