# Coordinator connection lifecycle and shutdown

The service calls `next_connection` with the native readiness and accept
operations. The same function accepts a scripted schedule in the tests.
The service passes its worker handles and listener result to `join_workers`
after requesting shutdown and dropping its listener and request sender.

| Boundary | Observed outcome |
| --- | --- |
| Readiness wait | An idle result and `Interrupted` retry. Readiness proceeds to accept. A terminal `WouldBlock` from readiness retains its kind and message. |
| Accept | `WouldBlock` and `Interrupted` each resume the readiness loop. A live connection is returned. A terminal accept error retains its kind and message. |
| Stop timing | A stop before readiness skips both operations. A stop observed after readiness prevents a new accept. |
| Reaping | Finished workers are removed while a blocked worker keeps its slot. A finished panic is joined and becomes the connection-worker diagnostic. |
| Worker joins | An otherwise successful result becomes the shutdown panic diagnostic when a worker fails. An earlier service error survives a later worker failure, and all handles are joined. |
| Readiness output | Closing the read end of the startup pipe before spawn produces the original pipe diagnostic. The process exits, the image bytes match, and a new writer can reopen revision 2. |
| Capacity and framing | An excess connection gets `Busy`. Truncated and expired input never enters the queue. A rejected credential gets `Unauthorized` without queue admission. The configured and plain success cases use the same service. |

| Worker panics | Earlier service succeeded | Shutdown result |
| --- | --- | --- |
| False | True | Success |
| True | True | Worker panic |
| False | False | Original service error |
| True | False | Original service error |

The existing commit-and-stop test reopens the dispatched revision and verifies
that the queued slot remains empty. The readiness fixture exercises the
actual stdout error path. Injection of native socket setter and thread launch
errors, the final admission-time check, and stop registration retain their
own open records.

## Qualification

The frozen revision `b3a2a18ab8058fcf0ed916940a297f28ff0a6dc1` was
tested with Rust 1.100.0. Arm64 macOS and arm64 Docker Linux/ext4 each
passed 51 unit and 20 integration tests in debug and release, with two
opt-in tests ignored on macOS and three on Linux. All-target Clippy
passed with warnings denied on both platforms. Windows MSVC x64 linked
the unit and integration test executables in both profiles. Native
Windows execution remains pending for this source.

The runner detected all 24 named mutations in each Unix campaign. The
selection contains 13 lifecycle controls and 11 existing queue, handoff
and startup controls. Archived verification checked the exact passing and
failing named tests, process cleanup, 89 supervised commands and 1,024
restored source hashes per campaign. Reconciliation inventoried 10,776
decisions across 223 files with zero failures. This slice accounts for
20 additional runtime source records. The refactor added three records,
bringing the open runtime count from 144 to 127. The tooling count
remains 7,458.

Two earlier freezes failed the mutation runner's preflight. Moving the
stop guard made the older queue selector match twice. The new admission
selector then expected 24 occurrences of `command` and found nine in
the production part of the file. The corrected selectors each match a
complete guard once. The registry's production-source test passed.
[Frozen source and evidence checksums](coordinator-service-lifecycle-b3a2a18.sha256)

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/service.rs` | `d895230b5e4321bea46aaa21d1c11d20c53ef1202969ff2b622e1daf38c15375` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:37:5– | 941..944 | for_iteration | dispatch_requests | A closed empty queue returns. The expiry and shutdown fixtures observe dispatch and retirement of a nonempty queue. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:54:11– | 1497..1500 | pattern_test | reap_connections | The same position result controls the while. Completed workers are removed and a live worker is left for a later pass. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:72:65– | 2094..2099 | match | serve | Both profiles reach bind on their own format and reject the opposite format. The native CLI cases re-run for this source. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:96:13– | 3076..3081 | while | serve | Native service tests enter with a ready client and finish on operator stop. The scripted stop and readiness cases check the corresponding helper results. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:96:19– | 3082..3085 | pattern_test | serve | The enclosing while uses the same optional connection. The test run includes admitted connections and operator stop. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:141:19– | 4810..4815 | match | serve_connection | The queue and handoff fixtures pass plain commands. The configured process fixture admits matching credentials and rejects a different one. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:154:5– | 5191..5196 | match | serve_connection | The handoff and capacity probes observe admission, a full queue and a disconnected queue with distinct outcomes. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:161:26– | 5524..5529 | match | serve_connection | Holding the admission sender through the deadline and dropping it immediately produce the two timeout-mapping outcomes. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:178:9– | 6164..6169 | match | next_connection | The script visits false and true readiness, retries Interrupted and preserves a terminal readiness refusal. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:187:9– | 6451..6456 | match | next_connection | The script returns a connection, retries each transient accept failure and propagates a terminal accept failure. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:204:5– | 6931..6934 | for_iteration | join_workers | The join fixture verifies the later worker is finished after each earlier result. |
