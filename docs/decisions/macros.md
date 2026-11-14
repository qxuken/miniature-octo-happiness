# Derive macro source and generated-code audit

The proc-macro crate has **108 production AST records**: Model 59, Record 29,
Resolution 20, and the three entrypoints zero. It also has **33 unexpanded macro
sites**, listed separately below. Neither count is a compiler MC/DC denominator:
quote templates contain runtime branches absent from the expansion-function AST,
and syn/quote/compiler internals are delegated dependencies. This tranche changes
only tests. P means directed predicate/arm controls; C overlapping syntax;
E execution/propagation evidence, not every callee error at every site; I an
invariant; G a specifically identified generated-code test gap.

## M1: Model input decisions

Reuse `model::tests::rejects_ambiguous_model_definitions_with_useful_diagnostics`
and `tests/compile_fail.rs::malformed_attributes_fail_in_downstream_compilation`.
New `model::decision_tests::metadata_and_field_options_have_rejected_and_valid_controls`
adds missing metadata, numeric overflow, a where-clause without generic parameters,
enum/unit shape, duplicate version/reserved/crate options, invalid crate path,
unknown metadata, duplicate field ID/unique/default/name, empty stored label,
stored-name collisions and uppercase generated-constant collisions. Positive
expansions include empty fields/reserved lists, unrelated attributes, renamed
crate paths, raw identifiers, unique/ordinary fields, implicit Default and an
explicit default expression. Successful output parses back as a syn::File.

The generic restriction OR has (false,false) accepted, (true,false) rejected for
a type parameter, and (false,true) rejected for a where clause. Active/reserved
field-ID collision similarly has neither collision accepted and each collision
separately rejected. Missing metadata/ID and duplicate options have named exact
diagnostics, rather than tests accepting any compile error. The shared nonzero
helper rejects zero and a value too large for u64. After named-field parsing,
field.ident cannot be absent; that defensive propagation is I for public derive
inputs. Malformed manually constructed syn ASTs are not public proc-macro input.

## M2: Record input and wire-order decisions

Reuse `record::tests::rejects_incomplete_or_ambiguous_wire_order` and downstream
compile-fail cases for omitted, duplicate and unknown fields. New
`record::decision_tests::record_options_and_struct_shape_reject_ambiguity` adds
missing key/format/order, duplicate fields/key/format/resolve/crate, unknown option,
wrong crate-literal type, enum/tuple/unit shapes and misplaced field attributes.
Positive controls include unrelated attributes, an empty named record, reordered
fields, renamed crate, an explicit resolution expression and generic/where-clause
preservation. Expressions for key/format/resolve deliberately remain Rust
expressions; their type correctness is checked by the downstream compiler, not
by a nonexistent macro-side branch.

## M3: Resolution selection

Reuse downstream diagnostic cases for struct-level resolve, unknown metadata
and duplicate resolve. New
`resolution::decision_tests::resolution_selection_and_attribute_locations_are_checked`
checks enum/tuple/unit shape, no selected fields, duplicate/wrong crate option,
field-level resolution metadata and non-path resolve attributes. Positive cases
exercise unselected fields, ignored attributes, renamed crate path, multiple
selected fields, generics and where clauses. Selection happens in declaration
order. Runtime resolution calls only the selected fields; Record's independent
wire-order list does not reorder Resolution's selection.

## M4: generated Record/Resolution behavior

`tests/generated.rs` uses the actual derives with a small independent local trait
API and instrumented one-byte field implementations. This isolates generated
control flow from engine behavior; it does not replace actual engine compatibility
checks. The three tests are:

- `emitted_codec_has_explicit_wire_order_and_exact_length_guards`: declaration
  order first/second/ignored becomes wire order second/first/ignored. Exact bytes,
  decode reconstruction, key/format/size, merge/check order and empty-record
  expansion are checked. Encode length 0/2/4 rejects Buffer for a three-byte
  record without touching output or invoking fields; decode rejects Corrupt.
- `emitted_calls_stop_at_the_first_field_error`: the second invoked field fails
  encode/decode/check_peer; later fields are not called and the exact error is
  propagated. Both direct Resolution and enabled automatic resolution propagate
  a selected field's error without invoking an unselected field.
- `automatic_resolution_flag_and_field_selection_are_independent`: the same
  const-generic record with AUTO=true resolves exactly first/second; AUTO=false
  invokes neither. Omitting the hook on Empty uses the trait default.

