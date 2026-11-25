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

## Verification

Frozen source `c4bab48499adffbc16030e78be763475dd9d7fe6` passed 43
coordinator tests in debug and release on arm64 macOS and arm64 Docker
Linux on ext4 with Rust 1.100.0. The ordinary suite left two opt-in native
maintenance tests ignored on both hosts and one dedicated-tmpfs test
ignored on Linux. Strict Clippy passed on both hosts.

The new handoff test caught four single-error substitutions. The existing
queue tests caught their three controls. Fresh extraction verified every
expected assertion, all 112 supervised command reports, both archived
runners and 970 copied source files per campaign.
[Archive checksums](coordinator-handoff-c4bab48.sha256)

The Windows coordinator oracle source-pin regression passed against the
existing 32 controls, and two Windows x64 MSVC debug test targets
cross-linked. The four new mutation controls run on Unix. Native Windows
execution of the new handoff regression awaits this batch's hosted run.

The source decision inventory is unchanged. The handoff evidence
classifies four runtime decisions, leaving 434 runtime and 7,514 tooling
gaps.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p write-coordinator
cargo xtask mutations --toolchain 1.100.0 \
  --case r1-connection-handoff- --case r1-coordinator-queue- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.
