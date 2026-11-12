# Direct macro expansion guard qualification

Four zero outcomes in the separate macro unit-executable view prompted this
follow-up. All four already had real downstream compilation rejection controls
in `crates/weekend-db-macros/tests/compile_fail.rs`. Their absence from a parent
unit-executable view was therefore an attribution gap, not proof of missing
behavioral tests across the project.

The new `tests/expansion_guards.rs` imports the production `model.rs` and
`resolution.rs` modules unchanged, and calls their expansion functions directly
in one test process. It adds four accepted/rejected pairs with exact diagnostic
strings; successful token output must parse as a `syn::File`. Importing these
modules also includes their three existing tests, giving seven tests total.
There is no copied implementation, production refactor or source-span change.
Actual downstream rustc diagnostic tests remain separate and unchanged.

## Pairs and measured outcomes

| Production location | Rejected / accepted control | True / false count |
|---|---|---|
| `model.rs:31:24–41`, prior `type_id` exists | Two `type_id` declarations / one declaration, otherwise valid metadata | 1 / 29 |
| `resolution.rs:9:12–43`, struct attribute is `resolve` | Struct `#[resolve]` / valid field-only marker | 1 / 6 |
| `resolution.rs:14:20–48`, resolution option is not `crate` | `unknown="crate::api"` / `crate="crate::api"`, otherwise identical valid field | 1 / 5 |
| `resolution.rs:59:20–58`, field already marked | Two field markers / one marker | 1 / 7 |

Exact diagnostics are respectively `duplicate type_id`, `put #[resolve] on a
field`, `expected crate`, and `duplicate #[resolve]`. The unknown-option case
uses a valid string value so ignoring the unknown name cannot merely hit an
unrelated missing-value parse error. Each predicate has both outcomes in the
same production expansion-function instantiation; counts also include reused
module tests.

## Execution and mutation controls

Pinned Rust 1.99.1 / LLVM 23.1.1, macOS arm64 and isolated Docker Linux aarch64:
seven tests passed in both debug and release, with strict targeted Clippy on
both. The Linux source was a clean archive of
`032dadf591b0a0ded4894898e8208283c50425fb` plus only the new test file; source
mounts were read-only, builds offline/locked and networking disabled. Image:
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
macOS's existing downstream compile-fail suite also passed unchanged.

Four mutations were tested separately in a disposable source copy. Each changed
only its selected guard from `if predicate` to `if (predicate) && false`, leaving
the rejection body and predicate evaluation intact. This also preserves the
`replace` side effect in the duplicate-field check. Each directed test compiled,
then failed its runtime assertion (exit 101); no compile error was accepted as
a killed mutation. Every mutated copy was restored afterward. These are narrow
sensitivity checks, not exhaustive mutation or compiler MC/DC qualification.

## Warning-free same-object attribution

A separate clean base archive plus the same test overlay was built with dated
Rust nightly `2026-10-09` (`a30aa9064df6a504b6a323c6884a2f4cfbfff666`, LLVM
23.1.3), `-C instrument-coverage -Z coverage-options=condition`. Build-time
profiles were removed before the seven tests ran. Exactly one runtime profile
was merged and reported against its own executable. Matching LLVM report,
export and HTML commands all returned zero with empty stderr.

Rust records imported source filenames as `tests/../src/model.rs` and
`tests/../src/resolution.rs`. A blanket `/tests/` filename exclusion would remove
these production file views, so this directed report excludes only external
registry/toolchain paths. It includes the harness and imported unit-test code.
The table above selects exact production-module spans and hashes; no percentage
is claimed for the crate or workspace. Generated runtime code, proc-macro host
execution in compiler children, Record expansion and other platforms retain
their separate evidence and obligations.

## Source and evidence binding

| File | SHA-256 |
|---|---|
| New `tests/expansion_guards.rs` | `77a4cbb9645813ff733a3a41ad6995751838dc2e1c280a11e0d32ae91f784e4b` |
| Unchanged production `src/model.rs` | `fa796e2ebcfbf9acfea6354b9b0ea7533b948c4a2252b5163e8d09b9a6e5c5f9` |
| Unchanged production `src/resolution.rs` | `a5c39e16b0c6d0dcfe382c673240baf76323fec22e3296be98ea8ee7855fa178` |

Local evidence: `target/macro-expansion-032dadf/evidence.json`, raw/indexed
profiles, `tests.log`, report/export/HTML outputs and empty diagnostic files;
`target/macro-expansion-mutations/evidence.json` and four failed-test logs;
`/tmp/macro-expansion-linux.json` / `.log`; and macOS
`/tmp/macro-expansion-guards-{debug,release,clippy}.log` plus
`/tmp/macro-expansion-compile-fail.log`. These are local records, not a permanent
artifact retention promise. No remote CI, publication or native Windows test
was run in this tranche.

Stable targeted commands (with `CARGO_INCREMENTAL=0` and isolated target paths):

```sh
cargo test --offline --locked -p weekend-db-macros --test expansion_guards
cargo test --offline --locked -p weekend-db-macros --test expansion_guards --release
cargo clippy --offline --locked -p weekend-db-macros --test expansion_guards -- -D warnings
cargo test --offline --locked -p weekend-db-macros --test compile_fail
```
