# Resolution derive decisions

The direct expansion fixtures keep the selected field and surrounding
declaration valid while varying one option or marker. The emitted-token
checks compare the complete generated implementation. The downstream rustc
suite checks the macro's public diagnostics for unsupported declaration shapes.

| Decision | False or success case | True or rejection case |
| --- | --- | --- |
| Struct attribute is `resolution` | Absent or unrelated attribute emits all paths through `::weekend_db` | The override emits the trait, actor, result and calls through `crate::api` |
| Crate option already seen | One override expands | Repetition in a single attribute or across two attributes gives `duplicate crate path` |
| Field attribute is `resolution` | Marked field with an unrelated attribute expands | Moving the override onto that field gives the struct-placement diagnostic |
| Field attribute is `resolve` | A doc attribute leaves the field out of the generated calls | Adding a bare marker after the doc attribute includes it in declaration order |
| Marker is anything other than a bare path | Bare marker expands | List and name-value forms give `expected #[resolve]` |
| Field has a recorded marker span | Unmarked field is omitted | Marked field appears exactly once |

The success control for the four parser returns is the complete
`#[resolution(crate = "crate::api")]` directive. The failure inputs reach
distinct stages. Missing `=` fails before the literal is parsed. A boolean
fails the string-literal parser. A string ending in `!` fails path parsing.
A name-value attribute fails the outer nested-meta parser. Each propagation
control changes the diagnostic at the named boundary. The assertion checks
the error text that the caller receives.

The four earlier Resolution controls cover a struct-level marker, an unknown
option, repeated field markers, and an empty selection. The existing
generated-code and engine CRDT tests cover calls in declaration order,
short-circuit on a field error, two-field convergence, durable reopen, and
stable repeat sync.

## Scoped decisions

| Source | SHA-256 |
| --- | --- |
| `crates/weekend-db-macros/src/resolution.rs` | `a5c39e16b0c6d0dcfe382c673240baf76323fec22e3296be98ea8ee7855fa178` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/resolution.rs:8:5– | 325..328 | for_iteration | expand | Zero, one and two struct attributes exercise default selection, custom paths and the error on a second crate option. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/resolution.rs:27:18– | 1011..1016 | match | expand | A named struct emits an implementation. Downstream rustc tests reject an enum and a union with the struct-only diagnostic. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/resolution.rs:28:31– | 1061..1066 | match | expand | Named fields expand. Downstream rustc tests reject tuple and unit structs with the named-fields diagnostic. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/resolution.rs:46:5– | 1545..1548 | for_iteration | expand | Empty fields reject with the empty-selection diagnostic. One and three fields emit only the marked calls. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/resolution.rs:48:9– | 1607..1610 | for_iteration | expand | Unattributed, doc-only, marked and mixed fields exercise traversal. Replacing an unrelated attribute with a misplaced directive gives the field diagnostic. |
| C | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/resolution.rs:64:12– | 2273..2276 | pattern_test | expand | The pattern is the sole operand of the enclosing field-enrollment decision at byte 2270. Its truth values and generated calls share the same control. |

These rows apply to the proc-macro target. The separately compiled fuzz
context, dependency parsing internals and other generated configurations
retain their own obligations.

## Verification

Frozen source `9813c75bdec7a30ab02bb56d669a08709673736e` passed with locked
Rust 1.100.0 on arm64 macOS and offline arm64 Docker Linux with ext4 build
and runtime directories. Each debug and release run passed 100 macro and
typed crate tests with one existing skip, and all 14 CRDT integration tests.
The CRDT suite includes two resolved fields in one commit and read-only
repeat sync. Strict Clippy passed on both hosts.

Each campaign detected all 14 Resolution mutations, including the four
earlier controls. The particular expansion test passed on pristine source
and failed under its mutation. Fresh extraction matched all 942 copied
files per campaign against the frozen Git tree and verified each isolated
source restoration, exact test result and process cleanup. The
[archive checksums](resolution-derive-9813c75.sha256) bind 236 command
reports, the runners and the replay inputs.

The macOS `expansion_guards` executable built with nightly `739adecd4`
and `-C instrument-coverage -Z coverage-options=condition` passed 22 tests.
Both outcomes appear at every reported condition in the expander. Replaying
the extracted profile with the saved executable reproduced the LLVM export
byte for byte.

| Condition in `resolution.rs` | True | False |
| --- | ---: | ---: |
| 9, struct marker | 1 | 22 |
| 12, struct crate directive | 19 | 3 |
| 14, unknown option | 1 | 19 |
| 17, repeated crate option | 3 | 16 |
| 49, field crate directive | 2 | 32 |
| 55, field marker | 28 | 4 |
| 56, invalid marker form | 4 | 24 |
| 59, repeated field marker | 1 | 23 |
| 64, recorded field marker | 21 | 5 |
| 68, empty selection | 2 | 19 |

The inventory still has 10,770 records. This batch accounts for 16 runtime
records with six predicate pairs, four propagation controls, five scoped
executions and one coupled pattern. Reconciliation leaves 630 runtime and
7,514 tooling gaps. The compiler counts apply to the macOS expansion test
binary. Native Windows execution and the separate fuzz target remain open.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p weekend-db-macros -p weekend-db-typed
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 --case r1-derive-resolution- \
  --output "$OUTPUT/debug"
cargo xtask mutations --toolchain 1.100.0 --case r1-derive-resolution- \
  --release --output "$OUTPUT/release"
```

Repeat the test commands with `--release` for optimized test execution.
