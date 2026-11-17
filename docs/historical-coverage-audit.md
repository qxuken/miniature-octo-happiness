# Historical aggregate coverage audit

On 2026-11-13, independent local review found that three previously recorded
workspace coverage aggregates emitted LLVM mapping-mismatch warnings. **Their
aggregate totals are not qualified coverage evidence.** This audit preserves the
historical totals and original manifests as records of what the tools printed;
it does not recalculate or silently replace them.

## Findings

| Historical source / mode | Previously printed headline | Diagnostic evidence | Disposition |
|---|---|---|---|
| `2490631dab43678279f8ba2c56458c8ea908c1b4`, stable line/workspace | 13,702 / 14,136 lines (96.93%) | Original report/export/show log warns 1,046 mismatches; retained-object report/export replay independently reproduces 1,046 | Withdraw aggregate qualification |
| Same source, nightly branch/workspace | 1,633 / 1,751 branch outcomes (93.26%) | Original report/export/show log warns 660 mismatches; retained-object report/export replay independently reproduces 660 | Withdraw aggregate qualification |
| `1ca5e5ab`, nightly condition/workspace | 1,179 / 1,545 branch outcomes (76.31%) | Original combined log records 451 mismatches at lines 498–500, then prints the same totals and workspace evidence path | Withdraw aggregate qualification; no retained-object replay available |

These warnings concern name/hash compatibility between profile records and
coverage mappings. They are not ordinary zero-execution counts and do not prove
an engine functional test failed. LLVM can return exit status zero while skipping
incompatible mappings. The old driver treated that status as sufficient and
recorded `measured`; the original diagnostic logs show that this was insufficient.
The present fail-closed diagnostic guard prevents that interpretation.

The qualification withdrawal applies to aggregate coverage measurements. It does
not independently invalidate functional, sanitizer, mutation, or manually traced
decision evidence from those revisions. Such evidence retains its own scope.
Compiler branch/condition coverage was never an MC/DC independence percentage.

## Exact retained-object replay

For both 2490631 runs, this audit reused the original 63 executable paths and
`tests.profdata` listed in their manifests. No source was recompiled, no tests
were rerun, no profile counters were edited, and no functions were excluded to
remove warnings. The original filename exclusions were preserved. The replay
ran matching `llvm-cov report` and `export`, with `--dump` for exact name/hash
failure details and every original `-object` argument.

- Line: Rust 1.99.1 (`58cf8f9c35b355199727627d07ae34a4e2846d67`), matching LLVM
  23.1.1. Both commands returned zero but emitted 1,046 mismatches: 19 unmangled
  `wdb_*` symbols repeated across 55 object mappings, plus one ring
  `Algorithm::tag_len` mapping.
- Branch: Rust nightly (`708c94ef093ecc42fb8851e17d8383d999727abc`), matching LLVM
  23.1.5. Both commands returned zero but emitted 660 mismatches: 12 unmangled
  `wdb_*` symbols across 55 object mappings.

The original manifests name workspace/all-feature scope, record stable source
hashes, and have `dirty: true`; this audit does not relabel them clean. The audit
binds the actual retained binaries by SHA-256, rather than inferring their
identity from source revision alone. For 1ca5e5a, its old source worktree and full
combined log remain, but the referenced coverage artifact directory no longer
exists. Its diagnosis is therefore based on contemporaneous logged warnings,
not a reconstructed replay or a new measurement.

Evidence is preserved locally under `target/coverage-audit-2490631/`:
`line-audit.json` and `branch-audit.json` contain commands, tool versions,
original-manifest/profile/object hashes and per-symbol mismatch counts. Their
report/export stdout and stderr are retained. Copies of all three original logs
and `historical-log-hashes.json` preserve the contemporaneous diagnostics.
These local artifacts are not a permanent retention promise.

