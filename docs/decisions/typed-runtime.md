# Typed runtime source decision audit

This audit binds the three typed runtime source files to **315 source records**:
196 propagation operators, 56 ifs, 24 loops, 13 matches, 11 ORs, six ANDs,
four pattern tests, three let-else statements and two match guards. Source syntax
is not compiler coverage: overlapping records do not create independent MC/DC
obligations. All records receive a disposition in the appendix. Production code
was unchanged. Eleven directed tests supplement the 26 existing model tests and
two process-evolution tests.

| Status | Meaning |
|---|---|
| P | Explicit predicate controls described below; compound guards use independent controls where feasible |
| C | Overlapping syntax of a classified enclosing guard |
| E | Execution/dispatch evidence from the named group; not a claim of all outcomes or errors |
| I | Rejected outcome structurally infeasible under the stated invariant |
| D | Error propagation delegated to schema/core/allocation/transaction evidence; each individual call site is not independently fault-injected |

## T1: catalog, registration and identities

`catalog_identity_unique_and_name_guards_have_independent_controls` compares
empty, 128-byte and 129-byte table names. Catalog controls independently replace
valid records/type/version with 3/0/0, replace a unique list with a scalar, and
replace a positive unique ID with zero or a duplicate. Rejections preserve the
pre-operation revision. `manual_models_exercise_duplicate_container_and_missing_unique_ids`
uses handwritten Model implementations to reach scalar/Map/List, duplicate and
missing unique-field descriptor defenses. The first-item `i > 0` guard prevents
index underflow: evaluating the previous element at i=0 is infeasible, so it is
not claimed as an unrestricted independent MC/DC pair. Both first/subsequent
positions and duplicate/nonduplicate lists are exercised.

`table_registration_distinguishes_descriptor_presence_and_catalog_version`
checks immutable same-version descriptors, changed unique layouts, catalog
upgrade with a prepublished descriptor or a missing descriptor, repeated/older
registration without a commit, and the missing-descriptor catalog-version guard.
The latter isolates catalog version 1 versus 2 with requested version 2 and
schema 2 absent. Existing `metadata_validation_context_safety_compaction_and_inconsistent_index_detection`
provides foreign-store, format and schema identity rejection evidence. `table_handle_identity_operands_reject_independently` retains a valid Table handle
while replacing its raw catalog with (matching type, matching records), (different
positive type, matching records), or (matching type, another existing collection).
The first reads the original row; either independently changed identity operand
returns TypeMismatch with no read-side commit. The catalog remains structurally
valid in all three controls, so earlier validation cannot mask the tested OR.

The fresh database initialization loop must receive collection IDs 1, 2, 3:
revision zero plus exclusive mutable ownership and the engine's fresh-ID invariant
make the unequal-ID branch unreachable without changing engine semantics.
Allocation/ID-exhaustion errors from create_collection are separate propagated
outcomes, not evidence for unequal returned IDs.

## T2: field projection, codecs and macro expansion

`generated_helpers_preserve_missing_defaults_and_separate_type_ids` isolates
raw-record and schema type mismatch. It compares physical missing/present fields
with Presence versus Option, checks schema-version rejection versus prospective
fallback, and rejects a physically present prospective ID collision. Existing
`prospective_known_id_collision_is_never_silently_overwritten`,
`defaults_belong_to_stored_schema_and_presence_range_and_envelope_survive` and
`presence_keeps_missing_even_when_the_descriptor_supplies_a_default` provide
migration, unknown-envelope and stored-default controls. The second schema.field
lookup in `__private::read` cannot become None after the first successful lookup:
Schema is immutable and validated; only actual accessor errors remain delegated.

`option_optional_and_nullable_flags_are_independent` provides (false,false),
(true,false), (false,true) type flags for Option's nested-nullability rejection.
`scalar_macro_instantiations_and_presence_dispatch_are_exercised` covers bool,
i64, u64, f64 scalar macros and i8/i16/i32/u8/u16/u32 narrowing macros. Each narrow
type has valid extrema and overflow; signed types additionally have underflow.
Wrong scalar kinds are rejected, NaN bits preserved, UTF-8 rejection exercised,
and Vec wrong-container/item paths and Option/Presence missing/null/value dispatch
are exercised. Existing `scalar_boundaries_binary_lookup_and_float_bit_identity_roundtrip`
adds binary and floating-key semantics. **Macro bodies and expansions are not in
the 315-record AST denominator.** This manual matrix supplies runtime evidence;
it is not expansion-level compiler MC/DC. Generated Model code is exercised by
the test models; procedural macro parsing/diagnostic branches belong to the
separate macro audit. Rust allocation failures for returned String/Vec values are
not controlled by the core allocation injector and remain delegated/open, not
falsely attributed to its sweep. `error_display_dispatch_preserves_context_and_fallbacks` exercises every Error
variant: engine/schema delegation, contextual IDs/reasons, migration text, and
every Debug fallback. Concrete engine/schema formatting is delegated to those
error types; formatter failure injection is not claimed.

## T3: physical indexes, corruption and retained state

`lookup_declared_kind_encoded_kind_and_null_exemption_have_pairs` uses a custom
LookupValue: declared wrong kind rejects even encoded None; declared matching
kind with encoded None bypasses key lookup; a matching Bytes value succeeds;
a wrong non-None encoded kind rejects. These isolate all operands of the lookup
kind guard. An ignored error poisons the transaction and rolls back earlier work.
`index_corruption_rejects_without_modifying_retained_roots_or_staged_writes`
compares correct, wrong-kind and wrong-owner claims and a missing primary row.
Bad claims cannot resolve a different record, cannot partially update an index,
and leave a retained checkpoint readable. IndexReport consistency independently
changes missing/orphaned/mismatched from zero to one.

Reused `missing_unique_defaults_remain_unindexed_through_crud_history_and_reopen`,
`migrating_missing_unique_fields_does_not_index_either_schema_default` and
`stale_default_index_cannot_resolve_a_physically_missing_field` establish the
physical-presence invariant: defaults are projections and never implicit index
entries. Reused check/rebuild tests exercise missing, orphaned, mismatched and
duplicate primary claims, atomic repair, concurrent conflicts, retained unknown
fields, allocation rejection and interrupted durable publication. The two-stage
record_keys presence/key guard is masked when the physical value is absent;
physical None supplies the present-but-no-key control.

## T4: writes, migration and poisoning

`migration_batch_limits_independently_prevent_callbacks` holds 17 IDs fixed and
sets writes/observations to (16,1024), (1024,16), (1024,1024). Each limit alone
rejects before callbacks; the permissive control reaches missing-row validation.
The existing migration batch tests cover zero/duplicate IDs, empty and resumed
batches, older/current/newer version handling, callback rollback and concurrent
selected-row conflicts. Existing CRUD/semantic migration tests cover primary and
index addition/removal, uniqueness conflicts and unknown-field preservation.
`ignored_write_errors_and_caught_panics_abort_every_preceding_change` establishes
poisoning for errors and caught panics; overlapping transaction tests establish
conflict behavior and atomic unique ownership. Loop execution does not imply all
possible iteration counts are exhausted. Callback/app behavior is not library
branch coverage.

## T5: delegated errors and remaining evidence limits

The core allocation sweep and storage fault tests in models.rs were rerun, as
were index rebuild allocation/crash tests. They verify atomic primary/index and
retained-root outcomes across their tested failures; they do not inject every
callee error at each of the 196 `?` sites. Primitive kind conversion and raw
schema validation delegate to [schema.md](schema.md), value/tree and transaction
ledgers. Standard-library allocator failure for application-facing buffers,
and independent controls for every remaining E-only guard are not closed by this
audit. The two initially identified feasible gaps (Display dispatch and info
identity independence) are now covered by the focused tests above. That historical audit did not include a mutation run or compiler-instrumented
coverage result; the subsequent T6 qualification below adds scoped evidence. The source appendix deliberately retains these
limits rather than assigning 315 records a blanket MC/DC pass.

## Source record appendix

Locations are line:column and source byte span, sufficient to bind overlapping
syntax to the inventory IDs at the hashes below. P identifies only explicit
controls above; E retains the narrower execution claim.

