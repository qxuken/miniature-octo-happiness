# Workspace coverage investigation at 21e5d26

**The original aggregate is degraded, not a qualified whole-workspace coverage
percentage.** Tests passed, but LLVM rejected some function/profile mappings.
The driver now fails closed on LLVM coverage warnings instead of treating exit
zero alone as successful measurement. No core production code changed.

## Source, execution and raw output

A clean isolated checkout of `21e5d2638c24c6254ced35d0077c55bb30a287b2` ran the
existing tool under Mise Rust 1.99.1/Python 3.14.8, selecting dated Rust
`nightly-2026-10-09` (`a30aa9064df6a504b6a323c6884a2f4cfbfff666`, LLVM 23.1.3).
Flags were `-C instrument-coverage -Z coverage-options=condition`; Cargo ran
`test --offline --locked --workspace --all-features`. The source hash map remained
stable. All 53 test-result summaries passed, totaling 405 reported test cases.
There were 65 collected executable artifacts and 51 reported source files.

Calibration measured exactly 2/6 branch outcomes for partial, 6/6 for
branch-complete and 6/6 for independent-complete. MC/DC instrumentation remains
unsupported; the condition-mode MC/DC report column has zero denominator.

The original tool exited zero and wrote status measured. That label is superseded
by this diagnosis and `target/coverage-diagnostic-recheck-21e5d26/evidence.json`;
the historical raw files are retained unchanged. Raw output, **not qualified
aggregate results**, was:

| Metric | Raw covered / count |
|---|---|
| Branch outcomes | 1,774 / 1,903 (93.22%) |
| Lines | 14,289 / 14,719 (97.08%) |
| Regions | 18,458 / 19,743 (93.49%) |
| MC/DC | 0 / 0; not measured |

Original evidence/export/report/HTML: `target/coverage-workspace-21e5d26`.
Full log: `/tmp/coverage-workspace-21e5d26.log`. Source inventory, compiler identity,
flags, exact object paths and exclusions are retained in evidence.json. External
tests/examples/tools, registry sources and remapped `/rustc/` sources are excluded
from the denominator; in-source unit-test code remains included. Integration and
application tests contribute execution, not their excluded external test sources.
Platform-disabled/uninstantiated code has no measured outcome.

## Why the aggregate is rejected

`llvm-cov report`, export and show emitted `628 functions have mismatched data`.
Independent `--dump` diagnosis identified 12 distinct names, not 628 independent
conditions: eleven unmangled `wdb_*` FFI symbols repeated across 57 objects,
plus one ring public-key function. For example, the pooled profile contains
`wdb_snapshot_acquire` hash `0xaae9a8e7c9a863c3`, while affected mappings request
`0x24268ee5508ef39d`. For the CLI parent, a ring mapping expects hash zero, but
its spawned app supplies the same symbol name with hash `0x8a588d17bbb3ff8b`.
These rejected mappings are distinct from ordinary unexecuted branches.
A nonsparse merge still warns (629 mismatches); it does not repair provenance.

Independent per-object experiments established a useful narrower path:

- The exact values integration binary passed seven tests, using only its private
  profile output. Report/export/show of that binary all had exit zero and empty
  stderr.
- The exact snapshot CLI integration parent passed one test. Its parent-PID-only
  profile reports cleanly. Merging all sixteen parent/child profiles recreates
  the ring mismatch. The clean parent report deliberately excludes application
  child execution and is not full CLI application coverage.

Proof: `/tmp/coverage-separation/proof.json`, with binary/profile/source hashes
and commands; `/tmp/weekend-coverage-profile-review.md` records independent
analysis. A future expanded report must map child profiles to actual executable
modules and retain variant provenance. Averaging or adding per-object percentages
would not yield a sound projectwide source-condition denominator. Unexecuted
objects must remain disclosed, not dropped to inflate results.

## Driver correction and validation

