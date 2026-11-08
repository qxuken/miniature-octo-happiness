# Source decision inventory

The isolated `tools/decision-inventory` utility inventories explicit Rust source
control flow in the workspace's library, proc-macro and binary targets. It is a
review denominator and audit aid, **not an MC/DC measurement or proof**. Every
record initially says `unreviewed`, including records in modules with existing
[manual decision pairs](../DESIGN.md#mcdc-decision-ledger).

From the repository root:

```sh
cargo run --offline --locked --manifest-path tools/decision-inventory/Cargo.toml -- . target/decision-inventory
cargo run --offline --locked --manifest-path tools/decision-inventory/Cargo.toml -- . target/decision-inventory-with-tests --include-tests
cargo test --offline --locked --manifest-path tools/decision-inventory/Cargo.toml
cargo test --offline --locked --release --manifest-path tools/decision-inventory/Cargo.toml
cargo clippy --offline --locked --manifest-path tools/decision-inventory/Cargo.toml --all-targets -- -D warnings
```

Dependencies must already be cached for offline execution. The tool has its own
locked workspace, is unpublished, and does not alter the root manifest. It reads
sources and runs `cargo metadata --offline --locked --no-deps`; it does not compile
the inspected workspace or execute its code. It writes/replaces `inventory.json`
and `inventory.md` in the chosen output directory. Do not place valuable files
with those names there.

## Scope and interpretation

Cargo metadata supplies each workspace member's library, proc-macro and binary
entry point. Traversal follows external modules, inline modules and literal
`#[path]` declarations. This includes core, typed facade, derive implementation,
apps and the registered admin/benchmark/file-worker binaries. The StudyProgress
support source is included because a shipped app imports it by path. Standalone
examples, integration tests, benchmarks, build scripts, dependency source and
isolated fuzz/tools workspaces are outside this inventory. The optional test mode
adds unit-test branches inside these same targets; it does not add Cargo test
or example targets.

The default evaluates `test` as false, propagating `all`, `any` and `not` through
three-valued cfg expressions. Platform, feature and other cfg predicates remain
unknown: potentially production branches from **all** configurations are visited,
including Linux, macOS and Windows adapters on any host. Required Cargo target
features are recorded without activating them. `--include-tests` makes `test`
unknown too, retaining both test and non-test source branches. This is a union
of source configurations, not a particular compiler build's denominator.

The following syntactic decision records are emitted:

| Kind | Source boundary |
|---|---|
| `if`, `while` | Condition selects body or alternative/exit |
| `for_iteration` | Iterator produces another item or finishes |
| `match` | Scrutinee selects among listed source patterns |
| `match_guard` | Guard accepts an arm or continues matching |
| `short_circuit_and`, `short_circuit_or` | RHS evaluated or skipped |
| `try_propagation` | `?` continues or propagates a residual |
| `pattern_test` | `let` expression matches or fails |
| `let_else` | Binding matches or enters its `else` branch |

A condition expression is split recursively at `&&`, `||` and parentheses. For
`if a && (b || c)`, the `if` record lists `a`, `b`, `c`; the logical operators
also have their own records. Those overlapping records are intentional and must
not be summed as unique machine branches or independent MC/DC obligations.
Negations, function calls and macro invocations remain opaque atoms. For match,
iteration, pattern and `?` records, `conditions` stores the relevant expression;
it does not assert that expression is a Boolean MC/DC condition.

## Machine output and identity

The JSON schema version is `1`. The top-level report contains:

- `tool_version`, `tool_source_sha256`: tool package version and exact main source
  identity; reproduce with its checked-in Cargo.lock as well.
- `files`: repository-relative source paths mapped to full-file SHA-256 hashes.
  `source_set_sha256` hashes the compact JSON serialization of that sorted map.
- `targets`: package/target names, Cargo target kinds, required features and
  repository-relative entry files.
- `decisions`: stable ordered records with target, owner function name, kind,
  source location, candidate conditions, alternatives, inherited cfg strings,
  `review: "unreviewed"`, and module-level manual ledger anchors.
- `exclusions`: cfg/test-filtered source entries; one exclusion can cover an
  entire module, so this is not a count of omitted decisions.
- `blind_spots`: macro/derive invocations, conditional attributes and unresolved
  modules requiring follow-up; `failures`: unreadable/unparseable files, explicit
  path failures, recursive inclusion or paths outside the source root.

A location has a repository-relative file, zero-based half-open byte range,
one-based start/end line and column, exact source text, and text SHA-256. Columns
are the parser's character columns; byte offsets are authoritative for slicing
UTF-8 source. A decision's location is its keyword/operator, or logical
expression; its condition locations carry the full candidate condition text.
The ID combines target, path, byte range and kind. It is deterministic for a
fixed source snapshot, not a durable semantic identifier after source edits.
Changes elsewhere in the file invalidate its full-file hash even when a keyword
hash remains unchanged. The owner field is a convenience function label, not a
fully qualified Rust symbol. The target/path/range disambiguates it.

`inventory.md` summarizes kind counts and every visited module's unreviewed
records, including zero-count modules. Ledger links use repository-root-relative
`DESIGN.md#...` paths; resolve them from the repository root if reading reports
outside it. Links identify relevant **module-level** manual evidence only. They
do not match records to decision pairs, mark coverage, or evaluate infeasibility.

## Limitations requiring explicit review

This parses pre-expansion source with `syn`; it does not expand macros, derives,
`include!` or `cfg_attr`. Invocations and derives are surfaced as blind spots,
including procedural macro quote bodies. Generated branches require compiler
expansion or a separately reviewed generator/template inventory. Custom attribute
macros are not interpreted. Attributes on items, expressions, methods, local
bindings and match arms are handled; associated constants and enum variant/field
attributes are not independently evaluated. Match alternatives list source
patterns, including cfg-filtered arms; they are not compiler reachability claims.

Literal `#[path]` is resolved relative to the containing file. The more complex
Rust path rules for `#[path]` inside inline modules and cfg-selected path
attributes are not modeled; none occurs in the measured production traversal.
Missing/ambiguous normal external modules are recorded as blind spots; explicit
missing paths and parse failures cause a nonzero exit after writing the partial
report. Zero failures is therefore insufficient to declare an inventory complete:
review blind spots too.

Implicit branches (bounds checks, allocation behavior, destructors, trait/operator
implementations, compiler lowering, panic paths) are outside this source inventory.
Irrefutable patterns are retained syntactically rather than proved infeasible.
The tool does not establish short-circuit masking, independence, branch execution,
test-to-condition traceability, or a coverage percentage. Those require manual
classification and instrumented evidence. To advance the denominator, review each
record as covered by named decision pairs, needing tests, infeasible with rationale,
or outside an explicitly justified safety scope; expand the blind spots first.

## Initial evidence (2026-11-09)

For source-set SHA-256
`ebb8157361edf4c6334f737385ebd0584ec0e050c9f7c8edab024699971e5638`, the production
union contains **2,380 unreviewed records, 45 files, 10 targets**, 18 cfg/test
exclusions, 206 unexpanded macro invocations and 68 unexpanded derives. There are
zero parse/path failures and zero unresolved-module or conditional-attribute blind
spots in this particular traversal. The unit-test-inclusive union contains 2,568
records, 48 files and 696 blind spots, also with zero failures.

Production record counts are 496 `if`, 10 `while`, 122 iterator loops, 96 matches,
36 match guards, 39 pattern tests, 19 let-else bindings, 39 `&&`, 144 `||`, and
1,379 `?` propagations. All remain unreviewed by this tool; existing manual ledgers
continue to provide only their documented scoped evidence.

Four fixture tests pass in debug and release: compound condition decomposition
and source identity; nested/path module traversal and cfg inheritance;
three-valued cfg evaluation; and visible derive/cfg_attr/missing-module blind
spots plus parse failure and UTF-8 source offsets. Strict Clippy passes. Two
consecutive production runs produced byte-identical JSON and Markdown. An
independent Python check verified all 5,523 production source/condition/exclusion/
blind-spot ranges and SHA-256 values against actual file bytes, all full-file
hashes and unique decision IDs. Regenerate after code changes; these counts are a
snapshot, not a permanent coverage assertion.


After allocation accounting, extracted application parsers and verification
diagnostics, the inventory contains 2,408 explicit records across 47 files and
10 targets, with 284 macro/derive blind spots, 28 test exclusions and no parse
failures. The recorded source-set hash is
`c3bb8892b935eb14e6399d6a7e54a1dea0a110ed68a164cdf73bcae655b858d7`;
output is `target/decision-inventory-current`. [Module audits](decisions/README.md)
classify 584 of these explicit records, with scoped proofs and delegated gaps.
This is an audit status, not a coverage percentage or an expanded FFI denominator.
