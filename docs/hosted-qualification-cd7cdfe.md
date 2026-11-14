# Hosted platform qualification at cd7cdfe

After the repository became public, GitHub Actions could execute the native
matrix. The previously unexecuted native Windows profile now has actual passing
evidence. Cargo packages remain `publish = false`; no registry publication or
production-release approval follows from this run.

Source checkpoint: `cd7cdfebc215ef0f3819abf7b3adeb8175156232`.
The [push run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43467171327)
builds that exact source. The separate
[PR run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43467182722)
uses synthetic merge `6ce0caffebbf62d88391459d77cbe6908e060c7e`; its artifacts
retain that identity. Neither is relabeled as the other source.
All six native package manifests are compared with a `cd7cdfe` Git archive:
Mac/Linux each match all 630 source hashes byte-for-byte. Windows each match
244 directly and 386 after the checkout's LF-to-CRLF conversion, with no
unexplained differences. Raw Windows hashes remain in the evidence; cross-OS
checkout byte identity is not claimed.

## Native checks and packages

All three native jobs in the exact-source push run passed:

| Target | Compiler | Package features | Hashed package payloads | Reported Rust test executions |
|---|---|---|---:|---:|
| `aarch64-apple-darwin` | Rust 1.99.1, LLVM 23.1.1 | `crdt,replication` | 47 | 844 |
| `x86_64-unknown-linux-gnu` | Rust 1.99.1, LLVM 23.1.1 | `crdt,replication` | 45 | 852 |
| `x86_64-pc-windows-msvc` | Rust 1.99.1, LLVM 23.1.1 | `crdt,replication` | 52 | 816 |

The native checker runs workspace formatting, strict all-feature/all-target
Clippy, debug/release Rust tests, examples, native reopening/interruption checks,
the renamed Rust consumer and shared-library ABI smoke. Mac/Linux additionally
run compiled C memory/file/thread consumers. Windows separately compiles and
runs C and C++ consumers against both debug and release DLLs. Each checker log
contains 114 successful Rust summaries. The counts include repeated profiles
and are not unique-test or coverage counts.

Each package records a clean source, the requested combined features, effective
build configuration and successful relocation. Downloads are checked against
the GitHub artifact ZIP digest, then every manifest payload hash is rechecked
locally, normalizing Windows path separators. The package profiles explicitly
exclude Odin, Rust source consumers and isolated Rust source consumers. Those
remain covered only by their separately named earlier evidence. Package tooling's
`candidate_eligible` flag is not a final production qualification.

## Helper, fuzz and dynamic-analysis scope

The hosted [helper controls](helper-controls.md) pass on Mac and Linux. Each
artifact reports 38 isolated normal/`-O` workers, 268 passes and four registered
skips (272 reported tests); all cleanup checks pass. Artifact payload hashes and
tools/external-input inventories match committed `cd7cdfe` bytes. The optional
native/workload skips remain explicit and provide no campaign evidence.

The [bounded fuzz run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43467182697)
passes all 22 targets and reports 21,441 actual runs. Its artifact digest and
synthetic merge source remain recorded separately from the earlier exact-21
campaign. This is finite testing, not an exhaustive state-space result.

The Linux C/Rust address-sanitizer and bounded atomic-model jobs pass. The former
records its calibration and three C consumers with nightly
`8eb778d04094d3af4b26580cab327e0413eb1e02`, LLVM 23.1.5 and Clang 21.1.8.
The push run's Rust AddressSanitizer job passes with a rebuilt standard library:
57 successful summaries report 422 passing test executions on Mac arm64. Its
actual installed compiler is `1.102.0-nightly (8eb778d04 2026-11-12)`; the workflow's
floating nightly selector is not represented as a dated compiler pin.
The PR's core-library coverage job also passes in library-only LINE mode with
Rust 1.99.1 and all features: 8,993/10,265 lines (87.61%) and 16,389/19,161 regions
(85.53%). Source stability passes and report/export/HTML LLVM diagnostics are
empty with zero exits. Branch and MC/DC denominators are zero because they are
unmeasured, not because every condition is covered. This single scoped report
does not restore the previously withdrawn pooled percentages; push and PR data
remain separate.
The PR ThreadSanitizer job also passes all 57 summaries/422 test executions,
using that nightly and a rebuilt standard library, without a TSan warning.
This is separate from the calibrated Linux ASan and atomic-model controls.

The duplicate push TSan run timed out at its configured 20-minute limit. Its
compiler phase took about 68 seconds, followed by 16 minutes of preceding tests,
leaving 155 seconds for the coordinator's full-capacity receipt test. The
successful PR run needed 241 seconds for that writers suite; ASan needed about
67 seconds and native Mac debug/release about 29/5 seconds. The timed-out run is
incomplete, not a passing race check. It contains no detected-race report.

One PR ASan job was cancelled before execution: zero runner ID, no steps, and
GitHub's annotation that a hosted runner could not be acquired. The successful
exact-source push ASan result remains distinct. The targeted retry
(`132053179677`) succeeds. The PR's completed attempt 2 consists of that one new
execution plus eight carried-forward successes; those eight are not fresh reruns.

## Failure found and remaining boundaries

The initial `4370b46` Windows run failed all-target Clippy because the Linux tmpfs
fault example called Linux/Mac in-place compaction APIs on Windows. `d43e542`
gates the entire drill to Linux and explicitly rejects other platforms before
filesystem work. Targeted local Mac/Windows/Linux Clippy and subsequent native
Windows CI pass. The Linux drill's body and supported runner contract are
unchanged. Windows compact copy remains a separate capability.

Completed results from the earlier `4370b46` and `d43e542` runs retain their
source/event identities. Their remaining engine jobs were explicitly cancelled
to release occupied Mac runners for the latest matrix. Cancelled or incomplete
jobs receive no passing credit.

The follow-up workflow keeps every workload and assertion, gives TSan 40
minutes while leaving ASan at 20, and emits uncaptured receipt progress after
each 128-commit restart plus replay/maintenance boundaries. The full 1,024 writes,
eight lost replies/restarts, three replay passes, backup, restore and compaction
checks remain. The existing native capacity regression passes locally in 22.12
seconds; strict coordinator Clippy, formatting and Actionlint 1.7.12 pass.
Independent review found no lost coverage or workload in this diff.

Engine checks now run on PR updates, `main` pushes and manual dispatch. A
feature branch without a PR needs manual dispatch; an open PR gets one matrix
per update rather than duplicate PR/push matrices. The fuzz workflow retains its
existing triggers. This reduces runner demand; the logs do not establish that
duplicate jobs caused the observed runtime variation. The larger TSan budget is
supported independently by the measured complete/incomplete runs.

These hosted runners qualify their actual native execution and the named
combined-feature package profile. They do not establish physical power-loss,
device flush honesty, actual quota exhaustion, every filesystem/architecture,
Windows in-place replacement, all delivered-language fault traces, every
feature-isolated package or final service/support budgets. Remaining feasible
decision pairs, MC/DC tooling limitations and the final immutable release bundle
remain open. Engine/application runtime, durability barriers and separate
follower durable acknowledgments were not changed by these fixes.

Local proof: `target/hosted-ci-4370b46/index.json`, SHA-256
`50202a1495c06aafc4d687bd147cb7d121f883f84791b3ddef37fac2b71916ba`.
The historical directory name is not its source identity. The index binds ten
terminal runs, 45 downloaded artifacts and 130 evidence/log files. Root review
independently rehashed every indexed file and ZIP digest, all six native package
payload/source maps and all four helper artifact payload/input maps, counts and
cleanup results. Old and cancelled outcomes remain visible beside the successful
profiles.
