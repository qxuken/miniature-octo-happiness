# Transaction observation and overlay scan decisions

The shared observation limit counts point and collection dependencies together.
The single-slot fixture separately tries a collection read after a point and a
point read after a collection. Reusing the same dependency stays within the cap.
The two-condition pairs exercise matching collection and key, a different
collection with the same key, and the same collection with a different key.

The mixed overlay uses two full collections. An inserted binary key is staged
before a deletion and fits after the deletion is applied. An independent
`BTreeMap` computes the expected rows for both collections. The comparison
checks exact binary-key order. A deletion and an insertion in the other
collection exercise the independent collection and value filters.

A failed observation remains retryable. Refusing point-key, point-vector or
collection storage leaves provider bytes and the budget at baseline. A draft
that experienced this refusal can commit after another writer edits the would-be
dependency. A sibling draft that repeats the observation successfully conflicts
after the same edit.

The owned-scan sweep covers observation capture, overlay removal and replacement,
and row materialization. Failed calls leave the draft active, preserve its
existing overlay and keep the published revision and values intact. The first
refusal occurs before the collection dependency. Later refusals release
temporary buffers and keep exactly the collection's charged observation. An
edit to a previously unobserved row distinguishes those cases at commit.
Conflicting commits put the draft in Aborted.

For the conjunction rows, the fixtures select (true, true), (false, true) and
(true, false) from the inputs. Rust short-circuits the second operand when the
first is false. Tests exercise both source outcomes and the independent changes
in input. The compiler and delegated iterator implementations retain their
own coverage scope.

## Verification at d7848bd

- Rust 1.100.0 debug and release passed 207 tests per profile on macOS arm64 and
  214 on Docker Linux arm64/ext4. These include the library, allocation,
  transaction, retention and engine decision suites. Both golden histories ran
  in every profile.
- Four directed integration tests passed under Miri. Strict Clippy passed on
  macOS and for Windows MSVC. All ten mutation-runner tests passed.
