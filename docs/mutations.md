# Safe guard mutation qualification

Run the private Rust controller from the repository root:

```sh
cargo xtask mutations --list
cargo xtask mutations --case tls- --output target/tls-mutations-new
cargo xtask mutations --case r1- --output target/r1-mutations-new
cargo xtask mutations --case r1- --release --output target/r1-mutations-release-new
cargo xtask mutations --case r1-inventory- --toolchain 1.99.1 --output target/r1-inventory-mutations-new
cargo xtask mutations --output target/all-mutations-new
```

On Unix, the 56 historical guard mutations retain their original expressions,
occurrence counts, package/test selection and replacement text in
`xtask/tests/mutations.json`. Its provenance binds the retired Python controller.
Thirteen [R1 controls](decisions/current.md) extend that registry to 69 cases,
reusing typed T11, CRDT C4 and snapshot R4/R5/R6 tests and adding direct admin
export controls and the existing TX-V1 validation-mode table. The historical fixture provenance applies to the original
56; the current reconciliation additionally pins the complete updated fixture.
Four [Cargo inventory controls](decision-inventory-catalog.md) bring the current
registry to 73 cases. They check example inclusion, zero target counts, historical
target eligibility and per-target scope. Their only supported standalone
manifest is `tools/decision-inventory/Cargo.toml`, paired with package
`weekend-decision-inventory`; root-package selections retain their prior commands.
Both test execution and package cleanup use that same registered manifest.
Ten [codec arithmetic and gap-classification controls](decisions/codec-arithmetic-current.md)
bring the registry to 83 cases (27 with the `r1-` prefix). Nine exercise alignment,
cursor boundaries and error propagation; one restores the historical-G promotion
defect. The short-tag and zero-limit regression supplies missing propagation
controls. Each case retains the same bounded runner and genuine test-failure
requirements. This finite batch does not qualify the complete codec or tool.
The [metadata/root-selection batch](decisions/codec-metadata-current.md) adds
35 `r1-metadata-` controls using existing tests, bringing the registry to 118
cases and the `r1-` prefix to 62. Header and metadata admission, separate magic
branches, witness comparison and all six root-slot schedules receive controls.
The three blocked layout atoms retain their partial dispositions; no omission
detection is claimed for them. The first 83 registry entries remain unchanged.
The standalone-manifest contract test selects its named case explicitly, so
appending root-package cases cannot change which manifest rejection is tested.
All source expressions are checked before an output or source copy is acquired.
Unix selections can be divided with `--shard-index I --shard-count N` after
prefix filtering. Defaults 0/1 retain the complete selection. The recorded
matching/selected names must form a disjoint, complete union across all shards;
one passing shard cannot qualify the complete registry. Invalid or empty shards
reject before output acquisition. The [CI shard contract](r1-ci-shards.md)
records the observed deadline failure and preserves all command and cleanup
requirements. The native Windows RNG controller remains unsharded.
The [object reader and recovery I/O batch](decisions/codec-graph-current.md)
adds 33 `r1-graph-` cases for a total of 151 cases, 95 with the `r1-` prefix.
The new extent regression requires rejection before the read, and the overlap
controls exercise both ordinary occupied regions and cached value starts.
The [COMMIT and retained catalog batch](decisions/codec-catalog-current.md)
adds 27 `r1-catalog-` cases, for 178 cases overall and 122 with the `r1-`
prefix. The frozen catalog runs selected 41, 41 and 40 cases. Hosted Unix jobs
for that batch select 31, 31, 30 and 30 cases per profile.
The [DATA directory batch](decisions/codec-data-current.md) adds 29
`r1-data-` cases, bringing the registry to 207 cases and the R1 set to 151.
Its five hosted Unix shards select 31, 30, 30, 30 and 30 cases per profile.
The [VALUE reader batch](decisions/codec-value-current.md) adds 35
`r1-value-` cases, bringing the registry to 242 cases and the R1 set to 186.
The six hosted Unix shards each select 31 cases per profile.
`--case` is a repeatable name-prefix filter; no matches reject. Every output is
new. Optional `--toolchain` selects an installed Rust toolchain explicitly.

The controller copies Git-listed inputs into an owned checkout, rejects symlink
escapes and skips its own output. The real checkout is never modified. Builds
use an owned Cargo target, offline/locked dependencies, all selected package
features and controlled instrumentation settings. `--release` selects optimized
tests on Unix and is recorded in the evidence. A single `--bin` target can be
selected without running unrelated zero-test binaries. Every distinct test command
first needs a successful, nonempty baseline. Each subprocess has a 120-second
deadline, bounded output, a process report and verified retirement.

