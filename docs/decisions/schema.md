# Raw schema source decision audit

This classifies every production `src/schema.rs` record from the source inventory
at the exact hash below. The denominator is **176 source records**: 111 `?`,
33 `if`, 12 loops, ten `||`, five pattern tests, two matches, two match guards
and one `&&`. Overlapping syntax is counted by the inventory, not as additional
independent MC/DC obligations. This is a source-to-test audit, not a compiler
coverage percentage. No production code changed.

New directed cases are in [schema_decisions.rs](../../tests/schema_decisions.rs).
All 15 existing [schema tests](../../tests/schema.rs) were reused and rerun.
The appendix assigns every record a status and evidence group:

| Status | Meaning |
|---|---|
| P | Predicate controls or compound independence pairs below |
| E | Execution, dispatch, loop or specific propagation evidence; not every callee error variant |
| C | Syntactic overlap with an enclosing classified predicate |
| I | Error outcome structurally infeasible on validated immutable handles; proof below |
| D | Primitive/allocator/transaction error propagation delegated to existing engine evidence; per-call error completeness is not claimed |

## S1: descriptor identity, attributes and kind dispatch

`descriptor_identity_names_and_container_guards_have_independent_controls`
compares valid type/version (1,1) against (0,1) and (1,0), separately for build
and raw descriptor parsing. These are the three independent controls for both
OR guards. MAX/MAX identifiers remain valid; this layer has no narrower numeric
range. `fields` Map versus Boolean and `reserved` List versus Boolean isolate
container guards. Missing keys and malformed unsigned/boolean/name attributes
exercise required-key/conversion failures.

The name OR guard has valid nonempty UTF-8, empty valid UTF-8, and nonempty
invalid UTF-8 controls. Each operand alone rejects. Field-kind match dispatch
accepts codes 1 through 7 using actual Boolean/Integer/Unsigned/Float/Bytes/Map/
List values; 0, 8 and MAX reject. The valid cases are in
`field_kind_default_presence_and_nullable_controls_are_explicit`.

## S2: field IDs, reserved IDs and duplicate names

`descriptor_ids_reservations_and_duplicate_names_have_valid_controls` compares
zero/unique/nonzero duplicate field IDs. For `id == 0 || exists`, zero versus a
valid unique ID isolates the first operand, while duplicate versus unique
nonzero IDs isolates the second. Existing zero IDs cannot be present in the
partially built map, so a zero-and-existing combination is infeasible and not
needed by these masking pairs. Reserved-ID checks use zero, an active field ID,
and a distinct ID in the same way. Reserved lists with distinct IDs and repeated
IDs isolate equality in the prior-item loop.

Two distinct fields with distinct names succeed; the same IDs with equal names
reject. Empty/single/multiple field and reservation sequences exercise loop
entry/exit. Eight-byte nonzero keys succeed; 7-/9-byte and zero keys fail. Field
ID MAX succeeds. The original `malformed_descriptors_and_record_shapes_are_checked`
also covers these errors and zero record setter/remover IDs.

## S3: presence, null, defaults and kind checking

`field_kind_default_presence_and_nullable_controls_are_explicit` crosses required,
nullable and default presence. Missing physical values return a declared default,
MissingField, or None according to those flags. Defaults never materialize fields
on read. A physical None is accepted only when nullable; a wrong non-None kind
returns WrongKind regardless of default. This supplies true/false controls for
value presence, default presence, requiredness, nullness, nullability and equal
kind. The seven kind alternatives all validate correct values. A None default
is accepted/rejected by nullable; original malformed-descriptor tests reject a
wrong-kind default.

Existing `missing_none_and_default_are_distinct_per_schema_version` establishes
that a physical None overrides a default and distinguishes removing a field.
Unknown IDs return UnknownField for schema projection. Unknown physical fields
are retained and tolerated unless explicitly reserved. Application-width/range
conversion is **outside this module**: `conversion_overflow_or_unknown_target_field_aborts_every_migrated_row`
and typed `scalar_boundaries_binary_lookup_and_float_bit_identity_roundtrip`
provide that separate evidence. No arbitrary application range policy is claimed.

