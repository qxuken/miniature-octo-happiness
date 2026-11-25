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
