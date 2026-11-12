# Separate executable coverage qualification

`tools/coverage-executables.py` measures each test executable against profiles from
its own process. It does not compute workspace coverage. The existing pooled
coverage driver still rejects LLVM mapping warnings; this companion does not
relax that gate.

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/test_coverage_executables.py
mise exec rust@1.99.1 python@3.14.8 -- python -O tools/test_coverage_executables.py
mise exec rust@1.99.1 python@3.14.8 -- python tools/coverage-executables.py \
  --mode condition --toolchain nightly-2026-10-09 \
  --output target/coverage-executables-new \
  --select weekend-db::values --select weekend-replicate::cli
```

Omit `--select` to attempt every eligible test artifact. Output must be new.
The driver is currently POSIX-only. It uses locked, offline Cargo builds, the
existing three-case instrumentation calibration, and the existing source-path,
nonzero-denominator and LLVM-warning checks. Condition instrumentation is not
MC/DC; no MC/DC qualification is inferred from branch totals.

## Attribution and rejection rules

Cargo's `compiler-artifact` messages must identify an executable with
`profile.test = true` and a library, binary-test or integration-test target kind.
Custom `harness = false` targets and benchmarks are excluded. Non-test production
binaries are recorded but never executed as libtest. Each test uses its package's
manifest directory, with dynamic Rust standard-library/dependency search paths
supplied as Cargo normally does.

Each execution has a new profile directory, a captured parent PID, and a bounded
wait (180 seconds by default, configurable up to 1,800). A timeout kills the
POSIX process group and rejects the result. Successful exit and a positive final
libtest summary are required. Empty tests and empty measured-source branch scopes
are explicitly unqualified; they do not stop subsequent targets. Test failures,
changed binaries, invalid profile names, missing parent profiles, LLVM warnings,
and source/tool drift reject qualification. Build timeout is 600 seconds.

Only `%p-%m.profraw` files matching the exact parent PID are merged. Child raw
profiles, their PIDs and hashes are retained **unqualified**. Their executable
identity is not inferred from PID alone. Build-time profiles are separately
retained and unqualified. Every discovered executable is recorded, including
unselected and excluded ones. The report binds source hashes, compiler, flags,
object hashes, profile hashes and LLVM diagnostics.

Reports are independent object views. Shared source appears in many views;
percentages must not be added or averaged, and uncovered branches in one view
need not be uncovered elsewhere. A CLI integration parent may exercise no
application code in its own address space. Such a report does not qualify the
spawned CLI: dropping those children from a global claim would be misleading.
The command exits nonzero if any selected artifact is rejected or unqualified;
`degraded-separated-reports` can still contain individually qualified reports.
There is no successful whole-workspace coverage claim hidden behind that status.
Doctests are not executed by this companion. No projectwide source-region union
or child executable attribution is implemented.

## Controls and scope

Nine deterministic boundary tests cover artifact admission, custom harness
rejection, package cwd, exact PID attribution, malformed/missing profiles,
failed/empty test summaries, an actual failing subprocess, changed objects,
source drift and successful-exit LLVM warning rejection. An actual subprocess
malformed-profile test verifies rejection metadata is written and the next
execution can proceed. Invalid raw names remain inventoried with a null PID;
they never become eligible parent profiles. The tests also run with
Python assertions disabled.

The real initial controls used an isolated archive/checkout of
`21e5d2638c24c6254ced35d0077c55bb30a287b2` with explicit coverage-tool overlays,
Rust nightly `2026-10-09` (LLVM 23.1.3) and condition instrumentation. Values ran
seven tests; snapshot CLI ran one. Each parent-only merge, report, export and HTML
command completed without warnings. The CLI's fifteen child profiles remain
unqualified. Merging those children into the parent deliberately reproduced a
real LLVM mapping warning, which the guard rejected. Evidence is retained under
`target/coverage-executables-controls` and
`target/coverage-executables-negative`; these are local qualification artifacts,
not committed test fixtures.

## Historical expanded exact-21 run

The final isolated run retained the same exact engine checkout with only
`tools/coverage.py`, this companion and its boundary test overlaid. Command:

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/coverage-executables.py \
  --mode condition --toolchain nightly-2026-10-09 \
  --output /Users/qxuken/projects/weekend-db/target/coverage-executables-workspace-final
```

Rust was `1.101.0-nightly`, commit
`a30aa9064df6a504b6a323c6884a2f4cfbfff666`, target `aarch64-apple-darwin`,
LLVM 23.1.3; flags were `-C instrument-coverage -Z coverage-options=condition`.
All source/tool checks remained stable. The driver discovered 65 executable
artifacts and attempted 48 libtest harnesses. Their 397 executed tests passed.
Thirty-six independent object reports qualified, with no LLVM diagnostics;
six targets had zero tests and six had no measured-source branch denominator.
Those twelve remain explicitly unqualified. The command therefore returned 1
with `degraded-separated-reports`, not a workspace percentage. Seventeen
noneligible executable artifacts were recorded without execution. The 209 child
profiles and 68 build profiles are retained, unqualified.

Empty tests: typed library, benchmark test harness, file-worker test harness,
weekend-admin test harness, and both task-tracker library/binary harnesses.
Empty measured-source scopes: study CLI integration, macro compile-fail/generated
integrations, admin decisions, snapshot audit CLI, and coordinator writers.
This is expected for several parent harnesses whose application code runs only
in children; it is not evidence that their child processes had no coverage.

Evidence: `target/coverage-executables-workspace-final/evidence.json`, SHA256
`71f4b697f0599c6bfe1dbf29a58c1ccc2b89eee630936a17094e6059e4950c46`.
Companion SHA256:
`48f2ce1929dd117958f1920098ee20456da1439bf39212da1c553ab7de9e171e`;
boundary test SHA256:
`ef212a650af92145949238a9b07620ce13913ea51728b6773a9a513603da120f`.
The earlier exploratory all-target run lacked Cargo's dynamic library search
path for the macro harness; the final run fixed that invocation and its five
macro unit tests passed. Earlier evidence is retained, not silently overwritten.

A useful gap-finding view is the macro unit executable: 96 of its 100 branch
outcomes were covered. Its four zero outcomes concern duplicate model `type_id`
and three malformed resolution attributes. Those outcomes may be exercised by
compiler children of compile-fail tests; this report alone does not establish a
whole-project testing gap. Follow up with targeted source/decision review, not
percentage aggregation.

## Failure-metadata correction and current controls

Review found that malformed profile names were rejected correctly during
selection, but the final inventory reparsed their PID and raised a second error,
preventing the per-target evidence file from being written. Inventory now
preserves invalid names with `pid: null` and `name_valid: false`; qualification
still rejects them. Nine boundary tests pass normally and under `-O`.

The expanded run above remains bound to its recorded previous driver hash; it
was not rerun after this metadata-only fix. Current-source values and snapshot
CLI controls were rerun against the same exact-21 checkout/tool overlay, using
the documented `--select` command with output
`target/coverage-executables-controls-fixed`. All eight tests passed, both parent
reports qualified, all LLVM diagnostics were empty, fifteen child profiles
remained unqualified, and source hashes stayed stable.

Current companion SHA256:
`32cc2fb9e8edcef6f5ab6e5e9f82357e4744a93f9f5f00562eda9adc5b48e014`;
current boundary tests:
`2b1448578cd52eede429190b0b528b32f456e5a84b740a9b7dbaac63fdd56120`.
Current control evidence SHA256:
`7da028f3cfb01e671329a6fd2bd66b81bccddd47e7000b494dc3e52a389f9149`.