- Both macOS profiles detected all 35 named controls. The ten
  clean-source reports verify 304 command retirements. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-observe-scan-` and
  repeat with `--release`.

The table records 11 directed source pairs, 24 execution or
propagation rows, 3 invariant rows and 1 coupled row.
The inventory reconciles 10,582 decisions and moves 39 out of the open
queue. The remaining 9,832 open records include 2,462 runtime records.
Assignment and commit preparation,
aggregate charge overflow, and the remaining tree and value decisions are
separate slices of batch 4.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/engine.rs` | `c1a9f0d9f416442840397a04b3b5d1ac7b3332b5d6dec98495d346d1420dea6d` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| P | weekend-db::weekend_db | src/engine.rs:733:9– | 25428..25430 | if | active | Active drafts operate. Committed, aborted and failed drafts reject reads and edits. |
| P | weekend-db::weekend_db | src/engine.rs:741:9– | 25688..25690 | if | charge | A zero transaction budget rejects observations. A funded draft captures observations and edits. |
| E | weekend-db::weekend_db | src/engine.rs:748:37– | 25930..25931 | try_propagation | observe_collection | A foreign collection returns ForeignStore through scan. |
| E | weekend-db::weekend_db | src/engine.rs:749:46– | 25978..25979 | try_propagation | observe_collection | A missing collection returns NotFound through scan. |
| P | weekend-db::weekend_db | src/engine.rs:750:9– | 25989..25991 | if | observe_collection | The same collection observation is reused. Another collection needs a slot. |
| P | weekend-db::weekend_db | src/engine.rs:753:9– | 26086..26088 | if | observe_collection | One collection observation fits. An existing point or collection exhausts capacity. |
| E | weekend-db::weekend_db | src/engine.rs:756:76– | 26294..26295 | try_propagation | observe_collection | Zero transaction bytes reject missing-point and scan observations. |
| E | weekend-db::weekend_db | src/engine.rs:757:54– | 26350..26351 | try_propagation | observe_collection | The first request is refused. Retrying records the collection dependency. |
| E | weekend-db::weekend_db | src/engine.rs:765:22– | 26706..26707 | try_propagation | observe | A terminal draft returns Closed through get before any observation. |
| E | weekend-db::weekend_db | src/engine.rs:766:37– | 26745..26746 | try_propagation | observe | A foreign collection returns ForeignStore through get. |
| E | weekend-db::weekend_db | src/engine.rs:767:24– | 26771..26772 | try_propagation | observe | A nonempty key is rejected while an empty key fits a zero key-byte cap. |
| P | weekend-db::weekend_db | src/engine.rs:768:9– | 26782..26785 | let_else | observe | A present key captures a point. An absent one captures its collection. |
| E | weekend-db::weekend_db | src/engine.rs:768:54– | 26827..26828 | try_propagation | observe | A missing collection returns NotFound through get. |
| E | weekend-db::weekend_db | src/engine.rs:771:9– | 26918..26920 | if | observe | An empty observation list misses. An existing identical point is reused at capacity. |
| P | weekend-db::weekend_db | src/engine.rs:774:22– | 26987..27030 | short_circuit_and | observe | The same identity reuses a point. Either independently differing component needs a slot. |
| P | weekend-db::weekend_db | src/engine.rs:778:9– | 27087..27089 | if | observe | One point fits. An existing point or collection exhausts capacity. |
| I | weekend-db::weekend_db | src/engine.rs:784:37– | 27368..27369 | try_propagation | observe | A valid Rust byte slice has at most isize::MAX bytes. Adding the fixed Point layout fits usize on supported ABIs. |
| E | weekend-db::weekend_db | src/engine.rs:785:10– | 27380..27381 | try_propagation | observe | The reopened present-point fixture reaches transaction-byte refusal. |
| E | weekend-db::weekend_db | src/engine.rs:787:36– | 27458..27459 | try_propagation | observe | Refusing the key copy records no dependency and reclaims scratch. |
| E | weekend-db::weekend_db | src/engine.rs:788:49– | 27509..27510 | try_propagation | observe | Refusing the point vector drops the already copied key. |
| E | weekend-db::weekend_db | src/engine.rs:798:28– | 27784..27785 | try_propagation | get | The observation error propagates through the public point read. |
| C | weekend-db::weekend_db | src/engine.rs:799:9– | 27795..27797 | if | get | The enclosing if shares the overlay lookup pattern decision. |
| P | weekend-db::weekend_db | src/engine.rs:799:12– | 27798..27801 | pattern_test | get | A matching overlay read returns the draft value. A miss returns the base value. |
| P | weekend-db::weekend_db | src/engine.rs:802:23– | 27879..27922 | short_circuit_and | get | Both collection and key have independent miss controls. |
| I | weekend-db::weekend_db | src/engine.rs:806:38– | 28021..28022 | try_propagation | get | A successful observation resolved the same immutable base collection. |
| E | weekend-db::weekend_db | src/engine.rs:812:27– | 28212..28213 | try_propagation | contains | Contains propagates the key limit from the underlying read. |
| E | weekend-db::weekend_db | src/engine.rs:860:45– | 29972..29973 | try_propagation | scan | Scan propagates refusal while recording the collection. |
| E | weekend-db::weekend_db | src/engine.rs:869:31– | 30360..30361 | try_propagation | scan_iter | The public iterator propagates the selected allocator refusal. |
| E | weekend-db::weekend_db | src/engine.rs:872:30– | 30491..30492 | try_propagation | scan_owned | The owned scan propagates all overlay preparation errors before yielding rows. |
| E | weekend-db::weekend_db | src/engine.rs:876:22– | 30670..30671 | try_propagation | scan_records | A terminal draft returns Closed through scan before observing. |
| E | weekend-db::weekend_db | src/engine.rs:877:36– | 30708..30709 | try_propagation | scan_records | The first scan refusal precedes the collection dependency. |
| I | weekend-db::weekend_db | src/engine.rs:878:53– | 30763..30764 | try_propagation | scan_records | The preceding collection observation resolved the same immutable base. |
| E | weekend-db::weekend_db | src/engine.rs:879:9– | 30790..30793 | for_iteration | scan_records | Empty and mixed overlays use the base and both write passes. The BTreeMap checks ordered results at capacity. |
| P | weekend-db::weekend_db | src/engine.rs:882:25– | 30868..30907 | short_circuit_and | scan_records | Choose only deletions for the scanned collection. Wrong-collection edits and puts are independent controls. |
| P | weekend-db::weekend_db | src/engine.rs:886:33– | 31017..31056 | short_circuit_and | scan_records | Choose only puts for the scanned collection. Wrong-collection edits and deletions are independent controls. |
| E | weekend-db::weekend_db | src/engine.rs:889:23– | 31105..31110 | match | scan_records | Mixed overlays remove and insert. Deleting before inserting admits the full collection fixture. |
| E | weekend-db::weekend_db | src/engine.rs:897:18– | 31380..31381 | try_propagation | scan_records | Refuse the path copy for the replacement. The earlier collection observation survives. |
| E | weekend-db::weekend_db | src/engine.rs:898:61– | 31443..31444 | try_propagation | scan_records | Refuse the deletion path copy. The earlier collection observation survives. |
| E | weekend-db::weekend_db | src/engine.rs:904:31– | 31592..31593 | try_propagation | count | Count propagates the closed-state error from the owned scan. |