The probes explicitly observe partial mutation before an error: a prior encoded
prefix or resolved field can already have changed. Generated methods do not
promise transactional rollback themselves; the replica's staged candidate and
publication rules provide database atomicity. No new atomicity claim is inferred.

Independent inspection used
`cargo +nightly rustc -p weekend-db-macros --test generated -- -Zunpretty=expanded`.
The output confirms the exact field split order, calls and error propagation,
const-generic AUTO branch, selected-field order and empty implementations. The
two trailing debug assertions are invariants: SIZE is the sum of constant field
sizes, exact length is checked before splitting, and every listed field is
consumed once. A SIZE sum overflow is a downstream constant-evaluation error;
field codec behavior itself remains the application's contract.

Actual engine compatibility is independently exercised by `tests/crdt.rs`, using
StudyProgress derived from Record and Resolution. In particular,
`sync_resolves_both_fields_in_one_commit_and_repeat_sync_is_read_only` checks an
explicit byte-for-byte wire oracle, both policies, durable committed values,
reopen and no repeat-sync commit. `peers_commit_the_same_policy_result_offline_and_converge_on_one_goal`
uses AUTO=false and manual policy commits; `offline_peers_converge_on_one_record_with_a_set_and_counter`
checks composition. These controls complement the small independent probe.

## M5: generated Model behavior

Reuse the typed crate's `tests/models.rs` suite: typed CRUD/index lookup, preserved
unknown fields, migrations/retired IDs, stored/default/missing distinctions,
constant-name hygiene, custom decoder errors, primary/index allocation rollback,
and retained history across reopening. Relevant named controls include
`typed_crud_lookup_unknown_fields_history_restore_and_reopen`,
`semantic_migration_retires_ids_and_failure_rolls_back_every_record`,
`generated_field_constants_never_shadow_model_identity_in_expansions`,
`defaults_belong_to_stored_schema_and_presence_range_and_envelope_survive`, and
`missing_unique_defaults_remain_unindexed_through_crud_history_and_reopen`.
Generated schema/decode/patch helper errors are E: these suites exercise real
propagation, not every custom FieldValue failure at every generated field site.

`generated_model_patch_rejects_wrong_type_before_touching_the_record` directly
calls generated `Row::patch` against type 701 and type 702 records with identical
unknown-field payloads. Matching type updates the known field and preserves the
unknown field; mismatching type returns TypeMismatch before either field changes.
Both original immutable records remain unchanged. This closes the direct
true/false pair for the emitted patch type-ID guard in debug and release.
No production defect was observed. Crate renaming has an existing typed consumer
fixture (`tests/renamed`); the new Record/Resolution runtime probe compiles against
an explicit alternate crate path.

## Parsing, fuzz and scope

All three lib.rs entrypoints use parse_macro_input and convert expansion errors
to compile_error. The existing downstream harness tests actual compiler diagnostics;
it removes parent sanitizer/target flags only for nested compile-only commands,
while ordinary macro/generated runtime tests remain in the parent build profile.
Expansion tests check syn::Error text and valid output syntax separately.
`fuzz/src/derive.rs` imports the same production expansion files, runs raw/generated
bounded token inputs through all three derives, and reparses output or diagnostics.
Its replay is a bounded syntax/panic control, not semantic correctness or coverage
of every Rust grammar production. Macro allocation exhaustion, proc-macro host
internals, compiler type checking, syn/quote parsing and generated std trait code
remain delegated. No new mutation percentage or compiler MC/DC score is claimed.

## Source record inventory

### crates/weekend-db-macros/src/lib.rs

Source SHA-256: `5d0708c15d60b2cdd679fbc0a08a6504caf6f136b90bb32be1c271a63753b9f4`.

Zero explicit AST records; the three parser macro sites are mapped below.

### crates/weekend-db-macros/src/model.rs

Source SHA-256: `fa796e2ebcfbf9acfea6354b9b0ea7533b948c4a2252b5163e8d09b9a6e5c5f9`.

