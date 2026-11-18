# Current metadata admission and root-selection decisions

This R1 batch starts at `2d1f81eb704dc45163b6432dfed141d8b4b38aaa`.
The production codec and its tests are unchanged. Reuse REC-V1/V3/V6 from
[DESIGN.md](../../DESIGN.md#rec-v6-validate-metadata-before-choosing-a-root)
and the historical [C3 ledger](codec.md#c3-metadata-and-root-selection), while
binding this batch to current spans and the current full-file hash below.
The earlier [arithmetic rows](codec-arithmetic-current.md) keep their exact pin.

## Current source pairs

| Control | Actual cases and outcomes | Independence scope |
|---|---|---|
| REC-LENGTH | A scripted backend reports below APPEND_START or above MAX_IMAGE_BYTES: Corrupt before any read. A supported fixture opens revision 2. | A=below minimum, B=above maximum: FF accepts; TF and FT reject. No oversized allocation. |
| REC-FORMAT | Valid magic/version opens; corrupt either separately to receive UnsupportedFormat before reading slots. | FF accepts; TF and FT reject. |
| REC-HEADER | Wrong checksum, reserved word, region size, append start, image cap, object cap or padding rejects at the header. Each non-checksum case repairs CRC. | Seven atoms: all false versus each single true, with all other atoms held false. |
| REC-IDENTITY | Valid identity opens; zero header identity with repaired CRC rejects before slot reads. | Both outcomes of the one-atom guard. |
| META-ENCODE-KIND / META-PARSE-KIND | The valid fixture emits and reads both root-slot and witness magic. Changing either producer or parser branch alone rejects it. | Both outcomes of each `witness` selection; other conditions in the valid fixture stay valid. |
| META-EMPTY / META-CHECKSUM | Zeroed or checksum-invalid newest witness leaves revision 2; losing both newest records permits revision 1; a surviving newest witness makes a lost newest slot Corrupt. | Absent versus valid, checksum-invalid versus valid. CRC failure means absent metadata, while checksummed structural damage rejects. |
| META-SHAPE | Slot and witness each reject wrong magic, version, physical index, identity or padding after CRC repair. | Five atoms: all false versus each single true. Rejection must stop in that metadata region. |
| META-LAYOUT-AB / META-LAYOUT-ABC | Wrong parity with repaired predecessor; wrong predecessor; end above physical length with descriptor/end relation preserved. | Prefixes with two and three atoms have all-false versus each single-true pairs. The complete eight-atom layout guard remains partial. |
| REC-WITNESS | Same root pair with one older witness opens; newer witness rejects; matching witness opens; same revision/different descriptor and end rejects. | A=newer, B=same revision, C=different Slot. F/F/T accepts, T/F/T rejects, F/T/F accepts, F/T/T rejects. First/second isolate A, first/fourth isolate B, third/fourth isolate C. |
| REC-ADJACENCY | Saved real slots: (2,5) rejects; (missing,5) opens 5; (2,missing) opens 2; (4,5) opens 5; (4,3) opens 4; (missing,missing) rejects. | The AST sees one `is_some_and` call condition. Existing presence/gap pairs and both newest parities are supplemental evidence for its delegated combinators. |
| REC-ROOT-EXISTS | The same slot matrix preserves all tail objects: one or two roots succeeds; no roots returns Corrupt. | Scoped Option error/success propagation, not a Boolean independence pair. |

The tests are `recovery_checks_image_length_before_reading_metadata`,
`recovery_rejects_each_invalid_header_field_before_reading_slots`,
`absent_or_torn_metadata_obeys_the_surviving_witness`,
`checksummed_slot_and_witness_metadata_must_have_valid_fields`,
`recovery_requires_witnesses_to_agree_with_the_selected_root` in the codec,
and `recovery_selects_the_newest_available_root_only_if_the_slots_agree` in
`tests/durability.rs`. Rejections preserve the complete source bytes and perform
no write/sync. Metadata cases check the last read's region, so a later graph or
witness rejection cannot mask omission of an earlier admission check.
These are directed source pairs; compiler MC/DC is not measured.

## Partial, coupled and delegated records

Do not promote the full layout guard to a complete pair. Its cap condition is
dominated: physical length is already capped, and `end > length` precedes
`end > MAX_IMAGE_BYTES`. When `descriptor + REGION == end`, the two alignment
predicates have the same truth value. Changing just one alignment while keeping
the other alignment and the descriptor/end relation false is impossible.
The tests exercise rejection, but these three atoms lack independent
accept/reject pairs on public open. The full guard and prefixes containing the
dominated cap retain B dispositions. The two shorter prefixes have only the
independent parity/predecessor/physical-length atoms.

C rows identify overlapping syntax under the documented parent pairs; they are
not additional independent branches. The fixed `0..2` read loop is I: exactly
two slot/witness positions, then exit. The witness iterator E row has empty,
one-witness and ordinary two-witness execution. Root-selection propagation E
covers no-root, witness mismatch, nonadjacent and successful selection. Backend
I/O propagation, other graph/cache rows, iterator internals, generated code and
compiler/platform obligations remain outside this batch.

## Guard mutations and execution scope

The 35 added `r1-metadata-` controls reuse these tests. They omit each independent
header/shape/layout atom, distinguish absence/checksum behavior, invert the
producer/parser magic branch separately, alter each witness condition, and
restore the six root-selection faults: missing-slot requirements, either fixed
parity preference, omitted adjacency, and wrong no-root error.
No omission control is claimed for the three blocked layout atoms.
The registry now contains 118 cases, 62 with `r1-`; its first 83 are unchanged.
The standalone-manifest contract test selects its named inventory case rather
than relying on the registry's last entry.

```sh
cargo +1.99.1 xtask mutations --case r1-metadata- --toolchain 1.99.1 --output target/metadata-debug-new
cargo +1.99.1 xtask mutations --case r1-metadata- --release --toolchain 1.99.1 --output target/metadata-release-new
```

The initial dirty-source debug prototype detected all 35 controls. It is a
feedback loop, not frozen platform qualification. Full frozen profile/platform
results are recorded only after they finish. Every baseline must be nonempty;
mutants must compile and fail a real test with exit 101. Timeouts, compile errors
and cleanup failures cannot qualify. Source restoration remains mandatory.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `34baf455a1e1ad7e8d76c932ed417fe60c324bbeaad64efd0ca398205012472d` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| P | weekend-db::weekend_db | src/storage/codec.rs:124:30– | 4099..4101 | if | metadata | Directed source pair above |
| P | weekend-db::weekend_db | src/storage/codec.rs:143:5– | 4674..4676 | if | parse_metadata | Directed source pair above |
| P | weekend-db::weekend_db | src/storage/codec.rs:146:5– | 4744..4746 | if | parse_metadata | Directed source pair above |
| P | weekend-db::weekend_db | src/storage/codec.rs:149:17– | 4837..4839 | if | parse_metadata | Directed source pair above |
| P | weekend-db::weekend_db | src/storage/codec.rs:150:5– | 4890..4892 | if | parse_metadata | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:150:8– | 4893..4952 | short_circuit_or | parse_metadata | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:150:8– | 4893..4997 | short_circuit_or | parse_metadata | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:150:8– | 4893..5034 | short_circuit_or | parse_metadata | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:150:8– | 4893..5081 | short_circuit_or | parse_metadata | Overlapping syntax under the documented parent pair |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:5– | 5286..5288 | if | parse_metadata | Partial layout guard: dominated cap and coupled alignment atoms |
| P | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5395 | short_circuit_or | parse_metadata | Directed source pair above |
| P | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5424 | short_circuit_or | parse_metadata | Directed source pair above |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5469 | short_circuit_or | parse_metadata | Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5520 | short_circuit_or | parse_metadata | Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5569 | short_circuit_or | parse_metadata | Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5627 | short_circuit_or | parse_metadata | Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5699 | short_circuit_or | parse_metadata | Partial layout guard: dominated cap and coupled alignment atoms |
| P | weekend-db::weekend_db | src/storage/codec.rs:1132:9– | 38699..38701 | if | recover | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1132:12– | 38702..38765 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| P | weekend-db::weekend_db | src/storage/codec.rs:1140:9– | 39004..39006 | if | recover | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1140:12– | 39007..39067 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| P | weekend-db::weekend_db | src/storage/codec.rs:1143:9– | 39138..39140 | if | recover | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39141..39225 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39141..39277 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39141..39335 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39141..39396 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39141..39453 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39141..39505 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| P | weekend-db::weekend_db | src/storage/codec.rs:1154:9– | 39643..39645 | if | recover | Directed source pair above |
| I | weekend-db::weekend_db | src/storage/codec.rs:1161:5– | 39818..39821 | for_iteration | recover | Fixed two-position loop; callee and compiler internals remain delegated |
| E | weekend-db::weekend_db | src/storage/codec.rs:1179:35– | 40649..40650 | try_propagation | recover | Scoped root-selection propagation or iterator execution above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1180:9– | 40660..40663 | for_iteration | recover | Scoped root-selection propagation or iterator execution above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1181:13– | 40721..40723 | if | recover | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1181:16– | 40724..40830 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1182:21– | 40777..40829 | short_circuit_and | recover | Overlapping syntax under the documented parent pair |
| P | weekend-db::weekend_db | src/storage/codec.rs:1187:9– | 40921..40923 | if | recover | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1192:7– | 41096..41097 | try_propagation | recover | Scoped root-selection propagation or iterator execution above |