| File:line:column [bytes] | Kind / owner | Status | Evidence |
|---|---|---|---|
| codec.rs:24:32 [769–770] | try_propagation / `encode_field` | D | T2/T5 |
| codec.rs:47:37 [1567–1568] | try_propagation / `encode_lookup` | D | T2/T5 |
| codec.rs:52:26 [1700–1701] | try_propagation / `encode_lookup` | D | T2/T5 |
| codec.rs:91:37 [3161–3162] | try_propagation / `encode` | D | T2/T5 |
| codec.rs:94:92 [3334–3335] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:95:42 [3378–3379] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:98:51 [3510–3511] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:109:29 [3822–3823] | try_propagation / `encode` | D | T2/T5 |
| codec.rs:112:67 [3970–3971] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:115:51 [4100–4101] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:124:33 [4367–4368] | try_propagation / `encode` | D | T2/T5 |
| codec.rs:125:9 [4378–4381] | for_iteration / `encode` | E | T2/T5 |
| codec.rs:126:51 [4447–4448] | try_propagation / `encode` | D | T2/T5 |
| codec.rs:126:53 [4449–4450] | try_propagation / `encode` | D | T2/T5 |
| codec.rs:131:9 [4566–4568] | if / `decode` | P | T2/T5 |
| codec.rs:134:58 [4713–4714] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:137:51 [4835–4836] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:138:9 [4846–4849] | for_iteration / `decode` | E | T2/T5 |
| codec.rs:140:68 [4963–4964] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:141:14 [4979–4980] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:154:9 [5297–5302] | match / `encode` | E | T2/T5 |
| codec.rs:156:33 [5379–5380] | try_propagation / `encode` | D | T2/T5 |
| codec.rs:160:9 [5480–5482] | if / `decode` | P | T2/T5 |
| codec.rs:163:37 [5586–5587] | try_propagation / `decode` | D | T2/T5 |
| codec.rs:170:9 [5737–5739] | if / `validate_type` | P | T2/T5 |
| codec.rs:170:12 [5740–5766] | short_circuit_or / `validate_type` | C | T2/T5 |
| codec.rs:194:9 [6467–6472] | match / `encode` | E | T2/T5 |
| codec.rs:196:39 [6562–6563] | try_propagation / `encode` | D | T2/T5 |
| codec.rs:204:9 [6808–6813] | match / `encode_field` | E | T2/T5 |
| codec.rs:206:41 [6900–6901] | try_propagation / `encode_field` | D | T2/T5 |
| codec.rs:210:9 [7002–7004] | if / `decode` | P | T2/T5 |
| codec.rs:213:44 [7121–7122] | try_propagation / `decode` | D | T2/T5 |
| lib.rs:36:9 [1071–1076] | match / `from` | E | T2/T5 |
| lib.rs:44:9 [1314–1319] | match / `fmt` | E | T2/T5 |
| lib.rs:76:9 [2576–2581] | match / `decode_error` | E | T2/T5 |
| lib.rs:88:9 [2976–2978] | if / `read` | P | T2/T5 |
| lib.rs:88:28 [2995–2996] | try_propagation / `read` | D | T2/T5 |
| lib.rs:89:13 [3021–3023] | if / `read` | P | T2/T5 |
| lib.rs:92:13 [3135–3137] | if / `read` | P | T2/T5 |
| lib.rs:92:32 [3154–3155] | try_propagation / `read` | D | T2/T5 |
| lib.rs:97:13 [3326–3328] | if / `read` | P | T2/T5 |
| lib.rs:105:37 [3647–3648] | try_propagation / `read` | D | T2/T5 |
| lib.rs:105:67 [3677–3678] | try_propagation / `read` | I | T2/T5 |
| lib.rs:106:9 [3688–3690] | if / `read` | P | T9 |
| lib.rs:112:9 [3883–3885] | if / `read` | P | T2/T5 |
| lib.rs:112:12 [3886–3936] | short_circuit_and / `read` | C | T2/T5 |
| lib.rs:112:51 [3925–3926] | try_propagation / `read` | D | T2/T5 |
| lib.rs:115:9 [4031–4036] | match / `read` | E | T2/T5 |
| lib.rs:115:44 [4066–4067] | try_propagation / `read` | D | T2/T5 |
| lib.rs:121:9 [4357–4362] | match / `set` | E | T2/T5 |
| lib.rs:121:37 [4385–4386] | try_propagation / `set` | D | T2/T5 |
| lib.rs:122:53 [4441–4442] | try_propagation / `set` | D | T2/T5 |
| lib.rs:123:41 [4485–4486] | try_propagation / `set` | D | T2/T5 |
| lib.rs:127:9 [4597–4599] | if / `validate` | P | T2/T5 |
| lib.rs:127:12 [4600–4658] | short_circuit_or / `validate` | C | T2/T5 |
| lib.rs:130:35 [4750–4751] | try_propagation / `validate` | D | T2/T5 |
| store.rs:108:9 [2497–2536] | short_circuit_and / `is_consistent` | C | T1/T4/T5 |
| store.rs:108:9 [2497–2560] | short_circuit_and / `is_consistent` | C | T1/T4/T5 |
| store.rs:130:21 [3193–3194] | try_propagation / `field` | D | T1/T4/T5 |
| store.rs:133:20 [3295–3296] | try_propagation / `uint` | D | T1/T4/T5 |
| store.rs:136:5 [3387–3389] | if / `id_key` | P | T9 |
| store.rs:143:77 [3595–3596] | try_propagation / `row_id` | D | T1/T4/T5 |
| store.rs:144:15 [3613–3614] | try_propagation / `row_id` | D | T1/T4/T5 |
| store.rs:156:43 [3859–3860] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:157:9 [3870–3872] | if / `parse` | P | T1/T4/T5 |
| store.rs:160:39 [3995–3996] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:160:41 [3997–3998] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:161:9 [4008–4011] | for_iteration / `parse` | E | T1/T4/T5 |
| store.rs:161:30 [4029–4030] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:163:29 [4086–4087] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:165:52 [4165–4166] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:166:13 [4180–4182] | if / `parse` | P | T1/T4/T5 |
| store.rs:166:16 [4183–4214] | short_circuit_or / `parse` | C | T1/T4/T5 |
| store.rs:172:46 [4393–4394] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:173:43 [4438–4439] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:174:46 [4486–4487] | try_propagation / `parse` | D | T1/T4/T5 |
| store.rs:177:9 [4528–4530] | if / `parse` | P | T1/T4/T5 |
| store.rs:177:12 [4531–4571] | short_circuit_or / `parse` | C | T1/T4/T5 |
| store.rs:177:12 [4531–4593] | short_circuit_or / `parse` | C | T1/T4/T5 |
| store.rs:183:35 [4766–4767] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:184:9 [4777–4780] | for_iteration / `encode` | E | T1/T4/T5 |
| store.rs:185:56 [4857–4858] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:185:58 [4859–4860] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:188:19 [4904–4905] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:189:60 [4965–4966] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:189:62 [4967–4968] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:190:57 [5025–5026] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:190:59 [5027–5028] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:191:60 [5088–5089] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:191:62 [5090–5091] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:192:41 [5132–5133] | try_propagation / `encode` | D | T1/T4/T5 |
| store.rs:197:41 [5234–5235] | try_propagation / `memory` | D | T1/T4/T5 |
| store.rs:204:72 [5473–5474] | try_propagation / `create_on` | D | T1/T4/T5 |
| store.rs:210:63 [5673–5674] | try_propagation / `open_on` | D | T1/T4/T5 |
| store.rs:214:72 [5896–5897] | try_propagation / `create_file` | D | T1/T4/T5 |
| store.rs:218:73 [6118–6119] | try_propagation / `open_file` | D | T1/T4/T5 |
| store.rs:224:59 [6363–6364] | try_propagation / `compact_file` | D | T1/T4/T5 |
| store.rs:230:9 [6659–6661] | if / `initialize` | P | T6 |
| store.rs:233:9 [6759–6762] | for_iteration / `initialize` | E | T1/T4/T5 |
| store.rs:234:13 [6795–6797] | if / `initialize` | I | T1/T4/T5 |
| store.rs:234:38 [6820–6821] | try_propagation / `initialize` | D | T1/T4/T5 |
| store.rs:239:60 [7012–7013] | try_propagation / `initialize` | D | T1/T4/T5 |
| store.rs:239:62 [7014–7015] | try_propagation / `initialize` | D | T1/T4/T5 |
| store.rs:240:20 [7036–7037] | try_propagation / `initialize` | D | T1/T4/T5 |
| store.rs:247:13 [7201–7203] | if / `from_database` | P | T6 |
| store.rs:255:13 [7453–7456] | for_iteration / `from_database` | E | T1/T4/T5 |
| store.rs:256:46 [7516–7517] | try_propagation / `from_database` | D | T1/T4/T5 |
| store.rs:269:43 [7920–7921] | try_propagation / `compact_to` | D | T1/T4/T5 |
| store.rs:272:56 [8030–8031] | try_propagation / `checkpoint` | D | T1/T4/T5 |
| store.rs:275:34 [8120–8121] | try_propagation / `history` | D | T1/T4/T5 |
| store.rs:278:40 [8230–8231] | try_propagation / `release_checkpoint` | D | T1/T4/T5 |
| store.rs:293:52 [8682–8683] | try_propagation / `read_retained` | D | T1/T4/T5 |
| store.rs:303:39 [9039–9040] | try_propagation / `write` | D | T1/T4/T5 |
| store.rs:304:9 [9050–9052] | if / `write` | P | T10 |
| store.rs:307:26 [9165–9166] | try_propagation / `write` | D | T1/T4/T5 |
| store.rs:313:9 [9427–9429] | if / `table` | P | T1/T4/T5 |
| store.rs:313:12 [9430–9465] | short_circuit_or / `table` | C | T1/T4/T5 |
| store.rs:317:42 [9600–9601] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:320:41 [9733–9734] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:323:9 [9828–9831] | for_iteration / `table` | E | T1/T4/T5 |
| store.rs:324:42 [9912–9913] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:324:73 [9943–9944] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:325:13 [9958–9960] | if / `table` | P | T1/T4/T5 |
| store.rs:325:16 [9961–9990] | short_circuit_and / `table` | C | T1/T4/T5 |
| store.rs:325:16 [9961–10038] | short_circuit_or / `table` | C | T1/T4/T5 |
| store.rs:332:49 [10233–10234] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:334:25 [10290–10291] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:335:9 [10301–10303] | if / `table` | E | T1/T4/T5 |
| store.rs:335:12 [10304–10307] | pattern_test / `table` | C | T1/T4/T5 |
| store.rs:336:13 [10346–10348] | if / `table` | E | T1/T4/T5 |
| store.rs:339:13 [10454–10456] | if / `table` | P | T1/T4/T5 |
| store.rs:343:13 [10607–10612] | match / `table` | E | T1/T4/T5 |
| store.rs:345:21 [10738–10740] | if / `table` | P | T1/T4/T5 |
| store.rs:348:21 [10919–10921] | if / `table` | P | T1/T4/T5 |
| store.rs:349:70 [11021–11022] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:358:24 [11390–11416] | match_guard / `table` | P | T1/T4/T5 |
| store.rs:363:58 [11598–11599] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:364:23 [11623–11628] | match / `table` | E | T1/T4/T5 |
| store.rs:366:48 [11730–11731] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:376:26 [12079–12080] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:377:55 [12136–12137] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:378:20 [12158–12159] | try_propagation / `table` | D | T1/T4/T5 |
| store.rs:386:67 [12459–12460] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:387:43 [12504–12505] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:388:50 [12556–12557] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:391:60 [12671–12672] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:392:17 [12690–12693] | for_iteration / `restore_table` | E | T1/T4/T5 |
| store.rs:392:57 [12730–12731] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:393:61 [12794–12795] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:393:63 [12796–12797] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:395:17 [12833–12836] | for_iteration / `restore_table` | E | T1/T4/T5 |
| store.rs:396:58 [12917–12918] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:396:86 [12945–12946] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:396:88 [12947–12948] | try_propagation / `restore_table` | D | T1/T4/T5 |
| store.rs:404:5 [13121–13123] | if / `info` | P | T9 |
| store.rs:408:63 [13352–13353] | try_propagation / `info` | D | T1/T4/T5 |
| store.rs:409:59 [13412–13413] | try_propagation / `info` | D | T1/T4/T5 |
| store.rs:410:6 [13420–13421] | try_propagation / `info` | D | T1/T4/T5 |
| store.rs:411:5 [13427–13429] | if / `info` | P | T1/T4/T5 |
| store.rs:411:8 [13430–13500] | short_circuit_or / `info` | C | T1/T4/T5 |
| store.rs:422:40 [13729–13730] | try_propagation / `decode` | D | T1/T4/T5 |
| store.rs:423:5 [13736–13738] | if / `decode` | P | T11 |
| store.rs:426:90 [13905–13906] | try_propagation / `decode` | D | T1/T4/T5 |
| store.rs:427:41 [13948–13949] | try_propagation / `decode` | D | T1/T4/T5 |
| store.rs:441:29 [14195–14196] | try_propagation / `get` | D | T3/T5 |
| store.rs:442:47 [14244–14245] | try_propagation / `get` | D | T3/T5 |
| store.rs:442:49 [14246–14247] | try_propagation / `get` | D | T3/T5 |
| store.rs:451:29 [14462–14463] | try_propagation / `list` | D | T3/T5 |
| store.rs:452:48 [14512–14513] | try_propagation / `list` | D | T3/T5 |
| store.rs:454:34 [14578–14579] | try_propagation / `list` | D | T3/T5 |
| store.rs:455:5 [14585–14588] | for_iteration / `list` | E | T3/T5 |
| store.rs:456:50 [14661–14662] | try_propagation / `list` | D | T3/T5 |
| store.rs:456:59 [14670–14671] | try_propagation / `list` | D | T3/T5 |
| store.rs:463:26 [14867–14872] | match / `index_key` | E | T3/T5 |
| store.rs:466:40 [15025–15026] | try_propagation / `index_key` | D | T3/T5 |
| store.rs:467:41 [15091–15092] | try_propagation / `index_key` | D | T3/T5 |
| store.rs:468:38 [15154–15155] | try_propagation / `index_key` | D | T3/T5 |
| store.rs:469:40 [15229–15230] | try_propagation / `index_key` | D | T3/T5 |
| store.rs:472:40 [15338–15339] | try_propagation / `index_key` | D | T3/T5 |
| store.rs:485:5 [15677–15679] | if / `record_keys` | P | T11 |
| store.rs:488:90 [15846–15847] | try_propagation / `record_keys` | D | T3/T5 |
| store.rs:489:25 [15873–15874] | try_propagation / `record_keys` | D | T3/T5 |
| store.rs:491:43 [15949–15950] | try_propagation / `record_keys` | D | T3/T5 |
| store.rs:492:5 [15956–15959] | for_iteration / `record_keys` | E | T3/T5 |
| store.rs:493:9 [15990–15992] | if / `record_keys` | E | T3/T5 |
| store.rs:493:12 [15993–15996] | pattern_test / `record_keys` | C | T3/T5 |
| store.rs:493:12 [15993–16096] | short_circuit_and / `record_keys` | C | T3/T5 |
| store.rs:493:44 [16025–16026] | try_propagation / `record_keys` | D | T3/T5 |
| store.rs:494:16 [16042–16045] | pattern_test / `record_keys` | C | T3/T5 |
| store.rs:494:69 [16095–16096] | try_propagation / `record_keys` | D | T3/T5 |
| store.rs:511:41 [16440–16441] | try_propagation / `index_state` | D | T3/T5 |
| store.rs:512:51 [16493–16494] | try_propagation / `index_state` | D | T3/T5 |
| store.rs:514:5 [16535–16538] | for_iteration / `index_state` | E | T3/T5 |
| store.rs:515:29 [16594–16595] | try_propagation / `index_state` | D | T3/T5 |
| store.rs:516:52 [16648–16649] | try_propagation / `index_state` | D | T3/T5 |
| store.rs:517:9 [16659–16662] | for_iteration / `index_state` | E | T3/T5 |
| store.rs:517:68 [16718–16719] | try_propagation / `index_state` | D | T3/T5 |
| store.rs:520:72 [16846–16847] | try_propagation / `index_state` | D | T3/T5 |
| store.rs:525:5 [16960–16963] | for_iteration / `index_state` | E | T3/T5 |
| store.rs:526:9 [17002–17004] | if / `index_state` | P | T3/T5 |
| store.rs:532:66 [17326–17327] | try_propagation / `index_state` | D | T3/T5 |
| store.rs:541:5 [17648–17651] | for_iteration / `index_state` | E | T3/T5 |
| store.rs:542:9 [17685–17690] | match / `index_state` | E | T3/T5 |
| store.rs:544:26 [17822–17864] | match_guard / `index_state` | P | T3/T5 |
| store.rs:569:41 [18475–18476] | try_propagation / `find` | D | T3/T5 |
| store.rs:570:5 [18482–18484] | if / `find` | P | T3/T5 |
| store.rs:573:94 [18669–18670] | try_propagation / `find` | D | T3/T5 |
| store.rs:575:25 [18723–18724] | try_propagation / `find` | D | T3/T5 |
| store.rs:576:44 [18768–18769] | try_propagation / `find` | D | T3/T5 |
| store.rs:577:49 [18819–18820] | try_propagation / `find` | D | T3/T5 |
| store.rs:578:5 [18826–18828] | if / `find` | P | T3/T5 |
| store.rs:578:8 [18829–18916] | short_circuit_or / `find` | C | T3/T5 |
| store.rs:578:34 [18855–18915] | short_circuit_and / `find` | C | T3/T5 |
| store.rs:584:5 [19070–19073] | let_else / `find` | P | T3/T5 |
| store.rs:584:65 [19130–19131] | try_propagation / `find` | D | T3/T5 |
| store.rs:587:5 [19175–19178] | let_else / `find` | P | T3/T5 |
| store.rs:587:68 [19238–19239] | try_propagation / `find` | D | T3/T5 |
| store.rs:590:63 [19341–19342] | try_propagation / `find` | D | T3/T5 |
| store.rs:591:42 [19385–19386] | try_propagation / `find` | D | T3/T5 |
| store.rs:591:75 [19418–19419] | try_propagation / `find` | D | T3/T5 |
| store.rs:592:90 [19510–19511] | try_propagation / `find` | D | T3/T5 |
| store.rs:595:5 [19674–19676] | if / `find` | P | T3/T5 |
| store.rs:595:36 [19705–19706] | try_propagation / `find` | D | T3/T5 |
| store.rs:599:44 [19838–19839] | try_propagation / `find` | D | T3/T5 |
| store.rs:600:41 [19880–19881] | try_propagation / `find` | D | T3/T5 |
| store.rs:601:5 [19887–19889] | if / `find` | P | T3/T5 |
| store.rs:601:51 [19933–19934] | try_propagation / `find` | D | T3/T5 |
| store.rs:609:62 [20265–20266] | try_propagation / `check_indexes` | D | T3/T5 |
| store.rs:628:9 [20970–20972] | if / `checked` | P | T6 |
| store.rs:632:9 [21149–21154] | match / `checked` | E | T3/T4/T5 |
| store.rs:638:17 [21306–21308] | if / `checked` | E | T3/T4/T5 |
| store.rs:646:67 [21626–21627] | try_propagation / `check_indexes` | D | T3/T4/T5 |
| store.rs:652:63 [21964–21965] | try_propagation / `rebuild_indexes` | D | T3/T4/T5 |
| store.rs:654:13 [22034–22037] | for_iteration / `rebuild_indexes` | E | T3/T4/T5 |
| store.rs:655:17 [22082–22084] | if / `rebuild_indexes` | P | T3/T4/T5 |
| store.rs:660:46 [22289–22290] | try_propagation / `rebuild_indexes` | D | T3/T4/T5 |
| store.rs:663:13 [22336–22339] | for_iteration / `rebuild_indexes` | E | T3/T4/T5 |
| store.rs:669:17 [22629–22631] | if / `rebuild_indexes` | P | T3/T4/T5 |
| store.rs:670:69 [22724–22725] | try_propagation / `rebuild_indexes` | D | T3/T4/T5 |
| store.rs:670:71 [22726–22727] | try_propagation / `rebuild_indexes` | D | T3/T4/T5 |
| store.rs:692:56 [23573–23574] | try_propagation / `insert` | D | T3/T4/T5 |
| store.rs:693:13 [23588–23590] | if / `insert` | P | T11 |
| store.rs:696:13 [23702–23704] | if / `insert` | P | T6 |
| store.rs:696:51 [23740–23741] | try_propagation / `insert` | D | T3/T4/T5 |
| store.rs:696:53 [23742–23743] | try_propagation / `insert` | D | T3/T4/T5 |
| store.rs:704:14 [24023–24024] | try_propagation / `insert` | D | T3/T4/T5 |
| store.rs:705:82 [24107–24108] | try_propagation / `insert` | D | T3/T4/T5 |
| store.rs:705:84 [24109–24110] | try_propagation / `insert` | D | T3/T4/T5 |
| store.rs:716:56 [24421–24422] | try_propagation / `update` | D | T3/T4/T5 |
| store.rs:717:61 [24484–24485] | try_propagation / `update` | D | T3/T4/T5 |
| store.rs:718:63 [24548–24549] | try_propagation / `update` | D | T3/T4/T5 |
| store.rs:719:13 [24563–24565] | if / `update` | E | T3/T4/T5 |
| store.rs:722:35 [24697–24698] | try_propagation / `update` | D | T3/T4/T5 |
| store.rs:723:80 [24779–24780] | try_propagation / `update` | D | T3/T4/T5 |
| store.rs:728:56 [24965–24966] | try_propagation / `delete` | D | T3/T4/T5 |
| store.rs:740:56 [25434–25435] | try_propagation / `migrate` | D | T3/T4/T5 |
| store.rs:741:13 [25449–25451] | if / `migrate` | P | T7 |
| store.rs:749:14 [25731–25732] | try_propagation / `migrate` | D | T3/T4/T5 |
| store.rs:751:13 [25777–25780] | for_iteration / `migrate` | E | T3/T4/T5 |
| store.rs:751:57 [25821–25822] | try_propagation / `migrate` | D | T3/T4/T5 |
| store.rs:756:34 [25997–25998] | try_propagation / `migrate` | D | T3/T4/T5 |
| store.rs:758:18 [26059–26060] | try_propagation / `migrate` | D | T3/T4/T5 |
| store.rs:775:13 [26709–26711] | if / `migrate_ids` | P | T3/T4/T5 |
| store.rs:775:16 [26712–26780] | short_circuit_or / `migrate_ids` | C | T3/T4/T5 |
| store.rs:779:44 [26946–26947] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:782:13 [27040–27042] | if / `migrate_ids` | P | T3/T4/T5 |
| store.rs:782:16 [27043–27121] | short_circuit_or / `migrate_ids` | C | T3/T4/T5 |
| store.rs:785:56 [27239–27240] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:786:13 [27254–27256] | if / `migrate_ids` | P | T7 |
| store.rs:794:14 [27536–27537] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:796:13 [27582–27585] | for_iteration / `migrate_ids` | E | T3/T4/T5 |
| store.rs:798:56 [27682–27683] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:798:58 [27684–27685] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:799:71 [27756–27757] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:806:18 [28011–28012] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:819:71 [28388–28389] | try_propagation / `migrate_record` | D | T3/T4/T5 |
| store.rs:820:9 [28399–28401] | if / `migrate_record` | P | T7 |
| store.rs:823:9 [28500–28502] | if / `migrate_record` | P | T8 |
| store.rs:826:42 [28614–28615] | try_propagation / `migrate_record` | D | T3/T4/T5 |
| store.rs:829:57 [28736–28737] | try_propagation / `migrate_record` | D | T3/T4/T5 |
| store.rs:830:10 [28748–28749] | try_propagation / `migrate_record` | D | T3/T4/T5 |
| store.rs:831:48 [28798–28799] | try_propagation / `migrate_record` | D | T3/T4/T5 |
| store.rs:844:29 [29170–29171] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:847:38 [29249–29250] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:849:25 [29312–29313] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:853:25 [29435–29436] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:855:51 [29521–29522] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:857:9 [29586–29589] | for_iteration / `put_raw` | E | T3/T4/T5 |
| store.rs:858:13 [29626–29628] | if / `put_raw` | P | T3/T4/T5 |
| store.rs:858:41 [29654–29655] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:862:9 [29786–29789] | for_iteration / `put_raw` | E | T3/T4/T5 |
| store.rs:863:13 [29830–29832] | if / `put_raw` | E | T3/T4/T5 |
| store.rs:863:16 [29833–29836] | pattern_test / `put_raw` | C | T3/T4/T5 |
| store.rs:863:62 [29879–29880] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:864:87 [29969–29970] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:865:17 [29988–29990] | if / `put_raw` | P | T3/T4/T5 |
| store.rs:868:17 [30106–30108] | if / `put_raw` | P | T3/T4/T5 |
| store.rs:873:9 [30259–30262] | for_iteration / `put_raw` | E | T3/T4/T5 |
| store.rs:874:13 [30299–30301] | if / `put_raw` | P | T3/T4/T5 |
| store.rs:875:45 [30388–30389] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:878:9 [30423–30426] | for_iteration / `put_raw` | E | T3/T4/T5 |
| store.rs:879:13 [30463–30465] | if / `put_raw` | P | T3/T4/T5 |
| store.rs:880:70 [30577–30578] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:880:72 [30579–30580] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:883:57 [30662–30663] | try_propagation / `put_raw` | D | T3/T4/T5 |
| store.rs:887:29 [30813–30814] | try_propagation / `remove_raw` | D | T3/T4/T5 |
| store.rs:888:9 [30824–30827] | let_else / `remove_raw` | P | T6 |
| store.rs:888:57 [30872–30873] | try_propagation / `remove_raw` | D | T3/T4/T5 |
| store.rs:891:9 [30930–30933] | for_iteration / `remove_raw` | E | T3/T4/T5 |
| store.rs:891:71 [30992–30993] | try_propagation / `remove_raw` | D | T3/T4/T5 |
| store.rs:891:73 [30994–30995] | try_propagation / `remove_raw` | D | T3/T4/T5 |
| store.rs:893:13 [31068–31070] | if / `remove_raw` | P | T6 |
| store.rs:893:42 [31097–31098] | try_propagation / `remove_raw` | D | T3/T4/T5 |
| store.rs:896:42 [31252–31253] | try_propagation / `remove_raw` | D | T3/T4/T5 |
| store.rs:898:44 [31308–31309] | try_propagation / `remove_raw` | D | T3/T4/T5 |

