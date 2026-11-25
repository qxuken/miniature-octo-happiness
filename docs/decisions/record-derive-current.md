# Record derive admission and wire options

The expansion fixtures check the generated trait path, key and format
constants, and presence of the optional resolution method. The existing
generated runtime tests compare explicit wire order and exact buffer lengths
with independently implemented field codecs. They also exercise field call
order, error propagation and the option to run automatic resolution.

| Predicate | False or pass case | True or alternate case |
| --- | --- | --- |
| Struct attribute is unrelated | A `record` directive supplies the options | An `allow` attribute is skipped |
| Option is `fields` | A valid crate or expression option follows its own path | The explicit wire list is parsed |
| Field list is already present | One list supplies the wire fields | A second list gives `duplicate fields list` |
| Option is `crate` | Default paths use `::weekend_db` | An override emits the implementation through `crate::api` |
| Crate path is already present | One override expands | A second override gives `duplicate crate path` |
| Option is `key` | Other valid options reach their own parsers | The expression becomes the `KEY` constant |
| Option is `format` | Other valid options reach their own parsers | The expression becomes the `FORMAT` constant |
| Option is `resolve` | An unknown option gives the list of supported options | The expression enables emission of the resolution method |
| Selected expression slot already holds an option | The first key, format or resolution option is accepted | A second assignment gives `duplicate record option`, including across attributes |
| A field carries a `record` attribute | An unrelated field attribute is allowed | The derive asks for `record` on the struct |

Each propagation control changes the diagnostic at one return. The valid
control supplies the key, format and complete wire list. Additional success
controls exercise a valid custom crate path and more than one attribute.
The outer parser has a separate test so the mutation reaches the
attribute-shape assertion directly.

| Return | Input showing the original diagnostic |
| --- | --- |
| Field identifier list | `fields(3)` reaches the identifier parser. A separate case with bare `fields` exercises the parenthesis helper. |
| Crate value separator | Bare `crate` requires `=`. |
| Crate string literal | `crate = true` requires a string literal. |
| Parsed crate path | `crate = "crate::api!"` reports an unexpected token. |
| Expression value separator | Bare `key` requires `=`. |
| Expression parser | A terminal `key =` reports the missing expression. |
| Outer attribute parser | `#[record = "invalid"]` requires parentheses. |
| Key presence | Format and field list without a key report the missing key. |
| Format presence | Key and field list without a format report the missing format. |
| Wire list presence | Key and format without a field list report the missing wire order. |

The three earlier controls cover duplicate, unknown and omitted fields in
the wire list. A successful permutation changes the encoded order, as checked
by the generated runtime fixture and the StudyProgress wire oracle.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `crates/weekend-db-macros/src/record.rs` | `328a45d7cfe315b3f1ff23e0627a8216ade91dcd265f62fae95a0c1b918e5ee8` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/record.rs:11:5– | 404..407 | for_iteration | expand | No attributes gives the missing-key diagnostic. One and two directives supply options and reject repetition. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/record.rs:55:18– | 2260..2265 | match | expand | A named struct emits the implementation. Downstream rustc rejects an enum and a union with the struct-only diagnostic. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/record.rs:56:31– | 2310..2315 | match | expand | Named fields expand. Downstream rustc rejects tuple and unit structs with the named-fields diagnostic. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/record.rs:67:5– | 2679..2682 | for_iteration | expand | Empty, single-field and multifield structs exercise the placement check. A field-level record directive stops expansion. |
| C | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/record.rs:68:12– | 2712..2715 | pattern_test | expand | The pattern is the sole condition of the enclosing decision at byte 2709. The placement test and mutation exercise both outcomes. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/record.rs:82:5– | 3063..3066 | for_iteration | expand | Empty, singleton and permuted lists exercise traversal. Repeated and unknown entries reject before generation. |

These records apply to the proc-macro target. Generated code, dependency
internals and separately compiled contexts have their own evidence.
