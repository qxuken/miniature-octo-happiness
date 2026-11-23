# Derive admission and generated layout

The direct expansion fixtures import the production `Model`, `Record` and
`Resolution` expanders. Valid controls produce token streams that parse as
Rust items. Invalid controls return the exact diagnostic. The downstream
compile-fail suite separately exercises rustc with the public derives.

## Source decisions

| Derive | Input pair or independently changed condition | Result |
| --- | --- | --- |
| `Model` | No parameters or where clause, then type parameters alone or a concrete where clause alone | Each independent operand rejects with the owned nongeneric diagnostic. |
| `Model` | New field ID, duplicate active ID, or reserved ID | Each collision rejects before field generation. |
| `Model` | One or two `type_id` declarations | The second declaration rejects. |
| `Record` | Valid permutation `b,a`, then `a,a`, `a,c`, or `a` | Duplicate, unknown and omitted fields produce distinct diagnostics. |
| `Resolution` | Valid selected field, then control-specific invalid syntax | A struct-level marker, unknown option, duplicate marker, and no selected fields each reject. |

Twelve `r1-derive-` controls compile in an isolated source copy and fail their
named expansion tests. Each condition in the Model OR guards is changed
separately. The manifest binds ten source controls in the proc-macro target.
Two additional OR syntax records share their enclosing decisions. The fuzz
crate's separately compiled target retains its own inventory obligations.

## Generated behavior

The new `Model` fixture compares its complete generated descriptor with an
independent raw map built from literal IDs, kind tags, stored labels, flags,
the default and the reserved-ID list. It then opens the typed database and
reads that exact descriptor from its schema collection using a literal
little-endian key. The fixture also checks the unique-index field ID and the
generated constants.

The existing `Record` and `Resolution` generated-code harness checks explicit
wire order, exact length, error propagation, and selection of automatic
resolution fields. The actual engine CRDT test compares derived StudyProgress
wire bytes against its independent oracle. It exercises both resolution
policies, durable reopen, and no additional commit on repeat sync.

## Mutation registry regressions

At base `316194b`, the command-construction test failed on the literal count
871 with 928 entries present. It now selects `memory-depth-zero` by name and
checks the command for that fixture. The expanded registry has 940 entries.
The package allowlist supports the known `weekend-db-macros` workspace target.
The new command test checks the exact test binary, release flag and filter.

The registry source matcher also found two occurrences of the original
transaction scan expression after `Snapshot::scan_iter` was added. The legacy
transaction control selects occurrence 1 and expects two matches. The newer
snapshot control selects occurrence 2. Both controls are included in the
verification campaign.

## Linux command fixture failure

The initial full Linux release run at `9aa6afb` failed while reading the fake
Cargo build trace. Adding stderr to the assertion in a disposable replay
showed two stressed `check` cases with the same missing trace and an
`ETXTBSY` launch error. A controlled build replay held a writable descriptor
in a sibling process and reproduced the launch error deterministically.
Closing that descriptor let the fake Cargo command log its call and exit 29.

The command tests now link their temporary `cargo` path to a checked-in
executable script. They therefore create no writable descriptor for the inode
they will execute. The exact Cargo arguments, nonzero exit, and check
short-circuit assertions still run. A missing trace now includes child stderr.

## Carried tooling evidence

The package allowlist edit moves two earlier selector records. Their
`select_cases` function bytes agree with the previous source, so their
earlier host and prefix selection evidence applies at the shifted locations.

| Source | SHA-256 |
| --- | --- |
| `xtask/src/mutations.rs` | `9a72b1ed112f56db1d27e67e351bd9559f15cfc6160fa385be986f4b759c7153` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | xtask::xtask | xtask/src/mutations.rs:298:20– | 9712..9717 | match | select_cases | Linux, macOS, and an unknown-host fallback are exercised. |
| E | xtask::xtask | xtask/src/mutations.rs:306:21– | 9941..10010 | short_circuit_or | select_cases | Empty, matching and nonmatching prefixes are exercised before sharding. |

## Verification

The test and mutation source was `e40727b4df78f646ad652c4e2c9a053cd4c4464a`.
Rust 1.100.0 ran the locked checks in both profiles on arm64 macOS and arm64
Docker Linux with source and build directories on ext4.

| Check | macOS per profile | Linux per profile |
| --- | --- | --- |
| Macro and typed crate suites | 90 passed, 1 ignored | 90 passed, 1 ignored |
| Core CRDT integration suite | 14 passed | 14 passed |
| xtask | 246 passed, 3 ignored | 275 passed, 1 ignored |
| Named mutation campaign | 14 detected | 14 detected |

The mutation campaigns include twelve derive controls and the two scan
controls. Eight Linux workers each ran the three release command tests 130
times with zero failures. Strict Clippy passed on both hosts. The macro and
typed targets also passed MSVC cross-target Clippy. Strict-provenance Miri
passed the golden typed descriptor fixture on macOS. Native Windows
execution remains open.

The macOS same-object expansion fixture at `9aa6afb` recorded branch counts
with nightly `708c94ef0` in condition coverage mode. Its expander and
expansion-test files are identical at the verified source above.

| Condition in the production expander | True | False |
| --- | ---: | ---: |
| Model has generic parameters | 2 | 37 |
| Model has a where clause | 2 | 35 |
| Model field ID repeats an active ID | 2 | 14 |
| Model field ID matches a reserved ID | 2 | 12 |
| Record lists a duplicate field | 2 | 12 |
| Record omits a field | 2 | 3 |
| Resolution selects no fields | 2 | 6 |

## Reproduce

```sh
cargo +1.100.0 test --offline --locked --all-features \
  -p weekend-db-macros -p weekend-db-typed
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --test crdt
cargo +1.100.0 test --offline --locked -p xtask
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-derive- \
  --case r1-observe-scan-scan-iterator-owned \
  --case r1-typed-index-allocator-snapshot-scan-status \
  --output target/r1/derive-admission-debug
```

Repeat with `--release` and a separate mutation output directory.