`tools/coverage.py` now records LLVM stage/command/status/stderr and rejects
warning-bearing report/export/HTML/calibration output. Exit-zero warnings set
status degraded and raise an error; nonzero LLVM exits set failed. Rejected
stdout/stderr remain available for diagnosis. Successful stages require no LLVM
warnings. Existing source stability, zero-denominator and repository-scope guards
remain in place.

Nine oracle tests pass normally and with Python -O. They include colored warning
text and ensure an exit-zero mismatch cannot become a measured result. An actual
LLVM recheck rejects the original workspace aggregate and accepts the prior
96e105f library report with empty stderr; that scoped library positive is
unaffected. Real fixture calibration with the updated driver also passed.

```sh
mise exec python@3.14.8 -- python tools/test_coverage.py
mise exec python@3.14.8 -- python -O tools/test_coverage.py
mise exec rust@1.99.1 python@3.14.8 -- python tools/coverage.py --mode condition --toolchain nightly-2026-10-09 --scope workspace --output target/workspace-coverage-new
```

Use an isolated stable checkout and fresh output directory. With these conflicting
pooled variants, the final command must reject rather than claim measured coverage.
Diagnostic recheck: `target/coverage-diagnostic-recheck-21e5d26/evidence.json`.
Updated fixture evidence: `target/coverage-diagnostics-calibration/evidence.json`.

The ordinary stable default was separately checked against exact 21e5d26 with
only the corrected coverage runner overlaid. Rust 1.99.1/LLVM 23.1.1 completed
the workspace tests, then its report returned zero with **685 mismatches**:
twelve unmangled FFI names repeated across 57 object mappings, plus one ring
`Algorithm::tag_len` mapping. The driver returned 1/status degraded. Switching
from nightly condition mode to stable line mode does not repair attribution.
Evidence: `target/coverage-line-workspace-21e5d26-final/evidence.json`, SHA-256
`c9ff1b1670b6207ff9e21aa4e77f98cd4105eb011a83d8862e19b4052c714a67`.
The [historical audit](historical-coverage-audit.md) also withdraws the original
1ca5e5a and 2490631 aggregate claims. Their original raw output is preserved.

| Changed assurance source | SHA-256 |
|---|---|
| tools/coverage.py | `5aa9fd93e647552ada3de2b1faebe908695e4b1fbe4b352c119fa05db958dcce` |
| tools/test_coverage.py | `3ef5c96da3cf1f89ee7c5f05da49fda5e08454ee1f2c02fe9e8f99957e7c4f0a` |

## Concrete regression shortlist from code and test review

Zero profile outcomes alone are insufficient: generic instantiations repeat spans,
killed child processes may not flush counters, and mismatches can omit mappings.
The following reachable public API cases were confirmed by inspecting existing
tests, independently of ambiguous FFI counters. Paths/lines refer to 21e5d26.

| Path and decision | Proposed regression and oracle |
|---|---|
| crates/weekend-db-typed/src/store.rs:628, poisoned Write | Catch a callback panic, then attempt another callback in the same Write. Require Closed before callback execution; revision/records/indexes unchanged. Existing panic test returned immediately. |
| crates/weekend-db-typed/src/store.rs:696, duplicate primary ID | Insert the same ID with a different unique value. Require AlreadyExists, preserving original record/index and rolling back earlier staged work. |
| crates/weekend-db-typed/src/store.rs:893, delete index ownership | Delete with missing/wrong-owner/wrong-type index claim. Require IndexInconsistent and atomic rollback, with retained lookup intact. Existing corruption cases exercised find/update. |
| crates/weekend-db-typed/src/store.rs:888, missing primary on delete | Return false without index changes; contrast with a successful deletion. Existing missing-field case still had a physical primary record. |

Lower-priority admission cases include nonfresh Store::initialize and malformed
format-marker Store::from_database. No missing test is inferred for invariant
size bounds, defensive trusted-directory rechecks, platform-disabled paths or
delegated native syscall failures. The [typed guard follow-up](typed-decision-gaps-21e5d26.md)
now tests the six selected guards using separate warning-free same-object
measurements and mutation controls; this original aggregate does not qualify
those additions or close MC/DC.
