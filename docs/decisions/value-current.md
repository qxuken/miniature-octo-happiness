# Owned value decisions

The selected-provider sweep refuses each allocation for map insertion,
map removal, middle list replacement, append, suffix insertion, suffix
removal and byte construction. A copying BTreeMap and Vec model supplies
the expected result after a successful retry. The input values, live-byte
charge and provider bytes return to the same baseline after each attempt.
Middle insertion and removal cover multiple shifted positions.

The depth and weight fixture builds a nested map and list with one deepest
child. Replacement or removal of that child lowers the depth and computes
the exact logical weight. Removing the final child gives depth one and
weight 16. Removing a missing map key under full refusal returns the same
node. Independent weight and depth cap inputs reject before tree allocation.

Shared children give a compact real DAG whose logical weight grows despite
shared physical storage. The arithmetic tests use it to reach map and list
aggregation overflow, including the final key addition during map editing.
The two-term sums in list edits are bounded by two admitted weights. The
removed weight belongs to the input list. The invariant rows below record
those arithmetic domains.

The equality controls distinguish shared identity, scalar bits, list prefix
lengths and corresponding items, and map length, keys, values and order.
The borrowed map view tests preserve the source row limit and use the
encoder's memory domain for scratch. The scoped Rust, selected-allocator
and encoder views each check the wrong-container path.

## Verification at 8d17398

- Rust 1.100.0 debug and release passed 240 tests per profile on macOS arm64
  and 247 on Docker Linux arm64/ext4. These include the value model, schema,
  selected and Global allocation, retention and durability tests, and
  both golden histories.
- Four value unit tests, the nested allocation sweep and the accessor test
  passed under Miri. Strict Clippy passed on macOS and for Windows MSVC. Ten
  mutation-runner tests passed.
