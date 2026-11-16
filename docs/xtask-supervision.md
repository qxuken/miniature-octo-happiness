# Bounded supervision and fault runners

The private Rust `xtask` owns POSIX child supervision and the native-error and
exact-delivery fault campaigns. It also runs the
[bounded helper controls](helper-controls.md).

```sh
cargo xtask faults native --source HEAD --output target/native-faults-new
cargo xtask faults delivered --package /absolute/linux-package --output target/delivered-faults-new
cargo xtask faults delivered --macos --package /absolute/macos-package --output target/delivered-macos-faults-new
cargo xtask helper-controls --output target/helper-controls-new
```

Privileged quota controllers use the prebuilt native xtask executable.
These commands use POSIX process inspection. Windows has its own native check,
build, package and cleanup entry points.

The hosted macOS helper run at `63a3d17` failed before the expected
RSS-probe diagnostic appeared. A controlled 125 ms delay at the first
observation reproduced that failure. The old 100 ms command deadline
expired before the injected sampler error could fire, and the supervisor
still reaped the child. The sampler cases now use a five-second ceiling.
The control keeps the 125 ms delay, checks the exact sampler diagnostic
and verifies retirement. The cases exercising timeout and other cleanup
failures keep their existing 100 ms ceiling.
The complete xtask unit suite passed on macOS (201 tests) and Docker
Linux arm64 (232 tests). The focused release control also passed on
Linux.
[Hosted macOS helper log](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43813246788/job/133119401956).

The hosted debug and release helper controls passed at `e1a3ada`.
macOS arm64 passed 424 controls and Linux x64 passed 486, with two
registered skips on each host. Both reports verified process cleanup
for all eight suites.
[macOS rerun](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43829449438/job/133170961027),
[Linux rerun](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43829449438/job/133170961390).

## Ownership and terminal reports

`xtask supervise` accepts one bounded JSON request on stdin. The request names an
argv, optional cwd/environment, two new log paths (the same path merges streams),
an optional new report path, timeout, per-stream byte ceiling, inclusive/exclusive
ceiling selection, optional inherited file-size limit, optional process journal,
and optional RSS sampling. Unknown fields, invalid limits, existing outputs and
dangling symlinks reject. Environment values are transported in memory and are
not copied into evidence. Helper campaigns bind their source inputs and binary
before and after the run.

Rust owns pipes, deadlines, output accounting, signals, RSS probes and retirement:

* Nonblocking reads have bounded work per turn, so a continuous writer cannot
  starve the deadline. Both streams have independent ceilings; written logs never
  exceed them. Resource observations reject **exactly** 8 MiB as well as overflow.
  Helper workers also inherit the regular-file limit for their descendants.
* `waitid(WNOWAIT)` observes exit without releasing the leader PID. Its group is
  retired even if the leader exited while descendants hold pipes open. Only then
  is the leader reaped. Cleanup is bounded and failure cannot qualify a run.
* Helper journals record PID, parent, group, start identity and state. Linux
  records boot clock ticks from procfs. Other Unix targets use `ps lstart`.
  Known detached sessions and reparented descendants are retired after their
  leader exits. Nested Rust-supervised fixtures append their identities to the
  outer journal. Recycled identities are excluded within the precision of the
  platform's start identity. This is trusted-suite ownership tracking.
* Process inventories enforce a two-second budget and an 8 MiB ceiling.
  Permission errors retiring a macOS zombie-only group are accepted only after
  proving leader exit and absence of live group members. Probe/journal errors
  still attempt direct group retirement and reaping, then fail qualification.
* SIGINT/SIGTERM produce cancellation outcomes. Fault campaigns keep a handler
  across commands so cancellation between commands still reaches Docker cleanup.
  Cleanup attempts both owned container and volume retirement, then queries both
  inventories independently; daemon errors cannot prove absence.
* Spawn, observation, output, wait, cancellation and cleanup failures leave
  terminal evidence. Observation and cleanup failures have separate fields;
  adapters refuse to call the independent oracle after supervision fails. A
  process killed with SIGKILL, an unwritable report filesystem or machine failure
  can prevent report completion; an initial marker never qualifies that run.

Start identity has clock-tick precision on Linux and second precision on other
Unix targets. RSS samples the main worker about every 50 ms. Bounded pipes can
add backpressure to verbose helpers. This qualification covers tooling.

## Historical supervision checkpoint

