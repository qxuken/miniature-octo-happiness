# Study listener setup failures

Study configures the listener and reads its bound address before it
writes readiness or launches either worker. Both setup operations
now report failure in the `listen` phase with their original I/O error.
The test supplies typed errors at the operation boundary and uses a
real loopback listener and an enrolled database.

| Setup outcome | Observable result |
| --- | --- |
| Nonblocking setup returns PermissionDenied | Reports the original listen error and skips the address lookup. |
| Address lookup returns InvalidInput | Reports the original listen error after nonblocking mode is set. |
| Both operations succeed | Reports the bound address before accepting interactive commands. |

Neither error writes readiness or consumes a pending local practice
command. A client queued on the listener sees EOF or a closed
connection. Once the exclusive file lease is released, the file
matches its original bytes and an independent open reads revision 2
with zero recorded practice minutes.

## Preserved source decisions

Existing startup, interactive, worker and status controls follow
their operations into the shared listener setup function.

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native.rs` | `6514b7eaead040c724b38f12a03ad1e4116722b2ca4d7a97d3a1d4d7faff734c` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:337:87– | 11656..11657 | try_propagation | run_with_listener_ops | The CLI enters this helper during normal service startup. The launch fixture supplies both errors at the shared builder boundary. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:341:17– | 11786..11789 | for_iteration | run_with_listener_ops | Empty input returns cleanly. A normal line followed by invalid data preserves the first reply. Quit skips the queued command. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:344:21– | 11947..11952 | match | run_with_listener_ops | Blank, quit, push, peer, incoming, sync-only practice and the general command path each exercise an arm. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:356:33– | 12568..12573 | match | run_with_listener_ops | The same loop observes idle before pushing, a refused connection in each diagnostic mode, and an acknowledged state after retry. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:391:30– | 14542..14547 | match | run_with_listener_ops | A committed edit and both modes of rejected commands exercise each reply arm. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:450:5– | 16863..16868 | match | launch_workers | Both workers start on success. A refused outgoing launch stops and explicitly joins the receiver before returning the original error. A separate case makes the receiver panic during this cleanup. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:60:9– | 1945..1948 | for_iteration | parse | Empty, singleton and both valid flag orders exercise traversal. Duplicate and unknown flags stop parsing. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:74:5– | 2380..2385 | match | expected_receipt | Empty operands, the selected receipt pair, wrong flag and wrong arity exercise the three arms. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:86:5– | 2706..2711 | match | main | Local dispatch and usage tests exercise the local commands. Native service tests enter both run modes. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:178:75– | 6454..6455 | try_propagation | initialize | Native initialization reaches the shared enrollment helper and reopens the record. The roster simulator tests the helper's collection-creation error. Native fault injection after successful file creation remains open. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:291:7– | 9744..9745 | try_propagation | run_with_listener_ops | The shared diagnostic wrapper returns the readiness write result. A closed stdout pipe reports its local I/O phase and preserves the original bytes. |
