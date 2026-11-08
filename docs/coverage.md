# Compiler coverage evidence

`tools/coverage.py` preserves its default workspace line/region report
and adds explicit branch, condition, and MC/DC requests. Each run writes
`evidence.json` with compiler/LLVM identity, flags, features, scope,
source hashes, exclusions, test artifacts, calibration, and totals.
The matching toolchain's `llvm-tools-preview` is required.

```sh
python3 tools/coverage.py
python3 tools/coverage.py --mode condition --scope library
python3 tools/coverage.py --mode branch --fixture-only
python3 tools/coverage.py --mode mcdc --fixture-only
```

The active toolchain is used by default for line mode. Other modes use
nightly unless `--toolchain` selects another installed toolchain. The
library scope runs `cargo test -p weekend-db --lib --all-features`.
Workspace scope runs the existing all-feature workspace tests. The script
replaces inherited Rust coverage flags and removes encoded Rust flags
that could override instrumentation. Outputs go to `target/coverage`
for line mode or `target/coverage-MODE` for the other modes. `--output`
selects another directory. Existing raw profiles in that run directory
are removed before collecting new counters.

Every branch/condition/MC/DC run first compiles the two-condition
`a && b` calibration fixture. The partial run executes only `(true,true)`.
The complete run additionally executes `(true,false)` and `(false,true)`.
The driver is excluded from instrumentation, leaving exactly four
branch outcomes. Partial coverage must report two covered outcomes,
and complete coverage must report all four. MC/DC additionally requires
two instrumented conditions with independent-effect evidence in the
complete run. Unsupported instrumentation and zero denominators fail
the requested mode. Source changes in the selected workspace scope
also reject the result.

Rust's branch and condition modes emit compiler branch measurements.
Condition mode also instruments eligible boolean expressions outside
branch conditions. These counts do not establish MC/DC independence.
The available modes and their scope are documented by the compiler.
[Rust coverage options](https://doc.rust-lang.org/unstable-book/compiler-flags/coverage-options.html)

On the tested `rustc 1.101.0-nightly (708c94ef0 2026-11-03)` with LLVM
23.1.5 on aarch64-apple-darwin, both branch and condition calibration
reported 2/4 outcomes for partial input and 4/4 for complete input.
The compiler rejected `-Z coverage-options=mcdc`, accepting only block,
branch, and condition. The MC/DC request therefore exits unsuccessfully
and records `status = unsupported`. LLVM's available MC/DC report column
has a zero denominator under condition instrumentation. Rust's project
notes record the previous implementation's removal and the ongoing
reimplementation goal. [Rust MC/DC project goal](https://goals.rust-lang.org/2026/mcdc-coverage-support.html)

Reports exclude paths under `tests`, `examples`, `tools`, the Rust
sysroot, and Cargo registry sources. Unit tests located inside `src`
remain in the measured files. Generic instantiations, proc-macro
expansions, and compiler-emitted mappings affect the denominator.
Uninstantiated code and platform-disabled code have no measured outcome.
The library report uses its unit tests only. Integration, file-process,
C/C++/Odin, and distributed qualification evidence remain separate.
The manual decision ledger and mutation results in `DESIGN.md` retain
their own scope and do not become a compiler-measured MC/DC percentage.

The library condition run passed on the working tree based on
`1e883c8851c7c247220c8368b960c48a5ba1a5e2`, with a stable source hash map
recorded in `target/coverage-condition/evidence.json`. It measured
601 of 857 compiler branch outcomes (70.13%), 4,721 of 7,006 lines
(67.39%), and 6,127 of 9,341 regions (65.59%). The corresponding
`export.json` retains per-file mappings and counts, and `html/index.html`
shows branch counts in source. These totals include the in-source unit
tests described above. The existing line mode also completed a library
run using the active stable toolchain, with its evidence stored in
`target/coverage-line-check`.