## Results and source binding

Historical disposition counts: C 21, D 195, E 53, I 2, P 44. T7 changes three E guard records to P; T8 adds the current-version skip pair; T9 adds ownership and stored-kind pairs and directly qualifies primary-ID admission; T10 qualifies outer write-poison admission; T11 qualifies stored-record type and insert-version admission. Current counts are C 21, D 195, E 43, I 2, P 54. All 315 source records are listed; the production source and denominator are unchanged.

Debug and release each passed 26 existing model tests, two process-evolution
tests, and six doctests (one documentation example ignored). All eleven new directed
tests passed debug and release. Warnings-denied Clippy passed for all typed tests.
The initial full runs contained the first five new tests; final focused debug and
release runs exercised all eleven. The integration follow-up adds a twelfth
directed test, `generated_model_patch_rejects_wrong_type_before_touching_the_record`:
it directly exercises the generated patch guard rather than Store admission,
preserves unknown fields, and passes focused debug/release and strict Clippy.
See the generated-code mapping in [macros](macros.md). No production change was needed.

```sh
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/typed-decisions cargo test --offline -p weekend-db-typed --all-features
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/typed-decisions cargo test --offline -p weekend-db-typed --all-features --release
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/typed-decisions cargo test --offline -p weekend-db-typed --all-features --test runtime_decisions
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/typed-decisions cargo test --offline -p weekend-db-typed --all-features --release --test runtime_decisions
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/typed-decisions cargo clippy --offline -p weekend-db-typed --all-features --tests -- -D warnings
```

