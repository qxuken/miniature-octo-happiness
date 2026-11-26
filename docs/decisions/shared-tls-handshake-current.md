# Shared TLS handshake admission and I/O

| Boundary | Observed outcomes |
| --- | --- |
| First handshake I/O | A failed client write preserves BrokenPipe and its message. A failed server read preserves ConnectionReset and its message. |
| Handshake and pending output | A complete drained handshake succeeds. Withholding Finished leaves the handshake in progress while the write queue is clear. Blocking the final write leaves pending output after the handshake completes. Either condition rejects independently. |
| Exact leaf check | With the handshake complete, the real certificate verified and ALPN agreed, a different application pin returns the exact pin diagnostic. |
| ALPN | With a completed handshake, flushed output and the exact pin, omitting the peer's protocol list produces no negotiated ALPN. The application returns its exact protocol diagnostic. |

Each probe runs in both endpoint roles in the Study binary, snapshot
replication binary and live-month example. The transport passes actual
Rustls records between two connections. The fixtures change peer
configuration and inject I/O results at the private transport boundary.

## Carried source records

| Source | SHA-256 |
| --- | --- |
| `apps/support/pinned_tls.rs` | `309e4f892388147b5061652c30e11b4fb8bf3775458e01294f6744da2a6df39d` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:100:78– | 3393..3394 | try_propagation | new | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:130:53– | 4373..4374 | try_propagation | client_session | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:282:16– | 8935..8938 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:288:13– | 9194..9199 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:100:78– | 3393..3394 | try_propagation | new | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:130:53– | 4373..4374 | try_propagation | client_session | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:282:16– | 8935..8938 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:288:13– | 9194..9199 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
