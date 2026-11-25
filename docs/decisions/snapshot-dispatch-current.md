# Snapshot command dispatch and preparation

The dispatch fixture varies only the command name at every accepted argument
count. A recognized command reaches its named phase. An unrecognized command
returns usage and leaves paths untouched. The earlier CLI fixtures cover
successful commands, both help aliases and the unsupported Windows commands.
For each command with options, the new process fixture checks a missing
value, wrong flag, invalid integer and zero. For the two send commands,
four exceeds the attempt cap of three. The unit fixture checks one, three
and the default, plus a non-Unicode value.

The preparation fixture supplies missing and corrupt inputs, an existing
destination and an invalid stream passed directly to the helpers. It
verifies the first reported error and its phase. Reopening a valid saved
capture returns its original descriptor and leaves all three files intact.

| Decision | Reference input | Comparison | Expected outcome |
| --- | --- | --- | --- |
| Help guard | Unknown single word | `--help` | Usage on stdout only for the help alias |
| Help guard | Unknown single word | `-h` | Usage on stdout only for the help alias |
| Attempt bounds | 1 | 0 | Admission at 1, bounds error at 0 |
| Attempt bounds | 1 | 4 | Admission at 1, bounds error at 4 |

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native.rs` | `877644213284371ed1fafff85a12eeeeb3f8afe14feb09703b18ab9223369e1a` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:84:5– | 3289..3294 | match | option | The option fixtures cover default, accepted argument pair and rejected argument shapes. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:86:26– | 3354..3366 | match_guard | option | The accepted option flag reaches numeric validation; the same-arity wrong flag returns usage. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:87:32– | 3403..3404 | try_propagation | option | Unit and CLI cases return the encoding or integer error for the corresponding invalid value. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:87:50– | 3421..3422 | try_propagation | option | Unit and CLI cases return the encoding or integer error for the corresponding invalid value. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:98:5– | 3695..3700 | match | main | The named-command cases and actual process suites exercise the dispatch arms, help, and usage. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:103:21– | 3848..3861 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:106:32– | 4001..4016 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:124:35– | 4671..4691 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:132:35– | 5005..5018 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:145:35– | 5601..5624 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:155:35– | 6096..6119 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:190:14– | 7375..7387 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:200:52– | 7883..7897 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:227:14– | 8788..8799 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:252:14– | 9586..9606 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:272:59– | 10323..10334 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:311:7– | 11802..11803 | try_propagation | prepare | A missing primary yields NotFound; a healthy primary is opened and exported. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:312:93– | 11897..11898 | try_propagation | prepare | Direct helper call with a zero stream propagates Invalid before opening the destination. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:313:99– | 11998..11999 | try_propagation | prepare | An existing destination returns AlreadyExists without changing its bytes; a fresh path exports. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:323:94– | 12270..12271 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:323:97– | 12273..12274 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:326:7– | 12396..12397 | try_propagation | prepare_saved | A valid descriptor combined with missing primary propagates NotFound. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:327:89– | 12487..12488 | try_propagation | prepare_saved | Direct saved-export reopen with a zero stream propagates Invalid. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:331:7– | 12648..12649 | try_propagation | prepare_saved | A missing export propagates NotFound. The intact image and descriptor reopen with unchanged bytes. |
