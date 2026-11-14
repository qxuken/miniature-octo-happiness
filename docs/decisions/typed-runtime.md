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
| lib.rs:106:9 [3688–3690] | if / `read` | E | T2/T5 |
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
| store.rs:136:5 [3387–3389] | if / `id_key` | P | T1/T4/T5 |
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
| store.rs:304:9 [9050–9052] | if / `write` | E | T1/T4/T5 |
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
| store.rs:404:5 [13121–13123] | if / `info` | E | T1/T4/T5 |
| store.rs:408:63 [13352–13353] | try_propagation / `info` | D | T1/T4/T5 |
| store.rs:409:59 [13412–13413] | try_propagation / `info` | D | T1/T4/T5 |
| store.rs:410:6 [13420–13421] | try_propagation / `info` | D | T1/T4/T5 |
| store.rs:411:5 [13427–13429] | if / `info` | P | T1/T4/T5 |
| store.rs:411:8 [13430–13500] | short_circuit_or / `info` | C | T1/T4/T5 |
| store.rs:422:40 [13729–13730] | try_propagation / `decode` | D | T1/T4/T5 |
| store.rs:423:5 [13736–13738] | if / `decode` | E | T1/T4/T5 |
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
| store.rs:485:5 [15677–15679] | if / `record_keys` | E | T3/T5 |
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
| store.rs:693:13 [23588–23590] | if / `insert` | E | T3/T4/T5 |
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
| store.rs:741:13 [25449–25451] | if / `migrate` | E | T3/T4/T5 |
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
| store.rs:786:13 [27254–27256] | if / `migrate_ids` | E | T3/T4/T5 |
| store.rs:794:14 [27536–27537] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:796:13 [27582–27585] | for_iteration / `migrate_ids` | E | T3/T4/T5 |
| store.rs:798:56 [27682–27683] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:798:58 [27684–27685] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:799:71 [27756–27757] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:806:18 [28011–28012] | try_propagation / `migrate_ids` | D | T3/T4/T5 |
| store.rs:819:71 [28388–28389] | try_propagation / `migrate_record` | D | T3/T4/T5 |
| store.rs:820:9 [28399–28401] | if / `migrate_record` | E | T3/T4/T5 |
| store.rs:823:9 [28500–28502] | if / `migrate_record` | E | T3/T4/T5 |
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

Disposition counts: C 21, D 195, E 53, I 2, P 44. All 315 source records are listed.

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
