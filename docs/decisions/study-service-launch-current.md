# Study service worker startup

The previous service used the scoped spawn convenience method twice.
If the second creation panics, unwinding joins the receiving worker
while its stop flag is still false. That join can wait indefinitely.
Rust documents the panic and implicit join behavior.
[Scoped spawn](https://doc.rust-lang.org/std/thread/struct.Scope.html#method.spawn)

Both launches now use the fallible thread builder. A receiving-worker
failure reports the original I/O error in the incoming phase. When the
outgoing launch fails, startup sets the stop flag, wakes the receiver and
joins it before reporting the original error in the connect phase. The
launch error retains precedence if the receiver also panics.

A deterministic builder fixture exercises successful startup, refusal at
each launch, and outgoing refusal followed by a receiver panic. The
second-launch cases wait for the receiver to start. The fixture checks
the actual stop flag and receiver completion before the scoped call
returns.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native.rs` | `90bd16b82af0c231e5b96831149c56bf18959714b98e0489a6a52a0ce42372a3` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:309:87– | 10923..10924 | try_propagation | run_with_io | The CLI enters this helper during normal service startup. The launch fixture supplies both errors at the shared builder boundary. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:411:5– | 15721..15726 | match | launch_workers | Both workers start on success. A refused outgoing launch stops and explicitly joins the receiver before returning the original error. A separate case makes the receiver panic during this cleanup. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:60:9– | 1945..1948 | for_iteration | parse | Empty, singleton and both valid flag orders exercise traversal. Duplicate and unknown flags stop parsing. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:74:5– | 2380..2385 | match | expected_receipt | Empty operands, the selected receipt pair, wrong flag and wrong arity exercise the three arms. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:86:5– | 2706..2711 | match | main | Local dispatch and usage tests exercise the local commands. Native service tests enter both run modes. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:178:75– | 6454..6455 | try_propagation | initialize | Native initialization reaches the shared enrollment helper and reopens the record. The roster simulator tests the helper's collection-creation error. Native fault injection after successful file creation remains open. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:263:7– | 9011..9012 | try_propagation | run_with_io | The shared diagnostic wrapper returns the readiness write result. A closed stdout pipe reports its local I/O phase and preserves the original bytes. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:313:17– | 11053..11056 | for_iteration | run_with_io | Empty input returns cleanly. A normal line followed by invalid data preserves the first reply. Quit skips the queued command. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:316:21– | 11214..11219 | match | run_with_io | Blank, quit, push, peer, incoming, sync-only practice and the general command path each exercise an arm. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:328:33– | 11835..11840 | match | run_with_io | The same loop observes idle before pushing, a refused connection in each diagnostic mode, and an acknowledged state after retry. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:363:30– | 13809..13814 | match | run_with_io | A committed edit and both modes of rejected commands exercise each reply arm. |

The nine carried rows retain the original evidence for command admission,
interactive I/O and outgoing status. The edit remaps their spans in the
startup caller. [Interactive I/O ledger](study-interactive-io-current.md) · [Outgoing peer status ledger](study-peer-status-current.md)

## Verification

Frozen source `2b3420f11751e9cc55a29149c275424c431d53f1` passed with Rust
1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4. Debug and
release each passed 64 Study tests and 14 core CRDT tests on both hosts.
Strict Clippy passed on both hosts.

The four launch controls detect a substituted I/O error, a changed
receiving phase, a receiver left running and an implicit join that
would let a receiver panic replace the launch error. All seven existing
outgoing-status controls passed in the same four configurations. Fresh
extraction checked the exact assertions, runner identities, all 160
supervised commands and 962 copied files per campaign.
[Archive checksums](study-service-launch-2b3420f.sha256)

The pinned macOS nightly `739adecd4` reran both outcomes at the four
outgoing status conditions and the focused launch test. The archived
executable and raw profiles reproduced the LLVM report byte for byte.
Actual OS thread exhaustion remains unobserved. Reconciliation retains
the 444 runtime and 7,514 tooling gaps.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 \
  --case r1-study-service-launch- --case r1-study-peer-status- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.
