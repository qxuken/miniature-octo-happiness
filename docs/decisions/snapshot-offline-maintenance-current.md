# Snapshot CLI offline maintenance decisions

Offline commands preserve typed identity, enrollment, verification
and storage errors. A manifest status failure now carries the phase
of the operation that requested it.

| Operation | Refusal and resulting state |
| --- | --- |
| Identity and enrollment | Malformed native argument encoding, wrong width, nonhex input and all-zero identities retain their original errors. Each enrollment position is checked independently. |
| Describe and open | Invalid stream input and a corrupt source return their original errors. Foreign enrollment fails the follower open before maintenance. |
| Prepare compaction | A failed initial status read reports storage_maintenance and skips compaction. A real hard-linked manifest is rejected with Invalid before replacement. |
| Finish compaction | A failed status read of the reopened manifest reports storage_maintenance and emits no result line. In the test the compaction has already shortened the manifest. |
| Status | Failed metadata verification reports storage_verify and emits no durable receipt. |

The Unix regression temporarily moves the private fixture manifest to
force a native NotFound metadata error at the chosen stage. It restores
the fixture and checks the exact selected image, revision 4 receipt
and retained revision 2. Admission refusals preserve both original files.
Normal status and compaction retain the existing report format.

## Preserved source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native.rs` | `e1bcaaac32f74872a520cdde3653d1909b0d86070798f488a14614cd40deb211` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:261:59– | 9925..9936 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:337:6– | 12699..12700 | try_propagation | announce | The closed-pipe process fails writing the capture line and never contacts the follower. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:338:19– | 12720..12721 | try_propagation | announce | A complete capture line followed by a failed flush stops delivery before TCP connect. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:344:7– | 12949..12950 | try_propagation | prepare | A missing primary yields NotFound; a healthy primary is opened and exported. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:345:93– | 13044..13045 | try_propagation | prepare | Direct helper call with a zero stream propagates Invalid before opening the destination. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:346:99– | 13145..13146 | try_propagation | prepare | An existing destination returns AlreadyExists without changing its bytes; a fresh path exports. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:356:94– | 13417..13418 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:356:97– | 13420..13421 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:359:7– | 13543..13544 | try_propagation | prepare_saved | A valid descriptor combined with missing primary propagates NotFound. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:360:89– | 13634..13635 | try_propagation | prepare_saved | Direct saved-export reopen with a zero stream propagates Invalid. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:364:7– | 13795..13796 | try_propagation | prepare_saved | A missing export propagates NotFound. The intact image and descriptor reopen with unchanged bytes. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:375:73– | 14079..14080 | try_propagation | deliver | The delivery gate reports capture output failure before connecting. A writable first line and successful flush let delivery proceed. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:382:52– | 14484..14485 | try_propagation | deliver | A writer that holds only the capture line refuses the acknowledgment line after the follower has made the install durable. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:383:23– | 14509..14510 | try_propagation | deliver | A writer that accepts both lines and refuses the second flush returns the I/O error. Retrying the same export preserves the installed revision and manifest size. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:397:15– | 14943..14944 | try_propagation | ship | The first authenticated peer reads a request byte and closes. The runner retries the original export and obtains its exact durable receipt. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:406:5– | 15121..15124 | for_iteration | retry_ship | Zero configured attempts covers empty iteration. Positive runs cover one attempted exchange and a second permitted attempt. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:407:9– | 15158..15163 | match | retry_ship | The scripted exchange supplies both returned receipts and terminal errors. The closed notice pipe still permits a second exchange. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:60:5– | 2555..2560 | match | ack_line | The initialized follower reports empty; after installation, each observer prints the same durable revision and digest. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:75:35– | 3089..3090 | try_propagation | write_report | Closed-pipe process cases return a local I/O diagnostic. Working stdout reports the expected text. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:94:5– | 3629..3634 | match | option | The option fixtures cover default, accepted argument pair and rejected argument shapes. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:96:26– | 3694..3706 | match_guard | option | The accepted option flag reaches numeric validation; the same-arity wrong flag returns usage. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:97:32– | 3743..3744 | try_propagation | option | Unit and CLI cases return the encoding or integer error for the corresponding invalid value. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:97:50– | 3761..3762 | try_propagation | option | Unit and CLI cases return the encoding or integer error for the corresponding invalid value. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:108:5– | 4035..4040 | match | main | The named-command cases and actual process suites exercise the dispatch arms, help, and usage. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:110:21– | 4163..4176 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:113:32– | 4316..4331 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:127:35– | 4930..4950 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:131:15– | 5145..5146 | try_propagation | main | An absent parent is rejected. Even with closed stdout, a valid initialization can be opened by status. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:134:35– | 5262..5275 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:141:35– | 5578..5601 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:146:92– | 5890..5891 | try_propagation | main | An inactive directory produces Invalid during maintenance. A regular inactive file is removed. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:152:35– | 6093..6116 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:179:14– | 6977..6989 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:189:52– | 7485..7499 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:196:49– | 7904..7905 | try_propagation | main | Closing stdout prevents the first capture line and reports an I/O diagnostic. A normal capture prints both exact lines. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:216:14– | 8390..8401 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:241:14– | 9188..9208 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
