# Study Windows startup and retry fixtures

The minimum-Rust Windows job for PR #82 head `e990a9f` compiled Study and
reported two failed unit tests in its x64 MSVC debug run. The runner used
the PR merge source `7100c9923a6d7a4ff1b584200c2c6aa39a4f134b`.
The job's `debug-1.stdout` contains both failures.
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45693315332/job/139484132013)

The native Windows job reported the same two test failures. The Study
files in the fetched merge source match the pushed head. The stored
minimum-Rust artifact matches GitHub's published SHA-256.
[Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45693315332/job/139484132487)

| Fixture | Hosted failure | Correction |
| --- | --- | --- |
| Local startup dispatch | Windows returned the intentional key-generation refusal and the test unwrapped it | Assert the exact refusal and absence of a credentials directory on Windows. Unix still verifies both generated files. |
| Outgoing status and retry | The direct TCP probe to a released port returned `TimedOut` | Retain the same listener across attempts. End the server's send half on the first accepted TCP connection and keep the socket open until the client's EOF report. Authenticate and serve the next two attempts normally. |

The retry probe checks the exact native EOF text or structured
`connect/io/unexpected_eof`, preserved receiver bytes before the retry,
and the resulting local and acknowledged state. The two successful
exchanges produce two acknowledgments and one receiver revision advance.
Reopening retains revision 3 and seven minutes. The existing two-process
lost-reply test continues to exercise the full receiving service.

## Verification

Frozen source `45ad8f23a23d20f967b78d7374cf04928374b16c` passed with
Rust 1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4.
Each debug and release run passed 65 Study tests, 14 core CRDT tests,
and all eight selected mutation controls. Strict Clippy passed both
hosts. The macOS compiler observed both outcomes at the keygen guard
and the four existing outgoing status conditions.

The x64 MSVC tests cross-linked in debug and release with Rust 1.100.0
and cargo-xwin 0.23.1. The replay verifies all 124 command reports,
966 copied files per campaign, the four cross-linked test targets in
each profile, and the archived digest of the failing Windows run.
Native Windows execution of the fix is a separate CI result.
[Archive checksums](study-windows-fixtures-45ad8f2.sha256)

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo xtask mutations --toolchain 1.100.0 \
  --case r1-study-cli-keygen --case r1-study-peer-status- \
  --output "$OUTPUT/debug"
cargo xwin test --offline --locked --all-features -p study-peer \
  --target x86_64-pc-windows-msvc --no-run
```

Repeat the commands with `--release` and a fresh mutation output path.
