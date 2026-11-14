# Continuous helper and oracle controls

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
