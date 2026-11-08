# Value source decision audit

The production AST inventory contains **79 records** for `src/value.rs`: 32
propagation operators, 14 ifs, ten let-else forms, nine matches, five match guards,
five ORs, three loops and one AND. Exact source hashes and every span appear
below. Overlapping expressions are not separate MC/DC obligations. Runtime code
is unchanged; new source additions are cfg(test) fixtures. No unsafe code changed,
so this tranche did not require a new Miri run.

P denotes predicate controls/pairs, E execution/dispatch/specific propagation,
C duplicate syntax, I a residual excluded by a stated invariant and D delegated
allocator/tree propagation. This is a source classification, not compiler-measured
MC/DC or a claim that every callee error is injected at each `?`.

## V1: construction, scalar bits and logical arithmetic

`constructor_depth_weight_or_and_scalar_bits_have_controls` constructs a real
one-child raw list with depth two and logical weight 32. Limits (2,32) accept,
(1,32) reject only depth and (2,31) reject only weight. These isolate the new
constructor OR without inventing invalid Value metadata. The same private test
controls canonical/noncanonical bits for None, Boolean, Map and List; integer,
unsigned and float all-bit patterns accept; Bytes rejects in scalar dispatch.

Logical weight is **not** the physical allocation size. New
`shared_children_make_aggregate_overflow_feasible_without_large_allocations`
doubles shared children in small persistent lists until a valid value weighs
isize::MAX-15. This needs roughly 60 small nodes on 64-bit, not an enormous
allocation. Candidate raw maps/lists containing three such children overflow
their aggregate checked sums and reject. A nearly full valid map plus another
large child and a 32-byte new key also overflows map_set's final key addition.
Those residuals are feasible and tested, not dismissed as huge-memory cases.
Empty aggregation has direct successful controls too.

Conversely, list set/push/insert add at most two already-valid logical weights,
each <=isize::MAX; their sum cannot exceed usize::MAX-1. Subtracted old weight
belongs to the container and cannot underflow. Byte length is bounded by
isize::MAX before adding fixed overhead 16, which cannot overflow usize on
supported 32-/64-bit targets. Those checked arithmetic residuals are I. The map
sum has a third key term, so the same proof deliberately does not apply there.

## V2: accessors, kind, context, byte and key boundaries

`scalar_accessors_and_container_type_errors_are_explicit` exercises every Kind
against every scalar accessor and len/is_empty, and every map/list operation's
wrong-container Type path with a same-store valid operand. Correct-kind behavior
is supplied by existing scalar/model tests and the new bounds test. Foreign
operands are tested separately for map_set and list_set/push/insert so they do
not mask the type controls. One-byte keys fit and two-byte keys fail at max_bytes1.

`bytes_length_and_total_weight_guards_precede_allocation` accepts two bytes at
weight limit18. Three bytes reject independently with max_bytes2 or with
max_bytes3 plus the same weight cap. Zero allocation budget proves both guards
reject before allocation. This complements existing depth/size/store tests.

## V3: immutable edits and list positions

Existing `value_edit_weight_and_child_depth_have_independent_boundaries` supplies
(F,F), (T,F) and (F,T) controls for the weight-or-child-depth guard in map_set,
list_set, list_push and middle list_insert. Tests assert old values remain intact
and use zero allocation budget on failing controls to distinguish early guards.
Existing random model/history and radix tests verify updates/deletes and suffix
rebuilding. New `foreign_context_key_bounds_and_list_positions_have_valid_controls`
covers empty/missing map removal, list access/removal/replacement at len,
insertion at zero/end/beyond-end, full list rejection and existing-item replacement.

Public wrapper bounds/type checks establish tree preconditions. A successful
list_get before list_set/list_remove proves the selected old item exists.
Middle insertion implies a nonempty list, so its last-item and suffix unwraps
are safe. Removing a valid index implies nonempty input for pop. The raw tree
has no holes below len; shifted indices stay in bounds. Tree allocation failures
at these calls are D rather than incorrectly called impossible.

## V4: equality and depth calculations