| Retained item | SHA-256 |
|---|---|
| 2490631 line original manifest | `7cbbcfba69be151fe2432b66f68622a2573d41b3943dd11ff591efeb68be53f7` |
| 2490631 line indexed profile | `c1ffd4f489395303ee68191de8a15df263f310854dd36f366393a35d2c9acebd` |
| 2490631 branch original manifest | `b0976970becf44fd1d33768fda6ea2dd824fd4ff4ad41337b04aa14861a35c03` |
| 2490631 branch indexed profile | `b94fb54cf7dc6e0198fc7b74703a1e871fb95b3229f7726d8fcf4ace702c6242` |
| Original line combined log | `4d80461533e8b16f9668922a5ffc90cbfc8d81633d68fda43bdb7b7cb9bde7dc` |
| Original branch combined log | `a9793981a195dc688b033b37ec79b52cfa83885cb7a4987beb5677fff483c823` |
| Original 1ca5e5a combined log | `5f51c4bd143d3406e73c743ca30543f4c6e74a6f66e399c62a37cdd746ef990f` |

The same failure class was independently reproduced on exact 21e5d26 with stable
line/workspace (685 mismatches) and dated-nightly condition/workspace (628).
Changing condition instrumentation back to stable line instrumentation does not
solve the aggregation problem.

## Proposed future coverage job

Use an explicit **engine-library unit-test scope** as the ordinary coverage gate:
`tools/coverage.py --scope library`. Label it that way in the job and artifacts;
this selection is `-p weekend-db --lib`, not every workspace library. Require
matching LLVM tools, source stability, valid denominators and zero mapping
warnings. Existing clean library measurements remain limited to their recorded
source/tool scope; requalify the chosen candidate locally before adopting a job.

Keep full-workspace functional tests as their own independent gate. For broader
coverage observations, use the [separate executable companion](coverage-executables.md)
with explicit eligible targets and per-parent profile provenance. Keep child and
build profiles unqualified until executable identity is established. Report
empty test harnesses and empty production scopes explicitly; do not quietly
remove them to improve a percentage. Selected meaningful targets can form a
small coverage matrix, each with its own denominator and warning-free result.

Retain `--scope workspace` only as an explicit fail-closed diagnostic request
until compatible mapping reconciliation is implemented. Do not pool unrelated
profiles, sum or average per-object percentages, suppress mismatch diagnostics,
or claim a source-region union that has not been validated. This audit makes no
workflow changes and runs no hosted CI.

## Locally qualified replacement scope

The proposed explicit library command was then executed locally on the same
clean archived engine `21e5d2638c24c6254ced35d0077c55bb30a287b2`, with only the
fail-closed `tools/coverage.py` overlay (SHA-256
`5aa9fd93e647552ada3de2b1faebe908695e4b1fbe4b352c119fa05db958dcce`).
The coverage fixture was unchanged. Mise selected Rust 1.99.1 and Python 3.14.8;
matching stable LLVM 23.1.1 tools were installed for that explicit toolchain.

```sh
CARGO_INCREMENTAL=0 mise exec rust@1.99.1 python@3.14.8 -- \
  python tools/coverage.py --scope library --output NEW_OUTPUT
```

**124 core-library unit tests passed**. Exactly one executable was reported;
report, export and HTML commands all returned zero with empty stderr. The final
status is `measured`, source stability checks passed, and recorded source hashes
were independently rechecked after execution. This validates the narrowed local
command, not a hosted job or a workspace aggregate.

The library-only line view reports 8,993 of 10,265 mapped lines (87.61%). Its
scope includes in-source core unit-test code and the linked library mappings
selected by the existing filename filters. It does not include the full
integration suite, typed/macros/apps coverage or platform-disabled branches.
Stable line instrumentation emitted no branch or MC/DC denominator; zero there
means unmeasured, not zero achieved coverage. This number must not be compared
as an improvement or regression against the rejected global totals above.

Evidence is `target/coverage-library-final-review/evidence.json`, report/export
and HTML, plus `artifact-sha256.json`; the full log is
`/tmp/coverage-library-final-review.log`. The manifest records exact compiler,
source hashes, flags, Cargo commands and LLVM command diagnostics.

| Artifact | SHA-256 |
|---|---|
| Qualified library manifest | `127a1bb7e452362c9d8d6eaa532eb6fd7bd3780a0bb8b79b73023f6eb69e8d2e` |
| Library indexed profile | `2be4a31b91321c1cfa07ae81c24dd24759a784d3ab850164bf68a19761376c60` |
| Core unit-test executable | `b2a125d3907e47b98d231589912bb99fbd51242339ec9a0a277581675b733767` |

No hosted CI was run and this audit itself made no workflow edits.
