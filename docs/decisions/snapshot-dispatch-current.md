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
| `apps/weekend-replicate/src/native.rs` | `cca34437b0e7b100cd5d031d3d607bfbacd18213a1ffe3e1fd1618e20c8bd027` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:92:5– | 3533..3538 | match | option | The option fixtures cover default, accepted argument pair and rejected argument shapes. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:94:26– | 3598..3610 | match_guard | option | The accepted option flag reaches numeric validation; the same-arity wrong flag returns usage. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:95:32– | 3647..3648 | try_propagation | option | Unit and CLI cases return the encoding or integer error for the corresponding invalid value. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:95:50– | 3665..3666 | try_propagation | option | Unit and CLI cases return the encoding or integer error for the corresponding invalid value. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:106:5– | 3939..3944 | match | main | The named-command cases and actual process suites exercise the dispatch arms, help, and usage. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:111:21– | 4092..4105 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:114:32– | 4245..4260 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:132:35– | 4915..4935 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:139:35– | 5247..5260 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:154:35– | 5881..5904 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:165:35– | 6396..6419 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:200:14– | 7681..7693 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:210:52– | 8189..8203 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:237:14– | 9094..9105 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:262:14– | 9892..9912 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:282:59– | 10629..10640 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:321:7– | 12108..12109 | try_propagation | prepare | A missing primary yields NotFound; a healthy primary is opened and exported. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:322:93– | 12203..12204 | try_propagation | prepare | Direct helper call with a zero stream propagates Invalid before opening the destination. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:323:99– | 12304..12305 | try_propagation | prepare | An existing destination returns AlreadyExists without changing its bytes; a fresh path exports. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:333:94– | 12576..12577 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:333:97– | 12579..12580 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:336:7– | 12702..12703 | try_propagation | prepare_saved | A valid descriptor combined with missing primary propagates NotFound. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:337:89– | 12793..12794 | try_propagation | prepare_saved | Direct saved-export reopen with a zero stream propagates Invalid. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:341:7– | 12954..12955 | try_propagation | prepare_saved | A missing export propagates NotFound. The intact image and descriptor reopen with unchanged bytes. |
