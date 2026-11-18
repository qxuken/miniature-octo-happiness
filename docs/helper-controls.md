# Continuous helper and oracle controls

Run the same private Rust controls as the macOS and Ubuntu helper jobs.
The SQLite comparator uses the system development library and pkg-config.

```sh
cargo fetch --locked
cargo fetch --locked --manifest-path tools/sqlite-baseline/Cargo.toml
cargo xtask helper-controls --output target/helper-controls
```

Choose a fresh output directory. The command discovers the exact executable of
each registered integration, unit and standalone SQLite test target from
bounded Cargo output. It binds their source paths and executable hashes, runs
the debug and release profiles, and matches libtest's complete reported results
against the discovered test identities. Missing or duplicate targets, omitted
tests, unregistered skips, empty execution, nonzero status and incomplete
cleanup all reject. The registered ignored TLS test requires separately frozen
inputs and has its own native result. macOS additionally registers the native
APFS busy-detach control, which creates an owned disk image and runs explicitly
in the native maintenance job. Linux does not register that absent test. The
ordinary helper run provides no execution credit for either skip. A real
libtest regression checks the APFS registration and still rejects unregistered
or missing skips. The five external fixture hashes and source inventory must
remain stable.

Each suite gets 90 seconds within a 600-second launch budget. Each worker has
independent 8 MiB stdout/stderr caps and an inherited regular-file limit. The
xtask unit binary allows files up to 128 MiB plus one byte for its sparse
oversize-input regression. Other workers have an 8 MiB file limit. The process
supervisor records terminal status and checks cleanup. Detached-session controls
cover timeout, early exit, a reparented same-group grandchild and a malformed
journal with a known child. The proof includes per-binary test identities,
process reports, source hashes and logs. Hosted CI excludes the intermediate
Cargo build directory from the upload.

The [conversion ledger](xtask-conversion.md) maps the frozen Python control IDs
to Rust. See [bounded supervision](xtask-supervision.md) for the process
ownership assumptions.

## Historical Python helper controller

`tools/check_helpers.py` runs every registered Python control suite in separate
normal and `-O` processes. This closes the local entry-point gap recorded at
`4370b46`; the workflow now schedules the same command on macOS and Ubuntu.
The suites test qualification helpers and their oracles. They do not launch the
full native, fuzz, sanitizer, coverage or workload campaigns.

```sh
mise exec python@3.14.8 -- python tools/check_helpers.py --output target/helper-controls
```

Choose a new output directory for each run. The runner refuses existing paths
and dangling symlinks, creates missing parents, and preserves failure evidence.
The CI job pins Mise 2026.11.6, Python 3.14.8 and the Mise action's immutable
v5.2.8 commit. The existing Windows job retains its three portable helper suites;
the complete runner requires POSIX process inspection and resource limits.

## Admission, execution and cleanup

The versioned `tools/helper_suites.json` must exactly cover discovered
`tools/**/test*.py` suites, excluding build/cache directories. Unknown or omitted
suites, duplicate registrations, symlinked sources, zero tests, incomplete test
execution, failures, expected-failure masking and unregistered skips reject the
run. Each worker reports actual discovered/executed test IDs and counts; exit
status alone cannot produce a passing result. The worker registers its module
before import, preserving normal dataclass and self-module behavior.

Two optional integration cases are explicitly skipped in each profile:

- `NativeTLS.test_authenticated_observations_restarts_dropped_reply_and_stale_capture`
  requires the separately built replica progress binary and fixture.
- `RunnerTests.test_frozen_positive_and_corrupted_oracle_controls` requires a
  frozen resource-envelope campaign.

Their enabling environment variables are cleared before import. These skips
provide no native or workload evidence. All remaining default test bodies must
execute. The tools tree and five external fixture/source inputs are hashed
before and after: the C header, Odin binding, storage codec/module and study
credential-rotation test. Missing, symlinked or changed inputs reject the run.

Each suite/profile has at most 90 seconds within a 600-second launch budget.
Inherited `RLIMIT_FSIZE` caps each regular output file at 8 MiB; result JSON is
bounded before parsing. Workers journal real `Popen` children without changing
their requested sessions or stdio. Cleanup covers the worker group, observed
descendants and journal-owned session groups, including a detached helper's
same-group grandchild after the helper exits. Cooperative cancellation precedes
bounded forced cleanup. Inspection or malformed-journal errors reject the run
after best-effort cleanup; they cannot bypass direct worker termination/reaping.

This is a watchdog for trusted Python controls, not a hostile subprocess
sandbox. Ownership uses PID plus `ps lstart` within bounded worker lifetimes;
the live tree covers the short spawn/journal window while its parent remains
alive. Arbitrary double-fork/session escape, PID-identity exhaustion, privileged
escape and uninterruptible kernel tasks are outside that claim. The trusted OS
process-table probe has a two-second timeout and an 8 MiB post-capture size check;
that probe is distinct from capped child output files.

## Frozen local results

The qualification uses a `d43e542` archive plus exactly the three new helper files
and workflow overlay, with all 629 source-file hashes frozen. Python 3.14.8 runs
the same inputs on both hosts:

| Profile | Workers | Reported tests | Passes | Registered skips |
|---|---:|---:|---:|---:|
| macOS arm64 | 38 | 272 | 268 | 4 |
| Docker Linux aarch64, ext4 | 38 | 272 | 268 | 4 |

All worker exits and cleanup checks pass; tools and external-input hashes stay
unchanged. Seventeen runner selftests are included in each profile. They exercise
false-green rejection, a real dataclass import, a real small file-size limit,
the overall deadline, source mutation, timeout/early exit, detached children,
orphaned grandchildren, failed inspection and malformed journals. Independent
review reran the previously leaking grandchild topology and verified its removal.

The Linux run uses an immutable Mise/Python image, no network, a private ext4
volume, two CPUs, 2 GiB memory and 256 PIDs. The parent rehashes the copied proof
and independently verifies removal of its exact container and volume. The new
image's base-layer prefix matches the previously qualified tool image.

Local proof: `target/helper-automation-4370b46/index.json`, SHA-256
`5ccf04bcd72be6fd24f2fe2a0fc37e1f5dd412b88ea3bcda2146db09ff0a6b35`.
The directory name preserves the starting checkpoint; its source binding names
`d43e542fc6279512180d47c958bdbc3d8f1b6e78` and every overlay hash. The index
rehashes 281 artifacts. These are helper-control results, not MC/DC percentages,
new native-engine qualification or a consolidated release candidate. The
separate [hosted Mac/Linux jobs](hosted-qualification-cd7cdfe.md) now pass against
committed `cd7cdfe`; their evidence retains its own source and host identities.

## Observer terminal-report regression

The [observer fix](helper-observer-finalization.md) reproduces an incomplete
report when cleanup's process-table proof raises before the final write. An
outer finalizer now saves startup, cleanup, wait and cancellation failures;
earlier observation errors and cleanup errors are recorded separately. Cleanup
proofs remain mandatory and errors still reject qualification. Five added tests
reject the old observer in both Python modes. The final complete Mac/Linux
registries each pass 354 controls and four registered skips across 52 workers;
300 additional output-limit iterations pass. Source/evidence bindings and the
limits of the historical intermittent reproduction are recorded in the report.

## Linux executable-fixture regression

PR #7's [native Linux job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44214295491/job/134500137879)
failed at source `9a25f7e` with `spawn child: Text file busy (os error 26)` in
`observer_preserves_arguments_and_retires_descendants_after_leader_exit`.
Its other twenty checks passed. The failing fixture writes a temporary shell
script in the parallel Rust test process, then executes that file directly.
A sibling fork can inherit its writable descriptor before `fs::write` closes
it. Closing the parent's copy does not close the child's copy; `CLOEXEC` takes
effect only when that child executes its program.

The new Linux test
`resource_campaign::tests::observer_runs_script_while_another_process_inherits_its_writer`
holds precisely that inherited descriptor after closing the parent's copy.
Direct execution must report `ETXTBSY`. Before the fix, the real observer also
fails with the exact CI error. The fixture adapter now prefixes `/bin/sh` to
the unchanged script/data/argument vector, so the interpreter reads the script
as data. Production executable requests receive no adapter, retry or new delay.

The regression runs in a bounded, single-test subprocess before creating the
writer and forking. This prevents the regression itself from retaining other
parallel tests' writers. Its child is killed and reaped even on assertion
unwinding. The outer bound is fifteen seconds; the observed command retains
its eight-second bound. Existing JSON, sentinel argument, RSS, output-limit,
deadline and descendant-retirement checks remain. The missing-executable case
still calls production `observe` directly and requires a terminal spawn error.
Removing only the interpreter-prefix line from the final source makes the
regression fail with `ETXTBSY`; restoring the exact source hash makes it pass.

Local qualification uses Rust 1.99.1 and the exact `9a25f7e` source plus the
reviewed patch. Linux source, vendor inputs, builds, temporary files and runtime
are on an owned ext4 Docker volume, with no network:

| Platform | Debug | Release | Other checks |
|---|---|---|---|
| macOS arm64/APFS | 237 passed, 2 explicit ignores | 237 passed, 2 explicit ignores | xtask Clippy and formatting |
| Docker Linux arm64/ext4 | 266 passed, 1 explicit ignore | 266 passed, 1 explicit ignore | inherited-writer regression, 20 repeat runs, xtask Clippy |

Counts include the two command tests and five supervision integration tests.
The registered ignores retain their existing separate qualification duties.
Run both profiles with `cargo +1.99.1 test --offline --locked -p xtask`
and `cargo +1.99.1 test --offline --locked --release -p xtask`.
Other checks are
`cargo +1.99.1 clippy --offline --locked -p xtask --all-targets -- -D warnings`
and `cargo +1.99.1 fmt --all --check`. Run the named regression with
`--bin xtask resource_campaign::tests::observer_runs_script_while_another_process_inherits_its_writer -- --exact --nocapture`.
Local logs and before/after patches are in `target/ci-pr7-fix/evidence/`;
this is a scoped helper regression, not new engine or MC/DC qualification.