## S4: evolution and descriptor loading

`successor_type_version_overflow_and_reserved_history_are_independent` uses
same-type/consecutive-version success, different-type/consecutive-version failure,
and same-type/nonconsecutive-version failure. These isolate the two successor
OR operands. Old version MAX cannot increment and rejects without wrapping.
The field match accepts retained same-kind fields and removed-but-reserved IDs;
wrong kind or unreserved removal rejects. Both match guards have true/false
controls within their respective Some/None patterns. Old reservations must remain.
Requiredness/default/nullability can change without changing kind, as the actual
raw convention permits.

`load_checks_requested_type_and_version_and_propagates_read_failure` writes a
valid descriptor under the requested key, then independently varies its embedded
type and version. The load OR guard rejects each mismatch. Missing descriptors,
an injected ReadView Io error, and malformed stored values have distinct
propagation controls. Transaction and snapshot ReadView implementations are
exercised by existing schema lookup dependencies and retained-schema tests.

## S5: publication and terminal state

`publish_existing_equality_predecessor_and_abort_state_have_controls` covers
absent/existing descriptors, equal idempotent publication versus unequal immutable
redefinition, version-one direct publication, and version-two predecessor lookup
success/missing predecessor. `foreign_values_and_incompatible_publication_abort_without_partial_catalog`
also checks incompatible evolution and foreign descriptor insertion. Prior staged
writes and published revision remain unchanged on error.

For A=`result.is_err()`, B=`tx.state() == Active`, success gives (F,T), an active
redefinition/predecessor/foreign-value failure gives (T,T), and an already-aborted
transaction gives (T,F). The first two isolate A; the latter two isolate B. The
active failing draft is aborted; already-aborted remains aborted. A successful
publication on a nonactive transaction is infeasible because tx.get rejects it
first. This is a feasible masking pair set, not a claim that all four combinations
must exist.

## S6: record envelopes, edits and unknown-field preservation

`record_envelope_unknown_fields_and_access_guards_are_explicit` uses (type,version,
fields)=(1,1,Map) as control, independently replacing each with zero/zero/List.
All three OR operands reject independently. Missing envelope keys, malformed
unsigned headers, scalar envelope and malformed field keys reject. Matching
schema envelopes succeed; independently mismatched type or version fail.

Zero field access rejects and valid/missing nonzero field access succeeds.
Existing malformed-record tests supply zero setter/remover controls. Unknown
MAX-ID data and an unknown envelope key survive set/remove and same-type schema
version change. Removing an absent field is a no-op. A reserved physical field
rejects validation. with_schema accepts same type and rejects another type.
Foreign-value setters and foreign-database version creation propagate ForeignStore
without changing the original persistent envelope.

## S7: invariant proofs and delegated propagation

Schema and Record raw values are private, validated at construction, and immutable.
`as_value()` permits building a new persistent Value, never mutating the retained
one. Therefore required descriptor/envelope keys cannot disappear inside an
existing handle. Descriptor field keys are already nonzero eight-byte IDs;
record keys have the same proof. Rechecking field IDs during validate/successor
cannot produce InvalidRecord. The prior-descriptor loop only reparses previously
accepted fields. Field.name's Bytes/UTF-8 expects follow its parser checks.

Reserved vectors have List kind before iteration, and indices are bounded by the
same immutable length. Earlier elements have passed unsigned conversion before
the duplicate loop revisits them. Schema-held reservation elements have all
passed those checks. These prove Type/Bounds residuals impossible at the I
records. Fixed metadata keys fit an existing validated handle. Schema field IDs also fit
the context that successfully read its eight-byte reserved metadata key. This
argument does **not** extend to Record field IDs: max_bytes is a distinct key
budget. `valid_empty_records_can_have_a_smaller_key_budget_than_field_ids` builds
a valid empty record at max_bytes 7, then observes Limit from eight-byte field
access and schema projection; the ordinary-budget control succeeds. The same
small budget rejects descriptor construction at its first field-ID lookup.
Those three real propagation records are E, not infeasible. No unsafe mutation
or invalid private-state manufacture is in scope.

