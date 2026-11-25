# Study service worker shutdown

The daemon now joins both workers before deciding which outcome to
return. In the earlier ordering, a receiving error could leave the
outgoing worker to Rust's implicit scope join. If the outgoing worker
panicked, that join could unwind the service and lose the receiver's
error. The focused test failed with a scoped-thread panic when run
against the earlier join order.

| Receiving outcome | Outgoing outcome | Local outcome | Reported outcome |
| --- | --- | --- | --- |
| Success | Success | Success | Success |
| Success | Success | Broken pipe | Original local command I/O error |
| Success | Panic | Broken pipe | Shutdown `Error::Panic` |
| Panic | Success | Success | Shutdown `Error::Panic` |
| Connection aborted | Panic | Broken pipe | Original incoming I/O error |
| Panic | Panic | Broken pipe | Shutdown `Error::Panic` |

The probes check both workers finish before the result returns, and
verify each returned error and its structured phase. Two mutation
controls delay the outgoing join past one of the receiver's failure
checks and restore the scoped-thread panic. The three propagation
controls substitute the error at the receiver join, receiver result and
outgoing join, respectively.

## Scoped source decisions

The startup and command rows below move with the current source.
Their original observations remain in the startup ledger.
[Service startup ledger](study-service-launch-current.md)

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native.rs` | `31436dd88779d36821eaf7008cb203c70bc0dbc6be214e882dfab3dda14f1def` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:60:9– | 1945..1948 | for_iteration | parse | Empty, singleton and both valid flag orders exercise traversal. Duplicate and unknown flags stop parsing. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:74:5– | 2380..2385 | match | expected_receipt | Empty operands, the selected receipt pair, wrong flag and wrong arity exercise the three arms. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:86:5– | 2706..2711 | match | main | Local dispatch and usage tests exercise the local commands. Native service tests enter both run modes. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:178:75– | 6454..6455 | try_propagation | initialize | Native initialization reaches the shared enrollment helper and reopens the record. The roster simulator tests the helper's collection-creation error. Native fault injection after successful file creation remains open. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:263:7– | 9011..9012 | try_propagation | run_with_io | The shared diagnostic wrapper returns the readiness write result. A closed stdout pipe reports its local I/O phase and preserves the original bytes. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:309:87– | 10923..10924 | try_propagation | run_with_io | The CLI enters this helper during normal service startup. The launch fixture supplies both errors at the shared builder boundary. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:313:17– | 11053..11056 | for_iteration | run_with_io | Empty input returns cleanly. A normal line followed by invalid data preserves the first reply. Quit skips the queued command. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:316:21– | 11214..11219 | match | run_with_io | Blank, quit, push, peer, incoming, sync-only practice and the general command path each exercise an arm. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:328:33– | 11835..11840 | match | run_with_io | The same loop observes idle before pushing, a refused connection in each diagnostic mode, and an acknowledged state after retry. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:363:30– | 13809..13814 | match | run_with_io | A committed edit and both modes of rejected commands exercise each reply arm. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:422:5– | 16130..16135 | match | launch_workers | Both workers start on success. A refused outgoing launch stops and explicitly joins the receiver before returning the original error. A separate case makes the receiver panic during this cleanup. |