One production occurrence changes at a time; the in-file test module stays
unchanged. The isolated package is cleaned before compilation and after source
restoration so coarse timestamps cannot reuse a mutant artifact. A detection
requires Cargo exit 101 and a genuine failed-test result. Compiler errors, zero
tests, timeouts, output/observation failures and unverified cleanup cannot count.
The terminal report preserves every result and records negative-control outcomes
explicitly. Every copied source hash must match after restoration, and the
original input hash map must remain stable.

The Unix registry binds 61 integration controls to specific libtest names
using `exact_test`. Cargo selects the same integration binary and passes
the name with `--exact`. The baseline and mutant must report `ok` and
`FAILED` for that test on stdout. The command report carries the required
name and marker alongside the general compile, exit and retirement checks.
The existing registry labels and mutation expressions are preserved.

A local pilot verified all eight integration names in both profiles. The
72 command reports include the selected passing baselines and the named
failures. Selecting an unrelated passing simulator test for a VALUE control
returns an uncaught result even though it compiles and runs. These pilot
receipts qualify the named selection contract for the sampled cases.

### Frozen named selection

Frozen `a42358e40ead8394918d297a52189cd8cacae118` passes all ten runner
tests and all 61 exact integration controls in debug and release on macOS
arm64/APFS and Docker Linux arm64/ext4 with Rust 1.99.1. The prefix search
also selects `r1-value-map-key-order`, which passes its library test.
Each profile has three disjoint shards of 21, 21 and 20 cases.

The audits match every expected single-test baseline and failure to its
compiled command, verify all 880 command retirements, and rehash every
restored source file. Docker ran offline with source, Cargo, builds and
runtime files on its owned ext4 volume. The export passed verification
before both containers and the volume were retired. The xtask tests also
compile for x86_64 Windows MSVC.

The local archive `target/archived-proof/r1-named-a42358e.tar.gz` contains
12,103 indexed payloads and replays both audits from a fresh extraction.
Its SHA-256 is `cecbc96ac135c9a922ff27b4c88451658fdd352d53a098c7865eafb75708dd98`.

This is a reviewed, finite mutation inventory, separate from compiler MC/DC and
fuzzing. It deliberately avoids mutations that fabricate invalid raw pointers or
invalidate object ownership. The source expressions and tests require review
when implementations change; a source mismatch rejects rather than selecting a
different expression silently.

An external `#[cfg(test)] mod tests;` declaration does not truncate production
source. Inline `#[cfg(test)] mod ... { ... }` bodies at the end of the supported
source layout remain protected; a regression distinguishes both forms. This
small transformation is not a general Rust rewriting API. The input snapshot
also binds `tools/admin.rs`, with a regression for changes to that source.

## Native Windows identity controls

Windows selects the separate two-case `xtask/tests/mutations-windows.json`
inventory. It requires a native MSVC host compiler:

```sh
cargo xtask mutations --case windows-rng- --toolchain 1.99.1 --output target/windows-rng-mutations-new
```

Omit `--release` on Windows: that controller already requires both profiles.

The guards reject a positive nonzero RNG status and preserve native diagnostics.
Both select the exact status-zero/positive/negative test introduced in
`721cbdb01424e67c339c0fc8fb306050f0ca929f`. The fixture pins that source file;
each execution separately records its actual Git head, tree, parents, dirty
status, copied input hashes, compiler and controller identity.

Every case/profile has a successful one-test baseline and a separate mutant
build, with fresh owned Cargo targets in debug and release. The existing
Windows process boundary acquires a suspended child in its Job Object before
resuming it, bounds runtime/output, and verifies retirement. A detection
requires exit 101, the selected failed test, its intended assertion and a
consistent nonempty result. Compilation errors, a different test, timeout,
output overflow, cancellation or unverified cleanup cannot qualify.

Evidence retains original/mutated source, raw logs/reports, hashes and the
actual libtest executables identified by Cargo. Source restoration and original
input stability are required. The Windows CI job uploads evidence even on
failure, excluding disposable build caches and the full copied source tree.
Unix keeps its existing inventory and controller behavior. Cross-compilation
and classifier unit tests do not establish native mutant detection; the
[identity report](native-identity-assurance.md) tracks that execution gate.