| File | SHA-256 |
|---|---|
| `crates/weekend-db-typed/src/codec.rs` | `7725f08e3d1df5180cf839899c929e9d6219c92e7352d17e2758ef3f1fe127aa` |
| `crates/weekend-db-typed/src/lib.rs` | `e6a706a0b017b605cdde1d660f5e2c36504a61239500acb8b18f3d1fca98345d` |
| `crates/weekend-db-typed/src/store.rs` | `2157407964fc4ff889737efb29f21270fe6ae2985db1288aac5d2dc740c1f7fa` |
| `crates/weekend-db-typed/tests/runtime_decisions.rs` | `81adbbd3942b414f9ad080348e866ac6bc94031e2ca0ffd01528069f2f0fcba5` |
| `crates/weekend-db-typed/tests/catalog_admission.rs` | `d309488423e6705daefc196e657419dcb51ce7497dd0858263d5d0a297ff1088` |
| `crates/weekend-db-typed/tests/models.rs` | `6802b09817f9524d222735080a1c01bb927bfb8619ccdf0d94dc7db1dee3b8d8` |
| `crates/weekend-db-typed/tests/process_evolution.rs` | `19a9580ae28deaf01f1161e40b9afbaf958dfc610170d01b1a020f1f0a1640a2` |


## T6: directly observed terminal, primary, delete and admission guards

