# Persistent tree source decision audit

The production AST inventory contains **69 records** for `src/tree.rs`: 32
propagation operators, 17 ifs, eight matches, three ANDs, three let-else forms,
two loops, two pattern tests, one while and one match guard. The exact source
hash and complete span classification are below. Counts include overlapping
syntax; they are not an MC/DC percentage or independent-condition denominator.
Only cfg(test) fixtures were added. Runtime and unsafe code were not changed.

Statuses are P predicate controls/pairs, E execution/dispatch/propagation evidence,
C overlapping syntax, I infeasible residual on valid structures under the stated
invariant, and D delegated allocator/primitive propagation. A D does not claim
all errors at that callsite were injected.

## T1: map lookup, capacity and leaf dispatch

Existing `map_capacity_and_collision_lookup_have_independent_controls` supplies
(!exists,at_capacity)=(T,T) rejection, (F,T) replacement and (T,F) insertion.
These isolate both capacity-AND operands. Empty roots, matching hash/key,
matching hash/different key and different hash/matching key control lookup.
New `branch_collapse_distinguishes_one_child_and_leaf_independently` also probes
a missing child slot in a branch. Insert exercises empty-node leaf creation,
equal-hash replacement/new collision entry, distinct-hash splitting and recursive
branch insertion. Existing `full_hash_collisions_and_long_prefixes` injects
identical hashes and hashes differing only in the last nibble, avoiding any
claim that random binary keys discover 64-bit collisions.

## T2: removal and branch compression

The new collapse test builds keys with hashes 0,16,1. For A=one remaining child,
B=that child is a leaf, deleting the hash-1 sibling gives (T,F), deleting the
hash-16 child gives (F,T), and then deleting the sibling from the two-leaf tree
gives (T,T). The first/third isolate B and second/third isolate A. Actual node
shapes and old-root values are asserted. Existing collision removal exercises
multi-entry leaf filtering; singleton removal returns None. Public map_remove
covers absent-key no-op and present-key removal.

A valid Branch contains at least two leaf entries. Insertion creates branches
only by splitting distinct hashes; unary chains end at a multi-child branch.
Removal from such a branch cannot erase its last entry. If only a leaf remains,
compression removes the unary branch. Therefore the no-first-live-child return
at line 245 is defensive and infeasible for a valid input tree. No fabricated
malformed node was used to count it as covered. Lookup-before-remove also proves
its unwrap paths have the selected child/key.

## T3: materialization and callbacks

Existing `nested_map_scans_enforce_zero_and_exact_buffer_budgets` controls both
entry-buffer and cumulative-key budget checks with just-below/exact limits,
empty output, shared child values and sorted binary keys. The oversized-scan
regression proves rejection precedes allocation under a zero allocation budget.
New `visit_error_stops_before_later_items_and_empty_visit_never_calls` gives empty,
leaf-collision and branch trees, successful callbacks and a first-item error;
the callback count proves no later item is visited after failure.

The multiplication guard cannot overflow in shipped materialize instantiations:
T is Value at both callsites, the tuple is 40 bytes on the tested 64-bit target
(20 on 32-bit), and validated max_entries is at most 1,000,000. This proof is
scoped to those instantiations, not hypothetical huge generic T types. During
key accumulation size has already passed the budget guard, and both budget and
validated key length are at most isize::MAX. Their sum is at most usize::MAX-1,
so the checked-add residual is infeasible on supported 32-/64-bit targets.
Output reservation/key-copy OOM remains delegated to engine allocation tests;
this tranche does not inject every allocator failure at every materialize call.

## T4: list shape, bounds, growth and shrink

New `list_capacity_shrink_bounds_and_tail_clear_have_controls` checks empty pop,
singleton pop, out-of-range get/set, full-capacity push versus replacement, and
old roots across 2,16,17,18,256,257,258 lengths. Growth occurs when capacity is
exactly reached; below-capacity insertion retains height. Tail clearing at old
length 17/257 removes an aligned child; length 18/258 descends into a partial
child. Existing radix/model tests add long sequences and middle edits.

For the shrink while A=height>0 and B=new length<=16^height, old length 17 gives
(T,T), length 18 gives (T,F), and length 2 gives (F,T). The latter matters:
length 16 at height zero would have B false and would not isolate A. Final
height and every remaining element are checked. Singleton returns before the
while, so it is not substituted for the length-two control.

Contiguous indices below len always have a root and selected children. Only
push creates new slots, set preserves population, and pop clears the final
slot/subtree while lowering len. Thus the Option residuals in List::get after
its bounds check are infeasible. At positive height root is Branch; the
unreachable let-else in shrink follows that invariant. list_set's wildcard
covers absent nodes but a wrong-kind node at a given height is infeasible.
With max_entries<=1,000,000, height never exceeds four; 16^(height+1)<=1,048,576.
The checked capacity exponent and height*4 shifts cannot overflow on supported
32-/64-bit targets. This is a bound proof, not impractically large test data.

## T5: allocation and scope

