# Shared TLS session and transport setup

| Boundary | Observed outcomes |
| --- | --- |
| Server configuration | A setup refusal preserves the specific Rustls error. The client and the trust root have already been configured. |
| Session construction | An invalid Rustls fragment-size configuration returns the constructor error in each role before using the expired socket. |
| Handshake entry | An expired socket returns TimedOut in each role. Its peer observes end of stream without receiving bytes. |
| Session completion | A finished and authenticated session returns TimedOut if its deadline has expired before admission. The server clears its listener cancellation token before transferring the stream. |
| Native accept configuration | An injected blocking-mode refusal preserves the error and ends the socket before reading or writing. |
| Socket deadlines | Read and write share timeout setup. An injected timeout setup failure skips socket I/O and preserves its code and diagnostic. Success passes the same socket and the requested time limit to the selected operation. |

The same TLS code is exercised in both binaries and the live-month
example. The example also runs its previously mapped configuration and
deadline controls against its own Cargo target. The native socket fixtures
use connected loopback endpoints. The setup failure fixtures inject errors
at the private call boundary.

## Qualification

Frozen source `0cd752bbdf77147b9c4182258c41735c78e5e9a0` used Rust 1.100.0 and locked
dependencies. Counts are per profile.

| Platform | Study | Snapshot | Live-month example | Named mutations caught |
| --- | ---: | ---: | ---: | ---: |
| macOS arm64 | 85 | 84 | 27 | 46 |
| Docker Linux arm64/ext4 | 85 | 85 | 27 | 46 |

One additional Linux storage test remained opt-in. Both packages passed
strict Clippy. Ten Windows MSVC x64 test executables linked in each profile.
Native execution remains pending for this source. The 46 controls include the existing reaper
control with its corrected readiness fixture.

Fresh extraction verified 173 supervised commands per Unix platform
and profile, 1,041 restored source hashes per campaign, the passing
baselines and each named failure. Sharing timeout setup removes one
duplicated source decision in each target. All shared TLS source records
have dispositions. The audit leaves 18 runtime gaps in Windows storage
and 7,384 tooling gaps.
[Frozen source and evidence checksums](shared-tls-transport-0cd752b.sha256)

## Carried source records

| Source | SHA-256 |
| --- | --- |
| `apps/support/pinned_tls.rs` | `c96c908c2d18af6ebf31d4dbaf0d3bcf56cfd395cf6b5ee29fe2b73f0416192c` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:115:78– | 3933..3934 | try_propagation | configure | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:153:53– | 5141..5142 | try_propagation | client_session_with | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:323:16– | 10293..10296 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:329:13– | 10552..10557 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:115:78– | 3933..3934 | try_propagation | configure | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | weekend-replicate::live_month | apps/support/pinned_tls.rs:115:78– | 3933..3934 | try_propagation | configure | Successful roots.add supplied an anchor, and the builder has no configured CRLs. Rustls 0.23.46 can return an empty-root or CRL parsing error from this builder. Both are infeasible for these arguments. |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:153:53– | 5141..5142 | try_propagation | client_session_with | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
| I | weekend-replicate::live_month | apps/support/pinned_tls.rs:153:53– | 5141..5142 | try_propagation | client_session_with | SERVER_NAME is the fixed valid ASCII DNS label used by this binary. The peer address and input records do not affect the conversion. |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:323:16– | 10293..10296 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| C | weekend-replicate::live_month | apps/support/pinned_tls.rs:323:16– | 10293..10296 | pattern_test | io | The optional token pattern and its enclosing if observe the same presence. With a token the operation receives a polling slice. |
| E | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:329:13– | 10552..10557 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
| E | weekend-replicate::live_month | apps/support/pinned_tls.rs:329:13– | 10552..10557 | match | io | Both retryable kinds resume with a token. Without one each timeout is returned. A terminal error preserves its message. The success result preserves the byte count. |