| Location | Kind | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 5:31 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 6:5 | `if` | P | M1 | `935f68319d4f` |
| 16:5 | `if` | C | M1 | `935f68319d4f` |
| 16:8 | `short_circuit_or` | P | M1 | `f582c8f42d08` |
| 27:5 | `for_iteration` | E | M1 | `10c22bcf4c76` |
| 28:9 | `if` | P | M1 | `935f68319d4f` |
| 30:17 | `if` | P | M1 | `935f68319d4f` |
| 31:21 | `if` | P | M1 | `935f68319d4f` |
| 34:56 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 34:65 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 34:67 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 35:24 | `if` | P | M1 | `935f68319d4f` |
| 36:21 | `if` | P | M1 | `935f68319d4f` |
| 39:56 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 39:65 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 39:67 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 40:24 | `if` | P | M1 | `935f68319d4f` |
| 43:21 | `for_iteration` | E | M1 | `10c22bcf4c76` |
| 43:87 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 44:46 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 45:25 | `if` | P | M1 | `935f68319d4f` |
| 50:24 | `if` | P | M1 | `935f68319d4f` |
| 51:21 | `if` | P | M1 | `935f68319d4f` |
| 55:40 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 55:59 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 55:68 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 60:15 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 68:7 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 70:95 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 71:18 | `match` | P | M1 | `4945a70fa7f9` |
| 72:28 | `match` | P | M1 | `4945a70fa7f9` |
| 96:5 | `for_iteration` | E | M1 | `10c22bcf4c76` |
| 100:82 | `try_propagation` | I | M1 | `8a8de823d5ed` |
| 107:9 | `for_iteration` | E | M1 | `10c22bcf4c76` |
| 108:13 | `if` | P | M1 | `935f68319d4f` |
| 110:21 | `if` | P | M1 | `935f68319d4f` |
| 111:25 | `if` | P | M1 | `935f68319d4f` |
| 114:55 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 114:64 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 114:66 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 115:28 | `if` | P | M1 | `935f68319d4f` |
| 116:25 | `if` | P | M1 | `935f68319d4f` |
| 120:28 | `if` | P | M1 | `935f68319d4f` |
| 121:25 | `if` | P | M1 | `935f68319d4f` |
| 124:40 | `if` | P | M1 | `935f68319d4f` |
| 125:41 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 125:50 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 129:28 | `if` | P | M1 | `935f68319d4f` |
| 130:25 | `if` | P | M1 | `935f68319d4f` |
| 134:45 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 134:64 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 135:25 | `if` | P | M1 | `935f68319d4f` |
| 142:19 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 147:11 | `try_propagation` | E | M1 | `8a8de823d5ed` |
| 148:9 | `if` | C | M1 | `935f68319d4f` |
| 148:12 | `short_circuit_or` | P | M1 | `40c83f105461` |
| 155:9 | `if` | P | M1 | `935f68319d4f` |
| 163:9 | `if` | P | M1 | `935f68319d4f` |
| 169:27 | `if` | P | M1 | `935f68319d4f` |

Totals: C=2, E=28, I=1, P=28; 59 records.

### crates/weekend-db-macros/src/record.rs

Source SHA-256: `328a45d7cfe315b3f1ff23e0627a8216ade91dcd265f62fae95a0c1b918e5ee8`.

| Location | Kind | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 11:5 | `for_iteration` | E | M2 | `10c22bcf4c76` |
| 12:9 | `if` | P | M2 | `935f68319d4f` |
| 16:13 | `if` | P | M2 | `935f68319d4f` |
| 17:17 | `if` | P | M2 | `935f68319d4f` |
| 22:88 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 25:13 | `if` | P | M2 | `935f68319d4f` |
| 26:17 | `if` | P | M2 | `935f68319d4f` |
| 29:47 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 29:66 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 29:75 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 32:43 | `if` | P | M2 | `935f68319d4f` |
| 34:20 | `if` | P | M2 | `935f68319d4f` |
| 36:20 | `if` | P | M2 | `935f68319d4f` |
| 41:13 | `if` | P | M2 | `935f68319d4f` |
| 44:38 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 44:47 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 46:11 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 49:84 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 50:93 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 53:7 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 55:18 | `match` | P | M2 | `4945a70fa7f9` |
| 56:31 | `match` | P | M2 | `4945a70fa7f9` |
| 67:5 | `for_iteration` | E | M2 | `10c22bcf4c76` |
| 68:9 | `if` | C | M2 | `935f68319d4f` |
| 68:12 | `pattern_test` | P | M2 | `5654d4106d70` |
| 82:5 | `for_iteration` | E | M2 | `10c22bcf4c76` |
| 83:9 | `if` | P | M2 | `935f68319d4f` |
| 92:88 | `try_propagation` | E | M2 | `8a8de823d5ed` |
| 96:5 | `if` | P | M2 | `935f68319d4f` |