Map enumeration and construction, persistent edits, observation collection and
transaction publication can still fail allocation/budget/context checks. D
records retain that delegation rather than calling them fully covered.
`wrapper_allocation_failures_abort_drafts_before_primary_index_or_migration_can_diverge`
sweeps allocation failures through the schema-based wrapper; `storage_faults_never_publish_a_partial_schema_primary_index_batch`
checks interrupted durable schema/data/index publication. Engine allocation,
value-boundary and transaction suites own their primitive decisions. These
support propagation behavior, but this tranche does not inject every possible
callee error at every `?`. Those per-call combinations remain outside the closed
local-predicate claim. Display formatting and standard-library UTF-8/numeric
conversion internals are external, not silently added to this source denominator.

## Complete source-record appendix

Each key is `line:start-column–end-column kind` in the hash-bound source. E and D carry the group
reference plus the scope above. Logical/pattern records marked C refer to the
same enclosing P decision, not additional independent tests.

| Record | Owner / expression | Status | Evidence |
|---|---|---|---|
| 63:23–24 try_propagation | `required` — `value.map_get(key)` | E | S1/S2 |
| 66:25–26 try_propagation | `unsigned` — `required(value, key)` | E | S1/S2 |
| 71:83–84 try_propagation | `field_id` — `bytes.try_into().map_err(\|_\| Error::InvalidRecord)` | E | S1/S2 |
| 72:5–7 if | `field_id` — `id == 0` | P | S1 |
| 100:45–46 try_propagation | `parse` — `required(&value, b"name")` | E | S1 |
| 101:74–75 try_propagation | `parse` — `name.as_bytes().map_err(\|_\| Error::InvalidDescriptor)` | E | S1 |
| 102:9–11 if | `parse` — `bytes.is_empty() std::str::from_utf8(bytes).is_err()` | P | S1 |
| 102:12–67 short_circuit_or | `parse` — `bytes.is_empty() std::str::from_utf8(bytes).is_err()` | C | S1 |
| 105:20–25 match | `parse` — `unsigned(&value, b"kind")?` | E | S1 |
| 105:51–52 try_propagation | `parse` — `unsigned(&value, b"kind")` | E | S1 |
| 115:53–54 try_propagation | `parse` — `required(&value, b"required")` | E | S1 |
| 117:51–52 try_propagation | `parse` — `required(&value, b"required")? .as_bool() .map_err(\|_\| Error::InvalidDescriptor)` | E | S1 |
| 118:64–65 try_propagation | `parse` — `required_value_bool(&value, b"nullable")` | E | S1 |
| 119:48–49 try_propagation | `parse` — `value.map_get(b"default")` | I | S3/S7 |
| 127:9–11 if | `parse` — `let Some(default) = &field.default` | P | S3 |
| 127:12–15 pattern_test | `parse` — `&field.default` | C | S3 |
| 130:55–56 try_propagation | `parse` — `field .check(0, default) .map_err(\|_\| Error::InvalidDescriptor)` | E | S3 |
| 135:9–11 if | `check` — `value.kind() == Kind::None` | P | S3 |
| 136:13–15 if | `check` — `self.nullable` | P | S3 |
| 141:16–18 if | `check` — `value.kind() == self.kind` | P | S3 |
| 149:25–26 try_propagation | `required_value_bool` — `required(value, key)` | E | S3 |
| 169:9–11 if | `build` — `type_id == 0 version == 0` | P | S1/S2 |
| 169:12–40 short_circuit_or | `build` — `type_id == 0 version == 0` | C | S1/S2 |
| 172:31–32 try_propagation | `build` — `db.map()` | D | S1/S2/S7 |
| 173:9–12 for_iteration | `build` — `fields` | E | S1/S2 |
| 174:13–15 if | `build` — `spec.id == 0 map.map_get(&field_key(spec.id))?.is_some()` | P | S1/S2 |
| 174:16–75 short_circuit_or | `build` — `spec.id == 0 map.map_get(&field_key(spec.id))?.is_some()` | C | S1/S2 |
| 174:64–65 try_propagation | `build` — `map.map_get(&field_key(spec.id))` | E | S1/S2/S7 |
| 178:23–24 try_propagation | `build` — `db .map()` | D | S1/S2/S7 |
| 179:66–67 try_propagation | `build` — `db.bytes(spec.name.as_bytes())` | D | S1/S2/S7 |
| 179:68–69 try_propagation | `build` — `db .map()? .map_set(b"name", &db.bytes(spec.name.as_bytes())?)` | D | S1/S2/S7 |
| 180:65–66 try_propagation | `build` — `db.unsigned(spec.kind as u64)` | D | S1/S2/S7 |
| 180:67–68 try_propagation | `build` — `db .map()? .map_set(b"name", &db.bytes(spec.name.as_bytes())?)? .map_set(b"kind", &db.unsigned(spec.kind as u64)?)` | D | S1/S2/S7 |
| 181:65–66 try_propagation | `build` — `db.boolean(spec.required)` | D | S1/S2/S7 |
| 181:67–68 try_propagation | `build` — `db .map()? .map_set(b"name", &db.bytes(spec.name.as_bytes())?)? .map_set(b"kind", &db.unsigned(spec.kind as u64)?)? .map_set(b"required", &db.boolean(spec.required)?)` | D | S1/S2/S7 |
| 182:65–66 try_propagation | `build` — `db.boolean(spec.nullable)` | D | S1/S2/S7 |
| 182:67–68 try_propagation | `build` — `db .map()? .map_set(b"name", &db.bytes(spec.name.as_bytes())?)? .map_set(b"kind", &db.unsigned(spec.kind as u64)?)? .map_set(b"required", &db.boolean(spec.required)?)? .map_set(b"nullable", &db.boolean(spec.nullable)?)` | D | S1/S2/S7 |
| 183:13–15 if | `build` — `let Some(default) = &spec.default` | P | S1/S2 |
| 183:16–19 pattern_test | `build` — `&spec.default` | C | S1/S2 |
| 184:59–60 try_propagation | `build` — `field.map_set(b"default", default)` | D | S1/S2/S7 |
| 186:59–60 try_propagation | `build` — `map.map_set(&field_key(spec.id), &field)` | D | S1/S2/S7 |
| 188:36–37 try_propagation | `build` — `db.list()` | D | S1/S2/S7 |
| 189:9–12 for_iteration | `build` — `reserved` | E | S1/S2 |
| 190:58–59 try_propagation | `build` — `db.unsigned(*id)` | D | S1/S2/S7 |
| 190:60–61 try_propagation | `build` — `retired.list_push(&db.unsigned(*id)?)` | D | S1/S2/S7 |
| 193:21–22 try_propagation | `build` — `db.map()` | D | S1/S2/S7 |
| 194:56–57 try_propagation | `build` — `db.unsigned(type_id)` | D | S1/S2/S7 |
| 194:58–59 try_propagation | `build` — `db.map()? .map_set(b"type", &db.unsigned(type_id)?)` | D | S1/S2/S7 |
| 195:59–60 try_propagation | `build` — `db.unsigned(version)` | D | S1/S2/S7 |
| 195:61–62 try_propagation | `build` — `db.map()? .map_set(b"type", &db.unsigned(type_id)?)? .map_set(b"version", &db.unsigned(version)?)` | D | S1/S2/S7 |
| 196:42–43 try_propagation | `build` — `db.map()? .map_set(b"type", &db.unsigned(type_id)?)? .map_set(b"version", &db.unsigned(version)?)? .map_set(b"fields", &map)` | D | S1/S2/S7 |
| 197:48–49 try_propagation | `build` — `db.map()? .map_set(b"type", &db.unsigned(type_id)?)? .map_set(b"version", &db.unsigned(version)?)? .map_set(b"fields", &map)? .map_set(b"reserved", &retired)` | D | S1/S2/S7 |
| 201:46–47 try_propagation | `from_value` — `unsigned(&raw, b"type")` | E | S1/S2 |
| 202:49–50 try_propagation | `from_value` — `unsigned(&raw, b"version")` | E | S1/S2 |
| 203:9–11 if | `from_value` — `type_id == 0 version == 0` | P | S1/S2 |
| 203:12–40 short_circuit_or | `from_value` — `type_id == 0 version == 0` | C | S1/S2 |
| 206:47–48 try_propagation | `from_value` — `required(&raw, b"fields")` | E | S1/S2 |
| 207:9–11 if | `from_value` — `fields.kind() != Kind::Map` | P | S1/S2 |
| 210:43–44 try_propagation | `from_value` — `fields.map_entries()` | D | S1/S2/S7 |
| 211:51–52 try_propagation | `from_value` — `required(&raw, b"reserved")` | E | S1/S2 |
| 212:9–11 if | `from_value` — `reserved.kind() != Kind::List` | P | S1/S2 |
| 215:9–12 for_iteration | `from_value` — `0..reserved.len()?` | E | S2 |
| 215:35–36 try_propagation | `from_value` — `reserved.len()` | I | S2/S7 |
| 217:29–30 try_propagation | `from_value` — `reserved .list_get(i)` | I | S2/S7 |
| 219:55–56 try_propagation | `from_value` — `reserved .list_get(i)? .as_u64() .map_err(\|_\| Error::InvalidDescriptor)` | E | S2 |
| 220:13–15 if | `from_value` — `id == 0 fields.map_get(&field_key(id))?.is_some()` | P | S2 |
| 220:16–68 short_circuit_or | `from_value` — `id == 0 fields.map_get(&field_key(id))?.is_some()` | C | S2 |
| 220:57–58 try_propagation | `from_value` — `fields.map_get(&field_key(id))` | I | S2/S7 |
| 223:13–16 for_iteration | `from_value` — `0..i` | E | S2 |
| 224:17–19 if | `from_value` — `reserved.list_get(j)?.as_u64()? == id` | P | S2 |
| 224:40–41 try_propagation | `from_value` — `reserved.list_get(j)` | I | S2/S7 |
| 224:50–51 try_propagation | `from_value` — `reserved.list_get(j)?.as_u64()` | I | S2/S7 |
| 229:9–12 for_iteration | `from_value` — `entries.iter().enumerate()` | E | S2 |
| 230:64–65 try_propagation | `from_value` — `field_id(key).map_err(\|_\| Error::InvalidDescriptor)` | E | S2 |
| 231:52–53 try_propagation | `from_value` — `Field::parse(value.clone())` | E | S2 |
| 232:13–16 for_iteration | `from_value` — `&entries[..i]` | E | S2 |
| 233:17–19 if | `from_value` — `Field::parse(previous.clone())?.name() == field.name()` | P | S2 |
| 233:50–51 try_propagation | `from_value` — `Field::parse(previous.clone())` | I | S2/S7 |
| 254:39–40 try_propagation | `field` — `required(&self.raw, b"fields")` | I | S2/S7/S7 |
| 255:37–38 try_propagation | `field` — `required(&self.raw, b"fields")? .map_get(&field_key(id))` | I | S2/S7/S7 |
| 260:51–52 try_propagation | `reserved` — `required(&self.raw, b"reserved")` | I | S2/S7/S7 |
| 261:9–12 for_iteration | `reserved` — `0..ids.len()?` | E | S2/S7 |
| 261:30–31 try_propagation | `reserved` — `ids.len()` | I | S2/S7/S7 |
| 262:13–15 if | `reserved` — `ids.list_get(i)?.as_u64()? == id` | P | S2/S7 |
| 262:31–32 try_propagation | `reserved` — `ids.list_get(i)` | I | S2/S7/S7 |
| 262:41–42 try_propagation | `reserved` — `ids.list_get(i)?.as_u64()` | I | S2/S7/S7 |
| 269:9–11 if | `successor_of` — `self.type_id != old.type_id old.version.checked_add(1) != Some(self.version)` | P | S4 |
| 269:12–91 short_circuit_or | `successor_of` — `self.type_id != old.type_id old.version.checked_add(1) != Some(self.version)` | C | S4 |
| 272:9–12 for_iteration | `successor_of` — `required(&old.raw, b"fields")?.map_entries()?` | E | S4 |
| 272:62–63 try_propagation | `successor_of` — `required(&old.raw, b"fields")` | I | S4/S7 |
| 272:77–78 try_propagation | `successor_of` — `required(&old.raw, b"fields")?.map_entries()` | D | S4/S7 |
| 273:36–37 try_propagation | `successor_of` — `field_id(&key)` | I | S4/S7 |
| 274:13–18 match | `successor_of` — `self.field(id)?` | E | S4 |
| 274:33–34 try_propagation | `successor_of` — `self.field(id)` | I | S4/S7 |
| 275:30–71 match_guard | `successor_of` — `new.kind == Field::parse(old_value)?.kind` | P | S4 |
| 275:65–66 try_propagation | `successor_of` — `Field::parse(old_value)` | I | S4/S7 |
| 276:25–43 match_guard | `successor_of` — `self.reserved(id)?` | P | S4 |
| 276:42–43 try_propagation | `successor_of` — `self.reserved(id)` | I | S4/S7 |
| 280:55–56 try_propagation | `successor_of` — `required(&old.raw, b"reserved")` | I | S4/S7 |
| 281:9–12 for_iteration | `successor_of` — `0..reserved.len()?` | E | S4 |
| 281:35–36 try_propagation | `successor_of` — `reserved.len()` | I | S4/S7 |
| 282:13–15 if | `successor_of` — `!self.reserved(reserved.list_get(i)?.as_u64()?)?` | P | S4 |
| 282:51–52 try_propagation | `successor_of` — `reserved.list_get(i)` | I | S4/S7 |
| 282:61–62 try_propagation | `successor_of` — `reserved.list_get(i)?.as_u64()` | I | S4/S7 |
| 282:63–64 try_propagation | `successor_of` — `self.reserved(reserved.list_get(i)?.as_u64()?)` | I | S4/S7 |
| 295:71–72 try_propagation | `load` — `view .get_record(collection, &descriptor_key(type_id, version))` | E | S4 |
| 296:58–59 try_propagation | `load` — `view .get_record(collection, &descriptor_key(type_id, version))? .ok_or(Error::Engine(crate::Error::NotFound))` | E | S4 |
| 297:45–46 try_propagation | `load` — `Self::from_value(value)` | E | S4 |
| 298:9–11 if | `load` — `schema.type_id != type_id schema.version != version` | P | S4 |
| 298:12–66 short_circuit_or | `load` — `schema.type_id != type_id schema.version != version` | C | S4 |
| 308:13–15 if | `publish` — `let Some(existing) = tx.get(collection, &key)?` | P | S5 |
| 308:16–19 pattern_test | `publish` — `tx.get(collection, &key)?` | C | S5 |
| 308:61–62 try_propagation | `publish` — `tx.get(collection, &key)` | D | S5/S7 |
| 309:24–26 if | `publish` — `existing == self.raw` | P | S5 |
| 315:13–15 if | `publish` — `self.version > 1` | P | S5 |
| 316:90–91 try_propagation | `publish` — `Self::load(tx, collection, self.type_id, self.version - 1)` | E | S5 |
| 317:45–46 try_propagation | `publish` — `self.successor_of(&previous)` | E | S5 |
| 319:48–49 try_propagation | `publish` — `tx.put(collection, &key, &self.raw)` | D | S5/S7 |
| 322:9–11 if | `publish` — `result.is_err() tx.state() == TransactionState::Active` | P | S5 |
| 322:12–69 short_circuit_and | `publish` — `result.is_err() tx.state() == TransactionState::Active` | C | S5 |
| 328:9–11 if | `envelope` — `self.type_id != record.type_id self.version != record.version` | P | S3/S6 |
| 328:12–76 short_circuit_or | `envelope` — `self.type_id != record.type_id self.version != record.version` | C | S3/S6 |
| 335:30–31 try_propagation | `read_field` — `self.envelope(record)` | E | S3/S6 |
| 336:35–36 try_propagation | `read_field` — `self.field(id)` | I | S3/S6/S7 |
| 336:67–68 try_propagation | `read_field` — `self.field(id)?.ok_or(Error::UnknownField(id))` | E | S3/S6 |
| 337:9–11 if | `read_field` — `let Some(value) = record.field(id)?` | P | S3/S6 |
| 337:12–15 pattern_test | `read_field` — `record.field(id)?` | C | S3/S6 |
| 337:46–47 try_propagation | `read_field` — `record.field(id)` | E | S3/S6/S7 |
| 338:36–37 try_propagation | `read_field` — `field.check(id, &value)` | E | S3/S6 |
| 341:9–11 if | `read_field` — `let Some(default) = field.default` | P | S3/S6 |
| 341:12–15 pattern_test | `read_field` — `field.default` | C | S3/S6 |
| 344:9–11 if | `read_field` — `field.required` | P | S3/S6 |
| 351:30–31 try_propagation | `validate` — `self.envelope(record)` | E | S3/S6 |
| 352:9–12 for_iteration | `validate` — `required(&self.raw, b"fields")?.map_entries()?` | E | S3/S6 |
| 352:55–56 try_propagation | `validate` — `required(&self.raw, b"fields")` | I | S3/S6/S7 |
| 352:70–71 try_propagation | `validate` — `required(&self.raw, b"fields")?.map_entries()` | D | S3/S6/S7 |
| 353:51–52 try_propagation | `validate` — `field_id(&key)` | I | S3/S6/S7 |
| 353:53–54 try_propagation | `validate` — `self.read_field(record, field_id(&key)?)` | E | S3/S6 |
| 355:9–12 for_iteration | `validate` — `record.fields()?.map_entries()?` | E | S3/S6 |
| 355:40–41 try_propagation | `validate` — `record.fields()` | I | S3/S6/S7 |
| 355:55–56 try_propagation | `validate` — `record.fields()?.map_entries()` | D | S3/S6/S7 |
| 356:36–37 try_propagation | `validate` — `field_id(&key)` | I | S3/S6/S7 |
| 357:13–15 if | `validate` — `self.reserved(id)?` | P | S3/S6 |
| 357:33–34 try_propagation | `validate` — `self.reserved(id)` | I | S3/S6/S7 |
| 376:21–22 try_propagation | `new` — `db.map()` | D | S6/S7 |
| 377:63–64 try_propagation | `new` — `db.unsigned(schema.type_id)` | D | S6/S7 |
| 377:65–66 try_propagation | `new` — `db.map()? .map_set(b"type", &db.unsigned(schema.type_id)?)` | D | S6/S7 |
| 378:66–67 try_propagation | `new` — `db.unsigned(schema.version)` | D | S6/S7 |
| 378:68–69 try_propagation | `new` — `db.map()? .map_set(b"type", &db.unsigned(schema.type_id)?)? .map_set(b"version", &db.unsigned(schema.version)?)` | D | S6/S7 |
| 379:46–47 try_propagation | `new` — `db.map()` | D | S6/S7 |
| 379:48–49 try_propagation | `new` — `db.map()? .map_set(b"type", &db.unsigned(schema.type_id)?)? .map_set(b"version", &db.unsigned(schema.version)?)? .map_set(b"fields", &db.map()?)` | D | S6/S7 |
| 383:80–81 try_propagation | `from_value` — `unsigned(&raw, b"type").map_err(\|_\| Error::InvalidRecord)` | E | S6 |
| 384:83–84 try_propagation | `from_value` — `unsigned(&raw, b"version").map_err(\|_\| Error::InvalidRecord)` | E | S6 |
| 385:44–45 try_propagation | `from_value` — `raw.map_get(b"fields")` | I | S6/S7 |
| 385:73–74 try_propagation | `from_value` — `raw.map_get(b"fields")?.ok_or(Error::InvalidRecord)` | E | S6 |
| 386:9–11 if | `from_value` — `type_id == 0 version == 0 fields.kind() != Kind::Map` | P | S6 |
| 386:12–40 short_circuit_or | `from_value` — `type_id == 0 version == 0` | C | S6 |
| 386:12–70 short_circuit_or | `from_value` — `type_id == 0 version == 0 fields.kind() != Kind::Map` | C | S6 |
| 389:9–12 for_iteration | `from_value` — `fields.map_entries()?` | E | S6 |
| 389:45–46 try_propagation | `from_value` — `fields.map_entries()` | D | S6/S7 |
| 390:27–28 try_propagation | `from_value` — `field_id(&key)` | E | S6 |
| 408:36–37 try_propagation | `fields` — `self.raw.map_get(b"fields")` | I | S6/S7 |
| 411:9–11 if | `field` — `id == 0` | P | S6 |
| 414:25–26 try_propagation | `field` — `self.fields()` | I | S6/S7 |
| 414:50–51 try_propagation | `field` — `self.fields()?.map_get(&field_key(id))` | E | S6/S7 |
| 417:9–11 if | `set` — `id == 0` | P | S6 |
| 422:51–52 try_propagation | `set` — `self.fields()` | I | S6/S7 |
| 422:83–84 try_propagation | `set` — `self.fields()?.map_set(&field_key(id), value)` | D | S6/S7 |
| 422:85–86 try_propagation | `set` — `self.raw .map_set(b"fields", &self.fields()?.map_set(&field_key(id), value)?)` | D | S6/S7 |
| 426:9–11 if | `remove` — `id == 0` | P | S6 |
| 431:51–52 try_propagation | `remove` — `self.fields()` | I | S6/S7 |
| 431:79–80 try_propagation | `remove` — `self.fields()?.map_remove(&field_key(id))` | D | S6/S7 |
| 431:81–82 try_propagation | `remove` — `self.raw .map_set(b"fields", &self.fields()?.map_remove(&field_key(id))?)` | D | S6/S7 |
| 435:9–11 if | `with_schema` — `self.type_id != schema.type_id` | P | S6 |
| 440:66–67 try_propagation | `with_schema` — `db.unsigned(schema.version)` | D | S6/S7 |
| 440:68–69 try_propagation | `with_schema` — `self.raw .map_set(b"version", &db.unsigned(schema.version)?)` | D | S6/S7 |