`equality_distinguishes_pointer_kind_lengths_keys_and_contents` uses a clone for
the pointer shortcut and separately constructed equal values for deep equality.
It compares equal/different scalar values, mismatched kinds, signed-zero and NaN
bit patterns, list lengths/prefixes/content, and map lengths/keys/content/order.
For list equality A=same length and B=all corresponding items equal, equal lists
give (T,T), a shorter equal prefix gives (F,T), and equal-length differing content
gives (T,F). These isolate both operands; B is conceptual when skipped. Map
length guard and missing/different corresponding value have controls too.

map_depth's visitor cannot error because its callback always returns Ok and tree
visit only propagates callback errors. Valid child depth<=64 makes +1 safe.
The equality visitor likewise only returns Ok, proving its unwrap. Private
node/weight invariants do not permit mutable cycles. These proofs do not cover
arbitrary unsafe/private-state corruption or standard-library iterator internals.

## V5: delegated and remaining scope

Tree paths, charged allocation and budget enforcement retain their own audits
and allocation-failure suites. D records do not claim all memory failures at all
Value callsites were measured. Existing seven value tests are rerun here;
adversarial-key/model fuzz is prior complementary evidence, not a new campaign
in this tranche. Compiler-generated enum handling, formatting and closure/iterator
branches beyond explicit AST records are outside this denominator. No blanket
MC/DC percentage or platform qualification follows from this audit.

## Complete record appendix

