# Safe guard mutation qualification

Run the private Rust controller from the repository root:

```sh
cargo xtask mutations --list
cargo xtask mutations --case tls- --output target/tls-mutations-new
cargo xtask mutations --output target/all-mutations-new
```

On Unix, the 56 reviewed guard mutations retain their original expressions,
occurrence counts, package/test selection and replacement text in
`xtask/tests/mutations.json`. Its provenance binds the retired Python controller.
All source expressions are checked before an output or source copy is acquired.
`--case` is a repeatable name-prefix filter; no matches reject. Every output is
new. Optional `--toolchain` selects an installed Rust toolchain explicitly.

The controller copies Git-listed inputs into an owned checkout, rejects symlink
escapes and skips its own output. The real checkout is never modified. Builds
use an owned Cargo target, offline/locked dependencies, all selected package
features and controlled instrumentation settings. Every distinct test command
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

This is a reviewed, finite mutation inventory, separate from compiler MC/DC and
fuzzing. It deliberately avoids mutations that fabricate invalid raw pointers or
invalidate object ownership. The source expressions and tests require review
when implementations change; a source mismatch rejects rather than selecting a
different expression silently.

## Native Windows identity controls

Windows selects the separate two-case `xtask/tests/mutations-windows.json`
inventory. It requires a native MSVC host compiler:

```sh
cargo xtask mutations --case windows-rng- --toolchain 1.99.1 --output target/windows-rng-mutations-new
```

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