Totals: C=1, E=14, P=14; 29 records.

### crates/weekend-db-macros/src/resolution.rs

Source SHA-256: `a5c39e16b0c6d0dcfe382c673240baf76323fec22e3296be98ea8ee7855fa178`.

| Location | Kind | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 8:5 | `for_iteration` | E | M3 | `10c22bcf4c76` |
| 9:9 | `if` | P | M3 | `935f68319d4f` |
| 12:9 | `if` | P | M3 | `935f68319d4f` |
| 14:17 | `if` | P | M3 | `935f68319d4f` |
| 17:17 | `if` | P | M3 | `935f68319d4f` |
| 21:42 | `try_propagation` | E | M3 | `8a8de823d5ed` |
| 21:61 | `try_propagation` | E | M3 | `8a8de823d5ed` |
| 21:70 | `try_propagation` | E | M3 | `8a8de823d5ed` |
| 23:15 | `try_propagation` | E | M3 | `8a8de823d5ed` |
| 27:18 | `match` | P | M3 | `4945a70fa7f9` |
| 28:31 | `match` | P | M3 | `4945a70fa7f9` |
| 46:5 | `for_iteration` | E | M3 | `10c22bcf4c76` |
| 48:9 | `for_iteration` | E | M3 | `10c22bcf4c76` |
| 49:13 | `if` | P | M3 | `935f68319d4f` |
| 55:13 | `if` | P | M3 | `935f68319d4f` |
| 56:17 | `if` | P | M3 | `935f68319d4f` |
| 59:17 | `if` | P | M3 | `935f68319d4f` |
| 64:9 | `if` | C | M3 | `935f68319d4f` |
| 64:12 | `pattern_test` | P | M3 | `5654d4106d70` |
| 68:5 | `if` | P | M3 | `935f68319d4f` |

Totals: C=1, E=7, P=12; 20 records.

## Supplemental unexpanded sites

These 33 sites are not added to the AST count as independent conditions. Quote
rows identify templates; repeated fields instantiate repeated helper calls. P/E
evidence applies to the described controls, not every possible application type.

