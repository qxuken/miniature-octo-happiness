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
| `src/engine.rs` | `17c2cb6848cb59efd25829a5bd228b01d8230863b0be83a2a05efad08bcd2008` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| P | weekend-db::weekend_db | src/engine.rs:734:9– | 25486..25488 | if | active | Active drafts operate. Committed, aborted and failed drafts reject reads and edits. |
| P | weekend-db::weekend_db | src/engine.rs:742:9– | 25746..25748 | if | charge | A zero transaction budget rejects observations. A funded draft captures observations and edits. |
| E | weekend-db::weekend_db | src/engine.rs:749:37– | 25988..25989 | try_propagation | observe_collection | A foreign collection returns ForeignStore through scan. |
| E | weekend-db::weekend_db | src/engine.rs:750:46– | 26036..26037 | try_propagation | observe_collection | A missing collection returns NotFound through scan. |
| P | weekend-db::weekend_db | src/engine.rs:751:9– | 26047..26049 | if | observe_collection | The same collection observation is reused. Another collection needs a slot. |
| P | weekend-db::weekend_db | src/engine.rs:754:9– | 26144..26146 | if | observe_collection | One collection observation fits. An existing point or collection exhausts capacity. |
| E | weekend-db::weekend_db | src/engine.rs:757:76– | 26352..26353 | try_propagation | observe_collection | Zero transaction bytes reject missing-point and scan observations. |
| E | weekend-db::weekend_db | src/engine.rs:758:54– | 26408..26409 | try_propagation | observe_collection | The first request is refused. Retrying records the collection dependency. |
| E | weekend-db::weekend_db | src/engine.rs:766:22– | 26764..26765 | try_propagation | observe | A terminal draft returns Closed through get before any observation. |
| E | weekend-db::weekend_db | src/engine.rs:767:37– | 26803..26804 | try_propagation | observe | A foreign collection returns ForeignStore through get. |
| E | weekend-db::weekend_db | src/engine.rs:768:24– | 26829..26830 | try_propagation | observe | A nonempty key is rejected while an empty key fits a zero key-byte cap. |
| P | weekend-db::weekend_db | src/engine.rs:769:9– | 26840..26843 | let_else | observe | A present key captures a point. An absent one captures its collection. |
| E | weekend-db::weekend_db | src/engine.rs:769:54– | 26885..26886 | try_propagation | observe | A missing collection returns NotFound through get. |
| E | weekend-db::weekend_db | src/engine.rs:772:9– | 26976..26978 | if | observe | An empty observation list misses. An existing identical point is reused at capacity. |
| P | weekend-db::weekend_db | src/engine.rs:775:22– | 27045..27088 | short_circuit_and | observe | The same identity reuses a point. Either independently differing component needs a slot. |
| P | weekend-db::weekend_db | src/engine.rs:779:9– | 27145..27147 | if | observe | One point fits. An existing point or collection exhausts capacity. |
| I | weekend-db::weekend_db | src/engine.rs:785:37– | 27426..27427 | try_propagation | observe | A valid Rust byte slice has at most isize::MAX bytes. Adding the fixed Point layout fits usize on supported ABIs. |
| E | weekend-db::weekend_db | src/engine.rs:786:10– | 27438..27439 | try_propagation | observe | The reopened present-point fixture reaches transaction-byte refusal. |
| E | weekend-db::weekend_db | src/engine.rs:788:36– | 27516..27517 | try_propagation | observe | Refusing the key copy records no dependency and reclaims scratch. |
| E | weekend-db::weekend_db | src/engine.rs:789:49– | 27567..27568 | try_propagation | observe | Refusing the point vector drops the already copied key. |
| E | weekend-db::weekend_db | src/engine.rs:799:28– | 27842..27843 | try_propagation | get | The observation error propagates through the public point read. |
| C | weekend-db::weekend_db | src/engine.rs:800:9– | 27853..27855 | if | get | The enclosing if shares the overlay lookup pattern decision. |
| P | weekend-db::weekend_db | src/engine.rs:800:12– | 27856..27859 | pattern_test | get | A matching overlay read returns the draft value. A miss returns the base value. |
| P | weekend-db::weekend_db | src/engine.rs:803:23– | 27937..27980 | short_circuit_and | get | Both collection and key have independent miss controls. |
| I | weekend-db::weekend_db | src/engine.rs:807:38– | 28079..28080 | try_propagation | get | A successful observation resolved the same immutable base collection. |
| E | weekend-db::weekend_db | src/engine.rs:813:27– | 28270..28271 | try_propagation | contains | Contains propagates the key limit from the underlying read. |
| E | weekend-db::weekend_db | src/engine.rs:861:45– | 30031..30032 | try_propagation | scan | Scan propagates refusal while recording the collection. |
| E | weekend-db::weekend_db | src/engine.rs:870:31– | 30419..30420 | try_propagation | scan_iter | The public iterator propagates the selected allocator refusal. |
| E | weekend-db::weekend_db | src/engine.rs:873:30– | 30550..30551 | try_propagation | scan_owned | The owned scan propagates all overlay preparation errors before yielding rows. |
| E | weekend-db::weekend_db | src/engine.rs:877:22– | 30729..30730 | try_propagation | scan_records | A terminal draft returns Closed through scan before observing. |
| E | weekend-db::weekend_db | src/engine.rs:878:36– | 30767..30768 | try_propagation | scan_records | The first scan refusal precedes the collection dependency. |
| I | weekend-db::weekend_db | src/engine.rs:879:53– | 30822..30823 | try_propagation | scan_records | The preceding collection observation resolved the same immutable base. |
| E | weekend-db::weekend_db | src/engine.rs:880:9– | 30849..30852 | for_iteration | scan_records | Empty and mixed overlays use the base and both write passes. The BTreeMap checks ordered results at capacity. |
| P | weekend-db::weekend_db | src/engine.rs:883:25– | 30927..30966 | short_circuit_and | scan_records | Choose only deletions for the scanned collection. Wrong-collection edits and puts are independent controls. |
| P | weekend-db::weekend_db | src/engine.rs:887:33– | 31076..31115 | short_circuit_and | scan_records | Choose only puts for the scanned collection. Wrong-collection edits and deletions are independent controls. |
| E | weekend-db::weekend_db | src/engine.rs:890:23– | 31164..31169 | match | scan_records | Mixed overlays remove and insert. Deleting before inserting admits the full collection fixture. |
| E | weekend-db::weekend_db | src/engine.rs:898:18– | 31439..31440 | try_propagation | scan_records | Refuse the path copy for the replacement. The earlier collection observation survives. |
| E | weekend-db::weekend_db | src/engine.rs:899:61– | 31502..31503 | try_propagation | scan_records | Refuse the deletion path copy. The earlier collection observation survives. |
| E | weekend-db::weekend_db | src/engine.rs:905:31– | 31651..31652 | try_propagation | count | Count propagates the closed-state error from the owned scan. |
