# Study interactive command I/O

Readiness is a local command I/O operation. If its output pipe is closed,
the process reports `local_command/io/broken_pipe` and exits 1 before
starting the workers. The database bytes are unchanged. A native process
test closes the reader before starting the child and checks the exact
diagnostic category and exit status.

Once ready, the command loop reports a bad UTF-8 input line as
`local_command/io/invalid_data`. A refused reply preserves the
`BrokenPipe` error across the worker shutdown. Tests reopen the database
through the native file adapter. A separate test verifies that shutdown
closes a queued TCP connection. [Listener retirement ledger](study-listener-retirement-current.md)

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

The following local admission rows retain their original evidence. Their
functions and decision spans are byte-identical on the new source. The
current Study suite reruns the probes for those paths. [Local admission ledger](study-cli-admission-current.md)

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:60:9– | 1945..1948 | for_iteration | parse | Empty, singleton and both valid flag orders exercise traversal. Duplicate and unknown flags stop parsing. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:74:5– | 2380..2385 | match | expected_receipt | Empty operands, the selected receipt pair, wrong flag and wrong arity exercise the three arms. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:86:5– | 2706..2711 | match | main | Local dispatch and usage tests exercise the local commands. Native service tests enter both run modes. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:178:75– | 6454..6455 | try_propagation | initialize | Native initialization reaches the shared enrollment helper and reopens the record. The roster simulator tests the helper's collection-creation error. Native fault injection after successful file creation remains open. |

Startup address and listener failures, native worker failures and the
full peer status outcome matrix remain open.

## Verification

The frozen source `dab12105708ee037d6c68b15553c88432d62e3bf` passed with
Rust 1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4. Both
profiles passed 56 Study tests and 14 core CRDT tests on each host.
Strict Clippy passed on both hosts.

All 15 mutations were detected in each platform and profile. A fresh
extraction checked the expected assertion for each mutation, all 208
process reports, both runner binaries and the 952 copied files per
campaign. The archive checksums bind the extracted inputs and outputs to
the frozen candidate. [Archive checksums](study-interactive-dab1210.sha256)

The pinned macOS nightly `739adecd4` recorded both outcomes for each
condition in the table. Merging the extracted raw profiles and exporting
coverage from the extracted test executable reproduced the report byte
for byte.

| Condition in `native.rs` | True | False |
| --- | ---: | ---: |
| 344, structured incoming summary | 3 | 2 |
| 352, sync-only edit guard | 4 | 3 |
| 353, structured sync-only rejection | 2 | 2 |
| 363, structured local command error guard | 3 | 3 |

Fourteen named decisions and four new scoped rows account for 18 runtime
records. The refactor also exposes two propagation sites to the source
inventory. One is the diagnosed readiness wrapper. The listener address
query remains open.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 --case r1-study-interactive- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.
