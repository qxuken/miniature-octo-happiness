# Compiler coverage evidence

`tools/coverage.py` offers workspace or engine-library line/region reports
and explicit branch, condition, and MC/DC requests. LLVM mapping warnings
reject a measurement even when LLVM exits zero. The ordinary configured job
now explicitly selects the core engine library; it does not measure the entire
workspace. See the [historical aggregate audit](historical-coverage-audit.md)
and [separate executable reports](coverage-executables.md). Each run writes
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

Every branch/condition/MC/DC run first compiles the three-condition
`(a && b) || c` calibration fixture. The partial case covers exactly 2/6
branch outcomes. The branch-complete case covers 6/6 but has independent
effects for only two conditions; the complete case covers 6/6 and supplies
all three independent effects. Only the decision is instrumented. A future
MC/DC mode must distinguish those last two cases, requiring 2/3 versus 3/3
conditions respectively. The [calibration report](coverage-calibration-96e105f.md)
records exact inputs and compiler behavior. Unsupported instrumentation and zero denominators fail
the requested mode. Source changes in the selected workspace scope
also reject the result.

Rust's branch and condition modes emit compiler branch measurements.
Condition mode also instruments eligible boolean expressions outside
branch conditions. These counts do not establish MC/DC independence.
The available modes and their scope are documented by the compiler.
[Rust coverage options](https://doc.rust-lang.org/unstable-book/compiler-flags/coverage-options.html)

The original two-condition fixture on `rustc 1.101.0-nightly
(708c94ef0 2026-11-03)`, LLVM 23.1.5, aarch64-apple-darwin reported
2/4 partial and 4/4 complete branch outcomes. The current three-condition
fixture adds the stronger independence distinction above.
The compiler rejected `-Z coverage-options=mcdc`, accepting only block,
branch, and condition. The MC/DC request therefore exits unsuccessfully
and records `status = unsupported`. LLVM's available MC/DC report column
has a zero denominator under condition instrumentation. Rust's project
notes record the previous implementation's removal and the ongoing
reimplementation goal. [Rust MC/DC project goal](https://goals.rust-lang.org/2026/mcdc-coverage-support.html)

The later [dated 2026-11-12 capability probe](local-toolchains-a56083c.md#dated-nightly-still-rejects-mcdc-instrumentation)
also rejects the actual MC/DC flag before fixture compilation. Its unsupported
result is separate from measured branch/condition data.

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

**Withdrawn aggregate:** a full all-feature workspace condition run at source
`1ca5e5ab` passed calibration and functional tests, but its original log contains
451 LLVM mapping mismatches. It printed
1,179 of 1,545 instrumented branch outcomes (76.31%), 8,969 of 10,025 lines
(89.47%), and 11,279 of 13,214 regions (85.36%). Evidence and per-file mappings
were originally in `target/coverage-workspace-1ca5e5a`; those artifacts expired,
while the original diagnostic log is preserved by the audit. The run used an
isolated worktree and verified stable source hashes. Its executed functional
scope included integration and workspace tests, with the path exclusions above.
Those printed totals are not qualified coverage; source stability alone did
not validate compatible profile mappings. The [historical audit](historical-coverage-audit.md)
withdraws this and both 2490631 aggregate claims. Functional tests and independently
scoped evidence keep their own status. The independent
[source inventory](decision-inventory.md) makes additional source obligations
and macro blind spots visible.
