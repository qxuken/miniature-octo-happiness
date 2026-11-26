# Supervisor result classification

| Classification | Required conditions |
| --- | --- |
| Ordinary success | Zero exit, no worker or cleanup failure and verified cleanup. |
| Expected error | A positive code, the exact exit status and diagnostic, verified cleanup and no cleanup failure, timeout, cancellation or explicit stop. |
| Controlled stop | Stop requested, SIGKILL exit, no worker or cleanup failure and verified cleanup. |

Each table starts with an accepted report. The test changes one condition
at a time and checks that classification rejects it. The nonpositive
code fixtures recompute the expected status and reason so those
comparisons still match. The source inventory recognizes each nested
short-circuit prefix. A condition can be false while the other conditions
in its prefix hold true. The predicates after a false condition are masked.

These cases exercise report classification after process supervision.
The process-level timeout, output limit and descendants tests retain
their original evidence scope.

## Qualification

Frozen source `24b6e342f80787f0bcb2bd18132efeac75b3a3d9` used Rust 1.100.0 and locked
dependencies. Each debug and release run passed 17 supervisor unit tests
and five integration tests on macOS arm64 and Docker Linux arm64/ext4.
Both hosts caught all 17 controls. Strict Clippy passed on both hosts
and for Windows MSVC x64. The Windows test executable linked in each profile.

Fresh extraction verified all 58 supervised commands, 1,046 restored
source hashes and the intended assertion for each mutation. This batch
reconciles 14 tooling records. The inventory leaves six Windows runtime
entries and 7,370 tooling entries open.
[Frozen source and evidence checksums](supervisor-result-pairs-24b6e34.sha256)
