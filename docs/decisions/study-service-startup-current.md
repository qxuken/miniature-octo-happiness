# Study service startup

The native service starts either with a collection ID or with a policy
file. The process test supplies equivalent enrollment through `run`
and `run-policy`. Both reach readiness, report the same default Study
record at revision 2 and quit without changing the database bytes. An
unrecognized command at the same arity exits without reaching readiness.

| Stage | Independent failure probes |
| --- | --- |
| Direct enrollment | An unrecognized actor returns the enrollment error. |
| Policy enrollment | A non-Unicode actor argument is rejected before the policy is read. A missing policy preserves the I/O error. An unsupported policy preserves the engine error. A valid policy with an unrecognized actor preserves the actor error. |
| Credentials and pin | Missing credentials report the original I/O error even when the pin is malformed. A missing pin reports its own error. A malformed pin reports the rustls error before the database is opened. |
| Opening the file | A missing database reports `NotFound` before an invalid listener address is parsed, and the path remains absent. |
| Local and remote addresses | Each non-Unicode argument and each syntactically invalid address preserves its separate error. An occupied listener reports the same native error kind and OS code as a direct bind. A peer-address failure allows immediate rebinding of the successfully opened listener. |
| Opening the replica | A valid policy with the other configured actor returns `ForeignStore` on the existing actor A file. |

The focused test checks the original bytes and acquires the file lease
again after every refusal. Reopening reports revision 2. The listener
flags and address query inside the service, native worker failures and
the full peer status matrix remain open.

## Decision pairs

The startup guard accepts either verb at the service arity. The first pair
compares an unknown verb with `run`. The second compares the same unknown
verb with `run-policy`. In the first accepted case the left condition
short-circuits the right. The separate enrollment branch selects either the
collection ID or the policy file. Both successful modes and the unknown
verb use the same native process fixture.

Fifteen propagation controls check the original error types at each
startup operation in the table. The native bind and its diagnostic
wrapper each have a control. Both address conversions distinguish the
argument decoding error from the address syntax error.

## Verification

The frozen source `1d4bb133c7c16fae04a885375741a1a4ec99beb3` passed
on arm64 macOS and offline arm64 Docker Linux on ext4 with Rust 1.100.0.
Each debug and release configuration passed 58 Study tests, 14 core
CRDT tests and all 18 mutation controls. Strict Clippy passed on both
hosts. Fresh extraction verified all 240 command reports, the named
assertions, the exact runners and 954 copied files per campaign.
[Archive checksums](study-startup-1d4bb13.sha256)

The macOS build with nightly `739adecd4` observed the following condition
outcomes in the CLI executable. Its extracted raw profiles and executable
reproduced the LLVM export byte for byte.

| Condition in `native.rs` | True | False |
| --- | ---: | ---: |
| 138, `run` verb | 1 | 2 |
| 138, `run-policy` verb | 1 | 1 |
| 139, direct enrollment | 1 | 1 |

Seventeen named decisions and the coupled short-circuit expression
account for 18 runtime records. Reconciliation leaves 468 runtime and
7,514 tooling gaps. All production source hashes retain their previous
values.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 --case r1-study-startup- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.
