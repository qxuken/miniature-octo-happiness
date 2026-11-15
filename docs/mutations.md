# Safe guard mutation qualification

Run the private Rust controller from the repository root:

```sh
cargo xtask mutations --list
cargo xtask mutations --case tls- --output target/tls-mutations-new
cargo xtask mutations --output target/all-mutations-new
```

The 56 reviewed guard mutations retain their original expressions, occurrence
counts, package/test selection and replacement text in
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