The six [source outcome pairs](../typed-decision-gaps-21e5d26.md) add public-API
controls at unchanged production store.rs lines 230, 247, 628, 696, 888 and 893.
The earlier appendix mapped the delete guards to related CRUD tests; these new
controls execute the delete-specific outcomes directly. Four E-only records
now have P dispositions; no production source, span or AST denominator changed.

- A clean write invokes a copied follow-up closure; the same closure is refused
  after a caught callback panic. Its side effect stays absent, earlier staged
  primary/index work rolls back, current revision and retained views stay intact,
  and a fresh Write remains usable. The same checked() instantiation records
  true/false 1/1, rather than pooling unrelated generic instantiations.
- Inserting an existing primary ID with a different unique value returns
  AlreadyExists; ignoring that error still discards earlier work. Original
  ownership survives and no replacement/staged index entry appears.
- Deleting an absent primary returns false without changing primary/index rows;
  a present correctly indexed row supplies the successful deletion control.
- Missing, wrong-owner and malformed unique claims each return IndexInconsistent
  on deletion. Earlier draft work is discarded, current corrupt rows remain
  unchanged, and the healthy retained primary/index view still resolves both rows.
- Initialization accepts a fresh revision-zero database and rejects nonfresh
  input without changing its durable image. Removing that early guard is caught
  even when the later collection-ID check returns the same error after mutation.
- Typed open requires the exact unsigned format marker 1. Absent catalog/marker,
  unsigned 0/2 and wrong-kind bytes return NotInitialized; every case preserves
  its complete simulated durable image.

Stable macOS and isolated Docker Linux each passed 16 runtime tests plus two
catalog admission tests in debug/release. Strict targeted Clippy passed. The
matching dated-nightly, same-object condition reports are warning-free and
record both outcomes for each selected source guard; their precise counts and
source/feature boundaries are in the linked report. These are direct predicate
pairs, not compiler-measured MC/DC or a validated global coverage percentage.

Six isolated production mutations were detected after positive baselines:
failed-Write admission removal, existing-primary admission removal, false-to-true
absent-delete result, delete ownership-guard removal, nonfresh initialization
admission removal, and format-marker comparison inversion. Each leaves guard
bodies otherwise intact. They do not mutate pointers or ownership machinery.
Reproduce with a fresh output directory:

```sh
CARGO_INCREMENTAL=0 mise exec rust@1.99.1 python@3.14.8 -- python tools/mutations.py --case typed- --output target/new-typed-mutations
```

The four runtime mutations are bound in
`target/typed-guard-mutation-proof-final/evidence.json`; the two admission cases
are in `target/typed-catalog-mutation-proof/evidence.json`. Standard allocator,
I/O propagation and remaining E/generated-code obligations remain unchanged.


## T7: emitted field validation and migration version admission

On 2026-11-16, three new tests in `tests/runtime_decisions.rs` qualify these
specific generated-code and version checks. The isolated controls use commit
`bbcf68dcfbb1246e24ed2a2cb80b1d12da12c21c` plus only that test-file overlay.
Production typed runtime and Model expansion code are unchanged. The complete
runtime-decision target now passes **19 tests** in both debug and release on
macOS arm64 with Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1); targeted all-feature
Clippy with warnings denied also passes.

### Generated nested-nullability validation

`assurance_generated_schema_rejects_nested_nullable_before_registration` calls
actual derived `Model::schema` methods and `Store::table`, rather than invoking
`FieldValue::validate_type` directly. An `Option<u64>` field produces an optional,
nullable unsigned descriptor and a usable table. An `Option<Option<u64>>` field
returns exactly `InvalidField { id: 0, reason: "nested nullable fields are ambiguous" }`
from both entry points. Registration rejection leaves the complete durable
image unchanged, including the accepted table's current and retained null row.

The critical emitted call is `model.rs:185`, executed by the generated schema
body at lines 198–200. The earlier flags test still supplies independent
optional/nullable predicate controls inside `Option::validate_type`; it cannot
detect omission of the derive's call. The new controls use two different fixed
model types and generated schema functions. They establish propagation and
omission sensitivity, **not** a same-generated-function MC/DC pair. No dynamic
validator state or altered public type invariant is introduced.

### Old-model catalog admission

`assurance_old_model_migrations_reject_newer_catalog_before_callbacks` checks
both `Write::migrate` (`store.rs:741`) and `Write::migrate_ids` (`store.rs:786`).
The same v1 model handle is accepted while the catalog is v1 and refused with
exact `UnsupportedVersion` after v2 registration, with its rows still v1. Each
API's accepted and rejected runs reuse the same lexical callsite and callback
type; evidence is scoped separately to the two API instantiations. Both controls
invoke zero conversion callbacks because accepted v1 rows already match v1.
The accepted control commits a preceding edit. Catching the rejected call still
leaves the Write terminal: its enclosing closure returns `Closed`, the earlier
edit is absent, and every catalog/schema/index/primary row, durable byte and
retained view remains identical.

Keeping rows at v1 matters: omitting either catalog guard then returns `Ok(0)`.
A later newer-row rejection cannot mask that omission. Existing
`tools/compatibility-typed-v1.rs::legacy_edit` already tests old-model insert
rejection; the new pairs address the separate migration entry points.

### Forward-row defense after raw writes

`assurance_migration_rejects_forward_rows_before_conversion` keeps the table
catalog and target model at v2, then supplies row versions 1, 2 and 3. It exercises
both migration APIs at unchanged callsites/callback types within each API.
A v1 row reaches the callback once and returns its deliberate `InvalidId`
sentinel; v2 is accepted as `Ok(0)` without conversion; v3 returns exact
`UnsupportedVersion` at `store.rs:820` before the callback. The sentinel makes
omission observable even if subsequent conversion or schema work could reject.

The v3 descriptor is published without advancing the catalog, and a public raw
write installs the newer envelope. This deliberately bypasses typed ownership;
normal typed-owned rows remain at or below their catalog version. The control
qualifies a defensive corruption/version check, not a supported downgrade or
an arbitrary application converter. Both error outcomes discard a preceding
edit in a separate table, preserve the complete current durable image and all
current/retained catalog, schema, index and primary rows, and leave lookups and
index reports consistent. The current-version accepted control commits that
separate edit while preserving the retained checkpoint.

### Isolated mutation evidence and limits

Four separate omissions each compile and fail the intended runtime assertion
in **both debug and release**, following passing baselines in each profile:

| Mutation | Observed assertion mismatch |
|---|---|
| Omit Model's emitted `validate_type` call | Generated schema returns no error instead of exact nested-nullability `InvalidField` |
| Disable only `migrate`'s catalog-version guard | `Ok(0)` instead of `UnsupportedVersion` |
| Disable only `migrate_ids`' catalog-version guard | `Ok(0)` instead of `UnsupportedVersion` |
| Disable only `migrate_record`'s forward-row guard | Callback sentinel `InvalidId` instead of `UnsupportedVersion` |

Guard mutations retain predicate evaluation and rejection bodies, adding
`&& false` only to the selected guard. The emitted-call mutation replaces that
one quote fragment with an empty fragment. Every mutation exits 101 from the
named test assertion; compile failures do not count as detected mutations.
No omission was masked by a later error in these fixtures.

All production mutations occur in separate source copies reconstructed from
the archived base plus the exact test overlay. Each case has its own Cargo
target directory, and mutation builds disable incremental compilation. The
omissions were never applied to shared production source. An initial attempt sharing one
target across different source roots reused a previous mutant in a later
positive baseline; that attempt is retained as **rejected** and contributes no
result. All four cases were repeated with distinct targets and passing positive
baselines before their mutations.

Local evidence is `/private/tmp/weekend-typed-assurance-mrzb2hdc/`: the base
archive, exact test source/patch, compiler identity, primary debug/release/Clippy
logs, per-case production patches and command logs, and `mutation-results.json`.
`evidence.json` SHA-256 is
`07a7713a2de0ae361509194324895f9c7777f2a5d29219420307ad3f4d3d9e6f`.
The verified host archive `/private/tmp/weekend-typed-assurance-mrzb2hdc.tar.gz`
retains that report and all 50 indexed payloads, including the rejected first
attempt. Its SHA-256 is
`6b7ba779a823e902b369770f564ef5a3706c03d54da5da7eb56e426ac5f2755e`.
The same archive is retained as
`target/archived-proof/typed-generated-migration-bbcf68d.tar.gz`.
Root review restored it and independently rehashed all 50 payloads and checked
all 16 positive-baseline/mutation outcomes against the retained runtime logs.
Source copies and build outputs remain separately available in the proof directory;
reconstruction uses the retained base tar, exact test overlay and per-case patch.
These are finite functional/predicate controls and mutation sensitivity results.
No compiler MC/DC measurement, whole-module completion, universal generated-code
coverage, native Linux/Windows execution or new runtime functionality is claimed.