| Entry point | Ownership/orchestration now |
|---|---|
| `compatibility.run` | Rust supervisor; existing endpoint/byte oracles stay independent |
| `quota.invoke` | Rust supervisor, actual fixture PID in command evidence; privilege/mount/loop policy remains Python |
| resource `observe` | Rust supervision/RSS/terminal outcome; Python phase/byte oracle |
| helper `execute` / process-table probe | Rust supervision/journal retirement/probe; Python registry, worker instrumentation and unittest result oracle |
| `cargo xtask faults native` | Rust archive, explicit driver overlay, immutable image, permission/ENOSPC execution, stability and cleanup; Python shim retired |
| `cargo xtask faults delivered` | Rust parent/worker, exact-package verification and independent exercise/recovery oracle; Python shim/oracle retired |

The delivered worker relocates the exact package, runs all six fault modes,
recovers each in a separate process without the shim, checks loader paths,
requires the disabled-injector assertion to fail, and rechecks input/binary hashes.
The parent verifies copied artifact hashes and independent owned-resource absence.
The archived engine/package and current runner have separate source identities.

Docker resource/resilience/maintenance adapters stage today's tracked workspace
separately and build only xtask natively, offline and locked, against the pinned
registry cache. This avoids requiring xtask in historical engines or copying a
macOS executable into Linux. The workspace layout and original lockfile are
preserved. Missing/untracked xtask inputs reject; checkpoint new files first.
The parent records the staged input hashes and verifies they stay unchanged in
its qualified delivery campaign. No host filesystem is deliberately exhausted.

The table above records the first migration checkpoint. The
[conversion ledger](xtask-conversion.md) records the subsequent retirement of
the Python controllers and validators.

## Historical regression mapping and results

| Preserved regression | Current control |
|---|---|
| Timeout kills/reaps worker and descendants | Rust `timeout_reaps_leader_and_cleans_descendants`; existing compatibility/quota/helper/resource real-child controls |
| Exited leader with live pipes; PID reserved through retirement | Rust `exited_leader_does_not_wait_for_descendant_pipe_eof`, `exited_leader_is_unreaped_when_its_group_is_retired`; existing Python real-grandchild controls |
| Stdout/stderr overflow and exact ceiling | Rust `bounds_both_streams_and_exact_limit`; Python resource 8/9 MiB and quota bound controls |
| Inherited regular-file ceiling | Rust `file_size_limit_is_inherited_by_descendants`; helper file/result JSON controls |
| Spawn failure and no overwrite | Rust `spawn_error_persists_terminal_report`, output-admission controls; Python observer terminal mapping |
| Probe, sampler, wait and combined observation/cleanup failure | Rust `probe_wait_sampler_and_combined_failures_still_reap` injects OS-operation failures while retiring real children and requires `waitpid` to report `ECHILD`; Python adapter controls persist separate failures and refuse the oracle |
| Malformed journal, detached sessions and reparented grandchildren | Rust malformed-journal control; existing Python helper real-process journal controls |
| Cancellation and final report | Rust CLI `cancellation_retires_worker_and_writes_terminal_report`; observer adapter cancellation control |
| Campaign cancellation and cleanup failure | Rust CLI `fault_campaign_cancellation_still_runs_absence_checks`; cleanup timeout/absence-query/owned-name controls; negative campaign evidence control |

Former Python mocks of Python's removed wait/probe loops have been moved to the
Rust OS-operation seam. The Python tests now validate transport/report behavior;
they are not counted as evidence that mocking Python still exercises Rust.
Three orchestration controls leave the Python suite for Rust, while a new
transport-import finalization regression is added; the complete registry reports **352 passes + four registered skips, 52 workers per
platform**, rather than the previous 356 passes. Suite discovery and skip gates
remain strict in normal and optimized Python modes.

