# Model derive schema decisions

The direct expansion tests vary struct and field options and inspect the
emitted type and version constants, crate paths, index constants, stored
labels, defaults and reserved IDs. The typed crate also checks the
independent golden descriptor through database reopen. The downstream
rustc suite exercises the public derive on named, tuple, unit, enum and
union declarations.

## Condition pairs

| Decision | False or success case | True or different-input case |
| --- | --- | --- |
| Numeric value is zero | A positive stable ID and version expand | Type ID, version, reserved ID and field ID each reject zero |
| Struct attribute is `weekend` | An unrelated attribute leaves the emitted metadata intact | The schema options are parsed |
| Struct option selects `type_id`, `version`, `reserved` or `crate` | Other supported options reach their own handlers | The chosen option changes its corresponding output |
| Version, reserved ID or crate override already present | Each first assignment is accepted | Each repetition produces its own diagnostic |
| Field attribute is `weekend` | Unrelated attributes leave the output intact | The field options are parsed |
| Field option selects `id`, `unique`, `default` or `name` | The other valid options reach their own handlers | Each option produces its own declaration or descriptor data |
| Field ID, unique marker, default or name already present | Each first assignment is accepted | Each repeated option produces its own diagnostic |
| Default expression follows `=` | Bare `default` emits `Default::default()` | The explicit expression appears in the generated field codec |
| Explicit name is empty | A stored name is emitted | An empty string rejects |
| Stored name is already present | Distinct stored labels expand | Repeating a label gives the stored-name diagnostic |
| Uppercased field constants collide | Distinct constants expand | The `a` and `A` fields with distinct stored labels give the constant collision diagnostic |
| Field is unique | The constant is a numeric field ID | The constant is a typed index and belongs to the emitted unique-ID set |

The earlier owned-struct, duplicate-type and active-versus-reserved-ID
controls retain their existing pairs.

## Propagation boundaries

The success controls supply both schema options and a complete field ID.
Each new propagation mutation replaces the diagnostic at one boundary.
The expected error text identifies the particular failing input.

| Boundary | Failure inputs |
| --- | --- |
| Integer conversion | One greater than `u64::MAX` reports overflow |
| Type ID, version and field ID parsers | A missing `=`, an invalid integer token and zero reach the three separate error returns for each option |
| Reserved IDs | A noninteger list item and zero reach separate returns |
| Crate path | Missing `=`, a boolean and a string with trailing path tokens reach distinct returns |
| Struct and field attributes | A name-value `weekend` attribute reaches the outer parser for each position |
| Required options | Missing type ID, version or field ID produces its own diagnostic |
| Explicit default | Missing expression after `=` reports a parser error |
| Stored name | Missing `=` and a nonstring value reach separate returns |

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `crates/weekend-db-macros/src/model.rs` | `fa796e2ebcfbf9acfea6354b9b0ea7533b948c4a2252b5163e8d09b9a6e5c5f9` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/model.rs:27:5– | 887..890 | for_iteration | expand | No attributes reports missing metadata. One attribute and split directives supply it. An unrelated attribute preserves it. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/model.rs:43:21– | 1696..1699 | for_iteration | expand | Empty, singleton, two distinct and repeated reserved IDs exercise iteration and early rejection. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/model.rs:71:18– | 2848..2853 | match | expand | Named structs expand. Downstream rustc reports the struct-only diagnostic for enums and unions. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/model.rs:72:28– | 2894..2899 | match | expand | Named fields expand. Downstream rustc reports the named-fields diagnostic for tuple and unit structs. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/model.rs:96:5– | 3588..3591 | for_iteration | expand | Empty, single-field and multifield models exercise traversal and distinct subsequent-field collisions. |
| I | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/model.rs:100:82– | 3758..3759 | try_propagation | expand | Public macro input passes through the locked syn 2.0.119 parser. Each field in `Fields::Named` is built by `Field::parse_named` with `ident: Some(ident)`. This expander leaves it intact before reading it. |
| E | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/model.rs:107:9– | 4018..4021 | for_iteration | expand | An unattributed field reaches missing ID. Single, split and mixed attributes exercise option accumulation and filtering. |
| I | weekend-db-macros::weekend_db_macros | crates/weekend-db-macros/src/model.rs:125:41– | 4961..4962 | try_propagation | expand | The immediately preceding test peeks `Token![=]` on this stream. The locked syn 2.0.119 `ParseNestedMeta::value` consumes that same token. No parser action advances the stream between the peek and the consume. |

The two invariants apply to parser-produced inputs. Compiler code generated
by `quote!`, the separate fuzz target and dependency internals retain
their own qualification scope.
