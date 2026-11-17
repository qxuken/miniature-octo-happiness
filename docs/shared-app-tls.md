# Shared private application TLS

This report records the hardening checkpoint `ed8f9f9`. The subsequent private
handshake seam, decision pairs and current source binding are documented in
[TLS handshake admission](tls-handshake-decisions.md).

`apps/support/pinned_tls.rs` is compiled under fixed profile bindings in both
study-peer and weekend-replicate. No public crate or database transport API was
added. Study binds SAN `study-peer`, ALPN `study-progress/1`, and a three-second
absolute I/O budget; snapshot binds SAN `weekend-replicate`, ALPN
`weekend-snapshot/1`, and thirty seconds. Connect consumes the client budget;
local disk operations remain outside it.

Independent real TCP reproduction against old study code admitted a session
with the exact pin present while final client Finished was withheld and rustls
was still handshaking. Another control offered only snapshot ALPN; old study
accepted with no ALPN. These demonstrate premature transport admission and
missing service binding, not an unauthorized database commit. Old loading also
accepted oversized credentials and 0644 keys and blocked opening a FIFO.

The shared implementation applies snapshot's handshake/write-drain, exact-pin,
ALPN and credential protections to both apps. Shared real TCP tests run under
each actual profile: authorized peers, unknown identities, signed alternate
leaves under the configured trust anchor rejected by exact pins, missing/other-
app ALPN and partial Finished with pin/ALPN already present. Credential tests
cover caps, FIFOs, private modes and valid loading. Existing process scenarios
retain study offline edits/lost reply/restart/repeat-sync and snapshot immutable
export/separate durable acknowledgment/recovery/maintenance oracles.

Upgrade both study CLIs together: old peers without required ALPN fail admission.
On-disk and application wire formats do not change. Trusted local directories
and external Windows ACL protection remain operational preconditions; this does
not implement race-proof hostile-path traversal or reject every symlink to a
trusted regular file. No core publication barrier or snapshot wire version changed.

Evidence is directed, not full TLS MC/DC: pending-write-only failure and client
partial-server-flight independence remain residuals. Dependency/OS failures are
delegated. Shared source has separate per-binary inventory records; reuse does
not merge the two target scopes.

Qualification: macOS and isolated aarch64 Linux Docker each passed debug/release
(14 study and 19 snapshot tests per profile) and strict all-target Clippy.
Mise-pinned Rust 1.99.1 plus cargo-xwin 0.23.1/SDK10.0.26100/CRT14.44.17.14
cross-linked both apps' five Windows test executables; no native Windows run
is claimed. Docker image ID c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
binds rustc 1.99.1/LLVM23.1.1. Logs: `/tmp/shared-tls-{debug,release,clippy,linux,windows}.log`.
Independent reviewer reproduced the old admission failures and checked fixed
negative and completed-handshake positive cases. The first release run exposed
a study fixture collision in PID/time-based temporary names; an atomic sequence
now distinguishes simultaneous test directories, and final suites passed.

The coordinator had the same blocking-secret-file hazard. A real CLI regression
first reproduced the FIFO hang with a five-second deadline and child cleanup.
It now checks both server and client rejection before input/database work, with
sentinel database bytes unchanged. Existing valid-secret authentication and
durable-receipt tests still pass. All five unit and five integration tests passed
in debug and release on macOS and Docker Linux; macOS strict Clippy passed.
Pre-open and opened-handle checks require a trusted, stable configuration
namespace; they do not establish resistance to hostile path replacement races.

Root review independently regenerated the inventory and matched all 125 snapshot
and 222 distributed-app records by target, source span, byte range, kind and
owner. App source/test hashes match the authors' evidence and the independent
TLS review. Sixteen evidence records/logs are preserved locally under
`target/local-qualification-shared-tls/index.json`, with SHA-256 digests. This
ignored bundle is not a published artifact.
All three applications also passed an offline, locked, all-target compile check
from the private source-copy helper's output. The new shared file was present;
compiler dependency files referenced no original application/library source.
This checks source-copy completeness, not package runtime relocation or isolation.

The broader sanitizer, coverage, fuzz and package results remain bound to
[2490631](local-qualification-2490631.md); they are not rerun claims for this
application change. The separate [unsafe-boundary review](unsafe-review-2490631.md)
also retains its stated source and platform scope. Native Windows remains open:
NTFS persistence/locking/reparse behavior, process interruption, socket/TLS
execution, externally provisioned ACLs, C/C++ consumers and private DLL relocation
need execution on Windows. Cross-linking does not qualify those behaviors.

## Deadline checks on buffered TLS I/O

The previous layer checked its absolute deadline only when rustls touched the
socket. A pinned TLS loopback test left one decrypted byte buffered and
expired the deadline. The old stream still returned the byte and accepted a
new write. The shared stream now checks that same deadline after the
handshake and before application read, write and flush. Socket calls still
use the remaining budget. The regression verifies that the expired read
leaves the buffered byte intact and that the write queues no new TLS output.
Both the snapshot and study binaries run that regression.

A separate snapshot test installs revision 4 and sends one 4 KiB region
of revision 5 over authenticated TCP. It keeps the sender socket open
while the follower's two-second test budget expires. The exchange returns
`Io`, the manifest and selected image stay byte-identical and a new poll
returns the exact revision 4 receipt with retained revision 2. Retry uses
the same capture, installs and reopens the exact revision 5 image, and a
duplicate leaves the manifest length unchanged. The production socket
budgets remain 30 seconds for snapshot and three seconds for study.

At `d1dec603f7d737d86c4cfefccffa01987c68dc8c`, macOS arm64 (Rust 1.99.1)
and Docker Linux arm64 (Rust 1.99.0) passed all 45 ordinary tests per build
profile in debug and release, plus all-target Clippy with warnings denied.
The opt-in Linux full-storage fixture was skipped. Windows x64 MSVC
cross-linked all seven test executables with cargo-xwin 0.23.1. Native
Windows execution remains open for this change.

```sh
cargo test --offline --locked -p weekend-replicate -p study-peer
cargo test --offline --locked --release -p weekend-replicate -p study-peer
cargo clippy --offline --locked -p weekend-replicate -p study-peer --all-targets -- -D warnings
```

The local archive `target/archived-proof/shared-tls-deadline-d1dec60.tar.zst`
contains both source checkpoints, the failing regression patch for the
baseline, platform logs and Windows test executable hashes. The Docker
image ID is `sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
The archive SHA-256 is
`40d4eb5463ac851d9446cf75814c8e800718a76ec4d55f351c5d6e9ef7981c3f`.
