# Snapshot listener setup failures

Snapshot service startup binds the listener, makes it nonblocking
and obtains the bound address before writing readiness. The new
test injects errors into the two configuration operations while
using a real loopback listener and a follower with a durable receipt.

| Configuration outcome | Observable result |
| --- | --- |
| Nonblocking setup returns PermissionDenied | The original listen error returns and the address callback is skipped. |
| Address lookup returns InvalidInput | The original listen error returns after successful nonblocking setup. |
| Both operations succeed | The CLI publishes the bound address and accepts authenticated traffic. |

Neither injected failure writes readiness. A client already queued
on the native listener observes EOF or a closed connection after
startup returns. The manifest and selected image stay byte-identical.
An independent open reads the same revision 4 receipt, current value
and retained root. The callback errors simulate failed operations.

The client timeout is installed while the listener remains open.
After a reset, macOS can reject a new timeout option with EINVAL.

## Preserved source decisions

The three existing bind, listen-diagnostic and readiness controls
follow the same operations into the tested startup function. The
earlier command and retry rows keep their current source bindings.

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native.rs` | `08bb98c0d40140e247b1560d73289c416538db846157b0cfb288935ebbbe802d` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:277:59– | 10644..10655 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:318:6– | 12097..12098 | try_propagation | announce | The closed-pipe process fails writing the capture line and never contacts the follower. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:319:19– | 12118..12119 | try_propagation | announce | A complete capture line followed by a failed flush stops delivery before TCP connect. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:325:7– | 12347..12348 | try_propagation | prepare | A missing primary yields NotFound; a healthy primary is opened and exported. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:326:93– | 12442..12443 | try_propagation | prepare | Direct helper call with a zero stream propagates Invalid before opening the destination. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:327:99– | 12543..12544 | try_propagation | prepare | An existing destination returns AlreadyExists without changing its bytes; a fresh path exports. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:337:94– | 12815..12816 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:337:97– | 12818..12819 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:340:7– | 12941..12942 | try_propagation | prepare_saved | A valid descriptor combined with missing primary propagates NotFound. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:341:89– | 13032..13033 | try_propagation | prepare_saved | Direct saved-export reopen with a zero stream propagates Invalid. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:345:7– | 13193..13194 | try_propagation | prepare_saved | A missing export propagates NotFound. The intact image and descriptor reopen with unchanged bytes. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:356:73– | 13477..13478 | try_propagation | deliver | The delivery gate reports capture output failure before connecting. A writable first line and successful flush let delivery proceed. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:363:52– | 13882..13883 | try_propagation | deliver | A writer that holds only the capture line refuses the acknowledgment line after the follower has made the install durable. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:364:23– | 13907..13908 | try_propagation | deliver | A writer that accepts both lines and refuses the second flush returns the I/O error. Retrying the same export preserves the installed revision and manifest size. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:378:15– | 14341..14342 | try_propagation | ship | The first authenticated peer reads a request byte and closes. The runner retries the original export and obtains its exact durable receipt. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:387:5– | 14519..14522 | for_iteration | retry_ship | Zero configured attempts covers empty iteration. Positive runs cover one attempted exchange and a second permitted attempt. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:388:9– | 14556..14561 | match | retry_ship | The scripted exchange supplies both returned receipts and terminal errors. The closed notice pipe still permits a second exchange. |
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
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:149:35– | 5896..5919 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:154:92– | 6208..6209 | try_propagation | main | An inactive directory produces Invalid during maintenance. A regular inactive file is removed. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:160:35– | 6411..6434 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:195:14– | 7696..7708 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:205:52– | 8204..8218 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:212:49– | 8623..8624 | try_propagation | main | Closing stdout prevents the first capture line and reports an I/O diagnostic. A normal capture prints both exact lines. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:232:14– | 9109..9120 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:257:14– | 9907..9927 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |

## Verification

Frozen source `0e633ce36e37132e1517ad166eef98f31eec2823` passed debug
and release on arm64 macOS and arm64 Docker Linux/ext4 with Rust 1.100.0.
Each profile passed 54 snapshot CLI tests on macOS, 55 on Linux and
11 core replication tests on both. The long manifest and Linux
exhaustion cases remained opt-in. Strict Clippy passed on both hosts.
Five Windows x64 MSVC debug targets cross-linked.

Both new controls replace the original error with Other. Each fails
the expected error-kind assertion. All 17 selected controls reached
their intended assertions in every campaign, including the earlier
bind and readiness controls. Fresh extraction checked 244 command
reports and 984 copied source files per campaign.
[Archive checksums](snapshot-listener-0e633ce.sha256)

Reconciliation closes the two remaining snapshot listener setup
propagation records and leaves 412 runtime and 7,514 tooling gaps.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p weekend-replicate
cargo test --offline --locked --all-features -p weekend-db --test replication
cargo xtask mutations --toolchain 1.100.0 --case r1-snapshot-listener- \
  --case r1-snapshot-retry- --case r1-snapshot-command-output- \
  --case r1-snapshot-startup- --case r1-snapshot-dispatch- --output "$OUTPUT/debug"
```

Repeat with `--release` and a fresh mutation output path.
