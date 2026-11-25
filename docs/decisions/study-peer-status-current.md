# Study outgoing peer status

An enrolled pair starts with zero acknowledgments. A commits seven practice
minutes at revision 3 while B stays at revision 2. The first connection
attempt reaches a released loopback port. Text output reports zero
acknowledgments and an error. JSON output reports
`connect/io/connection_refused`. B's file keeps its original bytes while
its service is stopped.

| Observation | Result |
| --- | --- |
| Outgoing work starts disabled | The peer command reports `peer 0 idle`. |
| Pushing is enabled with B stopped | The worker surfaces a connection failure. A's local commit remains independent of the reply. |
| B opens with its configured identity and A retries | The peer acknowledgment advances and reports seven minutes. B reports revision 3 with the same seven minutes. |
| A syncs again | Another acknowledgment arrives and B stays at revision 3. |
| Both peers quit after pushing is disabled | Both children exit successfully. B reopens at revision 3 with seven minutes. |
| B is interrupted after committing and before its reply in the lost-ack test | A reports `peer 0 error Io`. The existing retry test confirms B's published state and converges. |

The probes exercise the active and stopped loop, paused and enabled
pushing, successful and failed outcomes, and text and JSON error replies.
They check the resulting observations and worker exit without measuring
backoff intervals. Native worker join failures and the accepting worker's
operation errors retain their own decision records.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native.rs` | `9ced0c6b1d1ea19a0c2c827410755ad5775e4026c8271d1d7448a9f9822f9c68` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:326:33– | 11742..11747 | match | run_with_io | The same loop observes idle before pushing, a refused connection in each diagnostic mode, and an acknowledged state after retry. |

## Verification

The initial debug campaign at `3151c26` let the connection-error
substitution survive on both hosts. The structured diagnostic retained the
original cause, and the text assertion checked only its prefix. The
revised probe compares the complete text error with an independent native
connection attempt at the same address. It also checks the structured
phase, category and code. The archived initial results record the rejected
campaign.

Frozen source `cc7d291c92c1a404b52ada47b7cfabd57a422689` passed with Rust
1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4. Debug and
release each passed 59 Study tests, 14 core CRDT tests and all seven
mutation controls on each host. Strict Clippy passed on both hosts.
Fresh extraction verified the named failures, the exact runners, all 108
supervised command reports and 956 copied files per campaign.
[Archive checksums](study-peer-status-cc7d291.sha256)

The macOS condition profile from nightly `739adecd4` observed both
outcomes for the worker loop at line 282, the paused check at line 283,
the successful-acknowledgment check at line 296 and the structured-error
guard at line 328. The extracted executable and raw profiles reproduced
the LLVM export byte for byte.

Six named decisions and the status match account for seven runtime
records. Reconciliation leaves 461 runtime and 7,514 tooling gaps.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 --case r1-study-peer-status- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.