Mac arm64 and Docker Linux aarch64 helper registries pass with pinned Python
3.14.8. The native tmpfs permission/ENOSPC campaign passes against engine
`b09f10a7fff3b6b13f19be4522a5c9ffe6237252`. The delivered C campaign passes against
the previously qualified Linux package from
`c17cd76099336f279d4be731b14f8ee3409b3bf0`, whose manifest SHA-256 is
`1dd951082fcace18a5a1b7818dc30d3664f82dbc5a5df84fded56f6c5a26cbd8`.
This reuses the exact engine artifact; it does not attribute it to the new tool's
source. Both use immutable local image `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.

| Mode | Commit status | Visible | Fresh-process reopen | Retry | Retained |
|---|---:|---:|---:|---:|---:|
| Baseline | 0 | 4 | 4 | 4 | 2 |
| First sync failure | 14 | 3 | 3 | 4 | 2 |
| Second sync failure | 15 | 3 | 4 | 4 | 2 |
| Witness sync failure | 15 | 3 | 4 | 4 | 2 |
| Root-write failure | 15 | 3 | 3 | 4 | 2 |
| Actual tmpfs ENOSPC | 14 | 3 | 3 | 4 | 2 |

The disabled injector exits with SIGABRT (-6) at the expected ABI status assertion.
The compound Docker profile also passes native/inspect/eight-fault phases, using
its independent byte/digest/retained-root oracle and native Linux supervisor.
Containers and volumes are independently confirmed absent. A development run
correctly failed because pruning the runner workspace required rewriting its
lockfile; its evidence remains unverified, and cleanup passed. The corrected
workspace-preserving run passes with `--offline --locked`. Review also catches
and fixes an import-before-finalizer edge case: a deliberately unfinalized
variant fails the new report regression, and the corrected observer records
terminal fields even if its transport cannot import. Failed OS probes now have
bounded post-kill reaping as well.

Ignored proof lives under `target/xtask-supervision-qualification/`; it is local,
not permanent archival. Product MC/DC, physical power-loss coverage, native
Windows execution and final release qualification retain their own gates.


## Final reviewed qualification

| Check | Result |
|---|---|
| Mac arm64 Rust debug and release | 27 controls per profile |
| Docker Linux aarch64 Rust debug and release | 27 controls per profile |
| Full Mac and Linux helper registries, Python 3.14.8 normal/optimized | 352 passes + four registered skips each; all 52 workers retired |
| Strict all-target Clippy, formatting | Pass; Windows MSVC cross-target Clippy also passes |
| Native permission/ENOSPC drill | Pass; exact archived engine source and container absence verified |
| Delivered C ABI fault drill | Six modes, six independent recoveries, disabled-injector rejection; exact package/artifact hashes pass |
| Compound native / inspect / eight faults | Pass; complete byte/retained-root oracles and container/volume absence |
| Maintenance ENOSPC | Backup/restore/compaction failure, full-space reopening and successful retry; 32 fixture processes and 30 phase records pass |
| Repeated exact 8 MiB / overflow | 32 real iterations pass; temporary payloads removed |
| Deliberately unfinalized import variant | Rejected by the new terminal-report regression |
| Legacy native CLI under Python isolation | Missing module reproduced, import path fixed; isolated help passes in the registered control |

The proof review recomputes helper, source, copied artifact and exact-package
hashes, verifies worker/container/volume retirement, checks the four Rust test
logs, and confirms all **20** package manifests remain unpublished. Source hashes
bind the current Rust supervision code; reused engine source IDs stay explicit.
Cross-compilation checks Windows compatibility of existing commands and do not
qualify POSIX supervision as a native Windows implementation.

Final proof index `target/xtask-supervision-qualification/review.json` has SHA-256
`9b05901a864a22d429a53c128b967f95cf649430bdab79776b0afc830bf9bcfb`.
The final helper and native/delivery campaigns bind the reviewed supervisor.
Resource profile proofs retain their own staged-tool hashes; the later probe
retirement and compatibility-import fixes are covered by the final Rust/helper
reruns. Evidence is scoped to those recorded inputs, not an immutable release
candidate or a device power-loss qualification.

## Hosted quota inventory regression

Hosted run [43649630373](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43649630373)
against `61df2be` fails native EDQUOT qualification at its final tool-stability
check. The migration adds `supervision.py` to the initial inventory but omits it
from both independently written final inventories. Stable inputs therefore fail
in both native-commit and maintenance quota paths. This is a controller defect;
the gate remains fail-closed, and the failure does not qualify the engine run.

`qualification_tools` now constructs the inventory once for all three call sites.
A regression runs each complete `main` path with OS provisioning mocked at its
boundaries: unchanged tools qualify, while changing `supervision.py` still rejects
and preserves failed evidence plus the cleanup result. Both unchanged cases first
fail with the exact hosted `qualification tools changed` error. The fixture keeps
its backing inode small so it also runs under the helper's inherited file-size
ceiling. It performs no privileged mounts, quota changes or large allocations.

The quota controls now contain 13 tests, alongside three maintenance controls;
normal/optimized interpreter modes retain timeout, output, cleanup and independent
state oracles. Hosted EDQUOT remains the native filesystem verification for this
fix; unprivileged regression results do not substitute for that platform gate.

The corrected full helper registries pass **354 checks plus four registered
skips across 52 workers** on both Mac arm64 and Docker Linux aarch64 with pinned
Python 3.14.8. Quota's 13 and maintenance's three controls pass normally and
optimized. Source/binary stability and worker/container/volume retirement are
reviewed in local `target/ci-quota-inventory/review.json`, SHA-256
`5912160693a98ed58610ac4867032bb6ef939f70421953f9b70a6c5c602a1ec8`.
The first local end-to-end fixture exposed its oversized sparse-file truncation
under the inherited ceiling; replacing only that mocked provisioning operation
with a small inode fixes the test without relaxing the real file-size bound.

## TSan cancellation-fixture setup (2026-11-15)

Hosted run 43650789499 reaches the xtask cancellation control after the product
TSan tests and fails its ten-second readiness limit. The same focused nightly
TSan command reproduces locally. A tiny source archive alone does not fix it:
provenance still hashes the instrumented runner before launching the fake Docker
CLI. Temporary phase timing measures 13.794 seconds for a 32,375,608-byte local
instrumented executable, already exceeding the entire setup allowance.

The fixture now uses explicit minimal Git/Docker shell programs instead of
archiving the checkout or starting Python. Setup gets a finite sixty-second
allowance for instrumented provenance. The independent fifteen-second
cancellation/retirement bound, failed outcome, owned-resource absence checks and
all production deadlines stay unchanged. Real archive/engine identity continues
through the separate native fault drill. Temporary timing instrumentation is
removed after diagnosis; child stderr is retained by the fixture for useful
failure messages. This is a test setup budget correction, not a race waiver or
an exclusion of xtask from sanitizer coverage.

## Descendant discovery after reaping

Discovery follows descendants of already tracked parents, including a child
that starts a new process group. Before reaping it can traverse the reserved
root PID and its group. After reaping, traversal starts from previously owned
identities. A reused tracked PID with a different start time supplies neither
children nor group membership. An orphan in a previously recorded detached
group remains discoverable after its leader exits.

The directed process-table control covers both a detached descendant below an
already tracked parent and reused root/descendant PIDs. Real timeout, detached
early-exit, orphan and malformed-journal regressions also passed. macOS
passed 182 xtask tests in each profile. Docker Linux passed all 203 helper
tests per profile with eight clean worker retirements across both profiles.
The explicit TLS fixture skip remains one per profile. Docker resources were
confirmed absent. The Linux helper evidence SHA-256 is
`17da639746e8bddef40fcc6af358404d3732f531f1f1a165b4db247fc5d14ee7`.

## Linux process inventory under churn

The hosted Linux release helper at `5be73db` failed the campaign source-drift
and timeout control. Its retained stdout reports a numeric parse overflow and
lacks the original process row. [Hosted helper job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43705255065/job/132784834094)

Linux now reads `/proc/<pid>/stat` directly. Each row is capped at 4 KiB and
the combined inventory at 8 MiB. The parser reads the PID and the fields after
the parenthesized command name, including the unsigned start clock ticks. The
directed test covers parentheses, non-UTF-8 bytes and a newline in the name,
a start time above 32 bits, malformed delimiters, truncated input and a
mismatched PID. [proc_pid_stat(5)](https://man7.org/linux/man-pages/man5/proc_pid_stat.5.html)

A churn run of the first parser reproduced a separate failure when it read
a row while Linux was reaping the process. The kernel can sample a state and
then lose access to the signal handler, leaving PPID 0 and PGID -1. The
observer skips this pair and rows that disappear while being read. It still
rejects malformed negative identities and verifies that the inventory includes
the observer. The regression uses a running-state sample to exercise the race.
[Linux kernel procfs implementation](https://github.com/torvalds/linux/blob/master/fs/proc/array.c)

The macOS xtask suite passed 183 tests in each profile. On Docker Linux
arm64/ext4, `cargo xtask helper-controls --output NEW` passed 206 tests in
each profile, with the registered native TLS control skipped in each profile.
All eight workers retired cleanly and all 70 recorded artifact hashes matched.
A second pass used the release test executable emitted by the registry. With
`HELPER_PROCESS_JOURNAL` set and two shell loops creating short child processes,
it ran `campaign_source_drift_and_timeout_never_qualify` and
`paired_commands_overlap_and_both_reports_survive_failure` 60 times each using
libtest `--exact`. All 120 invocations passed and the container and volume were
absent after cleanup.

| Linux artifact | SHA-256 |
|---|---|
| Helper evidence | `b576e08ab628a9f7769fd9f96dbd5733f678f6e76bfa10d95f75906eff300fdf` |
| Churn run stdout | `264f2c1da3ad13d9db2a4da5b880834f1d90bf2963ef7beff7cbff5fbe802934` |

The updated supervisor also ran all six [delivered C fault cases](delivered-c-faults.md)
against the existing exact Linux package from `bc56084`. The controller
verified the inner evidence SHA-256
`d3aa5dc269d1d5cf9820dee347c1abb09d07e522e1c15a6ee4bf40fce892872c`
and confirmed that its owned container and volume were absent.