Existing `untouched_branches_are_shared_and_path_failures_preserve_roots`
checks structural sharing and sweeps allocation failures for split insertion,
list replacement and pop. Generic engine allocation/value tests supply broader
propagation evidence. D records retain per-call failure attribution as delegated;
no whole-tree MC/DC or every-generic-instantiation claim is made. Iterator,
Option and sorting implementation branches are outside the explicit AST count.

## Complete record appendix

| Span (start line:column–end line:column) / kind | Owner / expression | Status | Evidence |
|---|---|---|---|
| 47:44–47:45 try_propagation | `get_hashed` — `self.root.as_deref()` | E | T1 |
| 50:13–50:18 match | `get_hashed` — `node` | E | T1 |
| 52:28–52:30 if | `get_hashed` — `*h == hash` | P | T1 |
| 62:66–62:67 try_propagation | `get_hashed` — `children[slot(hash, level)].as_deref()` | E | T1 |
| 73:9–73:11 if | `set_hashed` — `!exists ; self.len >= mem.limits().max_entries` | P | T1 |
| 73:12–73:59 short_circuit_and | `set_hashed` — `!exists && self.len >= mem.limits().max_entries` | C | T1 |
| 76:43–76:44 try_propagation | `set_hashed` — `mem.copy(key)` | D | T1/T5 |
| 76:45–76:46 try_propagation | `set_hashed` — `mem.shared(mem.copy(key)?)` | D | T1/T5 |
| 77:72–77:73 try_propagation | `set_hashed` — `insert(mem, self.root.as_ref(), key, value, hash, 0)` | D | T1/T5 |
| 84:9–84:11 if | `remove` — `self.get(key).is_none()` | P | T2 |
| 88:78–88:79 try_propagation | `remove` — `remove(mem, self.root.as_ref().unwrap(), key, hash(key), 0)` | D | T2/T5 |
| 103:33–103:34 try_propagation | `materialize` — `self .len .checked_mul(std::mem::size_of::<(Vec<u8>, T)>()) .ok_or(Error::Limit)` | I | T3 |
| 104:9–104:11 if | `materialize` — `size > budget` | P | T3 |
| 108:67–108:68 try_propagation | `materialize` — `size.checked_add(key.len()).ok_or(Error::Limit)` | I | T3 |
| 109:13–109:15 if | `materialize` — `size > budget` | P | T3 |
| 113:11–113:12 try_propagation | `materialize` — `self.visit(&mut \|key, _\| { size = size.checked_add(key.len()).ok_or(Error::Limit)?; if size > budget { return Err(Error::Limit); } Ok(()) })` | E | T3 |
| 115:40–115:41 try_propagation | `materialize` — `mem.reserve(&mut out, self.len)` | D | T3/T5 |
| 117:45–117:46 try_propagation | `materialize` — `mem.copy_external(key)` | D | T3/T5 |
| 119:11–119:12 try_propagation | `materialize` — `self.visit(&mut \|key, value\| { out.push((mem.copy_external(key)?, project(value))); Ok(()) })` | D | T3/T5 |
| 131:13–131:18 match | `visit::walk` — `node` | E | T3 |
| 133:21–133:24 for_iteration | `visit::walk` — `entries` | E | T3 |
| 134:38–134:39 try_propagation | `visit::walk` — `f(key, value)` | E | T3 |
| 138:21–138:24 for_iteration | `visit::walk` — `children.iter().flatten()` | E | T3 |
| 139:39–139:40 try_propagation | `visit::walk` — `walk(child, f)` | E | T3 |
| 145:9–145:11 if | `visit` — `let Some(root) = &self.root` | P | T3 |
| 145:12–145:15 pattern_test | `visit` — `&self.root` | C | T3 |
| 146:26–146:27 try_propagation | `visit` — `walk(root, f)` | E | T3 |
| 153:33–153:34 try_propagation | `leaf` — `mem.reserve(&mut entries, 1)` | D | T1/T5 |
| 165:5–165:8 let_else | `insert` — `node` | P | T1 |
| 168:5–168:10 match | `insert` — `&**node` | E | T1 |
| 172:14–172:31 match_guard | `insert` — `*old_hash == hash` | P | T1 |
| 177:81–177:82 try_propagation | `insert` — `mem.reserve(&mut copy, entries.len() + usize::from(index.is_none()))` | D | T1/T5 |
| 179:13–179:15 if | `insert` — `let Some(i) = index` | P | T1 |
| 179:16–179:19 pattern_test | `insert` — `index` | C | T1 |
| 202:14–202:15 try_propagation | `insert` — `insert( mem, children[new].as_ref(), key, value, hash, level + 1, )` | D | T1/T5 |
| 215:14–215:15 try_propagation | `insert` — `insert( mem, children[index].as_ref(), key, value, hash, level + 1, )` | D | T1/T5 |
| 227:5–227:10 match | `remove` — `&**node` | E | T2 |
| 229:13–229:15 if | `remove` — `entries.len() == 1` | P | T2 |
| 233:54–233:55 try_propagation | `remove` — `mem.reserve(&mut copy, entries.len() - 1)` | D | T2/T5 |
| 238:15–238:16 try_propagation | `remove` — `mem.shared(MapNode::Leaf { hash, entries: copy, })` | D | T2/T5 |
| 243:99–243:100 try_propagation | `remove` — `remove(mem, children[index].as_ref().unwrap(), key, hash, level + 1)` | D | T2/T5 |
| 245:13–245:16 let_else | `remove` — `live.next()` | I | T2 |
| 248:13–248:15 if | `remove` — `live.next().is_none() ; matches!(&**first, MapNode::Leaf { .. })` | P | T2 |
| 248:16–248:81 short_circuit_and | `remove` — `live.next().is_none() && matches!(&**first, MapNode::Leaf { .. })` | C | T2 |
| 251:58–251:59 try_propagation | `remove` — `mem.shared(MapNode::Branch(children))` | D | T2/T5 |
| 288:9–288:11 if | `get` — `index >= self.len` | P | T4 |
| 291:44–291:45 try_propagation | `get` — `self.root.as_deref()` | I | T4 |
| 294:13–294:18 match | `get` — `node` | E | T4 |
| 297:77–297:78 try_propagation | `get` — `children[(index >> (height * 4)) & 15].as_deref()` | I | T4 |
| 304:9–304:11 if | `set` — `index >= self.len` | P | T4 |
| 314:14–314:15 try_propagation | `set` — `list_set( mem, self.root.as_ref(), self.height, index, value, )` | D | T4/T5 |
| 320:9–320:11 if | `push` — `self.len >= mem.limits().max_entries` | P | T4 |
| 327:33–327:34 try_propagation | `push` — `16usize .checked_pow((height + 1) as u32) .ok_or(Error::Limit)` | I | T4 |
| 328:9–328:11 if | `push` — `self.len == capacity` | P | T4 |
| 331:63–331:64 try_propagation | `push` — `mem.shared(ListNode::Branch(children))` | D | T4/T5 |
| 335:77–335:78 try_propagation | `push` — `list_set(mem, root.as_ref(), height, self.len, value)` | D | T4/T5 |
| 341:9–341:11 if | `pop` — `self.len == 0` | P | T4 |
| 344:9–344:11 if | `pop` — `self.len == 1` | P | T4 |
| 347:95–347:96 try_propagation | `pop` — `list_clear(mem, self.root.as_ref().unwrap(), self.height, self.len - 1)` | D | T4/T5 |
| 349:9–349:14 while | `pop` — `height > 0 ; self.len - 1 <= 16usize.pow(height as u32)` | P | T4 |
| 349:15–349:71 short_circuit_and | `pop` — `height > 0 && self.len - 1 <= 16usize.pow(height as u32)` | C | T4 |
| 350:13–350:16 let_else | `pop` — `&*root` | I | T4 |
| 370:5–370:7 if | `list_set` — `height == 0` | P | T4 |
| 371:25–371:30 match | `list_set` — `node.map(\|n\| &**n)` | E | T4 |
| 378:28–378:33 match | `list_set` — `node.map(\|n\| &**n)` | E | T4 |
| 389:10–389:11 try_propagation | `list_set` — `list_set( mem, children[slot].as_ref(), height - 1, index, value, )` | D | T4/T5 |
| 399:5–399:10 match | `list_clear` — `&**node` | E | T4 |
| 408:13–408:15 if | `list_clear` — `index & ((1 << (height * 4)) - 1) == 0` | P | T4 |
| 416:18–416:19 try_propagation | `list_clear` — `list_clear( mem, children[slot].as_ref().unwrap(), height - 1, index, )` | D | T4/T5 |

