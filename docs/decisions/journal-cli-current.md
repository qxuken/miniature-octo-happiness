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
a valid identity. The scripted peer test
`journal_init_creates_a_file_only_after_identity_and_empty_receipt`
checks the other operand. An all-zero identity ends the exchange before a
receipt query. With a valid identity, changing only the reply from
`receipt 0` to `receipt 00` rejects the initialization. EOF at either
exchange, an unexpected response verb and malformed identity digits also
leave the destination absent. The successful case reopens in a new process
and reports the agreed identity, slot and empty receipt.

The closed-pipe regression runs init, set, check and resume with a stdout
pipe whose reader has exited. It checks the native write diagnostic against
an independent write to the same pipe. The receipt survives the failed set
acknowledgment. A fresh resume reports revision 4, preserves the local image
and leaves the primary at revision 4. The existing lost-reply regression
checks the exact status lines with and without a pending intent.

The CLI rejection fixture also tries an already-leased database, an invalid
port, a directory as the credential file, malformed and non-Unicode slot
arguments and an existing journal path for an empty slot. Each case checks
the public error, and the fixture rechecks the original journal and receipt.

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
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:355:41– | 11698..11699 | try_propagation | run | EOF during the identity exchange returns the framing error. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:361:19– | 11985..11986 | try_propagation | run | Wrong response verb or malformed identity digits rejects before any receipt query. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:362:13– | 12000..12002 | if | run | Zero identity short-circuits the receipt query. A valid identity admits only the exact empty receipt. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:362:16– | 12003..12117 | short_circuit_or | run | Zero identity short-circuits the receipt query. A valid identity admits only the exact empty receipt. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:363:79– | 12101..12102 | try_propagation | run | EOF during the receipt exchange propagates before local file creation. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:337:56– | 10939..10940 | try_propagation | number | A non-Unicode slot returns the decimal argument diagnostic before creation. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:341:32– | 11079..11080 | try_propagation | sender | A nonnumeric port fails sender setup before reading the secret. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:342:49– | 11130..11131 | try_propagation | sender | A directory in place of the secret file fails sender setup. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:348:79– | 11357..11358 | try_propagation | run | An already-leased database returns Busy on status. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:350:36– | 11505..11506 | try_propagation | run | Malformed slot text propagates before sender setup. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:354:48– | 11655..11656 | try_propagation | run | Invalid port or invalid secret propagates before identity lookup. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:367:79– | 12350..12351 | try_propagation | run | An empty slot passes the remote check but an existing journal path returns AlreadyExists. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:380:61– | 12785..12786 | try_propagation | run | An already-leased database returns Busy before resume configures its sender. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:381:65– | 12852..12853 | try_propagation | run | The secret-file error propagates before the command can issue a request. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:381:67– | 12854..12855 | try_propagation | run | The lost-reply fixture rejects resume against a foreign primary, then resumes against the original. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:382:53– | 12909..12910 | try_propagation | run | A closed stdout reports the native write error on resume; subsequent resume returns the durable receipt. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:386:57– | 13093..13094 | try_propagation | run | An already-leased database returns Busy before check configures its sender. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:387:64– | 13159..13160 | try_propagation | run | The secret-file error propagates before the command can issue a request. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:387:66– | 13161..13162 | try_propagation | run | Journal check reports incompatible history, and reports aligned for the matching receipt. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:388:53– | 13216..13217 | try_propagation | run | A closed stdout reports the native write error on check; the journal image is unchanged. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:398:61– | 13575..13576 | try_propagation | run | An already-leased database returns Busy before set configures its sender. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:399:72– | 13649..13650 | try_propagation | run | The secret-file error propagates before the command can issue a request. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:399:74– | 13651..13652 | try_propagation | run | Set rejects the divergent primary; the pending intent subsequently commits on the original primary. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:400:53– | 13706..13707 | try_propagation | run | A closed stdout reports the native write error after durable set; the agreed revision reopens. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:294:10– | 9685..9686 | try_propagation | status | A closed stdout during initialization reports the write error after durable creation. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:302:9– | 9888..9890 | if | status | Exact status output before receipt recovery includes the pending line; after resume it omits that line. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:302:12– | 9891..9894 | pattern_test | status | Exact status output before receipt recovery includes the pending line; after resume it omits that line. |
