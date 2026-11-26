# Supervisor output and report flow

A child reading the report sees the incomplete marker. After the child
exits and cleanup is verified, the published report contains its exit and
output data. An invocation with no report path returns the final result
in memory. A test exercises both saved and unsaved invocations.

Distinct output paths preserve each stream's bytes and counters. A shared
path merges both streams onto the stdout pipe and counts both bytes there.
The tests check the file contents and reported counters in both cases.

A launch error records the original cause and certifies cleanup when no
child started. A live child follows the normal cleanup checks, including
the existing malformed-journal failure case. Preflight checks both logs
and the optional report before acquiring output. A report can neither
reuse either log path nor replace existing data.

Trace reports require an observed exit, verified cleanup and no recorded
failure. Ordinary mode accepts zero, while the expected-exit mode accepts
any observed exit. A missing exit fails in both modes.

| Source | SHA-256 |
| --- | --- |
| `xtask/src/supervise.rs` | `42643cd3bb9502d938b47af99dd8940fac642e57097c53b7728bc8d6f87021c2` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | xtask::xtask | xtask/src/supervise.rs:229:12– | 6500..6503 | pattern_test | run_controlled | The same Some or None input drives the initial-report guard. |
| C | xtask::xtask | xtask/src/supervise.rs:250:12– | 7362..7365 | pattern_test | run_controlled | The same Some or None input drives the terminal-report guard. |
| C | xtask::xtask | xtask/src/supervise.rs:243:12– | 7097..7100 | pattern_test | run_controlled | The same Ok or Err input drives the recorded launch error. |
| E | xtask::xtask | xtask/src/supervise.rs:214:9– | 5925..5928 | for_iteration | run_controlled | Preflight inspects both outputs and the optional report. A rejected third path prevents both log acquisitions and preserves the existing report. |

## Qualification

Frozen source `52605e480883931c83f5c61d775ca7b3d81225b7` used Rust 1.100.0.
Each debug and release run passed 20 supervisor unit tests, three trace
tests and five supervisor integration tests on macOS and Docker Linux
arm64/ext4. Both hosts detected all 12 mutations. Strict Clippy passed
on macOS, Linux and Windows MSVC x64. The Windows unit-test executable
linked in both profiles.

Fresh extraction verified the intended failure for every mutation,
45 supervised commands and 1,052 restored source files per campaign.
The first Linux release mutation run exhausted the Docker volume during
compilation. The complete serial rerun used the same source and passed
after generated build files were reclaimed. That attempt's logs are
included in the evidence. This batch reconciles 13 tooling records and
leaves 7,357 open tooling records.
[Frozen source and evidence checksums](supervisor-output-flow-52605e4.sha256)