## Results and binding

C=5, D=21, E=16, I=7, P=20; all 69 source records classified.

Debug/release scoped checks passed: six tree unit tests (three existing, three
new), two new private Value tests, four new public Value tests and seven existing
value/model/bounds tests. Clippy with warnings denied passed. Tests used
`CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/tree-assurance`; no new shared
workspace target was created. Commands, with `--release` repeated:

```sh
cargo test --offline --locked -p weekend-db --all-features --lib tree::
cargo test --offline --locked -p weekend-db --all-features --lib value::
cargo test --offline --locked -p weekend-db --all-features --test values --test value_decisions
cargo clippy --offline --locked -p weekend-db --all-features --lib --test value_decisions -- -D warnings
```

| File | SHA-256 |
|---|---|
| `src/tree.rs` | `bcec08cd41a96fa1e5f82fbeb2b469f4f459110995ee75abd5f6487e05c24cd0` |
| `tests/value_decisions.rs` | `eda8394a8b1305e110ec99bb7d50ba78436c1a0843a0f472cdd6213fc6284574` |
| `tests/values.rs` | `adcb83f2d92ccae41fb25505d7e4e22334a77d03eec945ea711ff1616c3c1067` |

Source hashes include cfg(test) additions although the denominator excludes test
bodies. No mutation campaign or compiler MC/DC percentage is claimed. D errors,
standard-library/compiler branches and future generic/platform variants remain
explicitly outside the closed local predicate/invariant controls.