| Input | SHA-256 |
|---|---|
| Final `tests/runtime_decisions.rs` | `f71b8e022802cc96ec819f7848e684ee3b616ba7ed12912ec26a1d4a7bd047f5` |
| Unchanged `src/store.rs` | `2157407964fc4ff889737efb29f21270fe6ae2985db1288aac5d2dc740c1f7fa` |
| Unchanged macro `src/model.rs` | `fa796e2ebcfbf9acfea6354b9b0ea7533b948c4a2252b5163e8d09b9a6e5c5f9` |

Reproduce the compatible baseline with the existing shared target directory:

```sh
cargo +1.99.1 test --offline --locked --target-dir target/xtask \
  -p weekend-db-typed --all-features --test runtime_decisions
cargo +1.99.1 test --offline --locked --target-dir target/xtask \
  -p weekend-db-typed --all-features --test runtime_decisions --release
cargo +1.99.1 clippy --offline --locked --target-dir target/xtask \
  -p weekend-db-typed --all-features --test runtime_decisions -- -D warnings
```


## T8: current-version migration skip

The existing
`assurance_migration_rejects_forward_rows_before_conversion` also qualifies
`store.rs:823`, `doc.version() == T::VERSION`, without new test code. Its target
model/catalog remain v2. Row versions 1 and 2 both make the preceding
`doc.version() > T::VERSION` guard false: v1 reaches the converter once and
returns its exact `InvalidId` sentinel; v2 returns `Ok(0)` without invoking it.
The accepted v2 control commits the preceding separate-table edit; the v1
callback-error control discards that edit and preserves the complete current
image. Both controls preserve all retained catalog/schema/index/primary rows.
Thus the current-version predicate's skip and continue outcomes are distinguished
without changing the preceding forward-version predicate.

Within each of `migrate::<RowV2>` and `migrate_ids::<RowV2>`, the versions loop
reuses the same lexical API callsite and callback type. The two API/callback
instantiations are separate evidence; their results are not pooled into a
compiler MC/DC count. The newer-row v3 rejection in T7 retains its different,
explicit raw-corruption scope.

On 2026-11-16, the test and production code were frozen directly from clean
commit `32fb0d68b43735fbe4f551d25d64445f487f7e0a` with `git archive`. No working-tree
source or test overlay was used. The single selected integration test passes in
debug and release with all features, offline/locked, Rust 1.99.1 (`58cf8f9c3`,
LLVM 23.1.1) on macOS arm64. These passing baselines execute both migration APIs
and all three row versions.

In only that isolated source copy, the selected guard becomes
`if (doc.version() == T::VERSION) && false`, preserving predicate evaluation and
the existing skip body. Both debug and release compile, then fail the intended
runtime assertion: the v2 row calls the converter and returns `Err(InvalidId)`
instead of `Ok(0)`. There is no masking by the earlier forward-version guard or
a later schema error. The test stops first in its all-row `migrate::<RowV2>`
case; this mutation result does **not** claim separate negative execution of the
selected-ID monomorphization. Its positive predicate/behavior controls still ran
in both unmodified baselines.

The copy and fresh target are owned by
`/private/tmp/weekend-typed-current-skip-w2tyxa5t/`. All four runs disable
incremental compilation with `CARGO_INCREMENTAL=0`. The omission was never
applied to shared production source. The compact verified archive
`/private/tmp/weekend-typed-current-skip-w2tyxa5t.tar.gz` contains the exact clean
source tar, source/compiler identities, one-line mutation patch, commands and
four logs. Its SHA-256 is
`d8e52ac5ae98eb117a866eb72b38a1cb3fef58adeebadeb5b5042793fcd544ca`;
the same archive is retained at
`target/archived-proof/typed-current-skip-32fb0d6.tar.gz`.
Root review restored all eight indexed payloads and checked the four
baseline/mutation logs independently.
`evidence.json` SHA-256 is
`8df90a6ff539a4a40967ad729b0670c7fb17092cb44a5ab150811bdd2e488d53`.
The unchanged test SHA-256 remains
`f71b8e022802cc96ec819f7848e684ee3b616ba7ed12912ec26a1d4a7bd047f5`,
and production `store.rs` remains
`2157407964fc4ff889737efb29f21270fe6ae2985db1288aac5d2dc740c1f7fa`.

This closes the named feasible skip/continue pair and shows one isolated guard
omission is observable. It adds no test boilerplate or runtime functionality,
compiler-instrumented MC/DC result, whole-module completion claim, or execution
on other platforms/compilers.


## T9: typed ownership, primary IDs and stored field kinds

Three finite tests add direct controls for `store.rs:404`, `store.rs:136` and
`lib.rs:106`. They reuse the existing Row model and snapshot-row comparison,
plus one small seeded-store helper. Isolated runs use clean base commit
`703948c4848b9fd802875d15a9a2dc0c4d465063` plus only the exact
`tests/runtime_decisions.rs` overlay. Typed runtime and macro production inputs
remain unchanged. Each new test passes independently in debug and release on
macOS arm64 with Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), offline/locked with all
features. Targeted Clippy with warnings denied, formatting and diff checks pass.
T7/T8 retain their separately recorded source and execution scope.

### Foreign table ownership without catalog masking

`assurance_foreign_table_rejects_before_an_absent_catalog_key_can_mask_ownership`
compares a valid local Row table with a valid table from another live Store.
The foreign table's catalog key is absent from the receiving Store. Both cases
use unchanged `Read::get<Row>` and `Write::get<Row>` callsites. Correct ownership
returns the local row; foreign ownership returns exact `Engine(ForeignStore)`.
A caught write-side rejection leaves the outer Write terminal (`Closed`) and
discards its preceding unique-field edit. The accepted control commits that
edit. Source-store bytes/current/retained rows and target retained rows remain
identical in both cases; rejected target writes preserve its full durable image
and current catalog/schema/index/primary rows. Current lookups and index checks
remain consistent.

The earlier compacted-copy test has the same table key in the new catalog.
Suppressing the typed ownership check there can still return `ForeignStore`
from a later core collection check, masking the omission. Here the first
foreign lookup instead becomes `Engine(NotFound)`, exposing the missing early
ownership rejection. The isolated mutation fails first in the snapshot-backed
Read call, before the later Write check. Its negative execution is therefore
scoped to `info<Row, Snapshot>`; baseline predicate/behavior controls also cover
the separate transaction-backed Write instantiation. No per-instantiation
compiler MC/DC count is inferred.

### Direct primary-ID-zero admission

`assurance_zero_primary_id_rejects_lookup_and_insert_without_partial_writes`
compares valid nonzero lookup/insertion IDs with zero through fixed API
callsites. The nonzero insert commits along with a preceding staged edit. Zero
returns exact `InvalidId`, poisons the draft even when caught, and makes its
outer closure return `Closed`; neither the earlier edit, zero primary nor its
unique-index claim is published. Direct zero lookup also returns `InvalidId`,
while the valid lookup resolves its expected row. Rejected current bytes and
all current/retained system/index/primary rows remain identical.

Earlier invalid migration-ID cases stop at `migrate_ids`' separate zero/duplicate
check (`store.rs:782`); they cannot establish `id_key(0)`'s own outcome at line
136. This new pair directly exercises the non-generic helper through insert
and lookup. Suppressing its guard changes the zero insert to `Ok(())`; the
mutation fails there before the subsequent lookup. It does not claim sensitivity
to omission of individual callers' helper invocations, nor test every integer ID.

### Stored kind versus Rust kind before scalar fallback

`assurance_stored_kind_rejects_before_the_scalar_decoder_can_mask_the_error`
passes valid standalone immutable schema/record handles to
`__private::read::<u64>`. Both records have the correct kind for their supplied
schema, and both schemas contain the requested field. One descriptor is Unsigned
and returns 7; the other is Bytes and returns exactly
`InvalidField { id: 1, reason: "stored schema kind differs from Rust type" }`.
The same generic helper and lexical callsite serve both controls. The caller
update callback executes once in either case, ruling out earlier admission as
the reason for rejection.

This is defensive helper/model compatibility coverage. The alternative
standalone schemas are never published as conflicting immutable descriptors;
no supported table-kind change or bypass of normal registration is claimed.
The call runs inside a typed update after another staged insert. The rejected
helper error aborts the whole draft, while the accepted conversion commits.
Both controls preserve the input schema and full raw record, including its
unknown field and envelope extension, and every retained system/index/primary
row. Rejection preserves the complete current durable image and rows; current
index checks remain consistent.

Disabling only `field.kind != T::KIND` does not make this input succeed:
`schema.read_field` accepts its physically correct Bytes value, then the u64
scalar decoder rejects it. An `is_err()` or category-only assertion would mask
the omission. The new exact-error assertion detects the fallback's different
`"wrong scalar kind"` reason. Existing lookup-kind tests target a distinct guard
at `store.rs:578` and are not substituted for this helper pair.

### Isolated omissions, provenance and limits

Each case has a separate archived-base source copy and fresh Cargo target with
`CARGO_INCREMENTAL=0`. Positive baselines pass in both profiles before changing
only the selected guard to `if (predicate) && false`, retaining predicate
evaluation and the rejection body. All three mutations compile and exit 101
from the intended named test assertion in both debug and release:

