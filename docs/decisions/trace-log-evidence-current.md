# Trace log evidence

A real child can exit cleanly even when a trace log is gone. The prior
runner qualified a zero exit and verified cleanup after the fixture
removed its own stdout log. The recorded stdout digest was null.

A trace now requires both digest reads before it can qualify. The
command record retains the exact exit code, ownership outcome and any
digest that was read. An unreadable log adds a diagnostic to the recorded
report, and an earlier failure retains precedence. The regression independently
removes stdout for an ordinary zero exit and stderr for an accepted
nonzero exit. Healthy commands still record both digests.

Admission checks both destinations before launch. A fixture preserves
each existing log in turn, checks that the other log stays absent, and
checks the original error in the returned result and evidence row.

| Source | SHA-256 |
| --- | --- |
| `xtask/src/trace_process.rs` | `ed3e89b7e430be459268b14101165663a98d1a8ad3278e0d2d017b867115496f` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | xtask::xtask | xtask/src/trace_process.rs:63:5– | 1861..1864 | for_iteration | admit | Each output can independently already exist. Preflight preserves its bytes, leaves the other path absent, and returns the exact admission error. |
| D | xtask::xtask | xtask/src/trace_process.rs:170:15– | 5198..5203 | match | execute | Successful commands record the report and both digests. Admission failure records the command and the exact error without a report. |
| C | xtask::xtask | xtask/src/trace_process.rs:174:16– | 5349..5352 | pattern_test | execute | The Some or None pattern and its enclosing if observe the same digest-error option. |
