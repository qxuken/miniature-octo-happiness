# Supervisor CLI response delivery

Running `xtask supervise` with stdout redirected to a read-only file
previously returned exit zero and no JSON response. The owned child had
finished, its output was captured, and the separate file report correctly
reported a zero exit and verified cleanup. Calling Rust's stdout flush
still reproduced the false success because the standard stdout adapter
handles `EBADF` as success.

The supervisor and process-table commands write through a duplicate of
the inherited stdout descriptor, then flush the local buffer before
returning. A write failure remains a command error. The verified child
cleanup and saved terminal report retain their original outcome. The
integration test checks both commands and the unchanged recipient file.
The controls restore the standard stdout writer or drop the explicit
flush to check the two independent parts of response delivery.

The carried records below precede the changed CLI functions and their
source spans are identical to the previous proof.
[Supervisor output qualification](supervisor-output-flow-current.md)

| Source | SHA-256 |
| --- | --- |
| `xtask/src/supervise.rs` | `41fe85d5b46bac45001a2e724b4b2d25ad737e5dfc1c5c522620bf0513be705d` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | xtask::xtask | xtask/src/supervise.rs:229:12– | 6500..6503 | pattern_test | run_controlled | The same Some or None input drives the initial-report guard. |
| C | xtask::xtask | xtask/src/supervise.rs:250:12– | 7362..7365 | pattern_test | run_controlled | The same Some or None input drives the terminal-report guard. |
| C | xtask::xtask | xtask/src/supervise.rs:243:12– | 7097..7100 | pattern_test | run_controlled | The same Ok or Err input drives the recorded launch error. |
| E | xtask::xtask | xtask/src/supervise.rs:214:9– | 5925..5928 | for_iteration | run_controlled | Preflight inspects both outputs and the optional report. A rejected third path prevents both log acquisitions and preserves the existing report. |
