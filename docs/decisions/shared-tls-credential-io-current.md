# Shared TLS credential file I/O

| Boundary | Observed outcomes |
| --- | --- |
| Certificate generation | The injected generator error keeps its concrete rcgen variant and message. A failed generation creates no directory. |
| Unix credential creation | An open failure leaves an empty private directory. A partial key write leaves one byte and skips the certificate. A certificate write failure leaves the complete key. Each error preserves its original kind and message. The incomplete directories fail to load. |
| Credential open and handle metadata | A refused open skips metadata and reads. A handle metadata error skips reads. A nonregular opened handle is rejected before reading, even after the path precheck passed. |
| Bounded credential read | A partial read returns the original read error and preserves the source bytes. Empty and over-limit data are rejected. |
| Credential mode and file type | Public files can be readable. Private keys require private mode on Unix. The path and opened handle both have to identify regular files. |

The Study binary, snapshot replication binary and live-month example each
compile and exercise the same credential path. The Unix provisioning
fixtures hold the finished prefix on disk and check the later calls were
skipped. This records the current failure behavior for the trusted
credential directory.

## Qualification

Frozen source `c6869a245a6a4139feb510a0f87ab55f12943cbf` used Rust 1.100.0 and locked
dependencies. Counts are per profile.

| Platform | Study | Snapshot | Live-month example | Named mutations caught |
| --- | ---: | ---: | ---: | ---: |
| macOS arm64 | 78 | 77 | 20 | 54 |
| Docker Linux arm64/ext4 | 78 | 78 | 20 | 54 |

One additional Linux storage test remained opt-in. Both packages passed
strict Clippy. Ten Windows MSVC x64 test executables linked in each profile.
Native execution remains pending for this source.

Fresh extraction verified all 192 supervised commands per Unix platform
and profile, 1,036 restored source hashes per campaign, the passing
baselines and each named failure. The credential batch reconciles
18 runtime records and 18 live-month tooling records. Reconciliation
leaves 58 runtime and 7,423 tooling gaps. Remaining runtime records are
in TLS session/transport and Windows storage.
[Frozen source and evidence checksums](shared-tls-credential-io-c6869a2.sha256)

## Carried source records

| Source | SHA-256 |
| --- | --- |
| `apps/support/pinned_tls.rs` | `8c90a079eb2242b292bf21f73e0f8a0ddda344169fa9382f1da40f34b55a3daa` |

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
