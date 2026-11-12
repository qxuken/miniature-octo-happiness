# Coverage calibration and scope qualification at 96e105f

Compiler-measured MC/DC is unavailable in the tested Rust compilers. The pinned
`nightly-2026-10-09` compiler is `1.101.0-nightly (a30aa9064 2026-10-08)` with
LLVM 23.1.3, aarch64-apple-darwin. The separately installed `nightly` alias was
also probed at exact compiler `708c94ef093ecc42fb8851e17d8383d999727abc` (LLVM
23.1.5). Both reject the actual instrumentation flag with exit 1:

```text
error: incorrect value `mcdc` for unstable option `coverage-options` - `block` | `branch` | `condition` was expected
```

`rustc -Z help` exposes coverage-options and no alternative MC/DC instrumentation
flag in this pinned compiler. No inference from LLVM report columns is needed:
the driver stops with status unsupported before calibration or library tests.
Evidence: `target/coverage-probe-mcdc-2026-10-09/evidence.json` and
`target/coverage-probe-mcdc-installed/evidence.json`. This is a statement about
these exact compiler builds, not all Rust versions or a promise of future support.

## Calibration reliability

The fixture now evaluates `(a && b) || c`; its driver is coverage-off.

| Profile | Inputs (A,B,C) | Branch outcomes | Required independent conditions if MC/DC becomes available |
|---|---|---|---|
| partial | TTF | 2/6 | 0/3 |
| branch-complete | TTF, TFT, FTF, FTT | 6/6 | 2/3 (A,C) |
| complete | previous plus TFF | 6/6 | 3/3 (A,B,C) |

B has no independent-effect pair in branch-complete: its false observation is
masked by C=true. Adding TFF supplies the B pair with TTF, keeping A=true and
C=false. A uses TTF/FTF; C uses FTF/FTT. Actual pinned compiler branch and
condition calibration measured exactly 2/6, 6/6 and 6/6. MC/DC values in this
table are required oracle expectations, **not measured MC/DC results**. A future
compiler must pass those exact controls before the driver reports MC/DC.

The former partial MC/DC check accepted any covered value unequal to the full
count, even missing or impossible values. Validation now requires exact expected
counts, valid integer types and nonzero denominators. Complete branches alone
cannot pass the MC/DC negative control. Missing metrics, multiple unmerged export
data sets and zero-denominator line/branch/MC/DC reports are rejected. Fixture-only
runs bind and recheck the fixture, runner and selected source hashes before
reporting calibrated; library mode also binds the fixture explicitly.

Eight Python oracle tests passed under Mise Python 3.14.8 normally and with -O.
They validate these fail-closed rules using synthetic export metadata; those
unit tests do not simulate successful compiler MC/DC instrumentation. Cargo
coverage commands now use --locked and disable incremental builds.

## Measured scope and current result

An initial library export exposed `/rustc/a30aa9064df6a504b6a323c6884a2f4cfbfff666/library/std/src/sys/thread_local/native/mod.rs`:
the old path filter missed Rust's remapped sysroot path, contributing two regions,
two lines and two functions (one covered in each category), with no branches.
The filter now excludes `/rustc/`; exported source files must also reside under
the selected repository roots. Unexpected external files or no measured files
fail the run, rather than silently enlarging the claimed scope.

Historical `target/qualification-2490631-coverage-{line,branch}/export.json`
were inspected independently: each contains 51 files, none under `/rustc/` or
`/library/std/`. Their recorded totals need no contamination correction.

The final condition run used an isolated checkout of
`96e105f80c189a2f9de12530518c4d834f2796ea` plus only the coverage changes. All
124 root-library unit tests passed with all features. The source-stability gate
passed; 29 repository source files were measured:

| Metric | Covered / total |
|---|---|
| Compiler branch outcomes under condition mode | 953 / 1,133 (84.11%) |
| Lines | 8,917 / 10,152 (87.83%) |
| Regions | 11,810 / 13,786 (85.67%) |
| MC/DC | zero denominator; **not measured** |

This runs `-p weekend-db --lib`, not integration tests, typed-package tests,
apps, other platforms or C consumers. In-source unit-test code remains in the
measured files. Uninstantiated/platform-disabled code has no measured outcomes;
macros and generics affect compiler mappings. The static AST inventory has its
own denominator. None of these branch percentages closes manual MC/DC gaps.

Evidence: `target/coverage-condition-library-isolated/{evidence.json,export.json,report.txt}`
and HTML. An earlier shared-tree attempt correctly failed stability when an
unrelated agent added a Cargo manifest; it is not reported as successful evidence.
No core source was changed to obtain these measurements.

## Reproduction

Use a fresh output directory and a stable source checkout. Install matching
llvm-tools-preview in the dated Rust toolchain. The tested commands were:

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/coverage.py --mode mcdc --toolchain nightly-2026-10-09 --fixture-only --output target/mcdc-probe
mise exec rust@1.99.1 python@3.14.8 -- python tools/coverage.py --mode branch --toolchain nightly-2026-10-09 --fixture-only --output target/branch-calibration
mise exec rust@1.99.1 python@3.14.8 -- python tools/coverage.py --mode condition --toolchain nightly-2026-10-09 --scope library --output target/condition-library
mise exec python@3.14.8 -- python tools/test_coverage.py
mise exec python@3.14.8 -- python -O tools/test_coverage.py
```

The first command is expected to fail with status unsupported. Default line
mode remains available; its zero/scope guards have oracle-test coverage, but
this tranche does not claim a new full line-mode execution.

| Qualification source | SHA-256 |
|---|---|
| `tools/coverage.py` | `699732c3188d8ffa9d73ce0afa8ccb6e977ed35b1cad71b7794d637424ea5e42` |
| `tools/coverage-fixture.rs` | `b10fcdb477e6765f12afa032c1f4d9374ab41507aa444c870a5660fcd8c84769` |
| `tools/test_coverage.py` | `75b056ab94d3749f0321737794c3b42651e7fdda0ad59a00c88708ad8a4f485f` |
