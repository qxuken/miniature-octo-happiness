# Follower overload and cancellation

## Blocked diagnostics regression

The Linux CLI regression at `848c188` fills the follower's stderr pipe
before it starts and keeps the pipe full. In the preceding build, four
rejected handshakes pinned all four workers in diagnostic writes. The
authenticated poll failed. The follower now keeps peer errors on the
connection path. With the same full pipe, the new server returns its exact
revision 4 receipt and exits after five accepted sockets. The manifest,
selected export and pipe contents compare byte for byte. Reopen verifies
the current row and retained revision 2.

## Canceling a blocked handshake

Both native Windows jobs at `9859b73` failed the same cancellation test.
On that build another handle tried to shut down a blocked socket. The
test still exceeded its five-second join budget and reported EOF
after the test released the raw peer.
[Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43800189584/job/133078305577),
[minimum Rust evidence](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43800189584/artifacts/14195294266)

At `7293df9ef636590f251336c85ee2ad95eb70f217`, the listener signals one
shared cancellation flag. Pending TLS reads and writes wait at most
100 ms per syscall and check the flag again between waits. They keep
the original absolute deadline. A completed handshake passes the same
remaining deadline to the authenticated channel. The shared test
observes an idle handshake across several waits and then checks
`ConnectionAborted` after cancellation. Local negative controls
independently remove the timed-wait retry and the cancellation check.
The first control fails after one wait interval, and the second reaches
the test's two-second cancellation timeout.

## Repeated saturation with a full diagnostics pipe

The `7293df9` release CLI ran on Docker Linux arm64/ext4 with Rust 1.99.0.
Its stderr pipe contained 65,536 bytes before the listener started and
remained full throughout. The first two phases held all four handshake
slots until the normal 30-second deadlines elapsed. The third held an
authenticated revision 5 transfer after one 4 KiB region and filled all
four authenticated queue slots. The sender's sockets stayed open until
the follower released them. All excess connections closed.

| Phase | Excess closes | Duration (s) | Peak sampled threads | Peak sampled file descriptors |
|---|---:|---:|---:|---:|
| First handshake saturation | 9,632 | 30.052 | 6 | 10 |
| Second handshake saturation | 8,896 | 30.055 | 6 | 10 |
| Authenticated queue saturation | 792 | 30.131 | 3 | 13 |

The process returned to two threads and six descriptors between phases.
Its Linux process RSS high-water mark was 4,068 KiB for this small fixture.
Every successful poll after overload returned the exact revision 4 receipt.
The selected image and manifest remained byte-identical until retry.
The same saved revision 5 image then installed successfully. Repeating
the shipment kept the manifest unchanged. After restart, the installed
bytes, receipt, current row and retained revision 2 verified. The pipe
still contained only its original prefill.

macOS arm64 with Rust 1.99.1 passed 50 application tests per build profile
and Docker Linux arm64 with Rust 1.99.0 passed 51, with debug and release
execution and strict all-target Clippy on each. The ordinary Linux suite
skipped the opt-in full-storage test. Both minimum Rust 1.99.0 and standard
Rust 1.99.1 cross-linked seven Windows MSVC x64 test executables each.
Native Windows x64 with Rust 1.99.0 and 1.99.1 passed the timed-wait and
follower cancellation tests in both debug and release at tested merge
`c13428b78f9b17a71477d80f1481225f8332925d` (PR head `3b63260`).
The uploaded source copies of the shared TLS helper, follower TLS adapter
and admission worker matched the PR head byte for byte. The minimum Rust
ZIP and both test transcripts matched their recorded SHA-256 values. The
standard job log records the tested merge and both sets of test passes.
[Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43802037286/job/133084312942),
[minimum Rust job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43802037286/job/133084312759),
[minimum Rust test artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43802037286/artifacts/14197491497)

The isolated Linux loopback profile leaves WAN behavior, fairness
under continuous saturation and deployment-specific resource budgets open.

The local archive `target/archived-proof/follower-overload-7293df9.tar.zst`
contains the frozen source, replay script, Rust load generator, logs and
exact old and new images. Extract it and run `bash run-soak.sh` with the
cached Cargo dependencies and the Docker image
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
The runner uses a private ext4 volume, loopback only, two virtual CPUs
and a 2 GiB container limit. It removes the container, volume and keys
after the run. The archive SHA-256 is
`4c36468d7c2c92bf776e31fde4f1c6adb4e9d5a3451c4230ef31c25e8b04373a`.
