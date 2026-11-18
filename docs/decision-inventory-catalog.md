# Repository Cargo decision inventory

The R1 follow-up starts at PR #7 head
`b32a868d3c66d9f18545d568006066e00b2045d0`. It expands source discovery;
it does not close the individual MC/DC obligations.

## Discovery and review contract

`docs/decisions/workspaces.json` explicitly lists all 21 maintained Cargo
manifests. Cargo metadata deduplicates those packages into fourteen workspaces.
The tool runs `cargo metadata --offline --locked --no-deps --format-version 1`
for each distinct workspace and never builds the inspected targets. A fixture
with a panicking build script verifies that property; missing lockfiles reject
without creating them. Empty-registry metadata discovery also succeeds for the
actual fuzz workspace. This enumerates source inputs, not resolved dependency
graph validity or compiled feature coverage.

For a Git checkout, the catalog must exactly match tracked `Cargo.toml` files.
A broken local `.git` rejects instead of silently selecting a smaller scope.
Archives use a filesystem walk, excluding `.git`, `target` and `probe/build`;
symlink directories cannot establish that filesystem scope. An archive inside
an unrelated parent Git checkout still uses its own filesystem scope. File
aliases, escaping paths, duplicate entries and missing manifests reject.
Manifest, lockfile and catalog bytes are checked again after metadata discovery.
Qualification still requires a frozen source; these checks do not provide an
atomic snapshot of a concurrently edited repository.

Without the catalog, the generic CLI explicitly reports `root_workspace`.
It makes no claim about independent workspaces elsewhere in the repository.
Both modes include libraries, procedural macros, binaries, examples, benches
and build scripts. Integration-test targets are listed as excluded until
`--include-tests` is selected. That optional mode also includes unit-test bodies
and cannot be combined with a production review.

The current catalog has 66 non-test Cargo targets and 46 explicitly excluded
integration-test targets. All twelve prior target IDs remain unchanged. Name
collisions use workspace manifest, package, kind and target name; packages in
independent workspaces and a same-named library/binary cannot collapse.
Required features, entry files and package/workspace identities are retained.

All live Cargo reports require review schema 2. Schema 1 remains readable for
historical source-only reports and their existing contract tests. Schema 2
pins the complete Cargo input scope and target catalog, assigns a scope to
every target and initializes every target's record count to zero. Twenty-two
fuzz wrappers have no visible AST decisions but remain listed with their
unexpanded macro blind spots. Zero is not a coverage or execution claim.

Record scope comes from its target. A runtime source imported into an example
or an oracle retains its file-group references but receives that target's
tooling scope. `source_scope` preserves the file group's original scope for
inspection. Historical rows additionally require an explicitly eligible target,
an unchanged full-file pin and an exact row identity. The initial allowlist is
the twelve prior targets. New contexts start with individual G dispositions;
a shared filename/hash cannot transfer runtime coverage or historical credit.
The eleven named source controls retain their exact target selectors.

The scanner rejects a shared file changing between target scans while retaining
its first provenance. A regression reproduces the previous overwrite. Git root
paths preserve spaces, including Unix trailing spaces. Those input properties
are separate from compiler expansion, native/dependency behavior and MC/DC.

Campaign input snapshots also bind both catalog JSON files and the inventory
and replica-progress-fixture workspaces, including their manifests, locks,
sources and tests. The copied-source map already bound these files; the added
regression checks that edits also change the original-input snapshot used at
campaign completion. This prevents the standalone runner extension from relying
on a smaller input-stability map.

## Reproduction

```sh
cargo +1.99.1 fetch --locked --manifest-path tools/decision-inventory/Cargo.toml
cargo +1.99.1 test --offline --locked --manifest-path tools/decision-inventory/Cargo.toml
cargo +1.99.1 test --offline --locked --release --manifest-path tools/decision-inventory/Cargo.toml
cargo +1.99.1 clippy --offline --locked --manifest-path tools/decision-inventory/Cargo.toml --all-targets -- -D warnings
cargo +1.99.1 run --offline --locked --manifest-path tools/decision-inventory/Cargo.toml -- . target/r1/catalog-review --review docs/decisions/current-scope.json
cargo +1.99.1 run --offline --locked --manifest-path tools/decision-inventory/Cargo.toml -- . target/r1/catalog-tests --include-tests
cargo +1.99.1 xtask mutations --case r1-inventory- --toolchain 1.99.1 --output target/r1/catalog-mutants-debug
cargo +1.99.1 xtask mutations --case r1-inventory- --release --toolchain 1.99.1 --output target/r1/catalog-mutants-release
```

Use fresh output paths. The [mutation runner](mutations.md) now accepts the
inventory crate's specific manifest for these registered cases, including
package cleanup and both profiles. Its nonempty-baseline, exit-101 assertion,
timeout, output and retirement checks remain required. Compiler failures and
uncaught mutations cannot pass.

| New control | Changed behavior | Regression |
|---|---|---|
| `r1-inventory-example-kind` | Treat an example as an excluded test target | Cargo catalog includes examples/build scripts/zero-record targets |
| `r1-inventory-target-count` | Initialize target counts to one | Catalog review retains the exact zero-record target denominator |
| `r1-inventory-historical-target` | Reuse rows in an ineligible target | Shared-file context keeps individual gaps |
| `r1-inventory-target-scope` | Use file scope instead of target scope | Shared runtime file receives tooling scope in the oracle target |

These four functional guard controls extend the thirteen existing R1 mutants;
they do not promote new inventory records to condition-independence pairs.
The historical 56-case fixture provenance retains its original scope.

## Checkpoint evidence

The expanded production inventory currently has 10,482 records in 215 files,
with 4,946 macro/attribute blind spots and no parse failures. The checked review
manifest supplies the final source bindings and per-target counts. Current
runtime and driver counts remain 3,023 and 156. New tooling contexts remain
unqualified until their individual records are reconciled.

Before/after logs for discovery, standalone mutation selection, shared-source
drift and trailing-space roots are under `target/r1/standalone-logs/`.
At frozen candidate `6024a8c`, macOS arm64/APFS passes 31 inventory tests and
238 xtask tests/two existing registered ignores in each of debug and release.
Both crates pass Clippy; inventory and review JSON are byte-identical between
the two tool profiles. All seventeen R1 mutants are caught in each profile.
Independent comparison verifies all 777 copied payloads and 336 then-declared
inputs against the immutable Git blobs. That candidate precedes the additional
snapshot bindings above; the superseding frozen-source qualification records
its own enlarged input map and test counts.

The initial Linux attempt stopped before tests because its older vendor archive
lacked the standalone crate's pinned `serde_json` 1.0.149. Regenerating the locked,
offline vendor set for the root and inventory workspaces fixes the input seed
without changing any lockfile or bypassing offline checks. Its archive SHA-256 is
`7775e0b19c5b40db61946a31589855313dc6c91c12a46c96a1076a0cc80bddf6`.
Linux and hosted native follow-ups are recorded after their actual completion.
CI runs discovery/reconciliation contracts in both
profiles on macOS, Linux and Windows, preserving committed bytes and using
fail-fast Bash steps. R1 mutation campaigns run on the two Unix lanes; Windows
retains its separate native RNG inventory.

All Cargo packages remain unpublished. Full per-decision reconciliation,
expanded/generated code, delegated native behavior and the remaining R2–R7
release gates stay open.
