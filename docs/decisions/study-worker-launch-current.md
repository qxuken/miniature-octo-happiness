# Study session worker launch

A session launch error escapes through the same propagation point as
the native thread builder. The native entry point supplies the standard
thread builder. The test admits one authenticated session and holds it
until shutdown. The next accepted socket reaches a scripted launch
refusal with `WouldBlock`. The original error kind and message return,
the second socket closes, and the active session finishes before the
service returns. The final status counts one served session, zero failed
sessions and zero busy refusals.

The previously qualified incoming-worker cases run against the current
source. These scoped rows retain their original meanings and point to
the current spans. [Original incoming worker ledger](study-incoming-worker-current.md)

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native/admission.rs` | `adfe170e38702c1845b7eb56a2d69d5d0ae9bfbb4b104467b3cb6f43d7864959` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:37:9– | 958..963 | match | report | The empty listener result reports no prior error. The native diagnostic scenario reports an incoming error in text mode. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:54:9– | 1440..1445 | match | record | The capacity test checks successful service and failed handshakes in the same worker registry. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:103:17– | 2820..2825 | while | receive_with | Active admission observes no completed workers before accepting and reaps a completed panicked worker in a later iteration. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:103:23– | 2826..2829 | pattern_test | receive_with | The worker completion probe distinguishes an empty search from a completed worker index. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:106:35– | 3033..3038 | match | receive_with | Native accept drives service. The scripted listener supplies would-block, both retryable errors and the fatal error. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:135:25– | 4390..4395 | match | receive_with | The capacity test checks a pinned peer admission and exact counts for dropped handshakes. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:137:36– | 4485..4488 | pattern_test | receive_with | One authenticated session enters the merge and holds its guard while the next valid peer reaches the busy outcome. |
| E | study-peer::study-peer | apps/study-peer/src/native/admission.rs:158:9– | 5465..5468 | for_iteration | receive_with | A fatal listener error with no sessions retires cleanly. The active-session cases join healthy and panicked workers. |

## Verification

Frozen source `edc0ae4717863de1d5980680b640d37af6fb2c94` passed with
Rust 1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4.
Debug and release passed 63 Study tests and 14 core CRDT tests on each host.
Each profile detected all ten incoming-worker mutation controls, including
the new substitution of the launch error. Strict Clippy passed on both
hosts. Fresh extraction checked the named assertions, the exact runners,
156 command reports and 960 copied files per campaign.
[Archive checksums](study-launch-edc0ae4.sha256)

Five macOS probes with nightly `739adecd4` reran the listener and join
conditions. The extracted executable and raw profiles reproduced the
LLVM export byte for byte. The launch error is a deterministic refusal
at the builder boundary. Native OS thread exhaustion remains unobserved.

The current inventory has 444 runtime and 7,514 tooling gaps. All 17
records in the incoming admission file now have scoped evidence.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 --case r1-study-incoming- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.
