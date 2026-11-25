# Study interactive command I/O

Readiness is a local command I/O operation. If its output pipe is closed,
the process reports `local_command/io/broken_pipe` and exits 1 before
starting the workers. The database bytes are unchanged. A native process
test closes the reader before starting the child and checks the exact
diagnostic category and exit status.

Once ready, the command loop reports a bad UTF-8 input line as
`local_command/io/invalid_data`. A refused reply preserves the
`BrokenPipe` error across the worker shutdown. Tests bind the same socket
again immediately and reopen the database through the native file adapter.

| Command or input | Result if the reply is refused | Reopened revision and practice |
| --- | --- | --- |
| `push off`, `peer`, `incoming` | `BrokenPipe` | 2, zero minutes |
| `practice 3` in sync-only mode, text and JSON | `BrokenPipe` | 2, zero minutes |
| `practice 3` with local edits enabled | `BrokenPipe` after commit | 3, three minutes |
| Unknown command, text and JSON | `BrokenPipe` | 2, zero minutes |
| Invalid UTF-8 after a successful `peer` reply | `InvalidData` | 2, zero minutes |

Both diagnostic modes exercise the incoming summary, sync-only rejection
and unknown-command reply. A staging probe contrasts the same practice
command with local edits enabled. A blank line produces no reply, EOF
stops cleanly, and a queued command after `quit` is skipped.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native.rs` | `9ced0c6b1d1ea19a0c2c827410755ad5775e4026c8271d1d7448a9f9822f9c68` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:263:7– | 9011..9012 | try_propagation | run_with_io | The shared diagnostic wrapper returns the readiness write result. A closed stdout pipe reports its local I/O phase and preserves the original bytes. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:311:17– | 10960..10963 | for_iteration | run_with_io | Empty input returns cleanly. A normal line followed by invalid data preserves the first reply. Quit skips the queued command. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:314:21– | 11121..11126 | match | run_with_io | Blank, quit, push, peer, incoming, sync-only practice and the general command path each exercise an arm. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:361:30– | 13716..13721 | match | run_with_io | A committed edit and both modes of rejected commands exercise each reply arm. |

Startup address and listener failures, native worker failures and the
full peer status outcome matrix remain open.
