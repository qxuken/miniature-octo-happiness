# Shared TLS deadline and cancellation decisions

Both binary targets compile the same TLS transport with their respective
deadline constants. The tests hold the absolute deadline fixed and drive
both an authenticated loopback channel and the existing socket operation
callback.

| Decision | Observable result |
| --- | --- |
| Socket deadline | A live deadline permits I/O. At zero remaining time the socket returns TimedOut ahead of the operation or cancellation check. |
| TLS stream construction | The wrapper preserves the supplied live deadline and refuses an expired socket with TimedOut. |
| Application I/O | After expiry a read leaves buffered plaintext in rustls and the caller buffer unchanged. Write and flush return TimedOut without adding plaintext or output. |
| Cancellation | With an active token each operation has a bounded polling slice. After a retryable timeout sets the token, the next iteration returns ConnectionAborted before operation reentry. |
| Retry guard | Without a token, each retryable error is returned. A live token retries WouldBlock and TimedOut. A live token with a terminal error returns the original error. Retrying preserves the absolute deadline. |
| Operation result | The callback returns its original terminal error or successful byte count. |

For the retry guard the timeout outcome is held fixed while token presence
changes. A second pair holds the token present and varies whether the error
is retryable. Both timeout alternatives have separate controls. The two
existing native socket timeout setter propagation sites retain their open
records. These cases exercise controlled deadlines and callback outcomes.
The existing stalled-handshake test also exercises real socket wait slices.
The monthly-workload example retains its own decision records.

## Exact source rows

| Source | SHA-256 |
| --- | --- |
| `apps/support/pinned_tls.rs` | `9672f006452a4c4ab9fde75f7461f9d44cf98c37fb1099deba726442253e697b` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:263:16– | 8298..8301 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:269:13– | 8557..8562 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:263:16– | 8298..8301 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:269:13– | 8557..8562 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
