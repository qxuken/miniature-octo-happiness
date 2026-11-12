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


The integrated audit snapshot contains **2,415 explicit records across 47 files
and 10 shipped targets**, with 284 macro/derive blind spots, 48 test exclusions
and no parse/path failures. Source-set SHA-256 is
`49f199c53ecc7ce7c646a4477415ef7b0ae8b1f0b82d005b74bd338091ae1597`.
A fresh independent check verified all 47 full-file hashes and 5,647 recorded
source/condition/exclusion/blind-spot ranges against actual bytes. Regeneration
output was `/tmp/weekend-final-inventory`; this temporary path is not a release
artifact retention promise. [Module audits](decisions/README.md) classify every
explicit record, including delegated and execution-only dispositions. Generated
FFI and derive code has supplemental evidence, not a compiler-expanded inventory.
The scanner itself still labels records unreviewed: manual dispositions reside
in linked ledgers, not an automatic coverage import. These counts are an audit
status, not MC/DC percentages or proof of every propagated failure.


## Authenticated snapshot CLI audit after 315068f

The current inventory adds **115** explicit records from the new app to the
2,415-record audit checkpoint: **2,530 records across 50 files**, with **310**
macro/derive blind spots and zero parse failures. Its source-set SHA-256 is
`695f67c71b81665b725555ac682d84eaae2994272683d2a73672187d7ef098cf`.
The full inventory is in `target/decision-inventory-snapshot-audit`.

The [snapshot CLI ledger](decisions/snapshot-cli.md) binds original 315068f and
current file hashes plus all 115 source spans. Runtime decisions are unchanged;
new tests prove that valid certificate-chain trust cannot bypass exact leaf pins
on either endpoint. It records 11 directed, 22 execution-only, 9 overlapping-syntax, 2 invariant
and 71 delegated dispositions and preserves
unfinished independence obligations for pending-write-only and client partial
handshake conditions, along with delegated OS/library/storage residuals.
A complete manual disposition map is not a complete MC/DC result. Focused macOS
debug/release/Clippy and Linux debug evidence is scoped in the ledger.

## Offline snapshot maintenance extension

The two CLI commands add 10 source records: two dispatch guards and eight
error propagations. The [app ledger](decisions/snapshot-cli.md) now maps all
125 records (11 directed, 24 execution-only, 9 overlapping, 2 invariant and
79 delegated). Global inventory: 2,540 records, 50 files, 314 macro/derive
blind spots, zero parse failures. Source-set SHA-256:
`133bdb4a94e46015da5aac537daaf358a28fb9b862d3a1bcea5319fb6da8b8d7`.
The inventory is in `target/decision-inventory-maintenance`; prior counts above
are historical. Wrapper tests reuse native core maintenance, without extending
its durability claims or treating delegated errors as independent conditions.

## Shared private TLS and coordinator pre-open admission

Current inventory: **2,557 records, 51 files, 316 macro/derive blind spots**, zero
parse failures. Snapshot remains 125 records; distributed apps now have 222.
Shared `apps/support/pinned_tls.rs` contributes 44 records for study and 44 for
snapshot, each with its own target identity; the stable per-profile constants
are not decision records. Coordinator adds pre-open file-type admission and
metadata propagation. Current source-set SHA-256: `e656b7667508b2ab60acc92dc5c1c35ffb9328ef45c254cdfd9dcb0168cf7595`.
Output: `target/decision-inventory-shared-tls`. All spans/hashes are rebound in
the app ledgers; delegated failures, hostile-path races, handshake independence
residuals and unexpanded macros remain explicit. These are dispositions, not
compiler-measured MC/DC or a claim that earlier 2490631 evidence covers new code.