## Results and source binding

Classification counts: C 16, D 42, E 49, I 34, P 35. All 176 records have a classification.

Debug and release each passed the 15 existing schema tests and nine new directed
tests. Warnings-denied Clippy passed for the new integration test. Commands used
a separate target directory:

```sh
CARGO_TARGET_DIR=target/schema-decisions cargo test --offline --locked -p weekend-db --all-features --test schema_decisions --test schema
CARGO_TARGET_DIR=target/schema-decisions cargo test --offline --locked -p weekend-db --all-features --release --test schema_decisions --test schema
CARGO_TARGET_DIR=target/schema-decisions cargo clippy --offline --locked -p weekend-db --all-features --test schema_decisions -- -D warnings
```

| File | SHA-256 |
|---|---|
| `src/schema.rs` | `defe72214ddbf59e05bf291dcd93f97bd92a56cfb143f075858dfcab7001181a` |
| `tests/schema_decisions.rs` | `ca5f9e7a26c635791848e70365c4f21be59bf38ab2d6117579ec69a954b8fb07` |
| `tests/schema.rs` | `11f76f4abac949e644104b9be6a3805f059d607d9e33593cba69ddeb869a5314` |

Typed regression names above are supporting prior evidence, not all rerun in this
tranche. The direct predicate pairs and invariant proofs close the local schema
control cases identified here; D propagation variants and external dependencies
remain explicitly delegated. This is not complete compiler-measured MC/DC.
