# Study Windows startup and retry fixtures

The minimum-Rust Windows job for PR #82 head `e990a9f` compiled Study and
reported two failed unit tests in its x64 MSVC debug run. The runner used
the PR merge source `7100c9923a6d7a4ff1b584200c2c6aa39a4f134b`.
The job's `debug-1.stdout` contains both failures.
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45693315332/job/139484132013)

| Fixture | Hosted failure | Correction |
| --- | --- | --- |
| Local startup dispatch | Windows returned the intentional key-generation refusal and the test unwrapped it | Assert the exact refusal and absence of a credentials directory on Windows. Unix still verifies both generated files. |
| Outgoing status and retry | The direct TCP probe to a released port returned `TimedOut` | Retain the same listener across attempts. End the server's send half on the first accepted TCP connection and keep the socket open until the client's EOF report. Authenticate and serve the next two attempts normally. |

The retry probe checks the exact native EOF text or structured
`connect/io/unexpected_eof`, preserved receiver bytes before the retry,
and the resulting local and acknowledged state. The two successful
exchanges produce two acknowledgments and one receiver revision advance.
Reopening retains revision 3 and seven minutes. The existing two-process
lost-reply test continues to exercise the full receiving service.
