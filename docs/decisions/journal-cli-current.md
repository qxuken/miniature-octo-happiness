# Journal command dispatch

The native process regression
`journal_cli_rejects_invalid_arguments_and_claimed_slots_without_writing`
commits 42 in slot 1023 and keeps the resulting journal image. It varies the
action and option independently at each supported argument count. Both check
and resume use the same argument count, so the suite also exercises each
accepted action there. The existing lost-reply test covers successful status,
resume and check. The regression verifies the parse diagnostic for each set
argument and rejects slot 1024 before creating a file.

Reinitializing occupied slot 1023 fails for both the existing journal and a
fresh destination. The journal bytes and coordinator receipt remain intact,
and the fresh path stays absent. This covers the occupied-slot operand with
a valid identity. The all-zero remote identity case remains open.

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/journal.rs` | `5c4153f888d12a15975f7699e8bdaee03daf613f8f7c3f184df3aeca2f3e3df9` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:337:65– | 10948..10949 | try_propagation | number | Malformed expected revision or value returns the numeric parse error; the valid pair commits. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:347:5– | 11266..11271 | match | run | Native process tests dispatch status, init, set, resume and check, plus the usage fallback. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:348:27– | 11305..11323 | match_guard | run | status opens the journal; other at the same arity returns usage. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:349:53– | 11421..11464 | match_guard | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:349:53– | 11421..11464 | short_circuit_and | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:351:13– | 11520..11522 | if | run | Slot 1023 initializes; slot 1024 returns Invalid with no journal file. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:379:47– | 12674..12719 | match_guard | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:379:47– | 12674..12719 | short_circuit_and | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:385:47– | 12987..13031 | match_guard | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:385:47– | 12987..13031 | short_circuit_and | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:392:16– | 13323..13365 | match_guard | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:392:16– | 13323..13365 | short_circuit_and | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:395:43– | 13458..13459 | try_propagation | run | Malformed expected revision or value returns the numeric parse error; the valid pair commits. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:396:37– | 13497..13498 | try_propagation | run | Malformed expected revision or value returns the numeric parse error; the valid pair commits. |
