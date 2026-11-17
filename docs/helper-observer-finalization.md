# Observer failure reports survive cleanup errors

The incomplete report recorded during [maintenance qualification](maintenance-space-exhaustion.md)
is reproduced deterministically and its finalization defect is fixed. The
observer now saves terminal failure metadata even when startup, cleanup, waiting
or cancellation raises. Cleanup failures still reject qualification; they do
not become successful observations. Production engine code and all 19 private
Cargo packages are unchanged.

## Cause and reproduction

Previously, `tools/resource-envelope/run.py::observe` wrote an initial JSON
marker containing only `passed: false` and RSS scope. It saved the final report
**after** signaling the child group, verifying an EPERM group and waiting for
the child. An exception in any of those cleanup steps jumped past the final
write. The output-limit control then read the initial marker and raised KeyError
for the absent `failure`, obscuring the original error. The historical Python
command below applies to the archived source documented in this report. The
maintained controls now run via `cargo test --offline --locked -p xtask
--bin xtask resource_campaign`.

The deterministic regression launches a real child, observes its exit without
reaping, exercises the PermissionError cleanup path and forces the process-table
probe to return a nonzero status. The observer raises `cannot verify retired
group`; before the fix the report remains the initial marker. The initial
regression fails in about 0.3 seconds under both normal and optimized Python:

```sh
mise exec python@3.14.8 -- python -I -B tools/resource-envelope/test_compound.py \
  CompoundControls.test_observer_cleanup_probe_failure_records_terminal_report
```

The pre-fix failure is `AssertionError: 'failure' not found in {'passed': False,
'rss_scope': ...}`. The final test retains that failed-probe case alongside
timeout, malformed-output and live-group rejection cases.

Twelve unmodified optimized worker repeats and 300 narrowed uninstrumented
output-limit iterations did **not** reproduce the spontaneous historical
trigger. Another 100 instrumented iterations observed the Mac's EPERM response
for exited groups without an incomplete report. The old logs did not preserve
the original assertion, so they cannot establish whether a transient process
state or probe failure triggered those particular runs. Fault injection proves
the report-loss mechanism; this is not evidence of a diagnosed kernel bug or
a reason to ignore failed cleanup proofs.

## Fix and regression scope

An outer `finally` now finalizes the report around log setup, process creation,
observation, cleanup and validation. It records the failure, child return code
(null if unavailable), elapsed time and RSS samples. Once a child is available,
cleanup also records its group and whether signaling succeeded. Existing errors
continue to propagate after the report is saved.

`cleanup_failure` records cleanup errors separately. An earlier observation
failure is preserved if cleanup also fails. The report cannot pass after failed
cleanup, and its result checker is not invoked in that case. Child reaping,
group ownership, EPERM proof requirements, time/output limits and no-overwrite
admission are unchanged. The old output-limit test now requires the actual
report's `failure` field and successful cleanup; its diagnostic fallback is
removed.

Five new registered tests cover:

- Nonzero, timed-out and malformed cleanup probes, plus a reported live group;
  every case saves the error, fails qualification and reaps the real child.
- Spawn failure, with null return code and a saved startup exception.
- Wait failure, with a saved cleanup exception and no result-checker call.
- An observation failure followed by a cleanup failure, preserving both errors.
- KeyboardInterrupt cancellation, preserving the exception and reaping the child.

All five reject the old `6888f38` observer in both Python modes when replayed
with the final tests. The expanded compound suite passes all ten tests in each
mode against the fix. The normal reproduction command above now passes.
The report must remain writable and the observer must survive to execute Python
cleanup; this does not promise a final report after SIGKILL or storage failure
while saving the report.

## Verified results

Both full registries use Mise-pinned Python **3.14.8**, normal and `-O` workers:

| Platform | Workers | Reported tests | Passed | Registered optional skips |
|---|---:|---:|---:|---:|
| macOS arm64 | 52 | 358 | 354 | 4 |
| Docker Linux aarch64 | 52 | 358 | 354 | 4 |

All worker exits and owned-process cleanup checks pass. All 121 helper source
hashes and the five external source/fixture inputs remain unchanged during each
run. Review recomputes all 180 evidence payload hashes per platform. Docker uses
no network, two CPUs, 2 GiB and 256 PIDs; its private container and volume are
independently proven absent afterward. Its immutable image is
`sha256:0d5a130caa56029628d7660b5e35060c342681fa97e779d62182d83951930656`,
containing Mise 2026.11.6 and Python 3.14.8.

A further **300** real output-limit iterations under the worker's inherited
8 MiB file-size limit pass against the fix: every report rejects oversized
output, includes terminal metadata and records no cleanup error. The worker's
owned-process cleanup passes. These are bounded helper tests, not a new native
engine campaign, compiler MC/DC result or release-candidate qualification.

The exact tested source is `6888f38d097f8ed7d4608e0b686860fd7f47b84c` plus
the two reviewed helper changes:

| File | SHA-256 |
|---|---|
| `tools/resource-envelope/run.py` | `d37ba8a65cb06e3fbcb221fdcb1bcd5d36eaab3eead12e5ebceaa66d11d0859e` |
| `tools/resource-envelope/test_compound.py` | `df83a58d53734d5db22ef3f843fbb661e667fc396c261246219dd55ff5947e5a` |

Ignored local proof is under `target/helper-observer-fix/`. Its independent
`index.json` has SHA-256
`87f965216800084bfebdc5274892ec33f58ec90b98d71ed42e943ad5533c8cac`.
Mac registry evidence hashes to
`33ca6341579a411aa51a5d2ee1710a5dac7473e8c6b8abd1017e83d8a60ceee1`;
Linux registry evidence hashes to
`febc823f09758505ec8535448d7ed84155d06f34f3816f2bdc6aefbb4b9e967b`.
The index binds both source overlays, copied payloads, negative-control results,
stress cases and cleanup. Local ignored evidence is not permanent archival.
Crates remain unpublished with `publish=false` in every manifest.
