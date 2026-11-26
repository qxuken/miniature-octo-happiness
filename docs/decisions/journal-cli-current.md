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
| `apps/write-coordinator/src/native/journal.rs` | `76aa147ef70e8f423870bded014438c2d8ad683e0a13c67c238c421fce96d35c` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:336:65– | 10926..10927 | try_propagation | number | Malformed expected revision or value returns the numeric parse error; the valid pair commits. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:346:5– | 11244..11249 | match | run | Native process tests dispatch status, init, set, resume and check, plus the usage fallback. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:347:27– | 11283..11301 | match_guard | run | status opens the journal; other at the same arity returns usage. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:350:53– | 11441..11484 | match_guard | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:350:53– | 11441..11484 | short_circuit_and | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:352:13– | 11540..11542 | if | run | Slot 1023 initializes; slot 1024 returns Invalid with no journal file. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:380:47– | 12713..12758 | match_guard | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:380:47– | 12713..12758 | short_circuit_and | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:386:47– | 13026..13070 | match_guard | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:386:47– | 13026..13070 | short_circuit_and | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:393:16– | 13362..13404 | match_guard | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:393:16– | 13362..13404 | short_circuit_and | run | For the accepted arity, vary action alone or option alone against the accepted invocation. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:396:43– | 13497..13498 | try_propagation | run | Malformed expected revision or value returns the numeric parse error; the valid pair commits. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:397:37– | 13536..13537 | try_propagation | run | Malformed expected revision or value returns the numeric parse error; the valid pair commits. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:356:41– | 11718..11719 | try_propagation | run | EOF during the identity exchange returns the framing error. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:362:19– | 12005..12006 | try_propagation | run | Wrong response verb or malformed identity digits rejects before any receipt query. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:363:13– | 12020..12022 | if | run | Zero identity short-circuits the receipt query. A valid identity admits only the exact empty receipt. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:363:16– | 12023..12137 | short_circuit_or | run | Zero identity short-circuits the receipt query. A valid identity admits only the exact empty receipt. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:364:79– | 12121..12122 | try_propagation | run | EOF during the receipt exchange propagates before local file creation. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:336:56– | 10917..10918 | try_propagation | number | A non-Unicode slot returns the decimal argument diagnostic before creation. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:340:32– | 11057..11058 | try_propagation | sender | A nonnumeric port fails sender setup before reading the secret. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:341:49– | 11108..11109 | try_propagation | sender | A directory in place of the secret file fails sender setup. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:348:43– | 11349..11350 | try_propagation | run | An already-leased database returns Busy on status. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:351:36– | 11525..11526 | try_propagation | run | Malformed slot text propagates before sender setup. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:355:48– | 11675..11676 | try_propagation | run | Invalid port or invalid secret propagates before identity lookup. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:368:79– | 12370..12371 | try_propagation | run | An empty slot passes the remote check but an existing journal path returns AlreadyExists. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:381:61– | 12824..12825 | try_propagation | run | An already-leased database returns Busy before resume configures its sender. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:382:65– | 12891..12892 | try_propagation | run | The secret-file error propagates before the command can issue a request. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:382:67– | 12893..12894 | try_propagation | run | The lost-reply fixture rejects resume against a foreign primary, then resumes against the original. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:383:53– | 12948..12949 | try_propagation | run | A closed stdout reports the native write error on resume; subsequent resume returns the durable receipt. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:387:57– | 13132..13133 | try_propagation | run | An already-leased database returns Busy before check configures its sender. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:388:64– | 13198..13199 | try_propagation | run | The secret-file error propagates before the command can issue a request. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:388:66– | 13200..13201 | try_propagation | run | Journal check reports incompatible history, and reports aligned for the matching receipt. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:389:53– | 13255..13256 | try_propagation | run | A closed stdout reports the native write error on check; the journal image is unchanged. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:399:61– | 13614..13615 | try_propagation | run | An already-leased database returns Busy before set configures its sender. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:400:72– | 13688..13689 | try_propagation | run | The secret-file error propagates before the command can issue a request. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:400:74– | 13690..13691 | try_propagation | run | Set rejects the divergent primary; the pending intent subsequently commits on the original primary. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:401:53– | 13745..13746 | try_propagation | run | A closed stdout reports the native write error after durable set; the agreed revision reopens. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:293:10– | 9663..9664 | try_propagation | status | A closed stdout during initialization reports the write error after durable creation. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:301:9– | 9866..9868 | if | status | Exact status output before receipt recovery includes the pending line; after resume it omits that line. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:301:12– | 9869..9872 | pattern_test | status | Exact status output before receipt recovery includes the pending line; after resume it omits that line. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:300:10– | 9855..9856 | try_propagation | status | A writer ending after the header preserves the exact header and returns WriteZero. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:308:14– | 10109..10110 | try_propagation | status | A writer ending after the receipt preserves the first two lines and returns WriteZero. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:377:14– | 12614..12615 | try_propagation | run | The initialization simulator injects failure at every collection and initial-state publication operation; reopening verifies the exact root revision and journal state. |
