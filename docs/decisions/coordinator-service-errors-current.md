# Coordinator service setup, listener faults and shutdown

The production listener uses the same lifecycle helpers as the fault
fixtures. The helper owns the listener and queue while workers run.
It publishes the stop flag, drops its listener and sender, then joins
the connection workers and the database owner.

| Boundary | Observed outcomes |
| --- | --- |
| Startup | Stop registration refusal leaves the path untouched. Listener setup refusal preserves its kind and message, frees the bound address, and leaves the image unchanged and writable at revision 2. |
| Database launch | A refused launch returns the original diagnostic. The listener and queue close and readiness is skipped. |
| Listener loop | A terminal accept error, panicked prior connection or refused connection launch each preserves the first error, joins the still-live connection and database workers, and releases the address. |
| Saturation | Nonblocking setup permits one Busy line. A refusal leaves the writer untouched. |
| Connection diagnosis | An error reaches the reporter unchanged. A success produces no diagnostic. The native process logs the malformed request and still serves the next valid request without changing the image. |
| Connection setup | The blocking-mode and write-timeout failures independently stop before reading or queuing. |
| Admission cutoff | A deadline expiring after the complete line produces the original timeout and closes the admission receiver before any reply. The expired request is unchanged, with its original deadline for dispatch retirement. |

Fault callbacks exercise the same error propagation and cleanup as the
real listener. Native coverage includes operator stop, short frames,
service startup, queue saturation and the real stdout failure path.
Platform stop-registration and readiness records remain open.

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/service.rs` | `0d69b2daf8815791a76e3c1374530f343317f2009001adf23f8f599296874a34` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:54:11– | 1497..1500 | pattern_test | reap_connections | The while loop uses the same completion pattern. Finished workers are removed and a live slot stays reserved. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:85:65– | 2469..2474 | match | serve_with | Each valid profile reaches native bind. The incompatible profile tests preserve their respective diagnostics. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:165:5– | 5565..5570 | while | serve_connections | The process enters with a ready client and exits on operator stop. The listener fault matrix exercises the same loop. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:165:11– | 5571..5574 | pattern_test | serve_connections | The successful optional connection has the same result as the enclosing while condition. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:219:19– | 7131..7136 | match | serve_connection_with | The capacity and retry tests exercise plain requests and the credentials fixture distinguishes the configured outcomes. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:232:5– | 7512..7517 | match | serve_connection_with | The queue fixtures observe admission, full capacity and disconnection with distinct responses. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:239:26– | 7845..7850 | match | serve_connection_with | The handoff fixture observes admission timeout and sender disconnection. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:256:9– | 8485..8490 | match | next_connection | The readiness schedule observes a live and idle result, Interrupted and terminal refusal. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:265:9– | 8772..8777 | match | next_connection | The accept schedule observes a live socket, both transient retries and a terminal error. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:282:5– | 9252..9255 | for_iteration | join_workers | The worker fixture observes completion of the last successful worker after each earlier outcome. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:37:5– | 941..944 | for_iteration | dispatch_requests | A closed channel stops dispatch. The shutdown and expiry fixtures observe a nonempty channel. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:185:8– | 6229..6232 | pattern_test | report_connection | The same Err pattern controls reporting. The probe observes the forwarded error, and the native process logs only the failed request. |
