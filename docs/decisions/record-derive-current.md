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

## Verification

Frozen source `db36b40276218332968adf0af473f69c9b143071` passed with
locked Rust 1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4.
Each debug and release run passed 106 macro and typed crate tests with
one existing skip, and all 14 CRDT tests. Strict Clippy passed on both hosts.

Each campaign detected all 23 Record mutations, including the three
earlier wire-list controls. The initial archive replay showed that the
outer-parser mutation changed a nested error before the attribute-shape
assertion ran. A dedicated attribute-shape test now isolates that case.
Fresh extraction verified the exact named tests, both diagnostic strings
for each of the ten propagation mutations, and all 944 copied source files
per campaign. The [archive checksums](record-derive-db36b40.sha256) bind
320 command reports, their runners and the compiler profile.

The macOS `expansion_guards` executable built with nightly `739adecd4`
and `-C instrument-coverage -Z coverage-options=condition` passed 28 tests.
Its profile and executable reproduced the LLVM export byte for byte after
extraction.

| Condition in `record.rs` | True | False |
| --- | ---: | ---: |
| 12, unrelated struct attribute | 4 | 54 |
| 16, fields option | 49 | 122 |
| 17, repeated fields option | 2 | 47 |
| 25, crate option | 12 | 110 |
| 26, repeated crate option | 2 | 10 |
| 32, key option | 52 | 58 |
| 34, format option | 49 | 9 |
| 36, resolve option | 7 | 2 |
| 41, repeated expression option | 7 | 101 |
| 68, record attribute on a field | 2 | 26 |
| 83, repeated wire field | 2 | 22 |
| 96, incomplete wire order | 2 | 13 |

The 26 source records consist of ten predicate pairs, ten propagation
controls, five scoped executions and one coupled pattern. The inventory
retains 10,770 records, with 604 runtime and 7,514 tooling gaps. Native
Windows and the separately compiled fuzz context remain open.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p weekend-db-macros -p weekend-db-typed
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 --case r1-derive-record- \
  --output "$OUTPUT/debug"
cargo xtask mutations --toolchain 1.100.0 --case r1-derive-record- \
  --release --output "$OUTPUT/release"
```

Repeat the test commands with `--release` for optimized test execution.
