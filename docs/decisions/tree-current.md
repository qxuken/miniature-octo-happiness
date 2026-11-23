# Persistent tree decisions

The collision and removal sweep compares each completed map edit with an
independent BTreeMap. The fixture uses real keys with forced equal hashes and
distinct hashes that share a prefix. A separate split fixture shares the
first fifteen nibbles. The sweep independently exercises the
public removal path with natural hashes. Every refusal keeps the original
maps readable, restores the selected provider and live-byte charge, and
permits a successful retry. Removing a missing key reuses the same root
without a request.

The list sweep covers replacement, growth at length 256, and recursive
clearing of a partial tail at length 258. Unchanged branches remain shared.
The shape test checks every surviving item and directly distinguishes
detaching an aligned child from clearing one slot in a partially filled
leaf. The independent shrink inputs are length 17 with both conditions
true, length 18 with the length condition false, and length 2 with the
height condition false. Singleton pop returns an empty root under full
provider refusal.

Row materialization checks the header budget and accumulated key budget
separately. Rejection invokes neither the allocator nor the projection.
At the exact limit the selected provider sweep refuses the output vector
and each key copy in turn. Borrowed and owned rows have the same binary
order, including a NUL key and a key starting with 0xff. The source
entries survive all attempts.

## Verification at 4d8e12f

- Rust 1.100.0 debug and release passed 231 tests per profile on macOS arm64
  and 238 on Docker Linux arm64/ext4. This includes the randomized nested
  copy model, selected and Global allocation tests, transactions,
  durability, retention, and both golden histories.
- Eight tree tests passed under Miri. Strict Clippy passed on macOS and
  for Windows MSVC. Ten mutation-runner tests passed.