- Both macOS profiles detected all 52 controls. The ten
  clean-source reports verify 416 command retirements. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-owned-value-` and
  repeat with `--release`. The first pass identified two insensitive
  fixtures. The final checks verify the exact Type error and remove from
  a map whose surviving branch still needs allocation.

The 81 source records comprise 32 directed pairs,
37 execution or propagation rows, 5 coupled rows and
7 invariant rows. For the OR guards, paired inputs toggle one
of the two conditions with the other false. The equality AND pairs hold
the other condition true. Standard-library iterator internals and
compiler coverage retain their own scope.
The inventory reconciles 10,582 decisions, with all 81 value records now
dispositioned. The 9,649 remaining open records include 2,279 runtime
records.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/value.rs` | `afcbbdab3833c9230027232a11b4ff55222d2912b3d1ac9671b897cb4da6f120` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/value.rs:64:37– | 1655..1656 | try_propagation | from_map | Sharing three large logical children overflows raw map accumulation. |
| E | weekend-db::weekend_db | src/value.rs:66:11– | 1687..1688 | try_propagation | from_map | The visitor returns aggregate Limit through from_map. |
| E | weekend-db::weekend_db | src/value.rs:72:9– | 1898..1901 | for_iteration | from_list | Empty and populated raw lists exercise aggregation and its zero-iteration control. |
| E | weekend-db::weekend_db | src/value.rs:75:76– | 2097..2098 | try_propagation | from_list | Three large shared children overflow raw list accumulation. |
| P | weekend-db::weekend_db | src/value.rs:80:9– | 2264..2266 | if | new | A depth-two, weight-32 list fits. Each independently tighter cap rejects. |
| C | weekend-db::weekend_db | src/value.rs:80:12– | 2267..2338 | short_circuit_or | new | The constructor OR shares the enclosing if decision. |
| E | weekend-db::weekend_db | src/value.rs:89:15– | 2556..2557 | try_propagation | new | Refusing the final value header drops prepared bytes or tree nodes. |
| E | weekend-db::weekend_db | src/value.rs:93:20– | 2675..2680 | match | scalar | Direct scalar constructors exercise all kind arms and invalid encodings. |
| P | weekend-db::weekend_db | src/value.rs:94:27– | 2714..2723 | match_guard | scalar | None accepts zero bits and rejects one. |
| P | weekend-db::weekend_db | src/value.rs:95:30– | 2768..2777 | match_guard | scalar | Boolean accepts zero and one and rejects two. |
| P | weekend-db::weekend_db | src/value.rs:99:26– | 2987..2996 | match_guard | scalar | The empty map accepts zero bits and rejects one. |
| P | weekend-db::weekend_db | src/value.rs:100:27– | 3049..3058 | match_guard | scalar | The empty list accepts zero bits and rejects one. |
| P | weekend-db::weekend_db | src/value.rs:106:9– | 3267..3269 | if | bytes_new | Two bytes fit the length cap and three exceed it before allocating. |
| I | weekend-db::weekend_db | src/value.rs:109:69– | 3425..3426 | try_propagation | bytes_new | The admitted length is at most isize::MAX and adding the fixed 16-byte overhead fits in usize. |
| P | weekend-db::weekend_db | src/value.rs:110:9– | 3436..3438 | if | bytes_new | At fixed length capacity, exact logical weight succeeds and overweight bytes reject before allocation. |
| E | weekend-db::weekend_db | src/value.rs:113:51– | 3577..3578 | try_propagation | bytes_new | Refusing the byte copy preserves provider bytes and live charge. |
| E | weekend-db::weekend_db | src/value.rs:119:9– | 3713..3718 | match | kind | Distinct Kind values are read for all eight variants. |
| E | weekend-db::weekend_db | src/value.rs:131:9– | 4152..4157 | match | as_bool | The Boolean accessor succeeds only for its own variant. |
| E | weekend-db::weekend_db | src/value.rs:137:9– | 4315..4320 | match | as_i64 | The signed accessor succeeds only for its own variant. |
| E | weekend-db::weekend_db | src/value.rs:143:9– | 4478..4483 | match | as_u64 | The unsigned accessor succeeds only for its own variant. |
| E | weekend-db::weekend_db | src/value.rs:149:9– | 4642..4647 | match | as_f64 | The float accessor succeeds only for its own variant and preserves bits. |
| E | weekend-db::weekend_db | src/value.rs:155:9– | 4823..4828 | match | as_bytes | The byte accessor succeeds only for its own variant. |
| E | weekend-db::weekend_db | src/value.rs:161:9– | 4984..4989 | match | len | Bytes, maps and lists provide length. Scalar lengths return Type. |
| E | weekend-db::weekend_db | src/value.rs:169:22– | 5251..5252 | try_propagation | is_empty | The empty query propagates Type for scalars. |
| P | weekend-db::weekend_db | src/value.rs:172:9– | 5322..5324 | if | same | Local children allow edits. A child from another database returns ForeignStore. |
| P | weekend-db::weekend_db | src/value.rs:179:9– | 5496..5498 | if | key | One byte fits the key cap and two reject. |
| E | weekend-db::weekend_db | src/value.rs:186:22– | 5708..5709 | try_propagation | map_get | The point lookup propagates the key limit. |
| P | weekend-db::weekend_db | src/value.rs:187:9– | 5719..5722 | let_else | map_get | A map supports lookup. Another Value kind returns Type. |
| E | weekend-db::weekend_db | src/value.rs:193:25– | 5945..5946 | try_propagation | map_set | Map setting propagates ForeignStore before editing. |
| E | weekend-db::weekend_db | src/value.rs:194:22– | 5969..5970 | try_propagation | map_set | Map setting propagates the key limit. |
| P | weekend-db::weekend_db | src/value.rs:195:9– | 5980..5983 | let_else | map_set | Map insertion succeeds and a same-store wrong-container edit returns Type. |
| P | weekend-db::weekend_db | src/value.rs:200:25– | 6181..6183 | if | map_set | Insertion charges the key. Replacement keeps the existing key charge. |
| E | weekend-db::weekend_db | src/value.rs:206:33– | 6447..6448 | try_propagation | map_set | The last key addition overflows after two representable logical child weights. |
| P | weekend-db::weekend_db | src/value.rs:207:9– | 6458..6460 | if | map_set | Exact aggregate weight and permitted depth succeed. Weight and child depth independently reject before allocation. |
| C | weekend-db::weekend_db | src/value.rs:207:12– | 6461..6566 | short_circuit_or | map_set | The map edit OR shares its enclosing if decision. |
| E | weekend-db::weekend_db | src/value.rs:212:57– | 6681..6682 | try_propagation | map_set | Refusing a copied map path leaves source values and charges intact. |
| I | weekend-db::weekend_db | src/value.rs:213:36– | 6719..6720 | try_propagation | map_set | The depth visitor always returns Ok and map traversal only propagates callback errors. |
| E | weekend-db::weekend_db | src/value.rs:217:22– | 6868..6869 | try_propagation | map_remove | Removal propagates the key limit before editing. |
| P | weekend-db::weekend_db | src/value.rs:218:9– | 6879..6882 | let_else | map_remove | Wrong-container removal returns Type. Map removal edits or reuses its node. |
| P | weekend-db::weekend_db | src/value.rs:221:9– | 6979..6982 | let_else | map_remove | Missing removal returns the same node with zero requests. Removing a present key changes content and depth. |
| E | weekend-db::weekend_db | src/value.rs:225:45– | 7170..7171 | try_propagation | map_remove | Refusing a nonempty map removal reclaims scratch. |
| I | weekend-db::weekend_db | src/value.rs:226:36– | 7208..7209 | try_propagation | map_remove | The depth visit of a valid edited map has an infallible callback. |
| P | weekend-db::weekend_db | src/value.rs:233:9– | 7582..7585 | let_else | map_entries | Legacy Rust rows accept a map and reject the other kinds. |
| P | weekend-db::weekend_db | src/value.rs:240:9– | 7819..7822 | let_else | map_entries_owned | Selected-allocator rows accept a map and reject a scalar. |
| P | weekend-db::weekend_db | src/value.rs:250:9– | 8107..8110 | let_else | map_entries_for_encoding | Borrowed rows accept a map and reject a scalar through the encoder entry point. |
| P | weekend-db::weekend_db | src/value.rs:259:9– | 8499..8502 | let_else | list_get | List lookup accepts a list and reports Type for other kinds. |
| E | weekend-db::weekend_db | src/value.rs:265:25– | 8750..8751 | try_propagation | list_set | Replacement propagates ForeignStore. |
| P | weekend-db::weekend_db | src/value.rs:266:9– | 8761..8764 | let_else | list_set | List replacement accepts a list and rejects other kinds. |
| E | weekend-db::weekend_db | src/value.rs:269:55– | 8909..8910 | try_propagation | list_set | Replacement at len returns Bounds. |
| I | weekend-db::weekend_db | src/value.rs:272:33– | 9037..9038 | try_propagation | list_set | The old child is included in the list weight. The remaining weight and the new child are at most isize::MAX. |
| P | weekend-db::weekend_db | src/value.rs:273:9– | 9048..9050 | if | list_set | Exact weight and depth fit. Each separate over-limit case rejects before allocation. |
| C | weekend-db::weekend_db | src/value.rs:273:12– | 9051..9156 | short_circuit_or | list_set | The list replacement OR shares its enclosing if decision. |
| E | weekend-db::weekend_db | src/value.rs:278:61– | 9275..9276 | try_propagation | list_set | A refused list replacement releases copies and preserves the original list. |
| E | weekend-db::weekend_db | src/value.rs:283:25– | 9469..9470 | try_propagation | list_push | Append propagates ForeignStore. |
| P | weekend-db::weekend_db | src/value.rs:284:9– | 9480..9483 | let_else | list_push | Append accepts a list and reports Type for the other kinds. |
| I | weekend-db::weekend_db | src/value.rs:290:33– | 9695..9696 | try_propagation | list_push | Two already-admitted logical weights sum to at most twice isize::MAX. |
| P | weekend-db::weekend_db | src/value.rs:291:9– | 9706..9708 | if | list_push | Exact limits fit. Weight and child depth independently reject before allocation. |
| C | weekend-db::weekend_db | src/value.rs:291:12– | 9709..9814 | short_circuit_or | list_push | The append OR shares its enclosing if decision. |
| E | weekend-db::weekend_db | src/value.rs:296:55– | 9927..9928 | try_propagation | list_push | Refused append releases the candidate path. |
| E | weekend-db::weekend_db | src/value.rs:306:25– | 10275..10276 | try_propagation | list_insert | Insertion propagates ForeignStore. |
| P | weekend-db::weekend_db | src/value.rs:307:9– | 10286..10289 | let_else | list_insert | Insertion accepts a list and reports Type for other kinds. |
| P | weekend-db::weekend_db | src/value.rs:310:9– | 10388..10390 | if | list_insert | Index len goes to append and len plus one returns Bounds. |
| P | weekend-db::weekend_db | src/value.rs:313:9– | 10469..10471 | if | list_insert | End insertion appends; middle insertion rebuilds the suffix in copying-model order. |
| I | weekend-db::weekend_db | src/value.rs:319:33– | 10667..10668 | try_propagation | list_insert | Both logical weights are at most isize::MAX before addition. |
| P | weekend-db::weekend_db | src/value.rs:320:9– | 10678..10680 | if | list_insert | Exact middle insertion fits. Each independently larger dimension rejects before allocation. |
| C | weekend-db::weekend_db | src/value.rs:320:12– | 10681..10786 | short_circuit_or | list_insert | The middle-insertion OR shares its enclosing if decision. |
| E | weekend-db::weekend_db | src/value.rs:325:87– | 10931..10932 | try_propagation | list_insert | The first copy of the tail can be refused. |
| E | weekend-db::weekend_db | src/value.rs:326:9– | 10942..10945 | for_iteration | list_insert | The suffix controls include no shifts and multiple shifts. |
| E | weekend-db::weekend_db | src/value.rs:327:76– | 11058..11059 | try_propagation | list_insert | Refusal during a multi-item suffix move releases all partial copies. |
| E | weekend-db::weekend_db | src/value.rs:329:57– | 11127..11128 | try_propagation | list_insert | Refusing the final target set releases the already shifted suffix. |
| P | weekend-db::weekend_db | src/value.rs:335:9– | 11416..11419 | let_else | list_remove | Removal accepts a list and reports Type for other kinds. |
| E | weekend-db::weekend_db | src/value.rs:338:55– | 11564..11565 | try_propagation | list_remove | Removal at len returns Bounds. |
| E | weekend-db::weekend_db | src/value.rs:340:9– | 11612..11615 | for_iteration | list_remove | Tail and head removals exercise empty and populated shifts. |
| E | weekend-db::weekend_db | src/value.rs:341:76– | 11720..11721 | try_propagation | list_remove | Refusing a shift preserves all source positions. |
| E | weekend-db::weekend_db | src/value.rs:343:35– | 11767..11768 | try_propagation | list_remove | Refusing the candidate tail pop releases shifted paths. |
| I | weekend-db::weekend_db | src/value.rs:358:7– | 12127..12128 | try_propagation | map_depth | The callback cannot return an error and the admitted child depth bounds its increment. |
| P | weekend-db::weekend_db | src/value.rs:369:9– | 12380..12382 | if | eq | A clone shares identity; separately built values use structural comparison. |
| E | weekend-db::weekend_db | src/value.rs:372:9– | 12458..12463 | match | eq | Each same-kind alternative and a cross-kind comparison are checked, including float bits and binary bytes. |
| P | weekend-db::weekend_db | src/value.rs:380:17– | 12928..12992 | short_circuit_and | eq | List length and corresponding item equality independently control the result. |
| P | weekend-db::weekend_db | src/value.rs:382:45– | 13051..13069 | match_guard | eq | Separate equal-length maps compare structurally; a length mismatch rejects. |
| P | weekend-db::weekend_db | src/value.rs:385:21– | 13171..13173 | if | eq | The corresponding key and value must match, regardless of insertion order. |
