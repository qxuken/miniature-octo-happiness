# Snapshot command output and receipt reporting

The snapshot CLI uses the fallible report writer for help, describe
and both poll reports. The local verified revision is written and
flushed before dialing a follower. The follower receipt is written
after the authenticated poll has verified the response.

| Scenario | Observed result |
| --- | --- |
| Closed stdout during help or describe | One structured broken-pipe diagnostic and exit 1. |
| Closed stdout before polling | The held listener has no queued connection. |
| First report flush fails | The original broken-pipe error is attributed to local output. The listener has no queued connection. |
| Receipt flush fails | The writer has seen a local revision 5 line and a distinct follower revision 4 receipt. The original output error is returned. |
| Fresh authenticated poll succeeds | Both exact lines are visible. The follower retains revision 4, its history and byte-identical manifest. |

The local CLI and injected writer probes exercise different failure
points. The capture and shipment writer tests also run. The original
CLI reproduces exit 101 on a closed stdout pipe.

## Preserved source decisions

The output refactor shifts source positions and moves the report body
into the shared local writer. The scoped evidence below is rebound to
the current source digest using the same conditions and source spans.

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native.rs` | `12a7d4a871c1475d134e1a886db50bfe110c071695ea990829b492591e46dd88` |

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

At `23f1b3079f302c892d65f657bbea25188bad0ef8`, Rust 1.100.0 debug and
release passed 51 CLI tests on arm64 macOS and 52 on arm64 Docker
Linux. The Linux source, builds and runtime files were on ext4.
Both profiles passed 11 core replication tests and strict Clippy on
both hosts. The separate long manifest growth and Linux exhaustion
cases stayed in their opt-in lanes. Five Windows x64 MSVC debug
targets cross-linked.

The five new controls reproduce both former print panics, an error
substitution at the local poll gate, an error substitution in the
shared writer and a swallowed receipt-output failure. All twelve
selected controls reached their expected assertions in every campaign.
Fresh extraction verified 176 command reports, the exact runners and
all 979 copied files per campaign. [Archive checksums](snapshot-command-output-23f1b30.sha256)

Source reconciliation preserves the existing readiness and dispatch
evidence. The shared flush and local poll report each have a new
propagation control. The refactor exposes one additional source record.
The ledger has 419 runtime and 7,514 tooling gaps.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p weekend-replicate
cargo test --offline --locked --all-features -p weekend-db --test replication
cargo xtask mutations --toolchain 1.100.0 --case r1-snapshot-command-output- \
  --case r1-snapshot-startup- --case r1-snapshot-dispatch- --output "$OUTPUT/debug"
```

Repeat with `--release` and a fresh mutation output path.
