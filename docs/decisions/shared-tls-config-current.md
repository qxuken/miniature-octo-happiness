# Shared TLS configuration and credential decisions

Both CLIs independently exercise credential loading, configuration and
loopback socket refusal with the same transport source.

| Boundary | Outcome |
| --- | --- |
| Unix credential directory | A missing parent retains NotFound. A second creation attempt retains AlreadyExists and preserves the original key and certificate bytes. The accepted directory and key have modes 0700 and 0600. |
| Credential loading | With the other file held valid, a missing certificate or key retains its original I/O error. A healthy load preserves the certificate and key. File-size and pre-open type checks still run. |
| Peer admission | The local leaf as the configured peer returns the specific self-pin error. A distinct, valid peer permits construction. A malformed peer certificate returns the Rustls root-store error. |
| Local key | A malformed local key and a valid key paired with the wrong certificate preserve the Rustls client builder error. |
| Socket open | A refused loopback connection returns the original I/O kind. The existing live listener cases cover acceptance. |
| Leaf pin | With two valid certificates present, putting the pinned certificate first accepts and placing it second rejects. Absent, empty and different leaves also reject. |

The locked Rustls verifier builder receives one parsed trust anchor and no
CRLs. The client name conversion receives a fixed valid label on each
binary. These input invariants account for two propagation records per
binary. The subsequent server key setup has its own open record. The ring
ECDSA implementation can request entropy during a second key setup even
after the client builder accepted the key. The Unix creation and file read
faults beyond the measured calls retain separate records.

## Exact source rows

| Source | SHA-256 |
| --- | --- |
| `apps/support/pinned_tls.rs` | `c27bb3e2158dd15c2fd3ba5242459ac61aaa68059bf05f7d98bbd6d39b786fd5` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:263:16– | 8298..8301 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:269:13– | 8557..8562 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:263:16– | 8298..8301 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:269:13– | 8557..8562 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:81:78– | 2756..2757 | try_propagation | new | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:111:53– | 3736..3737 | try_propagation | client_session | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:81:78– | 2756..2757 | try_propagation | new | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:111:53– | 3736..3737 | try_propagation | client_session | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
