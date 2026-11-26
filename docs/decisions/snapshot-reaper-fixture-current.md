# Snapshot handshake reaper fixture

The optimized macOS suite at `163ab32c79c2b1d967e03d4aaf95140ccfb31b91` timed
out in `acceptance_propagates_a_reaped_handshake_panic`. The fixture reported
readiness once. If the first nonblocking accept returned WouldBlock, the
admission loop correctly went back to polling, but the fixture only returned
false afterward. A directed refusal reproduced the original timeout.

The fixture now polls the actual loopback listener and injects one
WouldBlock before allowing the next accept. The test still requires the
exact reaper diagnostic and confirms the accepted peer socket closes.
The focused optimized test passed 100 repeats after the correction.
The complete macOS snapshot binary suite passed 75 tests.

## Carried source records

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native/admission.rs` | `1f8a4b980720e8fa5fee5aa7325af7390597dcb0f021ed990b40691677a0ad5a` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:44:9– | 1179..1182 | for_iteration | serve | The existing process suites cover an empty follower after a rejected connection and one or more admitted exchanges with the current durable receipt. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:50:15– | 1553..1554 | try_propagation | serve | The Linux full-storage process test exercises successful channel service, staging refusal with the prior receipt, and a native selection write failure that terminates the follower with RecoveryRequired. The separate omission control suppresses that exit. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:121:17– | 3711..3716 | match | admit_with | Timeout and interruption retry. Positive readiness reaches accept. A fatal readiness return preserves the supplied OS error. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:130:35– | 4130..4135 | match | admit_with | WouldBlock and Interrupted retry without consuming the admission limit. A fatal return preserves its OS error. Real loopback acceptance is exercised by the launch and queue tests. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:151:28– | 5036..5039 | pattern_test | admit_with | The same result drives the successful TLS pattern. A closed preauthentication client queues nothing. A validated connection queues until capacity. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:197:5– | 6484..6487 | for_iteration | join_handshakes | An exhausted zero limit joins no TLS workers. Nonempty lists join healthy and panicking threads even when an earlier admission error must survive. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/admission.rs:28:11– | 733..736 | pattern_test | reap | Same pattern as the completed-worker loop. A pending worker stays queued while a completed worker is joined. |