| Span (start line:column–end line:column) / kind | Owner / expression | Status | Evidence |
|---|---|---|---|
| 61:37–61:38 try_propagation | `from_map` — `weight .checked_add(key.len()) .and_then(\|n\| n.checked_add(value.weight())) .ok_or(Error::Limit)` | E | V1 |
| 63:11–63:12 try_propagation | `from_map` — `map.visit(&mut \|key, value\| { depth = depth.max(value.node.depth + 1); weight = weight .checked_add(key.len()) .and_then(\|n\| n.checked_add(value.weight())) .ok_or(Error::Limit)?; Ok(()) })` | E | V1 |
| 69:9–69:12 for_iteration | `from_list` — `0..list.len()` | E | V1 |
| 72:76–72:77 try_propagation | `from_list` — `weight.checked_add(value.weight()).ok_or(Error::Limit)` | E | V1 |
| 77:9–77:11 if | `new` — `depth > mem.limits().max_depth ; weight > mem.limits().max_value_bytes` | P | V1 |
| 77:12–77:83 short_circuit_or | `new` — `depth > mem.limits().max_depth \|\| weight > mem.limits().max_value_bytes` | C | V1 |
| 86:15–86:16 try_propagation | `new` — `mem.shared(Node { data, depth, weight, })` | D | V1/V5 |
| 90:20–90:25 match | `scalar` — `kind` | E | V1 |
| 91:27–91:36 match_guard | `scalar` — `bits == 0` | P | V1 |
| 92:30–92:39 match_guard | `scalar` — `bits <= 1` | P | V1 |
| 96:26–96:35 match_guard | `scalar` — `bits == 0` | P | V1 |
| 97:27–97:36 match_guard | `scalar` — `bits == 0` | P | V1 |
| 103:9–103:11 if | `bytes_new` — `bytes.len() > mem.limits().max_bytes` | P | V1 |
| 106:69–106:70 try_propagation | `bytes_new` — `bytes.len().checked_add(16).ok_or(Error::Limit)` | I | V1 |
| 107:9–107:11 if | `bytes_new` — `weight > mem.limits().max_value_bytes` | P | V1 |
| 110:51–110:52 try_propagation | `bytes_new` — `mem.copy(bytes)` | D | V1/V5 |
| 116:9–116:14 match | `kind` — `&self.node.data` | E | V2 |
| 128:9–128:14 match | `as_bool` — `self.node.data` | E | V2 |
| 134:9–134:14 match | `as_i64` — `self.node.data` | E | V2 |
| 140:9–140:14 match | `as_u64` — `self.node.data` | E | V2 |
| 146:9–146:14 match | `as_f64` — `self.node.data` | E | V2 |
| 152:9–152:14 match | `as_bytes` — `&self.node.data` | E | V2 |
| 158:9–158:14 match | `len` — `&self.node.data` | E | V2 |
| 166:22–166:23 try_propagation | `is_empty` — `self.len()` | E | V2 |
| 169:9–169:11 if | `same` — `self.mem.same(&other.mem)` | P | V2 |
| 176:9–176:11 if | `key` — `key.len() > self.mem.limits().max_bytes` | P | V2 |
| 183:22–183:23 try_propagation | `map_get` — `self.key(key)` | E | V2 |
| 184:9–184:12 let_else | `map_get` — `&self.node.data` | P | V2 |
| 190:25–190:26 try_propagation | `map_set` — `self.same(value)` | E | V2 |
| 191:22–191:23 try_propagation | `map_set` — `self.key(key)` | E | V2 |
| 192:9–192:12 let_else | `map_set` — `&self.node.data` | P | V3 |
| 197:25–197:27 if | `map_set` — `old.is_some()` | P | V3 |
| 203:33–203:34 try_propagation | `map_set` — `self .weight() .checked_sub(removed) .and_then(\|n\| n.checked_add(value.weight())) .and_then(\|n\| n.checked_add(added_key)) .ok_or(Error::Limit)` | E | V3 |
| 204:9–204:11 if | `map_set` — `weight > self.mem.limits().max_value_bytes ; value.node.depth >= self.mem.limits().max_depth` | P | V3 |
| 204:12–205:63 short_circuit_or | `map_set` — `weight > self.mem.limits().max_value_bytes \|\| value.node.depth >= self.mem.limits().max_depth` | C | V3 |
| 209:57–209:58 try_propagation | `map_set` — `map.set(&self.mem, key, value.clone())` | D | V3/V5 |
| 210:36–210:37 try_propagation | `map_set` — `map_depth(&map)` | I | V3 |
| 214:22–214:23 try_propagation | `map_remove` — `self.key(key)` | E | V3 |
| 215:9–215:12 let_else | `map_remove` — `&self.node.data` | P | V3 |
| 218:9–218:12 let_else | `map_remove` — `map.get(key)` | P | V3 |
| 222:45–222:46 try_propagation | `map_remove` — `map.remove(&self.mem, key)` | D | V3/V5 |
| 223:36–223:37 try_propagation | `map_remove` — `map_depth(&map)` | I | V3 |
| 230:9–230:12 let_else | `map_entries` — `&self.node.data` | P | V3 |
| 237:9–237:12 let_else | `list_get` — `&self.node.data` | P | V3 |
| 243:25–243:26 try_propagation | `list_set` — `self.same(value)` | E | V3 |
| 244:9–244:12 let_else | `list_set` — `&self.node.data` | P | V3 |
| 247:55–247:56 try_propagation | `list_set` — `list.get(index).ok_or(Error::Bounds)` | E | V3 |
| 250:33–250:34 try_propagation | `list_set` — `(self.weight() - old.weight()) .checked_add(value.weight()) .ok_or(Error::Limit)` | I | V3 |
| 251:9–251:11 if | `list_set` — `weight > self.mem.limits().max_value_bytes ; value.node.depth >= self.mem.limits().max_depth` | P | V3 |
| 251:12–252:63 short_circuit_or | `list_set` — `weight > self.mem.limits().max_value_bytes \|\| value.node.depth >= self.mem.limits().max_depth` | C | V3 |
| 256:61–256:62 try_propagation | `list_set` — `list.set(&self.mem, index, value.clone())` | D | V3/V5 |
| 261:25–261:26 try_propagation | `list_push` — `self.same(value)` | E | V3 |
| 262:9–262:12 let_else | `list_push` — `&self.node.data` | P | V3 |
| 268:33–268:34 try_propagation | `list_push` — `self .weight() .checked_add(value.weight()) .ok_or(Error::Limit)` | I | V3 |
| 269:9–269:11 if | `list_push` — `weight > self.mem.limits().max_value_bytes ; value.node.depth >= self.mem.limits().max_depth` | P | V3 |
| 269:12–270:63 short_circuit_or | `list_push` — `weight > self.mem.limits().max_value_bytes \|\| value.node.depth >= self.mem.limits().max_depth` | C | V3 |
| 274:55–274:56 try_propagation | `list_push` — `list.push(&self.mem, value.clone())` | D | V3/V5 |
| 284:25–284:26 try_propagation | `list_insert` — `self.same(value)` | E | V3 |
| 285:9–285:12 let_else | `list_insert` — `&self.node.data` | P | V3 |
| 288:9–288:11 if | `list_insert` — `index > list.len()` | P | V3 |
| 291:9–291:11 if | `list_insert` — `index == list.len()` | P | V3 |
| 297:33–297:34 try_propagation | `list_insert` — `self .weight() .checked_add(value.weight()) .ok_or(Error::Limit)` | I | V3 |
| 298:9–298:11 if | `list_insert` — `weight > self.mem.limits().max_value_bytes ; value.node.depth >= self.mem.limits().max_depth` | P | V3 |
| 298:12–299:63 short_circuit_or | `list_insert` — `weight > self.mem.limits().max_value_bytes \|\| value.node.depth >= self.mem.limits().max_depth` | C | V3 |
| 303:87–303:88 try_propagation | `list_insert` — `list.push(&self.mem, list.get(list.len() - 1).unwrap().clone())` | D | V3/V5 |
| 304:9–304:12 for_iteration | `list_insert` — `(index..list.len() - 1).rev()` | E | V3 |
| 305:76–305:77 try_propagation | `list_insert` — `next.set(&self.mem, i + 1, list.get(i).unwrap().clone())` | D | V3/V5 |
| 307:57–307:58 try_propagation | `list_insert` — `next.set(&self.mem, index, value.clone())` | D | V3/V5 |
| 313:9–313:12 let_else | `list_remove` — `&self.node.data` | P | V3 |
| 316:55–316:56 try_propagation | `list_remove` — `list.get(index).ok_or(Error::Bounds)` | E | V3 |
| 318:9–318:12 for_iteration | `list_remove` — `index..list.len() - 1` | E | V3 |
| 319:76–319:77 try_propagation | `list_remove` — `next.set(&self.mem, i, list.get(i + 1).unwrap().clone())` | D | V3/V5 |
| 321:35–321:36 try_propagation | `list_remove` — `next.pop(&self.mem)` | D | V3/V5 |
| 336:7–336:8 try_propagation | `map_depth` — `map.visit(&mut \|_, v\| { depth = depth.max(v.node.depth + 1); Ok(()) })` | I | V4 |
| 347:9–347:11 if | `eq` — `self.node.ptr_eq(&other.node)` | P | V4 |
| 350:9–350:14 match | `eq` — `(&self.node.data, &other.node.data)` | E | V4 |
| 358:17–358:81 short_circuit_and | `eq` — `a.len() == b.len() && (0..a.len()).all(\|i\| a.get(i) == b.get(i))` | P | V4 |
| 360:45–360:63 match_guard | `eq` — `a.len() == b.len()` | P | V4 |
| 363:21–363:23 if | `eq` — `b.get(k) != Some(v)` | P | V4 |

## Results and binding

C=5, D=11, E=26, I=7, P=30; all 79 source records classified.

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
| `src/value.rs` | `11eedcbe0f50fd16f917a193ee65825cc6eaeed4309fc037165972d0a21c9da2` |
| `tests/value_decisions.rs` | `eda8394a8b1305e110ec99bb7d50ba78436c1a0843a0f472cdd6213fc6284574` |
| `tests/values.rs` | `adcb83f2d92ccae41fb25505d7e4e22334a77d03eec945ea711ff1616c3c1067` |

Source hashes include cfg(test) additions although the denominator excludes test
bodies. No mutation campaign or compiler MC/DC percentage is claimed. D errors,
standard-library/compiler branches and future generic/platform variants remain
explicitly outside the closed local predicate/invariant controls.
