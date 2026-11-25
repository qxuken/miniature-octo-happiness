# Coordinator connection handoff

An actual loopback client sends a complete `get` command. The test
controls the two channels between the connection and dispatcher. For
each failed handoff the connection returns its original I/O kind and
the client receives no reply bytes.

| Boundary | Observed result |
| --- | --- |
| Request queue already disconnected | `BrokenPipe` before queue admission |
| Request queued and admission sender dropped | `ConnectionAborted` |
| Admission sender held through the request deadline | `TimedOut` |
| Dispatcher admits the request and drops its reply sender | `ConnectionAborted` |
| Dispatcher admits the request and supplies a value | Exactly one reply, `value 2 0` |
| Live queue at capacity | Existing probe observes `error Busy` and verifies the waiting requests remain queued |

The transport fixture controls the dispatcher outcome. The existing
shutdown and expired-request tests exercise actual command dispatch and
persistence separately.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/service.rs` | `a5c5dd34ac9de1685f58631f3ccde36e15771baca3a5e3871e86d8869a2bbaf6` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:177:5– | 6111..6116 | match | serve_connection | The handoff and capacity probes observe admission, a full queue and a disconnected queue with distinct client and connection outcomes. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:184:26– | 6444..6449 | match | serve_connection | Holding the admission sender through the deadline and dropping it immediately produce the two timeout-result arms. |
