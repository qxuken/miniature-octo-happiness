# Coordinator platform signals and shared listener readiness

The native listener passes the status from poll or WSAPoll to a small
result decoder. Only the failing status invokes the native error reader.
The coordinator uses normal signal registration and console callbacks
through the same shutdown boundaries exercised by the fixtures.

| Boundary | Observed outcomes |
| --- | --- |
| Real listener | The idle socket reports false. A local connection makes it ready, and accepting the socket restores the idle result. The test runs in both applications. |
| Unix poll result | Zero is idle and one is ready. A negative status preserves the Interrupted error and diagnostic. Error retrieval is called once on failure and is skipped on success. |
| Windows WSAPoll result | Idle and ready skip error retrieval. The SOCKET_ERROR result returns the preserved Winsock number. Both applications exercise the same production decoder. |
| Unix registration and rollback | Refusing the first registration propagates PermissionDenied. Refusing the second removes the first real handler. A normal drop unregisters both handlers in the completed registration. |
| Windows console registration | A detached process skips registration. An attached successful registration reads no last error. An attached failure preserves the Windows error number. |
| Windows console events and teardown | Interrupt and break independently mark stop and return handled. Another event leaves the flag clear. Removal is called once for an installed callback and is skipped for a detached process. |

The Windows result and callback logic also executes in portable unit tests.
Native Windows operator-stop and listener execution provide the platform
evidence for the Win32 calls at the linked boundary.

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/stop.rs` | `ceb965a764f6bb90386fff7d031918c390e66fa1d72919e860836da0ee54cfc5` |
| `apps/support/listener_ready.rs` | `0ea50472dbdbf0a26d98252bf93909e4b090fb675024d12ccc206f9de79c8dd9` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/stop.rs:31:9– | 861..864 | for_iteration | register_with | Both scheduled signals install in order. The injected first and second refusals leave zero and one installed handler for cleanup. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/stop.rs:46:9– | 1185..1188 | for_iteration | drop | The empty rollback, partial rollback and normal drop inspect zero, one and two handler registrations. Re-unregistering the handles confirms each was removed. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/stop.rs:120:8– | 3199..3226 | short_circuit_and | register_console | The same attachment and native return values control the enclosing if. The detached callback would fail if called, and the probe confirms it is skipped. |