- Both macOS profiles detected all 49 named controls. The ten
  clean-source reports verify 390 command retirements. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-tree-` and repeat
  with `--release`.

The 72 rows comprise 20 directed pairs, 40 execution
or propagation rows, 5 coupled rows and 7 invariant
rows. Invariants apply to the shipped Value row instantiations and valid
map and list structures. Standard-library iterator and compiler coverage
retain their own scope.
The inventory reconciles 10,582 decisions. All 72 tree records have a
source disposition, with 71 fewer open records in this slice. The
remaining 9,730 open records include 2,360 runtime records.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/tree.rs` | `f2b4099a101747b657f8990d223710cbd176f8ef9e251b5799fa4744ccd8c8b3` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/tree.rs:48:44– | 1251..1252 | try_propagation | get_hashed | Empty lookup returns absence; a populated tree selects its root. |
| E | weekend-db::weekend_db | src/tree.rs:51:13– | 1308..1313 | match | get_hashed | Lookup traverses branches and reaches leaves in the collision and split fixtures. |
| P | weekend-db::weekend_db | src/tree.rs:53:28– | 1404..1406 | if | get_hashed | Equal and unequal hashes isolate the leaf hash gate. |
| E | weekend-db::weekend_db | src/tree.rs:63:66– | 1808..1809 | try_propagation | get_hashed | The collapse fixture visits an occupied slot and misses an empty slot. |
| P | weekend-db::weekend_db | src/tree.rs:74:9– | 2189..2191 | if | set_hashed | A full tree rejects a new key. Replacing an existing key and adding below the cap succeed. |
| C | weekend-db::weekend_db | src/tree.rs:74:12– | 2192..2239 | short_circuit_and | set_hashed | The capacity conjunction shares the enclosing if decision. |
| E | weekend-db::weekend_db | src/tree.rs:77:43– | 2332..2333 | try_propagation | set_hashed | Refusing key storage preserves the original root. |
| E | weekend-db::weekend_db | src/tree.rs:77:45– | 2334..2335 | try_propagation | set_hashed | Refusing the shared key frees its previously copied bytes. |
| E | weekend-db::weekend_db | src/tree.rs:78:72– | 2408..2409 | try_propagation | set_hashed | Path preparation refusal preserves the input map and its charge. |
| P | weekend-db::weekend_db | src/tree.rs:85:9– | 2610..2612 | if | remove | Missing removal returns the same root with zero requests; present removal changes the result. |
| E | weekend-db::weekend_db | src/tree.rs:89:78– | 2781..2782 | try_propagation | remove | The natural-hash removal sweep refuses path copying through the real wrapper. |
| E | weekend-db::weekend_db | src/tree.rs:100:74– | 3210..3211 | try_propagation | materialize | Selected row reserve refusal propagates through the Rust output path. The Global failure suite exercises copied keys and external buffers. |
| E | weekend-db::weekend_db | src/tree.rs:115:47– | 3669..3670 | try_propagation | borrowed_entries | The borrowed view rejects a header or key beyond the budget before reserving rows. |
| E | weekend-db::weekend_db | src/tree.rs:124:71– | 4014..4015 | try_propagation | materialize_with | Owned materialization returns the budget error before project or allocation. |
| E | weekend-db::weekend_db | src/tree.rs:125:65– | 4081..4082 | try_propagation | materialize_with | Refusing a copied key drops partial materialization. |
| I | weekend-db::weekend_db | src/tree.rs:131:33– | 4312..4313 | try_propagation | check_row_budget | Production callers use Value rows. The validated entry count is at most one million and the fixed row size fits on supported ABIs. |
| P | weekend-db::weekend_db | src/tree.rs:132:9– | 4323..4325 | if | check_row_budget | An empty tree fits zero bytes and a nonempty header can exceed the budget. |
| I | weekend-db::weekend_db | src/tree.rs:136:67– | 4491..4492 | try_propagation | check_row_budget | At production callsites, the prior admitted size and a valid slice length are each at most isize::MAX. |
| P | weekend-db::weekend_db | src/tree.rs:137:13– | 4506..4508 | if | check_row_budget | Headers fit while accumulated key bytes exceed the budget by one. The exact budget succeeds. |
| E | weekend-db::weekend_db | src/tree.rs:149:40– | 4879..4880 | try_propagation | collect_ordered | The first refusal precedes row visits. |
| E | weekend-db::weekend_db | src/tree.rs:151:37– | 4957..4958 | try_propagation | collect_ordered | An error during row construction drops the partially filled vector. |
| E | weekend-db::weekend_db | src/tree.rs:153:11– | 4990..4991 | try_propagation | collect_ordered | The outer collector propagates walk errors and reclaims scratch. |
| E | weekend-db::weekend_db | src/tree.rs:165:13– | 5378..5383 | match | visit::walk | Callback tests cover both leaf and branch traversal. |
| E | weekend-db::weekend_db | src/tree.rs:167:21– | 5462..5465 | for_iteration | visit::walk | The collision leaf visits two entries on success and stops at the first callback error. |
| E | weekend-db::weekend_db | src/tree.rs:168:38– | 5529..5530 | try_propagation | visit::walk | Leaf callback failure stops before the next item. |
| E | weekend-db::weekend_db | src/tree.rs:172:21– | 5639..5642 | for_iteration | visit::walk | The split branch walks populated children and skips the empty slots. |
| E | weekend-db::weekend_db | src/tree.rs:173:39– | 5718..5719 | try_propagation | visit::walk | A failed child walk stops sibling traversal. |
| C | weekend-db::weekend_db | src/tree.rs:179:9– | 5812..5814 | if | visit | The if shares the root-presence pattern outcome. |
| P | weekend-db::weekend_db | src/tree.rs:179:12– | 5815..5818 | pattern_test | visit | An empty root visits zero items; a populated root enters the walk. |
| E | weekend-db::weekend_db | src/tree.rs:180:26– | 5870..5871 | try_propagation | visit | Root callback errors propagate through the public traversal wrapper. |
| E | weekend-db::weekend_db | src/tree.rs:187:33– | 6074..6075 | try_propagation | leaf | A new leaf vector can be refused during split insertion. |
| P | weekend-db::weekend_db | src/tree.rs:199:5– | 6336..6339 | let_else | insert | Insertion into an absent node builds a leaf; insertion into an existing node dispatches by kind. |
| E | weekend-db::weekend_db | src/tree.rs:202:5– | 6420..6425 | match | insert | Insertion exercises a same-hash leaf, a distinct-hash split and recursive branch editing. |
| P | weekend-db::weekend_db | src/tree.rs:206:14– | 6522..6539 | match_guard | insert | A full collision appends to the leaf. A different hash splits the leaf. |
| E | weekend-db::weekend_db | src/tree.rs:211:81– | 6795..6796 | try_propagation | insert | A refused collision copy releases scratch and preserves both input maps. |
| C | weekend-db::weekend_db | src/tree.rs:213:13– | 6860..6862 | if | insert | The if shares the collision-replacement pattern outcome. |
| P | weekend-db::weekend_db | src/tree.rs:213:16– | 6863..6866 | pattern_test | insert | The collision fixture replaces one matching key and appends a missing key. |
| E | weekend-db::weekend_db | src/tree.rs:236:14– | 7645..7646 | try_propagation | insert | A long-prefix split releases every partially assembled node on refusal. |
| E | weekend-db::weekend_db | src/tree.rs:249:14– | 8052..8053 | try_propagation | insert | Refused recursion on an existing branch preserves its shared sibling. |
| E | weekend-db::weekend_db | src/tree.rs:261:5– | 8289..8294 | match | remove | The fixture removes from a collision leaf and through branch children. |
| P | weekend-db::weekend_db | src/tree.rs:263:13– | 8360..8362 | if | remove | Removing a sole leaf entry returns no root. Removing one collision entry retains a leaf. |
| E | weekend-db::weekend_db | src/tree.rs:267:54– | 8530..8531 | try_propagation | remove | Refusing the filtered collision vector preserves its input. |
| E | weekend-db::weekend_db | src/tree.rs:272:15– | 8734..8735 | try_propagation | remove | Refusing the new leaf header drops the filtered collision vector. |
| E | weekend-db::weekend_db | src/tree.rs:277:99– | 8977..8978 | try_propagation | remove | A failed recursive removal preserves the parent branch. |
| I | weekend-db::weekend_db | src/tree.rs:279:13– | 9046..9049 | let_else | remove | A valid branch has at least two entries with distinct hashes and one removal leaves a live child. |
| P | weekend-db::weekend_db | src/tree.rs:282:13– | 9143..9145 | if | remove | One remaining leaf collapses. One remaining branch and two children independently retain the branch. |
| C | weekend-db::weekend_db | src/tree.rs:282:16– | 9146..9211 | short_circuit_and | remove | The collapse conjunction shares the enclosing if decision. |
| E | weekend-db::weekend_db | src/tree.rs:285:58– | 9333..9334 | try_propagation | remove | Refusing the parent header drops the edited subtree without changing the old root. |
| P | weekend-db::weekend_db | src/tree.rs:322:9– | 10151..10153 | if | get | An existing index is readable. The index at len is missing, including at a radix boundary. |
| I | weekend-db::weekend_db | src/tree.rs:325:44– | 10252..10253 | try_propagation | get | Every in-range index has a root by contiguous-list construction. |
| E | weekend-db::weekend_db | src/tree.rs:328:13– | 10320..10325 | match | get | Index lookup traverses both branches and slots in leaves. |
| I | weekend-db::weekend_db | src/tree.rs:331:77– | 10533..10534 | try_propagation | get | Every child on an in-range path exists by contiguous-list construction. |
| P | weekend-db::weekend_db | src/tree.rs:338:9– | 10710..10712 | if | set | Replacing the final valid index succeeds and setting at len returns Bounds. |
| E | weekend-db::weekend_db | src/tree.rs:348:14– | 10978..10979 | try_propagation | set | Refusing a replacement path keeps the base list and shared sibling. |
| P | weekend-db::weekend_db | src/tree.rs:354:9– | 11139..11141 | if | push | Appending below the limit succeeds. A full list returns Limit. |
| I | weekend-db::weekend_db | src/tree.rs:361:33– | 11418..11419 | try_propagation | push | At one million entries the height is at most four and capacity is at most 16 to the fifth, within usize. |
| P | weekend-db::weekend_db | src/tree.rs:362:9– | 11429..11431 | if | push | Below capacity the shape is reused. Lengths 16 and 256 require a new root before push. |
| E | weekend-db::weekend_db | src/tree.rs:365:63– | 11611..11612 | try_propagation | push | Refusing the new branch header at length 256 leaves the full input intact. |
| E | weekend-db::weekend_db | src/tree.rs:369:77– | 11744..11745 | try_propagation | push | Refusals after root growth drop the new path and preserve the full input. |
| P | weekend-db::weekend_db | src/tree.rs:375:9– | 11885..11887 | if | pop | Empty pop returns Bounds and a populated list takes the tail path. |
| P | weekend-db::weekend_db | src/tree.rs:378:9– | 11961..11963 | if | pop | Singleton pop clears the root without allocation and longer lists clear a tail path. |
| E | weekend-db::weekend_db | src/tree.rs:381:95– | 12120..12121 | try_propagation | pop | Refusing partial tail clearing preserves the length-258 input. |
| P | weekend-db::weekend_db | src/tree.rs:383:9– | 12169..12174 | while | pop | Length 17 shrinks. Lengths 18 and 2 independently hold structure or height outside the shrink test. |
| C | weekend-db::weekend_db | src/tree.rs:383:15– | 12175..12231 | short_circuit_and | pop | The shrink conjunction shares the while decision. |
| I | weekend-db::weekend_db | src/tree.rs:384:13– | 12246..12249 | let_else | pop | A positive-height list has a branch root, and the first retained child exists. |
| P | weekend-db::weekend_db | src/tree.rs:404:5– | 12721..12723 | if | list_set | Height zero updates a leaf. Positive height selects or allocates a child. |
| E | weekend-db::weekend_db | src/tree.rs:405:25– | 12762..12767 | match | list_set | Leaf editing copies an existing leaf or starts an absent leaf. A branch cannot occur at height zero. |
| E | weekend-db::weekend_db | src/tree.rs:412:28– | 13029..13034 | match | list_set | Branch editing copies a populated branch or starts an absent branch. A leaf cannot occur at positive height. |
| E | weekend-db::weekend_db | src/tree.rs:423:10– | 13395..13396 | try_propagation | list_set | Refused recursive setting drops the assembled path. |
| E | weekend-db::weekend_db | src/tree.rs:433:5– | 13605..13610 | match | list_clear | Tail clearing reaches both branches and leaves. |
| P | weekend-db::weekend_db | src/tree.rs:442:13– | 13947..13949 | if | list_clear | An aligned tail detaches the whole child. A partial leaf retains its first item and clears its second slot. |
| E | weekend-db::weekend_db | src/tree.rs:450:18– | 14256..14257 | try_propagation | list_clear | Refused recursive clearing drops the private path and retains the old tail. |
