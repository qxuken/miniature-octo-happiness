# Study listener retirement

The macOS release suite at `f297851` failed when an interactive test
tried to bind its former ephemeral address after shutdown. A directed
replay inserted a competing listener after the service returned. The
competitor acquired the address, and the old assertion then failed
with the same `AddrInUse` error. The original failure log and directed
replay are retained separately. [Reproduction checksums](credential-file-rejected-f297851.sha256)

The interactive tests still bound how long they wait for shutdown,
check exact replies and reopen the database to verify the committed
state. The listener retirement test connects a client before invoking
an already-stopped receiver. The client stays queued on that listener
and sees EOF or a closed-connection error when the receiver returns.
A control that omits the listener drop leaves the client open and the
read times out.

The test-only changes preserve the same production decision spans as
the worker launch campaign. The same full Study suites rerun its
original probes. [Worker launch ledger](study-worker-launch-current.md)

## Preserved source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native/admission.rs` | `45f09848e1f7aa738a5030bd444454162c1d7a73b2bf3d9fce31076995cd344d` |

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

The same frozen macOS and ext4 Linux campaigns pass the queued-client
probe and detect the omitted-close control in debug and release.
The 15 existing interactive controls still reach their expected
assertions on both hosts. Sixteen additional runs of the macOS
interactive release test passed. The Windows debug test targets
cross-linked. [Campaign results](credential-file-admission-current.md#verification)
