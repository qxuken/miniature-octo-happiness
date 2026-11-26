# TLS socket-refusal fixture

The Ubuntu native job for `c44c26d` observed ConnectionReset in the
refusal test. A directed macOS run reproduced that exact result by
rebinding the released loopback address and letting the replacement peer
close during the client handshake.

The corrected fixture supplies ConnectionRefused at the socket-opening
boundary. It checks the address, timeout, error type, kind and diagnostic.
Normal calls use TcpStream::connect_timeout through that boundary.
Existing loopback sessions and integration tests exercise the real
connection and handshake. The focused corrected test passed 100
optimized repetitions.

[Original Ubuntu job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45756615197/job/139696612741)

## Qualification

Frozen source `9e50334d202c8a860e0295a4cd9b11b79db5e24e` used Rust 1.100.0. Each profile passed
196 tests on macOS arm64 and 197 on Docker Linux arm64/ext4. The Study
binary, snapshot binary and live-month example each caught its refusal
mutation on both platforms. Windows MSVC x64 linked ten test
executables in each profile. Hosted verification of this fix is pending.

Fresh extraction verified the original and directed failure logs, the
corrected test, the intended mutation failures and the exact source
copies. [Frozen source and evidence checksums](tls-connect-refusal-9e50334.sha256)

## Carried source records

| Source | SHA-256 |
| --- | --- |
| `apps/support/pinned_tls.rs` | `056b374b234ba260a66cfe1916300838d5861594a61a53bc9c1b8cc0ac889e3c` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:331:16– | 10536..10539 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:337:13– | 10795..10800 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:115:78– | 3933..3934 | try_propagation | configure | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:161:53– | 5384..5385 | try_propagation | client_session_with | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
| C | weekend-replicate::live_month | apps/support/pinned_tls.rs:331:16– | 10536..10539 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | weekend-replicate::live_month | apps/support/pinned_tls.rs:337:13– | 10795..10800 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| I | weekend-replicate::live_month | apps/support/pinned_tls.rs:115:78– | 3933..3934 | try_propagation | configure | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | weekend-replicate::live_month | apps/support/pinned_tls.rs:161:53– | 5384..5385 | try_propagation | client_session_with | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:331:16– | 10536..10539 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:337:13– | 10795..10800 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:115:78– | 3933..3934 | try_propagation | configure | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:161:53– | 5384..5385 | try_propagation | client_session_with | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
