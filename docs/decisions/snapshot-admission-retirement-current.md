# Snapshot admission retirement decisions

The accept loop observes completion of the scoped follower before polling and
again after readiness. The same retirement functions run in the service and
under tests with real scoped worker handles. The tests release the follower
while readiness is pending and observe completion before returning ready or
timed out. A controlled handshake panic reaches the accept-loop reaper.

| Boundary | Outcomes held fixed for the decision pairs |
| --- | --- |
| Accept loop | A live follower with an exhausted limit causes no readiness call. A live follower with budget polls. A finished follower with budget ends polling. |
| Readiness return | Positive readiness with a live follower can accept. A follower that finishes during readiness leaves the socket unaccepted. |
| Finite drain | With the follower live, a completed handshake is removed and an active one is allowed to finish. An empty set skips the loop. A finished follower leaves pending handshakes for shutdown. |
| Pause | After reaping the final handshake the drain skips the pause. A held handshake enters the pause while it is live. |
| Cancellation | Successful admission with a live follower leaves cancellation clear. Either an admission error while the follower lives or a finished follower after success sets it. |
| Reaper error | The accept loop and drain each propagate the original error from a joined handshake panic. |

The ordinary controls retain the earlier readiness, launch, queue and join
witnesses. Linux qualification also runs the existing opt-in follower
exhaustion test on a private 8 MiB tmpfs. The real staging and manifest writes
exercise the last propagation branch. The test checks the old receipt after
reopen, exact bytes and the retained root, then verifies retry and repeat
delivery after space is freed. An omission control of the same source
site must fail the original server-exit assertion. This control uses the
opt-in native fixture and runs separately from the portable registry.

## Exact source rows

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native/admission.rs` | `2750283885e215f7bef0331283c9386cdfdf22d5161af2ce64d6bf5f588a995f` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:44:9– | 1179..1182 | for_iteration | serve | The existing process suites cover an empty follower after a rejected connection and one or more admitted exchanges with the current durable receipt. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:121:17– | 3711..3716 | match | admit_with | Timeout and interruption retry. Positive readiness reaches accept. A fatal readiness return preserves the supplied OS error. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:130:35– | 4130..4135 | match | admit_with | WouldBlock and Interrupted retry without consuming the admission limit. A fatal return preserves its OS error. Real loopback acceptance is exercised by the launch and queue tests. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:151:28– | 5036..5039 | pattern_test | admit_with | The same result drives the successful TLS pattern. A closed preauthentication client queues nothing. A validated connection queues until capacity. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:197:5– | 6484..6487 | for_iteration | join_handshakes | An exhausted zero limit joins no TLS workers. Nonempty lists join healthy and panicking threads even when an earlier admission error must survive. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:28:11– | 733..736 | pattern_test | reap | Same pattern as the completed-worker loop. A pending worker stays queued while a completed worker is joined. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:50:15– | 1553..1554 | try_propagation | serve | The Linux full-storage process test exercises successful channel service, staging refusal with the prior receipt, and a native selection write failure that terminates the follower with RecoveryRequired. The separate omission control suppresses that exit. |