| Selected omission | Actual versus required result |
|---|---|
| `info` ownership guard | `Engine(NotFound)` versus `Engine(ForeignStore)` |
| `id_key` zero guard | `Ok(())` versus `InvalidId` |
| `read::<u64>` stored-kind guard | `InvalidField` with `"wrong scalar kind"` versus the explicit stored-schema/Rust-kind reason |

No omission was applied to shared production source. There was no observed
runtime defect and no runtime edit. These are specific functional/predicate
controls and omission-sensitivity results, not compiler-instrumented MC/DC,
whole-module completion, all generic instantiations, or other-platform evidence.

Local proof: `/private/tmp/weekend-typed-admission-dm_di240/`. It retains the base
source tar, exact test source/patch, source/compiler identities, six focused
host baseline logs, strict Clippy output, and each isolated case's positive and
negative logs/production patch. `evidence.json` SHA-256 is
`21802eba77d8841578c7d58bde13a8c8541d3737766c1ddf287c5cd536c33bbe`.
The verified compact archive `/private/tmp/weekend-typed-admission-dm_di240.tar.gz`
contains the report and all 32 indexed payloads; its SHA-256 is
`f1cd04f901b124d84fef6ed7b8b157f1e16d20d2ecbe1b4907e8767b316b1cd0`.
Build outputs and reconstructed source copies remain separately in the proof
directory. Original production hashes remain the T7 pins above; the final test
source SHA-256 for this tranche is
`00a3c88b7e6847a7bc41207528345a74faf1fb78768584b136da1b0ba6d38d8d`.

Root independently restored all 32 indexed payloads and verified the twelve
baseline/mutant command results, their compiled assertion failures and the exact
test-source digest. The protected copy is
`target/archived-proof/typed-admission-703948c.tar.gz`, with the same archive
digest above. The complete 22-test `runtime_decisions` suite also passes in
debug and release with Rust 1.99.1, offline/locked, all features; its logs are
retained at `/private/tmp/weekend-typed-admission-full/`. This broader regression
run is separate from the archive's focused omission experiments.

## T10: outer write-poison admission after a caught callback panic

The existing
`caught_callback_panic_closes_the_write_before_another_callback` test
(`tests/runtime_decisions.rs:17–95`) directly qualifies `Store::write`'s
`write.failed` guard at `store.rs:304`. Clean source commit
`f751c5a501686b40516e85e47394ccd1a02e9daf` supplies both production and test
inputs, without an overlay or new test. Its focused positive baseline passes
in debug and release on macOS arm64 with Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1),
offline/locked with all typed features.

The test first accepts a clean callback, then stages a unique-field update,
catches a second update callback's panic, and verifies that the poisoned Write
refuses another callback with exact `Engine(Closed)`. Although its outer closure
returns `Ok(())`, `Store::write` must return exact `Engine(Closed)`. Baseline
assertions preserve current revision, both original primary rows and unique
lookups, and both retained primary rows. The staged unique claim stays absent,
current indexes remain consistent, and a fresh Write still succeeds.

This uses `Store::memory`: there is **no durable-byte oracle**. It also does not
compare complete raw collections or a retained-view index report. Successful and
terminal outer `Store::write` calls use different closure types; they establish
source-level outcomes, not a same-instantiation outer-function pair. The copied
follow-up callback's separate same-`checked`-instantiation claim remains T6's
scope and is not transferred to `Store::write`.

Ordinary caught typed errors would mask removal of this outer guard: `checked`'s
error arm aborts the underlying transaction (`store.rs:638–639`), so core commit
can independently return `Closed` (`src/engine.rs:877–878`). A caught callback
panic bypasses that error arm. The later refused call exits at `store.rs:628`
before touching the transaction, leaving its earlier staged work active. This
existing panic control therefore isolates the outer admission obligation.

After both positive baselines passed, an isolated copy changed only
`if write.failed` to `if write.failed && false`, retaining its rejection body.
The omission compiles and fails the named test in **both debug and release**:
at `runtime_decisions.rs:58`, the outer result becomes `Ok(())` instead of
`Err(Engine(Closed))`. These failing runs stop at that assertion; their later
preservation assertions are not reached. Baseline and mutant builds use
distinct fresh targets with `CARGO_INCREMENTAL=0`. Shared production and test
files were never mutated. This is functional omission sensitivity, not compiler
MC/DC, coverage of every generic instantiation, or whole-module completion.

Proof directory: `/private/tmp/weekend-typed-outer-poison-akpc0uny/`. Its
`evidence.json` indexes 14 payloads, including the clean source tar, exact test
and original/mutated store sources, single-guard patch, compiler/source
identities, runner and all four command logs. The manifest SHA-256 is
`c051cb8979c0c9573d479191078cb3bbd358b2e9802f700fcc53819981ff358e`.
The compact archive
`/private/tmp/weekend-typed-outer-poison-akpc0uny.tar.gz` was restored and every
indexed payload and command outcome verified; its SHA-256 is
`cd108b695f12ac2b8207c707d280552c54383a506a43020fae3b0551615530d3`.
Build outputs and reconstructed sources are excluded. Production store SHA-256
remains `2157407964fc4ff889737efb29f21270fe6ae2985db1288aac5d2dc740c1f7fa`;
the unchanged test SHA-256 is
`00a3c88b7e6847a7bc41207528345a74faf1fb78768584b136da1b0ba6d38d8d`.

Root separately restored and verified the 14 payloads and four command outcomes,
the base Git archive, all nine input hashes and the single-guard mutation.
The protected copy is
`target/archived-proof/typed-outer-poison-f751c5a.tar.gz`, with the same archive
digest above. No new test or runtime source was needed.

## T11: stored-record identity and old-model insert admission

Two tests qualify the three remaining identified feasible admission gaps at
unchanged `store.rs:423`, `485` and `693`. They reuse Row/RowV2, the deliberately
permissive handwritten `Manual<0>` model, the seeded-store helper and complete
snapshot-row comparisons. There is no new model, dependency or runtime edit.
All isolated inputs are clean commit
`403936057c856acfc7cc3ba63aef00487def5727` plus only the exact
`tests/runtime_decisions.rs` overlay; concurrent snapshot-worker changes are
excluded. Typed production hashes remain the original pins above.

### Stored-record type before decoder and schema fallbacks

`assurance_stored_record_types_reject_before_decoder_and_schema_fallbacks`
(`runtime_decisions.rs:1127`) keeps the receiving catalog, table handle,
descriptor, record version and primary ID valid. A raw transaction writes a
structurally valid record with either the correct positive type ID or another
positive type ID. This is a deliberately pre-existing raw-access invariant
breach, not a supported typed write or a repair operation.

The same `Read::get<Manual<0>>` callsite returns the row or exact typed
`TypeMismatch`. The existing manual decoder accepts any envelope; a generated
decoder would independently return the same `TypeMismatch` and mask omission
of `decode`'s guard. The same `Read::check_indexes` callsite accepts the valid
row or returns exact typed `TypeMismatch`. Without `record_keys`' guard,
subsequent schema validation instead returns `Schema(Incompatible)`; a generic
error assertion would mask this omission.

The baseline separately catches the same rejection from `Write::check_indexes`
after a staged edit to another table. The outer Write returns exact `Closed`
and publishes none of that edit. The accepted control commits it. Rejection
preserves the complete durable image and current system/catalog, descriptor,
index and both primary collections, including the exact pre-existing wrong-type
row. Both controls preserve every corresponding retained row, and the side
table's current index remains consistent. No claim is made that an index report
for the deliberately corrupt table is available or that rejection repairs it.

### Old-model insert after catalog upgrade

`assurance_old_model_insert_rejects_newer_catalog_without_partial_writes`
(`runtime_decisions.rs:1225`) retains the same Row-v1 handle and compares its
catalog at v1 versus upgraded v2. An earlier update of an existing v1 row through
the old model remains valid in either case. Inserting a fresh nonzero primary
ID and unused unique value then returns `Ok(())` or exact `UnsupportedVersion`.
The old descriptor remains published, excluding missing-schema, duplicate-ID
and uniqueness failures as substitute rejection oracles.

Catching the upgraded-catalog rejection leaves the outer Write `Closed` and
preserves the complete current durable image and all current/retained
system/schema/index/primary rows. The accepted control commits both changes.
Exact primary and unique lookups and current index consistency distinguish
successful insertion from an unobserved partial change.

### Isolated omissions and qualification

Each guard receives its own source copy from the clean archive. Every baseline
and mutant command, separately in debug and release, uses an initially absent
target directory and `CARGO_INCREMENTAL=0`. Each command has a 360-second bound
and ten-second kill grace. After positive baselines pass, only the selected
guard becomes `if (predicate) && false`, preserving predicate evaluation and
its rejection body. All six mutants compile and fail the intended assertions
with exit 101; none is a compile error or timeout:

| Omitted guard | Actual versus required result | First failing observation |
|---|---|---|
| `decode` raw type, line 423 | `Ok(Some(()))` versus `Err(TypeMismatch)` | Snapshot-backed `get`, test line 1154 |
| `record_keys` raw type, line 485 | `Err(Schema(Incompatible))` versus `Err(TypeMismatch)` | Snapshot-backed `check_indexes`, test line 1166 |
| `insert` catalog version, line 693 | `Ok(())` versus `Err(UnsupportedVersion)` | Write `insert`, test line 1242 |

