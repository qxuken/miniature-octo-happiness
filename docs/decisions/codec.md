# Codec source decision audit

The source inventory identifies **253 production AST records** in
`src/storage/codec.rs`: 141 `?`, 45 ifs, 44 overlapping OR records, 14 loops,
four pattern tests, three matches and two AND records. Every record is mapped
below. Nested syntax is not an independent-condition denominator; this is not a
compiler MC/DC percentage or a claim that malformed images are exhaustively tested.
No production codec behavior changed in that inventory tranche. The later
[identity-index follow-up](#value-identity-index-follow-up) changes encoding
lookup and retains this historical table's source scope.

P = directed predicate/arm controls; C = overlapping syntax; E = execution or
error propagation evidence, not every callee outcome at every site; I = invariant
exclusion; B = mixed feasible and coupled/invariant conditions; D = delegated
platform-specific conversion. Evidence codes refer to the sections below and
existing REC-V1 through REC-V7 in DESIGN.md. Those ledgers' independence limitations
remain in force; this source inventory does not upgrade them to full MC/DC.

## C1: checksums, alignment and cursor arithmetic

`checksum_known_vector_and_padding` verifies CRC-64/ECMA's known vector and
checksum exclusion/padding behavior. The vector exercises both high-bit branches
of the eight-bit recurrence; empty/populated payloads exercise byte-loop entry.
The bit loop always iterates eight times when entered. This is algorithm evidence,
not a cryptographic collision/security guarantee.

New `alignment_and_cursor_boundaries_preserve_failed_take_position` controls
align(0), align(1), align(REGION), align(cap), align(cap+1), and align(u64::MAX).
Exact cap succeeds; representable rounding above cap and arithmetic overflow
return Limit. For Cursor, take(0) does not advance, a successful byte advances to
position 1, position 1 + usize::MAX overflows and an ordinary beyond-end take
both return Corrupt without changing position. Completing the remaining byte
changes done from Corrupt to success. Count/key length equality succeeds;
maximum+1 is Limit; short integer and short key bytes are Corrupt. These are
sequential arithmetic/bounds checks, not fabricated compound truth pairs.

## C2: encoding and cache reuse

New `writer_cap_rejects_before_allocator_and_encoder_last_region_is_atomic`
uses a real 64 MiB+1 byte slice, not an invalid declared Rust slice. Under zero
allocation claims, adding exactly cap bytes returns OutOfMemory, while cap+1
returns Limit before allocation; both leave the writer empty. The encoder starts
at cap-REGION, successfully writes one region ending exactly at cap, then rejects
the next object with Limit and leaves the prior bytes intact. The large-slice test
is excluded under Miri to avoid expensive interpreter allocation.

New `encoder_start_and_cached_collection_counter_have_independent_controls`
compares start=APPEND_START to aligned below-start and unaligned above-start,
isolating each operand of the admission OR. The same DATA revision/counter reuses
its offset without appending bytes; changing only the counter returns Invalid,
keeps bytes/cache length unchanged. Existing shared-value/compaction tests cover
value cache reuse and fresh encoding, current/historical roots and empty/populated
collection/record/map/list/retention loops.

New `all_value_kinds_roundtrip_and_noncanonical_scalars_reject` round-trips None,
both booleans, i64::MIN, u64::MAX, an explicit NaN payload, negative zero, empty
bytes/map/list. Kind and scalar bits are checked after reopen. Repaired-checksum
None with bits=2 and Boolean with bits=2 reject Corrupt. Encoder accessor failures
selected by the same immutable kind are infeasible Type errors; list indices
come from that same list's length. DATA collection keys are always the engine's
eight-byte IDs; invalid-width conversion and its visit propagation are internal
invariants. The record visitor contains no fallible operation.

Encoding allocation rejection is reused from
`tests/durability.rs::preparation_and_backend_allocation_failures_preserve_the_acknowledged_state`
and `tests/compaction.rs::allocation_failures_during_copy_and_verification_never_change_the_source`.
These sweeps and public cap/headroom tests supply preparation rollback evidence;
they do not promise every allocation is reached in every kind-specific fixture.

## C3: metadata and root selection

Reuse REC-V1/V3/V6 and these exact tests:

- `recovery_checks_image_length_before_reading_metadata` and
  `recovery_rejects_each_invalid_header_field_before_reading_slots`.
- `absent_or_torn_metadata_obeys_the_surviving_witness` and
  `checksummed_slot_and_witness_metadata_must_have_valid_fields`.
- `recovery_requires_witnesses_to_agree_with_the_selected_root` and
  `tests/durability.rs::recovery_selects_the_newest_available_root_only_if_the_slots_agree`.

They distinguish absent/bad-checksum metadata from checksummed structural damage,
wrong format versus corrupt representation, both slot parities, missing slots,
nonconsecutive revisions, witness agreement and witness absence. Invalid metadata
stops before graph traversal and performs no source write/sync. Slot end above
format cap cannot independently fail after physical length is already bounded
and end>length is checked first. The two alignment predicates are coupled with
the descriptor/end relation. These three REC-V6 limitations remain B, not P.

## C4: selected object and value graph

Reuse REC-V4/V7: `selected_graph_errors_do_not_reopen_the_older_root`,
`graph_traversal_checks_offsets_lengths_order_and_payload_shape`,
`graph_rejects_overlapping_objects_even_when_both_are_checksummed`,
`unaligned_reference_cannot_reuse_a_cached_value_region`,
`graph_rejects_a_changed_header_on_full_object_read`, and
`checksummed_but_malformed_payloads_are_checked_before_use`.

Controls cover backward/aligned references, bounds and arithmetic, every object
header field, born revision, checksums/padding, stable first/full headers,
depth, sorted/duplicate keys, kind/tag errors and region occupancy. Overlap is
tested in both decode orders. The changed-header adapter repairs the checksum and
uses another valid born revision, isolating header stability. Existing no-fallback
fixtures remove the newest witness and establish that removing only the newest
slot allows the intact older graph to reopen.

New `every_reachable_payload_prefix_rejects_without_writing_or_fallback` retains
a snapshot, recovers the fixture to enumerate reachable VALUE and DATA cache
offsets plus the selected descriptor, and replaces each payload by every shorter
prefix. It updates declared payload length, zeros the rest of the region, and
repairs CRC, and clears the newest witness so it cannot independently forbid
fallback. Every selected small object remains one physical region, avoiding an
unrelated changed-alignment error. Every truncated image returns Corrupt, source
bytes stay unchanged and no Write/Sync is traced. This closes cursor-read prefix
cases through the real decoder, including retained catalog fields. Some DATA and
VALUE objects are shared with the prior root; this prefix sweep does not assert
that the older graph is independently healthy for every mutation. The separate
REC-V4/V7 fixtures establish that stronger no-fallback property.

`recovery_preserves_shared_value_identity_and_fails_allocation_safely` and the
compaction allocation sweep cover cached identity and fallible decoder structures.
`deterministic_byte_fuzz_and_truncation_never_panic` remains bounded fuzz evidence,
not exhaustive parser assurance.

## C5: directories and retained roots

Reuse REC-V5/V7: `data_cache_accepts_sharing_and_rejects_inconsistent_aliases`,
`empty_data_graph_still_requires_a_next_collection_id`,
`collection_directory_rejects_each_invalid_id_or_revision`, and
`retained_catalog_bounds_ids_revisions_and_references_are_checked`.
They independently vary DATA expected/born revisions and zero next collection,
collection ID order/range and content revision, record key/write revisions,
retained ID order/range, capture/data revision inequalities, next-ID/count bounds,
and retained next-collection ceiling. Duplicate revision at a distinct DATA
offset rejects before reading it; matching DATA offset/revision reuses cache.
The new prefix sweep also reaches every shorter directory/catalog byte payload.

## C6: delegated propagation and diagnostics

`verification_diagnostics_identify_metadata_and_selected_graph_failures`,
`verification_diagnostics_preserve_io_resource_errors_and_success_has_no_marker`,
and `discarded_salvage_probe_does_not_label_destination_failure` check scoped
context, native/resource category preservation and handled-probe clearing.
Read/length error paths reuse
`tests/durability.rs::recovery_errors_and_short_io_do_not_damage_a_clean_image`
and backend allocation sweeps. Diagnostics closures propagate the same errors;
these wrappers add no recovery fallback. Native I/O, allocator implementation,
Map/List operations and std iterator/Option/Result internals are delegated to their
own ledgers. An E row is not an assertion that all transitive errors were forced
at that call site.

## Invariant and platform dispositions

- Valid encoded payload slices have length at most isize::MAX. Adding the 64-byte
  object header cannot overflow u64 on supported 64-bit targets. Writer's current
  length is at most 64 MiB; adding another valid slice cannot overflow usize there,
  although exceeding the format cap is feasible and tested.
- Public storage bounds/alignment constrain encoder offsets. Encoded unique values
  consume at least one region each; the five reserved regions mean the encoder
  cache cannot reach MAX_OBJECTS before the physical image cap. Imported private
  cache/start arguments are internal caller contracts, not public validation APIs.
- Recovery unique objects occupy nonoverlapping aligned regions within a 64 MiB
  image, so object count cannot reach MAX_OBJECTS. Total successful object work
  is at most image bytes plus one 64-byte header per occupied region, below twice
  the image cap. The work overflow/limit errors are infeasible after these guards.
- Reader object end checked-add cannot overflow after offset and aligned size
  bounds; its parent-bound comparison remains feasible. Region slice bounds
  follow the same physical extent and allocated region_count. Empty/nonempty
  region indexes and actual overlapping ranges are exercised separately.
- A cached value ending beyond its new parent would overlap that already verified
  parent. Cached DATA uses the same selected descriptor parent on reuse. Those
  cached end-error outcomes remain infeasible on the stable public-open path;
  DATA revision mismatch remains independently feasible.
- u64-to-usize conversions cannot fail on this 64-bit host. The source keeps
  checked conversions for other widths; no 32-bit execution is claimed here.
- Scalar accessor/collection-key visitor invariants above remain I. None require
  forged references, giant fake slices or violating Rust pointer validity.

Revisit these proofs when image limits, cache policy, object alignment, graph
reference direction, or pointer-width support changes. Macro/unwrap helper bodies
are not silently counted: fixed-width get/put helpers use lengths established by
fixed headers or successful Cursor::take; derive-generated code is delegated.
The source inventory remains syntactic and excludes generated std/library code.

## Source records

Source SHA-256: `4a8d4313f25d5f501583e6fcf03853360386c76aa07b0abf33d2628b3c6df16a`.
Location is line:column; hash is the first 12 hex digits of the source-token
SHA-256. The snapshot includes tests; production locations are unchanged.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 24:5 | `for_iteration` / `update_crc` | E | C1 | `10c22bcf4c76` |
| 26:9 | `for_iteration` / `update_crc` | E | C1 | `10c22bcf4c76` |
| 27:19 | `if` / `update_crc` | P | C1 | `935f68319d4f` |
| 55:29 | `try_propagation` / `align` | E | C1 | `8a8de823d5ed` |
| 56:5 | `if` / `align` | P | C1 | `935f68319d4f` |
| 84:30 | `if` / `metadata` | P | C3 | `935f68319d4f` |
| 103:5 | `if` / `parse_metadata` | P | C3 | `935f68319d4f` |
| 106:5 | `if` / `parse_metadata` | P | C3 | `935f68319d4f` |
| 109:17 | `if` / `parse_metadata` | P | C3 | `935f68319d4f` |
| 110:5 | `if` / `parse_metadata` | C | C3 | `935f68319d4f` |
| 110:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `27fa4fcd4e2b` |
| 110:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `e6c977387404` |
| 110:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `69389a6c906f` |
| 110:8 | `short_circuit_or` / `parse_metadata` | P | C3 | `42aac0cda7dc` |
| 124:5 | `if` / `parse_metadata` | C | C3 | `935f68319d4f` |
| 124:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `89ffac32e3d5` |
| 124:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `66c6b5da88e9` |
| 124:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `2a4c6f49a419` |
| 124:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `c7d1cb4a43ed` |
| 124:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `090df06194bd` |
| 124:8 | `short_circuit_or` / `parse_metadata` | C | C3 | `db59a120e9c0` |
| 124:8 | `short_circuit_or` / `parse_metadata` | B | C3 | `0fd2b42d83f6` |
| 170:9 | `if` / `add` | P | C2 | `935f68319d4f` |
| 178:55 | `try_propagation` / `add` | E | C2 | `8a8de823d5ed` |
| 186:37 | `try_propagation` / `key` | E | C2 | `8a8de823d5ed` |
| 207:37 | `try_propagation` / `object_born` | I | C2: valid-slice arithmetic | `8a8de823d5ed` |
| 208:10 | `try_propagation` / `object_born` | E | C2 | `8a8de823d5ed` |
| 212:33 | `try_propagation` / `object_born` | I | C2: public storage extent | `8a8de823d5ed` |
| 216:33 | `try_propagation` / `object_born` | E | C2 | `8a8de823d5ed` |
| 217:48 | `try_propagation` / `object_born` | E | C2 | `8a8de823d5ed` |
| 233:9 | `if` / `value` | C | C2 | `935f68319d4f` |
| 233:12 | `pattern_test` / `value` | P | C2 | `5654d4106d70` |
| 236:9 | `if` / `value` | I | C2 | `935f68319d4f` |
| 240:41 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 241:9 | `match` / `value` | P | C2 | `4945a70fa7f9` |
| 242:41 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 243:67 | `try_propagation` / `value` | I | C2: immutable kind/index | `8a8de823d5ed` |
| 243:70 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 244:56 | `try_propagation` / `value` | I | C2: immutable kind/index | `8a8de823d5ed` |
| 244:65 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 245:57 | `try_propagation` / `value` | I | C2: immutable kind/index | `8a8de823d5ed` |
| 245:59 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 246:54 | `try_propagation` / `value` | I | C2: immutable kind/index | `8a8de823d5ed` |
| 246:66 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 247:56 | `try_propagation` / `value` | I | C2: immutable kind/index | `8a8de823d5ed` |
| 247:58 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 249:50 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 250:50 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 251:17 | `for_iteration` / `value` | E | C2 | `10c22bcf4c76` |
| 252:52 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 253:38 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 254:40 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 258:40 | `try_propagation` / `value` | I | C2: immutable kind/index | `8a8de823d5ed` |
| 258:49 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 259:17 | `for_iteration` / `value` | E | C2 | `10c22bcf4c76` |
| 259:40 | `try_propagation` / `value` | I | C2: immutable kind/index | `8a8de823d5ed` |
| 260:63 | `try_propagation` / `value` | I | C2: immutable kind/index | `8a8de823d5ed` |
| 260:65 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 261:40 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 265:63 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 266:45 | `try_propagation` / `value` | E | C2 | `8a8de823d5ed` |
| 275:9 | `if` / `data` | C | C2 | `935f68319d4f` |
| 275:12 | `pattern_test` / `data` | P | C2 | `5654d4106d70` |
| 280:13 | `if` / `data` | P | C2 | `935f68319d4f` |
| 286:40 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 287:37 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 288:51 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 290:67 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 292:83 | `try_propagation` / `data` | I | C2: internal visitor/key invariant | `8a8de823d5ed` |
| 295:11 | `try_propagation` / `data` | I | C2: internal visitor/key invariant | `8a8de823d5ed` |
| 297:9 | `for_iteration` / `data` | E | C2 | `10c22bcf4c76` |
| 298:28 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 299:36 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 300:48 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 302:60 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 306:15 | `try_propagation` / `data` | I | C2: internal visitor/key invariant | `8a8de823d5ed` |
| 308:13 | `for_iteration` / `data` | E | C2 | `10c22bcf4c76` |
| 309:50 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 310:33 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 311:40 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 312:36 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 316:67 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 318:50 | `try_propagation` / `data` | E | C2 | `8a8de823d5ed` |
| 338:5 | `if` / `encode` | C | C2 | `935f68319d4f` |
| 338:8 | `short_circuit_or` / `encode` | P | C2 | `aeae0b46445c` |
| 342:45 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 345:49 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 356:55 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 358:30 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 359:25 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 360:35 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 361:34 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 362:47 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 363:5 | `for_iteration` / `encode` | E | C2 | `10c22bcf4c76` |
| 364:76 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 365:32 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 366:43 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 367:27 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 368:48 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 370:66 | `try_propagation` / `encode` | E | C2 | `8a8de823d5ed` |
| 394:35 | `try_propagation` / `take` | E | C1 | `8a8de823d5ed` |
| 400:43 | `try_propagation` / `int` | E | C1 | `8a8de823d5ed` |
| 403:43 | `try_propagation` / `count` | E | C1 | `8a8de823d5ed` |
| 403:73 | `try_propagation` / `count` | D | 64-bit conversion / target scope | `8a8de823d5ed` |
| 404:9 | `if` / `count` | P | C1 | `935f68319d4f` |
| 411:36 | `try_propagation` / `key` | E | C1 | `8a8de823d5ed` |
| 415:9 | `if` / `done` | P | C1 | `935f68319d4f` |
| 448:9 | `if` / `object` | C | C4 | `935f68319d4f` |
| 448:12 | `short_circuit_or` / `object` | C | C4 | `cb11ceef493e` |
| 448:12 | `short_circuit_or` / `object` | P | C4 | `6c931886a408` |
| 457:52 | `try_propagation` / `object` | E | C4 | `8a8de823d5ed` |
| 458:9 | `if` / `object` | C | C4 | `935f68319d4f` |
| 458:12 | `short_circuit_or` / `object` | C | C4 | `c09c418424c9` |
| 458:12 | `short_circuit_or` / `object` | C | C4 | `ff6fed7900bf` |
| 458:12 | `short_circuit_or` / `object` | C | C4 | `9ccc6eafe37a` |
| 458:12 | `short_circuit_or` / `object` | P | C4 | `318ff90bad4d` |
| 466:91 | `try_propagation` / `object` | D | 64-bit conversion / target scope | `8a8de823d5ed` |
| 469:35 | `try_propagation` / `object` | E | C4 | `8a8de823d5ed` |
| 470:61 | `try_propagation` / `object` | E | C4 | `8a8de823d5ed` |
| 474:35 | `try_propagation` / `object` | E | C4 | `8a8de823d5ed` |
| 476:9 | `if` / `object` | P | C4 | `935f68319d4f` |
| 481:9 | `if` / `object` | C | C4 | `935f68319d4f` |
| 481:12 | `short_circuit_or` / `object` | B | C4 | `363b0c9c1711` |
| 482:17 | `short_circuit_and` / `object` | B | C4 | `089d7cd21007` |
| 486:43 | `try_propagation` / `object` | I | C4 | `8a8de823d5ed` |
| 496:33 | `try_propagation` / `object` | I | C4 | `8a8de823d5ed` |
| 498:52 | `try_propagation` / `object` | E | C4 | `8a8de823d5ed` |
| 500:51 | `try_propagation` / `object` | E | C4 | `8a8de823d5ed` |
| 501:9 | `if` / `object` | C | C4 | `935f68319d4f` |
| 501:12 | `short_circuit_or` / `object` | C | C4 | `5503de27a0d2` |
| 501:12 | `short_circuit_or` / `object` | P | C4 | `9413dc57684c` |
| 510:9 | `if` / `object` | P | C4 | `935f68319d4f` |
| 511:67 | `try_propagation` / `object` | E | C4 | `8a8de823d5ed` |
| 516:35 | `try_propagation` / `object` | I | C4 | `8a8de823d5ed` |
| 534:9 | `if` / `value_inner` | C | C4 | `935f68319d4f` |
| 534:12 | `short_circuit_or` / `value_inner` | P | C4 | `56a65bb1ad7f` |
| 544:9 | `if` / `value_inner` | C | C4 | `935f68319d4f` |
| 544:12 | `pattern_test` / `value_inner` | P | C4 | `5654d4106d70` |
| 545:13 | `if` / `value_inner` | I | C4 | `935f68319d4f` |
| 550:56 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 552:31 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 553:21 | `match` / `value_inner` | P | C4 | `4945a70fa7f9` |
| 555:40 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 556:28 | `match` / `value_inner` | P | C4 | `4945a70fa7f9` |
| 564:21 | `if` / `value_inner` | P | C4 | `935f68319d4f` |
| 569:19 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 571:84 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 571:86 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 573:72 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 576:17 | `for_iteration` / `value_inner` | E | C4 | `10c22bcf4c76` |
| 577:70 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 578:21 | `if` / `value_inner` | P | C4 | `935f68319d4f` |
| 582:56 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 582:77 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 583:56 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 585:47 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 588:72 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 590:17 | `for_iteration` / `value_inner` | E | C4 | `10c22bcf4c76` |
| 591:56 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 591:77 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 592:54 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 594:49 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 598:22 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 599:45 | `try_propagation` / `value_inner` | E | C4 | `8a8de823d5ed` |
| 624:9 | `if` / `data_inner` | C | C5 | `935f68319d4f` |
| 624:12 | `pattern_test` / `data_inner` | P | C5 | `5654d4106d70` |
| 625:13 | `if` / `data_inner` | C | C5 | `935f68319d4f` |
| 625:16 | `short_circuit_or` / `data_inner` | B | C5 | `bf802b7aa985` |
| 630:9 | `if` / `data_inner` | P | C5 | `935f68319d4f` |
| 637:55 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 639:36 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 640:43 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 641:9 | `if` / `data_inner` | C | C5 | `935f68319d4f` |
| 641:12 | `short_circuit_or` / `data_inner` | C | C5 | `d81ab9aabb76` |
| 641:12 | `short_circuit_or` / `data_inner` | P | C5 | `52858e16cb4b` |
| 644:64 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 647:9 | `for_iteration` / `data_inner` | E | C5 | `10c22bcf4c76` |
| 648:34 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 649:48 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 650:13 | `if` / `data_inner` | C | C5 | `935f68319d4f` |
| 650:16 | `short_circuit_or` / `data_inner` | C | C5 | `2a3bef87a17b` |
| 650:16 | `short_circuit_or` / `data_inner` | C | C5 | `7fbdac888097` |
| 650:16 | `short_circuit_or` / `data_inner` | P | C5 | `4e84f8a0644d` |
| 658:76 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 661:13 | `for_iteration` / `data_inner` | E | C5 | `10c22bcf4c76` |
| 662:66 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 663:17 | `if` / `data_inner` | P | C5 | `935f68319d4f` |
| 667:50 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 668:17 | `if` / `data_inner` | C | C5 | `935f68319d4f` |
| 668:20 | `short_circuit_or` / `data_inner` | P | C5 | `143610e5b1c7` |
| 671:52 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 671:65 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 679:18 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 688:14 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 690:22 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 691:50 | `try_propagation` / `data_inner` | E | C5 | `8a8de823d5ed` |
| 717:38 | `try_propagation` / `recover` | E | C3 | `8a8de823d5ed` |
| 718:9 | `if` / `recover` | C | C3 | `935f68319d4f` |
| 718:12 | `short_circuit_or` / `recover` | P | C3 | `37a4f9be6203` |
| 722:7 | `try_propagation` / `recover` | E | C6 + C3 | `8a8de823d5ed` |
| 725:42 | `try_propagation` / `recover` | E | C3 | `8a8de823d5ed` |
| 726:9 | `if` / `recover` | C | C3 | `935f68319d4f` |
| 726:12 | `short_circuit_or` / `recover` | P | C3 | `31c4e050f52c` |
| 729:9 | `if` / `recover` | C | C3 | `935f68319d4f` |
| 729:12 | `short_circuit_or` / `recover` | C | C3 | `ec439331f614` |
| 729:12 | `short_circuit_or` / `recover` | C | C3 | `6642ccf77ddb` |
| 729:12 | `short_circuit_or` / `recover` | C | C3 | `7ad2c0651c5f` |
| 729:12 | `short_circuit_or` / `recover` | C | C3 | `f6e9c74d5404` |
| 729:12 | `short_circuit_or` / `recover` | C | C3 | `3e6431f700fb` |
| 729:12 | `short_circuit_or` / `recover` | P | C3 | `a4f00646c53a` |
| 740:9 | `if` / `recover` | P | C3 | `935f68319d4f` |
| 744:7 | `try_propagation` / `recover` | E | C6 + C3 | `8a8de823d5ed` |
| 747:5 | `for_iteration` / `recover` | E | C3 | `10c22bcf4c76` |
| 750:69 | `try_propagation` / `recover` | E | C3 | `8a8de823d5ed` |
| 752:11 | `try_propagation` / `recover` | E | C6 + C3 | `8a8de823d5ed` |
| 756:73 | `try_propagation` / `recover` | E | C3 | `8a8de823d5ed` |
| 758:15 | `try_propagation` / `recover` | E | C6 + C3 | `8a8de823d5ed` |
| 765:35 | `try_propagation` / `recover` | E | C3 | `8a8de823d5ed` |
| 766:9 | `for_iteration` / `recover` | E | C3 | `10c22bcf4c76` |
| 767:13 | `if` / `recover` | C | C3 | `935f68319d4f` |
| 767:16 | `short_circuit_or` / `recover` | P | C3 | `be72c825f5bb` |
| 768:21 | `short_circuit_and` / `recover` | P | C3 | `632f62867d3f` |
| 773:9 | `if` / `recover` | P | C3 | `935f68319d4f` |
| 778:7 | `try_propagation` / `recover` | E | C6 + C3 | `8a8de823d5ed` |
| 792:68 | `try_propagation` / `recover` | E | C4 | `8a8de823d5ed` |
| 793:9 | `if` / `recover` | C | C4 | `935f68319d4f` |
| 793:12 | `short_circuit_or` / `recover` | P | C4 | `bb84ff7d8e75` |
| 797:36 | `try_propagation` / `recover` | E | C4 | `8a8de823d5ed` |
| 798:36 | `try_propagation` / `recover` | E | C4 | `8a8de823d5ed` |
| 799:41 | `try_propagation` / `recover` | E | C4 | `8a8de823d5ed` |
| 800:9 | `if` / `recover` | C | C4 | `935f68319d4f` |
| 800:12 | `short_circuit_or` / `recover` | P | C4 | `db0a1c357fe7` |
| 803:35 | `try_propagation` / `recover` | E | C4 | `8a8de823d5ed` |
| 804:9 | `if` / `recover` | P | C5 | `935f68319d4f` |
| 807:57 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 809:66 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 814:11 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 817:41 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 819:9 | `for_iteration` / `recover` | E | C5 | `10c22bcf4c76` |
| 820:34 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 821:49 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 822:38 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 823:54 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 824:13 | `if` / `recover` | C | C5 | `935f68319d4f` |
| 824:16 | `short_circuit_or` / `recover` | C | C5 | `50314ea88a6c` |
| 824:16 | `short_circuit_or` / `recover` | C | C5 | `cb15d174a983` |
| 824:16 | `short_circuit_or` / `recover` | C | C5 | `ddc1fa408af6` |
| 824:16 | `short_circuit_or` / `recover` | P | C5 | `27e2fc03ede7` |
| 834:77 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 835:13 | `if` / `recover` | P | C5 | `935f68319d4f` |
| 842:15 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 849:22 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |
| 851:62 | `try_propagation` / `recover` | E | C5 | `8a8de823d5ed` |

Totals: B=4, C=48, D=2, E=137, I=18, P=44; **253 records**.

## Executed validation

On macOS/aarch64, all-feature debug and release passed 27 codec unit tests,
14 durability tests and ten compaction tests (51 per profile). The two small
arithmetic/cursor and encoder-start/cache-counter tests additionally passed Miri
on the installed nightly. Final targeted reruns covered the cleared-witness
prefix sweep and empty CRC assertion. Strict all-feature library/test Clippy and
formatting passed. Runs used `CARGO_INCREMENTAL=0` and the reusable
`CARGO_TARGET_DIR=target/verification-diagnostics`.

No production edit, new mutation run, 32-bit execution, compiler MC/DC percentage,
or native hardware power-loss assurance is claimed. Prior REC-V1..7 mutation and
fuzz evidence is reused at its documented scope. The inventory completed without
parse failures; this tranche adds the five named tests and this source audit.

## VALUE identity index follow-up

The [production index qualification](../serialization-identity-index.md) adds
these source-level controls without relabeling the historical 253-record table:

| Decision | Directed evidence / disposition |
|---|---|
| `get`: empty/nonempty table | Initial empty lookup versus first insertion and clone hit; all test-build encoder lookups compared with the original linear search |
| `position`: empty/occupied bucket, same/different identity | Four directed last-bucket collisions wrap through slots 0..2, successful hits and a distinct allocation with equal bytes; growth and rebuild preserve lookup results |
| `reserve`: above `MAX_OBJECTS` | `MAX_OBJECTS+1` returns Limit before a zero allocation allowance is consulted; public images hit their region cap before a full maximum cache plus metadata can be emitted |
| `reserve`: entries within half capacity versus growth | Empty reserve, four entries in eight slots and fifth-entry rebuild; collision test also reverses cache order and adds a duplicate |
| `reserve`: new-table allocation failure | Growth OOM preserves old slots and exact live charge; 63/64-byte admission pair exercises Limit versus success for an eight-slot table on 64-bit targets; dropping the table returns charge to zero |
| rebuild: empty/populated cache and first/duplicate identity | Empty-cache budget test, reversed cache, duplicate first-match test; golden lifecycle appends after reopening a decoder-built cache |
| table size, mask, probe termination and index bounds | Invariants: cache entries bounded by `MAX_OBJECTS`, slots power-of-two and at least twice admitted entries, no deletion; every occupied slot refers to a live cache-owned Value. Multiplication/next-power arithmetic cannot overflow at that bound on supported 64-bit targets |
| delegated reserve/object/cache failures | Existing all-allocation commit/copy sweeps and crash/budget suites pass in Mac/Linux debug/release. Added table uses the already-tested `ChargedVec` allocator, not a new unsafe allocator |

Two index controls also pass Miri. Golden fingerprints and ten exact baseline
image comparisons cover emission order/sharing; the independent scaling driver
verifies full current/retained state. There is no new mutation score or compiler
MC/DC claim. Fresh inventory reports 2,571 total production records and 317
macro blind spots with no parse failures. Old source locations/dispositions
remain historical; this follow-up is the named evidence for the changed lookup.

## DATA revision index follow-up

The [DATA index report](../serialization-data-index.md) qualifies the later
sorted revision index without relabeling the earlier source-location table.

| Decision / algorithm | Evidence / disposition |
|---|---|
| `reserve`: above object bound | Maximum-sized table admission versus `MAX_OBJECTS+1` rejection; rejection precedes the zero-allocation allowance and size rounding |
| `reserve`: requested entries above current length | Empty/no allocation versus first reserve and growth; reuse within existing rounded capacity succeeds under zero allocation allowance |
| delegated charged reserve outcomes | Initial and growth OOM preserve prior order/live charge; exact initial one-slot and two-slot growth budgets distinguish Limit/success, including old/new overlap; drop returns charge to zero |
| sort / lower bound / equality | Unsorted cache, duplicate revision with different counters, empty cache, absent/present revisions, gaps, zero and u64 maximum; test-build encoder results checked against the original linear first match |
| insertion order / cache indices | Front/middle/end insertions and capacity growth, with cache remaining in original order. Rebuild sorts `(revision, original position)` to keep first-match behavior. Lower bound and slice rotation are delegated standard-library algorithms, not claims of full coverage of std internals |
| cached next-collection validation | Existing encoder counter success/Invalid control is unchanged. Duplicate test selects the first original counter; the new index cannot substitute a later entry |
| arithmetic and lifetime bounds | At most 16,384 entries; rounded capacity/subtraction and cache length plus one fit supported 64-bit sizes. No new pointers or unsafe ownership. All indexed cache entries remain live and cache indices do not move when index order rotates |
| preparation errors and durability | Existing all-allocation commit/copy sweeps, budget, crash/recovery and retained-root tests pass in Mac/Linux debug/release. No index allocation occurs after publication begins |

New 64-root golden history exercises metadata reuse, nonconsecutive revisions,
reopening, release, backup/compaction and subsequent append/retain. Seventeen
exact baseline image comparisons and the independent full-state scaling oracle
pass. Two index controls pass pinned Miri; seven instrumentation/launcher
controls pass normally and under optimized Python. Fresh inventory is 2,577
records with 318 macro blind spots and no parse failures. This is source-bound
guard/algorithm evidence, not a new mutation score or compiler MC/DC percentage.

## Buffer growth follow-up

Candidate `742079c` changes reservation policy while preserving the exact
17 baseline lifecycle/history images on Mac/Linux. Source SHA-256:
`fc5aded243d9859d5b97f65c5506a200448d335ccdf2e290d91bb27781c4a7fb`.
The historical source table and earlier index follow-ups keep their scope.

Writer `add` adds one predicate, `bytes.len() > self.bytes.len()`, and one net
additional propagation record. The new
`writer_bulk_appends_reserve_known_size_and_small_fields_reuse_capacity` test
appends 17 then 18 bulk bytes using exact live capacities 17/35, then one byte
using rounded capacity 64. Peak is 99 (35+64), all bytes survive, and another
append reuses capacity with allocation disabled. Drop returns live charge to
zero. It passes debug/release and Miri. Existing Writer length/maximum guards
remain mapped above; generic reserve/admission/null controls are delegated to
the [memory follow-up](memory.md#charged-geometric-growth-follow-up).

Encoder aggregate bytes and VALUE/DATA cache growth, decoder cache appends and
initial encoder cache copies now use the bounded charged helper. Known-size
final decoder DATA projection stays exact. These sites replace their existing
`?` callees; existing all-allocation candidate/copy sweeps, budget failure and
native/simulated recovery tests rerun against the new policy. Cache order,
sharing, checksums, padding, graph verification, authoritative root selection
and barriers are unchanged. The [compound comparison](../serialization-buffer-growth.md)
checks all roots and separate follower acknowledgment through the existing
core-package tests; it does not qualify distributed deployment timing or
exhaustively inject every new compound allocation site. New AST inventory is
2,584 records/51 files with 318 macro blind spots and no parse failures;
source records are not a compiler MC/DC percentage.