| Source / line | Macro site | Evidence / disposition | Token hash |
| --- | --- | --- | --- |
| `lib.rs`:17 | `parse_macro_input!` | E: parser/token construction; downstream compile-fail | `73939c86d38d` |
| `lib.rs`:24 | `parse_macro_input!` | E: parser/token construction; downstream compile-fail | `73939c86d38d` |
| `lib.rs`:31 | `parse_macro_input!` | E: parser/token construction; downstream compile-fail | `73939c86d38d` |
| `model.rs`:25 | `syn::parse_quote!` | E: parser/token construction; M1 | `b6f6a38ab136` |
| `model.rs`:42 | `syn::parenthesized!` | E: parser/token construction; M1 | `a3e84d63a760` |
| `model.rs`:43 | `syn::Token!` | E: parser/token construction; M1 | `1f34ee315310` |
| `model.rs`:124 | `syn::Token!` | E: parser/token construction; M1 | `ec94d1182185` |
| `model.rs`:127 | `syn::parse_quote!` | E: parser/token construction; M1 | `789dbe154ced` |
| `model.rs`:159 | `format_ident!` | E: parser/token construction; M1 | `f86d7445ec5d` |
| `model.rs`:171 | `quote!` | E: generated Model helpers (M5) | `98430fdd12d6` |
| `model.rs`:172 | `quote!` | E: generated Model helpers (M5) | `c2c27e36112b` |
| `model.rs`:174 | `quote!` | E: generated Model helpers (M5) | `3c016c0d55e7` |
| `model.rs`:179 | `quote!` | E: generated Model helpers (M5) | `fc6b32f3bb8b` |
| `model.rs`:180 | `quote!` | E: generated Model helpers (M5) | `8e20484883bd` |
| `model.rs`:183 | `quote!` | E: generated Model helpers (M5) | `8a40083feedc` |
| `model.rs`:184 | `quote!` | E: generated Model helpers (M5) | `8e20484883bd` |
| `model.rs`:185 | `quote!` | E: generated Model helpers (M5) | `d75c01262ef7` |
| `model.rs`:186 | `quote!` | E: generated Model helpers (M5) | `f664439c4cfc` |
| `model.rs`:187 | `quote!` | E: generated Model helpers (M5) | `69faec179bda` |
| `model.rs`:188 | `quote!` | E: generated Model helpers (M5) | `4df9878d3f2e` |
| `model.rs`:192 | `quote!` | P: patch type-ID guard; E: other Model helpers (M5) | `105f2abd49d7` |
| `record.rs`:21 | `syn::parenthesized!` | E: parser/token construction; M2 | `33138714caab` |
| `record.rs`:22 | `syn::Token!` | E: parser/token construction; M2 | `1f34ee315310` |
| `record.rs`:48 | `syn::parse_quote!` | E: parser/token construction; M2 | `5866fbf0cedf` |
| `record.rs`:103 | `quote!` | P/E: generated field calls/guards (M4) | `d19c09764d72` |
| `record.rs`:105 | `format_ident!` | E: parser/token construction; M2 | `807c94085f85` |
| `record.rs`:108 | `quote!` | P/E: generated field calls/guards (M4) | `b8d5cf7ce1b6` |
| `record.rs`:118 | `quote!` | P/E: length guards/field propagation; I: exact-consumption assertions (M4) | `ae36477f9849` |
| `resolution.rs`:6 | `syn::parse_quote!` | E: parser/token construction; M3 | `5866fbf0cedf` |
| `resolution.rs`:56 | `matches!` | E: parser/token construction; M3 | `6454d0382464` |
| `resolution.rs`:72 | `quote!` | P/E: generated field calls/guards (M4) | `d19c09764d72` |
| `resolution.rs`:75 | `quote_spanned!` | P/E: generated field calls/guards (M4) | `ea116ba2b12e` |
| `resolution.rs`:78 | `quote!` | P/E: generated field calls/guards (M4) | `422bf928652f` |

Locations are line:column; token hashes are the first 12 SHA-256 digits from the
source inventory. Whole-file hashes include test-only additions; production
source is unchanged. No target-specific production cfg branches exist in these
expansion functions; cfg(test) modules are excluded from the source denominator.

## Executed results

On macOS/aarch64, all ten macro tests passed in debug and release: five expansion
unit tests, two downstream diagnostic/harness tests and three generated-runtime
tests. Strict macro-crate all-target Clippy and formatting passed. The final
resolution-error assertion was rerun in both profiles. Real-consumer debug runs
passed 24 typed Model tests and 11 CRDT integration tests. Derive fuzz replay
passed six tests (the five imported unit tests plus its bounded replay). These
consumer/replay runs were not newly repeated in release for this tranche.

All builds used `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/verification-diagnostics`.
Independent expanded output was inspected from the installed nightly; syntax
inventory completed with zero parse failures. No production implementation edit,
new long fuzz campaign, new mutation run, or whole-project readiness claim is made.
The generated fixture source SHA-256 is `ef6e20402ab96be325f1ed46758270595a9d197de4ee7951af44868fcef57ec3`.

## Direct expansion attribution follow-up (032dadf)

The [direct expansion guard report](../macro-expansion-guards-032dadf.md) adds
`tests/expansion_guards.rs` without changing any production source hash or AST
inventory span. Its four exact-diagnostic pairs cover Model's duplicate type ID
(`model.rs:31`) and Resolution's struct-level marker, unknown option and duplicate
field marker (`resolution.rs:9`, `:14`, `:59`). All four already had downstream
compile-fail controls; the new harness makes both outcomes attributable to the
same directly instrumented expansion functions, addressing the zero outcomes in
the separate macro unit-executable view without mislabeling them globally untested.

Seven harness/module tests pass debug/release on macOS and Linux, strict targeted
Clippy passes, and the downstream diagnostic suite remains passing. Four
body-preserving guard mutations each compile and fail the corresponding test.
Warning-free same-object condition profiles show true/false counts 1/29, 1/6,
1/5 and 1/7 respectively. Imported `tests/../src` paths require the dedicated
report's explicit module/span attribution; no project or crate percentage, nor
compiler MC/DC result, is inferred. Source hashes and exact commands/evidence
are bound in the linked report.
