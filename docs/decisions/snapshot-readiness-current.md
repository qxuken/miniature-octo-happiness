# Snapshot follower startup diagnostics

The follower configures the listener and obtains its bound address
under the listen phase. It writes and flushes the readiness line
through the existing report writer before admitting any connection.

| Startup outcome | Process observation |
| --- | --- |
| Address already held by another listener | Exit 1 before readiness. The structured listen diagnostic preserves the native bind error. |
| Readiness pipe has no reader | Exit 1 with one local-command broken-pipe diagnostic. |
| Healthy stdout and listener | Readiness precedes application traffic. A fresh authenticated poll returns the previously verified durable receipt. |

The test checks the manifest and selected image byte for byte after
each failure. An independent status process returns the same
revision 4 and digest. The original closed-pipe fixture exited 101
from the readiness print panic.

Nonblocking mode and bound-address queries remain open for
forced native failure. The source inventory now exposes the
address-query propagation directly.

## Preserved source evidence

All production code preceding the listener setup is byte-identical
to the prior checkpoint. The following rows reattach its scoped
output and command evidence to the current whole-file digest.

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native.rs` | `34ccc9a74b1afa965a2104021e7cfca7ca7abeccaaa74523b5a89f725bead0f1` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:282:59– | 10629..10640 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:314:6– | 11858..11859 | try_propagation | announce | The closed-pipe process fails writing the capture line and never contacts the follower. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:315:19– | 11879..11880 | try_propagation | announce | A complete capture line followed by a failed flush stops delivery before TCP connect. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:321:7– | 12108..12109 | try_propagation | prepare | A missing primary yields NotFound; a healthy primary is opened and exported. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:322:93– | 12203..12204 | try_propagation | prepare | Direct helper call with a zero stream propagates Invalid before opening the destination. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:323:99– | 12304..12305 | try_propagation | prepare | An existing destination returns AlreadyExists without changing its bytes; a fresh path exports. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:333:94– | 12576..12577 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:333:97– | 12579..12580 | try_propagation | prepare_saved | A corrupt descriptor fails before checking even a missing primary. A valid descriptor permits the next operation. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:336:7– | 12702..12703 | try_propagation | prepare_saved | A valid descriptor combined with missing primary propagates NotFound. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:337:89– | 12793..12794 | try_propagation | prepare_saved | Direct saved-export reopen with a zero stream propagates Invalid. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:341:7– | 12954..12955 | try_propagation | prepare_saved | A missing export propagates NotFound. The intact image and descriptor reopen with unchanged bytes. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:352:73– | 13238..13239 | try_propagation | deliver | The delivery gate reports capture output failure before connecting. A writable first line and successful flush let delivery proceed. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:359:52– | 13643..13644 | try_propagation | deliver | A writer that holds only the capture line refuses the acknowledgment line after the follower has made the install durable. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:360:23– | 13668..13669 | try_propagation | deliver | A writer that accepts both lines and refuses the second flush returns the I/O error. Retrying the same export preserves the installed revision and manifest size. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:60:5– | 2555..2560 | match | ack_line | The initialized follower reports empty; after installation, each observer prints the same durable revision and digest. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:73:35– | 2993..2994 | try_propagation | print_report | Closed-pipe process cases return a local I/O diagnostic. Working stdout reports the expected text. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:92:5– | 3533..3538 | match | option | The option fixtures cover default, accepted argument pair and rejected argument shapes. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:94:26– | 3598..3610 | match_guard | option | The accepted option flag reaches numeric validation; the same-arity wrong flag returns usage. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:95:32– | 3647..3648 | try_propagation | option | Unit and CLI cases return the encoding or integer error for the corresponding invalid value. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:95:50– | 3665..3666 | try_propagation | option | Unit and CLI cases return the encoding or integer error for the corresponding invalid value. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:106:5– | 3939..3944 | match | main | The named-command cases and actual process suites exercise the dispatch arms, help, and usage. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:111:21– | 4092..4105 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:114:32– | 4245..4260 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:132:35– | 4915..4935 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:136:15– | 5130..5131 | try_propagation | main | An absent parent is rejected. Even with closed stdout, a valid initialization can be opened by status. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:139:35– | 5247..5260 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:154:35– | 5881..5904 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:159:92– | 6193..6194 | try_propagation | main | An inactive directory produces Invalid during maintenance. A regular inactive file is removed. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:165:35– | 6396..6419 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:200:14– | 7681..7693 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:210:52– | 8189..8203 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:217:49– | 8608..8609 | try_propagation | main | Closing stdout prevents the first capture line and reports an I/O diagnostic. A normal capture prints both exact lines. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:237:14– | 9094..9105 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:262:14– | 9892..9912 | match_guard | main | The exact verb reaches its phase. Changing only the verb returns usage and preserves the directory. |

## New propagation controls

| Source operation in `serve` | Deliberate change | Regression observation |
| --- | --- | --- |
| Bind error, bytes 16147..16148 | Replace the native bind result with a synthetic permission error | The diagnostic loses the recorded native error code. |
| Listen wrapper, bytes 16275..16276 | Route the same error to the local-command phase | The regression expects the listen phase. |
| Readiness output, bytes 16327..16328 | Restore the panicking stdout call | The closed-pipe process exits 101 and prints the panic. |

## Verification

Frozen source `350591c6d2eeb865f14ef65148d0c3fa6bb2f2c1` passed both
profiles on arm64 macOS and arm64 Docker Linux with Rust 1.100.0.
The source, builds and runtime temporary files were on ext4 in Docker.
Each profile passed 50 snapshot CLI tests on macOS, 51 on Linux and
11 core replication tests on each. The long manifest growth case
and the dedicated Linux exhaustion fixture stayed in their opt-in
qualification lanes. Strict Clippy passed on both hosts.

All three startup controls and four existing dispatch controls
failed their designated assertion in each campaign. Fresh extraction
verified 112 command reports, the exact runners and all 976 copied
files per campaign. Five Windows x64 MSVC debug targets cross-linked.
[Archive checksums](snapshot-readiness-350591c.sha256)

Final reconciliation preserves the 34 earlier scoped rows, records
three new propagation controls and exposes one additional listener
decision in the inventory. It leaves 420 runtime and 7,514 tooling
gaps.

The previous PR #82 head `fc05d80` finished with 28 passing checks
and one planned skip. That hosted run precedes the readiness fix.
[Engine checks](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45696024354),
[bounded fuzzing](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45696024163)

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p weekend-replicate
cargo test --offline --locked --all-features -p weekend-db --test replication
cargo xtask mutations --toolchain 1.100.0 --case r1-snapshot-startup- \
  --case r1-snapshot-dispatch- --output "$OUTPUT/debug"
```

Repeat with `--release` and a fresh mutation output path.

## Native Windows confirmation

The native Rust 1.100.0 Windows job at PR #82 head `06fb27c` passed the
snapshot CLI enrollment, bounded server, restart and polling regression
in both profiles. The closed readiness pipe and native bind error checks
were part of that regression. The downloaded artifact matches the exact
snapshot CLI source and test bytes at the test merge and PR head.
[Native minimum-Rust job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45706498857/job/139529935557), [Archive checksums](windows-msrv-06fb27c.sha256)
