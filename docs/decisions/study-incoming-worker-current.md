# Study incoming worker shutdown

The native entry point supplies `TcpListener::accept` to the same loop
used by the tests. The test listener injects individual accept results.
The session tests keep a real local listener and pinned peer credentials.

A scripted listener retries `WouldBlock`, `Interrupted` and
`ConnectionAborted`, then returns the original `PermissionDenied`.
It admits no sessions and sets the stop flag. The existing capacity test
also observes real nonblocking accept, four pending handshakes, refusal
at the session cap, and at most one authenticated merge.

| Completion observed by the listener | Result | Successful sessions |
| --- | --- | ---: |
| An active session finishes on normal shutdown | Success | 1 |
| The listener shuts down normally and the final join finds a panic | `Error::Panic` | 0 |
| The listener reports a fatal I/O error and the final join finds a panic | Original `PermissionDenied` | 0 |
| A completed worker panics while admission is active | The reaping join returns `Error::Panic` and admission stops | 0 |

The final-join condition changes outcome with either operand independently.
The healthy and panicked sessions share normal shutdown for the first
pair. The two panicked sessions vary whether the listener already failed
for the second pair. Native thread-creation failure retains its own
open propagation record.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native/admission.rs` | `6b990a48452dda2ba7fae7e604fcc61f6f0dc80853983c7d8e56bc0b0b7bdd0c` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:37:9– | 958..963 | match | report | The empty listener result reports no prior error. The native diagnostic scenario reports an incoming error in text mode. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:54:9– | 1440..1445 | match | record | The capacity test checks successful service and failed handshakes in the same worker registry. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:94:17– | 2675..2680 | while | receive_with_accept | Active admission observes no completed workers before accepting and reaps a completed panicked worker in a later iteration. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:94:23– | 2681..2684 | pattern_test | receive_with_accept | The worker completion probe distinguishes an empty search from a completed worker index. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:97:35– | 2888..2893 | match | receive_with_accept | Native accept drives service. The scripted listener supplies would-block, both retryable errors and the fatal error. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:125:21– | 4186..4191 | match | receive_with_accept | The capacity test checks a pinned peer admission and exact counts for dropped handshakes. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:127:32– | 4273..4276 | pattern_test | receive_with_accept | One authenticated session enters the merge and holds its guard while the next valid peer reaches the busy outcome. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:147:9– | 5186..5189 | for_iteration | receive_with_accept | A fatal listener error with no sessions retires cleanly. The active-session cases join healthy and panicked workers. |

## Verification

Frozen source `2d87db34da4dbc0eb4802908711a97b93368fc30` passed with Rust
1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4. Each debug
and release configuration passed 62 Study tests, 14 core CRDT tests and
all nine mutation controls. Strict Clippy passed on both hosts. Fresh
extraction verified the exact failing assertions, all 140 supervised
command reports, the runners and 958 copied files per campaign.
[Archive checksums](study-incoming-2d87db3.sha256)

The macOS profile from nightly `739adecd4` observed both outcomes at the
loop and would-block guards, the session cap, the merge lock pattern and
both final-join conditions. Two separate mutations and the scripted error
sequence verify each retryable error within the `matches!` guard.
The extracted executable and raw profiles reproduced the LLVM export
byte for byte.

Seven named decisions, the coupled final-join expression and eight
scoped rows account for 16 runtime records. Reconciliation leaves 445
runtime and 7,514 tooling gaps.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 --case r1-study-incoming- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.