The first two mutations fail before the later transaction-backed checks. Their
omission sensitivity is scoped to those Snapshot instantiations; baseline
Write poisoning/preservation evidence is separate. Mutants stop at their named
first assertions and do not execute the later preservation assertions.

On macOS arm64, Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), all six focused
baselines pass. Full offline/locked, all-feature debug and release suites each
pass 60 tests: 54 integration tests, including 24 runtime-decision tests, and
six doctests; one documentation example remains ignored. Strict all-target
Clippy with warnings denied passes. Source and lockfile input hashes match
before and after qualification; all thirteen owned target directories are
removed. These are functional/source-level controls, not compiler MC/DC or
other-platform execution.

Proof: `/private/tmp/weekend-typed-t11-d0dqac/`. `evidence.json` indexes 63
retained payloads, including the exact base source tar, test source/patch,
original and three mutated store sources, individual guard patches, compiler
and source identities, runners, fifteen exact command records and logs, and
hashes of the twelve focused executables. Build outputs and reconstructed
source copies are excluded from the compact archive. The manifest SHA-256 is
`6ea0a1c84001a82d47a067e8f6ed76270deaa720e87447e115a62f0b161cda39`.
The restored archive `/private/tmp/weekend-typed-t11-d0dqac.tar.gz` verifies all
63 indexed payloads; archive SHA-256 is
`ffb01a4b6266c1617656bd995fa3c06033590534572df0a8d9b45159a1b3c011`.
The final test source SHA-256 is
`a10cdc51904c34c7e6286c3c84d082cf323df67ad149ec1c694241194a9c5e26`.

Root independently restored all 63 indexed payloads and verified fifteen
command outcomes, exact compiled mutation assertions, the clean base Git
archive, all input hashes and each single-guard source transformation. The
protected copy is
`target/archived-proof/typed-admission-followup-4039360.tar.gz`, with the same
archive digest above. Independent read-only review found no actionable issue.

A preliminary runner setup at `/private/tmp/weekend-typed-t11-GoKeAT/` ran two
passing baselines, then exited 43 because its substitution left the selected
guard unchanged. No mutant command ran there. It is retained separately and
excluded from the final proof above; the final runner checks the original line
and constructs the changed line directly.

### Finite guard-assurance boundary and remaining E evidence

This completes the identified feasible admission-guard batch for the pinned
typed runtime source. Completion means explicit accepted/rejected controls,
unmasked exact oracles for the identified gaps, atomicity/retained-state
baselines and the scoped omission results above. It does not mean zero E
records, whole-module compiler MC/DC, every generic instantiation, every error
at every propagation site or a complete supported-platform matrix.

The remaining 43 E records deliberately retain their narrower dispositions:

- Twenty-four loops retain execution evidence rather than an invented claim
  to exhaust all iteration counts. Initialization/open's three-item loops have
  fixed structural bounds. CRUD, restore, migration and index-repair tests
  supply their existing functional and fault evidence.
- Thirteen matches retain dispatch evidence. Error conversion/formatting,
  Option/Presence, schema loading, index comparisons and checked-operation
  results delegate callee failures as stated in T5. In particular, the
  `index_key` match's Boolean/Integer/Unsigned unique-key arm matrix remains a
  separate explicit execution limitation; scalar codec tests do not substitute
  for those unique-index paths.
- Five other `if` records already have behavioral alternatives: table entry
  presence/type (`335/336`), physical field/key presence (`493`), update needing
  migration (`719`), and existing index claim (`863`). Existing registration,
  metadata, missing/default, CRUD and uniqueness tests remain their evidence.
  They are not cosmetically relabelled or assigned additional mutation claims.
- The non-Active outcome of `checked`'s guard at `638` is structurally excluded
  by the current public Write interface: its transaction is private; typed
  operations do not commit it; an earlier checked error aborts it while leaving
  `failed=true`, blocking re-entry at `628`; outer commit occurs afterward at
  `307`. This is a scoped invariant, not a fabricated terminal-transaction test
  or a new I disposition. Removing that abort can also be masked by the outer
  `write.failed` rejection.

The second presence/key condition at `493` is intentionally unevaluated for an
absent field; explicit null supplies the present-but-no-key control. Unsupported
container keys are excluded by normal typed registration/validation but can be
reached after raw metadata corruption, so no unconditional infeasibility claim
is made. Standard-library allocation failures and individual schema/core/storage
propagations retain the delegated limits in T5. Production inputs and the
315-record denominator remain unchanged.

## T12: Boolean, Integer and Unsigned unique-key dispatch

The existing `Scalars` model and
`scalar_boundaries_binary_lookup_and_float_bit_identity_roundtrip`
(`tests/models.rs:709`) now make its existing flag, signed and counter fields
unique. No model, helper, dependency or runtime implementation is added. This
qualifies the finite unique-key dispatch matrix left separate in T11; the
`index_key` match at `store.rs:463` retains its E disposition. All dispositions
and the 315-record denominator remain unchanged.

Two rows independently exercise Boolean true/false, Integer i8 minimum/maximum
and Unsigned u32 maximum/zero through actual typed `find` calls before and after
reopen. Signed/unsigned values are widened to their stored 64-bit representation.
Absent signed-zero and unsigned-one lookups supply miss controls. Existing
binary and float behavior remains: the exact NaN payload is indexed and read,
then an update replaces that key with negative zero while another row retains
positive zero; the retained checkpoint still resolves the original NaN bits.
Current and retained index reports remain consistent.

The initial entire raw index collection must equal exactly ten independently
assembled keys and unsigned owner IDs, not merely keys recreated by lookup.
The current private layout is collection-ID LE64, field-ID LE64, one literal
kind tag, then the payload. Fixture tags are Boolean 1, Integer 2, Unsigned 3,
Float 4 and Bytes 5. Boolean payloads are one byte, integer/float payloads eight
little-endian bytes, and byte-string payloads exact bytes. The fixture calls no
runtime index encoder and derives no tags from `Kind`; correlated writer/lookup
encoding errors therefore cannot satisfy this raw-entry oracle. This records
the tested private implementation layout, not a new public-format promise.

Each field 1/2/3 collision changes only that field on row 2; the other four
unique values remain distinct or unchanged self-owned claims. It returns exact
`UniqueViolation(field_id)`, avoiding an earlier different-field collision as
a substitute oracle. A preceding nested-list edit to row 1 must roll back even
when the collision error is caught: the outer Write returns `Closed`, revision
stays unchanged, and every current/retained system/catalog, descriptor, index
and primary row remains equal. Following the float update, MemoryStore reopen
preserves complete current primary/index rows; every retained collection row
still equals its pre-update snapshot. These are logical-row and byte-adapter
reopen checks, not a complete durable-image comparison or native filesystem,
crash or power-loss qualification.

### Source and bounded execution

All retained commands run from clean
`3db5bcdd20de297b8b5424c1fbc2d1e316e2145d` plus only the exact `models.rs`
overlay. Its original source SHA-256 is
`6802b09817f9524d222735080a1c01bb927bfb8619ccdf0d94dc7db1dee3b8d8`;
the final source SHA-256 is
`ff3aefad4f4efc37164c69e0355c2c27f81f0254bd5cfda5eb6f087f5afff932`.
The exact patch SHA-256 is
`ba542346362a258e30615bd42f3857a7bcd08e0eecfc4e1770caee35509cb65c`.
The clean Git archive SHA-256 is
`590eb6c5628e1201071d6f539209d65d3efab054e4904e6ef76556b20479d963`.
Production inputs, other tests and Cargo manifests/lock remain unchanged.

On macOS arm64 with Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), the focused test
passes in debug and release. Full all-feature typed debug/release suites each
pass 60 tests (54 integration tests and six doctests), with one ignored
documentation example. Strict all-target Clippy with warnings denied passes.
All five commands use offline/locked dependencies, `CARGO_INCREMENTAL=0`, a
separate initially absent target and a 360-second timeout plus ten-second kill
grace. Source/lock input hashes before and after match; all five owned targets
are removed. Formatting and diff checks pass.

Proof: `/private/tmp/weekend-typed-t12-L3NRSM/`. Its `evidence.json` indexes 27
payloads, including the exact source tar, original/final test source and patch,
source/compiler identities, runner, five command records and complete logs,
and model executable hashes from the four test commands. The manifest SHA-256
is `708f77a09c97ce6bd5dd862bb0666ea12e7e2512178ef2418cdfc0c75ffe19c4`.
The compact archive `/private/tmp/weekend-typed-t12-L3NRSM.tar.gz` was restored
and all 27 indexed payload hashes verified; its SHA-256 is
`3ee3b0c59fe2018b08466f97c91e257c7875cf36046709de8a351d10747143f3`.
Reconstructed source and build outputs are excluded from that archive.

Root independently restored all 27 payloads, verified five command outcomes
and full-suite totals, and matched the clean base archive, unchanged production
inputs and exact test overlay. The protected copy is
`target/archived-proof/typed-scalar-index-3db5bcd.tar.gz`, with the same archive
digest above. Root code review found no actionable issue.

This is dispatch execution and finite functional evidence, with no mutation
experiment or automatic compiler MC/DC claim. It does not exhaust i64/u64
ranges, all Rust field types/generic instantiations, delegated errors or other
platforms. The structural, propagation, allocator and unsupported-container
limits in T5/T11 remain in force; no E record is cosmetically promoted to P.
