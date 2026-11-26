# Snapshot capture, poll and delivery decisions

Capture prints its export details through the shared report writer. It emits
those details only after the first capture line flushes. Both lines retain
their existing bytes and the local command phase for output failures.

| Boundary | Expected observable result |
| --- | --- |
| Capture admission | Invalid stream or missing source creates no export. An occupied descriptor leaves its bytes unchanged and preserves an openable export at revision 4 with retained revision 2. |
| Capture output | The first failed flush leaves only the capture line. Refusal to write the detail line preserves that line and the WriteZero result. A final failed flush can follow both lines and still returns the original I/O error. |
| Poll | A refused connection reports connect and an interrupted authenticated reply reports replication. Each preserves the local revision line, emits no follower receipt and leaves the follower at its previous durable revision. |
| Delivery | After a lost reply and durable install, the same connection refusal in plain mode includes the uncertainty and recovery guidance. Structured mode preserves the original I/O kind and reports an unconfirmed follower outcome. Neither mode emits a follower receipt. Reopening and retrying the same export confirm the original digest and leave the manifest unchanged. |

The ten named controls cover eight current source records and exercise both
detail-output boundaries through the reused writer. For the delivery
diagnostic guard the same source, saved export, refused endpoint and
attempt count are held fixed while the diagnostic setting changes.

Two propagation sites have an infeasible error outcome in this CLI path.
Identity parsing follows the fixed-width ASCII hex check. The range of
unsigned 32-digit hexadecimal values fits exactly in u128. Native poll
has already validated the nonzero stream and opened a verified, exclusively
leased file. Successful recovery installs storage in a fresh database and
rejects a zero file identity. That database is unused by other threads, and
the snapshot read before Primary::new only observes the revision. The
broader library still checks missing storage, recovery and invalid
enrollment. The two I rows bind these caller invariants.

## Qualification

Frozen source `f9ba262e9dc2c80aeb5329d3827f756ad21cd884` ran with
Rust 1.100.0 and locked dependencies.

| Platform | Debug and release, per profile |
| --- | --- |
| macOS arm64 | 60 snapshot CLI tests passed and 11 core replication tests passed with one ignored. The exact tests detected all 10 controls. |
| Docker Linux arm64/ext4 | 61 snapshot CLI tests passed with one ignored and 11 core replication tests passed with one ignored. The exact tests detected all 10 controls. |
| Windows x64/MSVC | Five CLI test executables cross-linked. Native execution of this source remains open. |

Clippy with warnings denied passed on macOS and Linux. Fresh extraction
verified the exact failed assertions, 38 supervised commands and 1,003
restored source inputs per campaign. The source review accounts for all
103 records in `apps/weekend-replicate/src/native.rs`. The full inventory
still has 326 runtime and 7,468 tooling gaps.
[Frozen campaign checksums](snapshot-command-effects-f9ba262.sha256)

## Exact source rows

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native.rs` | 1bc1c4acbb9396680175062487e6eb8920faf843390084a0206e442dd6a64a22 |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| I | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:44:41– | 2120..2121 | try_propagation | identity | Exactly 32 ASCII hexadecimal digits represent 0 through 2^128 - 1. Parsing fits u128. The zero check follows parsing. The test covers both accepted endpoints and earlier rejection. |
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
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:205:14– | 8013..8024 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:230:14– | 8811..8831 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:250:59– | 9548..9559 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| I | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:259:87– | 10028..10029 | try_propagation | main | The stream is nonzero. A successful exclusive file open installed verified storage with nonzero identity in a fresh unpoisoned writer. Before construction the only observation is a snapshot revision. Recovery, missing storage and foreign inputs have earlier exits. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:326:6– | 12322..12323 | try_propagation | announce | The closed-pipe process fails writing the capture line and never contacts the follower. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:327:19– | 12343..12344 | try_propagation | announce | A complete capture line followed by a failed flush stops delivery before TCP connect. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:345:7– | 12968..12969 | try_propagation | prepare | A missing primary yields NotFound; a healthy primary is opened and exported. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:346:93– | 13063..13064 | try_propagation | prepare | Direct helper call with a zero stream propagates Invalid before opening the destination. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:347:99– | 13164..13165 | try_propagation | prepare | An existing destination returns AlreadyExists without changing its bytes; a fresh path exports. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:357:94– | 13436..13437 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:357:97– | 13439..13440 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:360:7– | 13562..13563 | try_propagation | prepare_saved | A valid descriptor combined with missing primary propagates NotFound. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:361:89– | 13653..13654 | try_propagation | prepare_saved | Direct saved-export reopen with a zero stream propagates Invalid. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:365:7– | 13814..13815 | try_propagation | prepare_saved | A missing export propagates NotFound. The intact image and descriptor reopen with unchanged bytes. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:376:73– | 14098..14099 | try_propagation | deliver | The delivery gate reports capture output failure before connecting. A writable first line and successful flush let delivery proceed. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:383:52– | 14503..14504 | try_propagation | deliver | A writer that holds only the capture line refuses the acknowledgment line after the follower has made the install durable. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:384:23– | 14528..14529 | try_propagation | deliver | A writer that accepts both lines and refuses the second flush returns the I/O error. Retrying the same export preserves the installed revision and manifest size. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:398:15– | 14962..14963 | try_propagation | ship | The first authenticated peer reads a request byte and closes. The runner retries the original export and obtains its exact durable receipt. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:407:5– | 15140..15143 | for_iteration | retry_ship | Zero configured attempts covers empty iteration. Positive runs cover one attempted exchange and a second permitted attempt. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:408:9– | 15177..15182 | match | retry_ship | The scripted exchange supplies both returned receipts and terminal errors. The closed notice pipe still permits a second exchange. |
