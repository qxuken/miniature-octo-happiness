# Linux supervisor process identity

The supervisor decodes Linux process identity from the bytes of a procfs
stat record. A fixture exercises command names containing parentheses,
non-UTF-8 bytes and a newline, checks field 22 against the full unsigned
range, and verifies that a later field does not displace it. A live check
compares the process table and the observer's own stat record.

Twelve controls preserve the first malformed field's error. Missing
delimiters or required fields retain their exact diagnostic. Invalid
UTF-8 outside the command name retains the UTF-8 error. Invalid or
out-of-range integers retain their parse error. Each failure changes
the first affected field while retaining the preceding fields.

| Decision | Accepted or reaped case | First condition false | Second condition false |
| --- | --- | --- | --- |
| Delimiters are ordered and followed by a space | Ordinary valid row | End precedes start with a trailing space | Correct order without the space |
| Reported PID matches and is positive | PID 42 matches | Reported 43 for requested 42 | Both values zero |
| State has one alphabetic ASCII byte | R | RR | 8 |
| Reaped identity has parent zero and group minus one | Parent 0, group -1 | Parent 1, group -1 | Parent 0, group 0 |
| Live parent and group are nonnegative | Parent 1, group 42 | Parent -1, group 42 | Parent 1, group -1 |

The independent delimiter, PID, state and sign pairs exercise guards in
the existing error-checking macro. The generated branches retain their
separate compiler coverage scope. The reaped test also verifies a live
identity with parent and group zero. A negative group with a nonzero
parent yields an error, so a malformed row cannot silently disappear
as if reaped. PID and state guards run before later field conversions.

Both source representations of the reaped decision and the twelve
propagation records have named source dispositions. The 23 controls
also cover removing the individual macro predicates and choosing the
first closing delimiter instead of the last one.

| Source | SHA-256 |
| --- | --- |
| `xtask/src/supervise.rs` | `0e4c2f9c5df44fd47c4ff1abbe27439d5e201928cbef9d54d572620ccb9e8a41` |

## Carried supervisor output decisions

The four records below retain their earlier observations. The production
source preceding the test module is byte-identical to `3f742b8`, and the
existing supervisor tests passed in both profiles on macOS and Linux.
[Supervisor output qualification](supervisor-output-flow-current.md)

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | xtask::xtask | xtask/src/supervise.rs:229:12– | 6500..6503 | pattern_test | run_controlled | The same Some or None input drives the initial-report guard. |
| C | xtask::xtask | xtask/src/supervise.rs:250:12– | 7362..7365 | pattern_test | run_controlled | The same Some or None input drives the terminal-report guard. |
| C | xtask::xtask | xtask/src/supervise.rs:243:12– | 7097..7100 | pattern_test | run_controlled | The same Ok or Err input drives the recorded launch error. |
| E | xtask::xtask | xtask/src/supervise.rs:214:9– | 5925..5928 | for_iteration | run_controlled | Preflight inspects both outputs and the optional report. A rejected third path prevents both log acquisitions and preserves the existing report. |

## Qualification

Frozen `05e99c6fe3f1b737c70ffe9022fe35ce91d30334` used Rust 1.100.0. Docker Linux arm64
on ext4 passed 23 supervisor unit tests and five integration tests in
each profile. All 23 Linux controls were caught in debug and release.
The macOS host passed 20 supervisor unit tests and five integration
tests per profile. Strict Clippy passed on both hosts and for Windows
MSVC x64. The procfs tests ran on Linux.

Fresh extraction verified all 75 supervised commands and 1,057
source hashes per campaign, including the named assertion for every
control. The completed reconciliation closes 14 previously open tooling
records and leaves 7,343 tooling gaps.
[Frozen source and evidence checksums](supervisor-proc-identity-05e99c6.sha256)
