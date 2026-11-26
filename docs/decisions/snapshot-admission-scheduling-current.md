# Snapshot listener admission scheduling decisions

The listener uses its native readiness and accept operations and launches named
scoped workers. The private operation callbacks let a deterministic test supply
individual readiness, accept and launch errors. The thread and socket ownership
and the authenticated queue are exercised with real loopback connections.

| Boundary | Observable result |
| --- | --- |
| Readiness | A timeout or Interrupted retries without accepting. Ready proceeds to accept. A fatal result retains the supplied OS error. |
| Accept | WouldBlock and Interrupted retry with the same accept budget. A fatal result retains the supplied OS error. The zero budget makes no readiness call. |
| Worker launch | Follower launch refusal closes queued connections. Handshake launch refusal closes the new connection and cancels and joins the stalled worker. Both preserve the original error. |
| Handshake admission | At capacity the excess socket closes while the admitted sockets remain live. Unsuccessful handshakes do not queue a channel. Successful ones reach the bounded ready queue. |
| Worker join | A completed worker is reaped while a live worker stays queued. A healthy shutdown succeeds. A worker panic wins over success and preserves an earlier admission error. |
| Finish | Follower panic precedes a native listener error. Otherwise the original native error takes precedence over the follower diagnostic. |

For the shutdown guard, the pair with an otherwise successful admission holds
that outcome fixed and varies whether the joined worker panics. A second pair
holds the panic fixed and varies the previous admission result. The predicates
overlap the source short-circuit record. The two accept retry errors are tested
independently because the scanner sees the pattern macro as one predicate.

The exhausted budget case covers one operand of the accept loop. The other
loop and cancellation operands, the post-readiness follower exit, the drain
sleep guard, both reaper call sites, and the real follower recovery-required
error still need their own evidence. Simulator recovery tests remain scoped
to their storage backend. The launch refusals are injected at the builder
boundary and preserve the original error through the same launch pipeline.

## Qualification

Frozen source `1662c2f408f08f3ab691315fe99b306d1d483cd8` ran with
Rust 1.100.0 and locked dependencies.

| Platform | Debug and release, per profile |
| --- | --- |
| macOS arm64 | 63 snapshot CLI tests passed and 11 core replication tests passed with one ignored. The exact tests detected all 18 controls. |
| Docker Linux arm64/ext4 | 64 snapshot CLI tests passed with one ignored and 11 core replication tests passed with one ignored. The exact tests detected all 18 controls. |
| Windows x64/MSVC | Five CLI test executables cross-linked. Native execution of this source remains open. |

Clippy with warnings denied passed on macOS and Linux. Fresh extraction
verified the exact failed assertions, 64 supervised commands and 1,005
restored source inputs per campaign. Of 29 source records in this module,
11 remain open. The repository inventory has 308 runtime and 7,468
tooling gaps. [Frozen campaign checksums](snapshot-admission-scheduling-1662c2f.sha256)

## Exact source rows

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native/admission.rs` | `7b26ee0b29a9cd70e955fec57b8eba0742e4fa66c7c8565751898ba8dc6dbd0f` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:27:11– | 640..643 | pattern_test | reap | Same pattern as the completed-worker loop. A pending worker stays queued while a completed worker is joined. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:108:17– | 3228..3233 | match | admit_with | Timeout and interruption retry. Positive readiness reaches accept. A fatal readiness return preserves the supplied OS error. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:117:35– | 3636..3641 | match | admit_with | WouldBlock and Interrupted retry without consuming the admission limit. A fatal return preserves its OS error. Real loopback acceptance is exercised by the launch and queue tests. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:138:28– | 4542..4545 | pattern_test | admit_with | The same result drives the successful TLS pattern. A closed preauthentication client queues nothing. A validated connection queues until capacity. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:168:5– | 5689..5692 | for_iteration | join_handshakes | An exhausted zero limit joins no TLS workers. Nonempty lists join healthy and panicking threads even when an earlier admission error must survive. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:43:9– | 1086..1089 | for_iteration | serve | The existing process suites cover an empty follower after a rejected connection and one or more admitted exchanges with the current durable receipt. |
