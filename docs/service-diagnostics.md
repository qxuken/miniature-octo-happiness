# Bounded service diagnostic canary drill

This F06 follow-up checks the actual `weekend-replicate` and `study-peer`
process entry points. It covers operational stdout/stderr, including startup
failure and a live service rejecting input. It does not add a logging framework,
change error/acknowledgment semantics or claim that arbitrary application output
is safe. No runtime deficiency was reproduced in the cases below, so this change
adds regression tests without changing either service's runtime code.

## Observed behavior

Both entry points print the returned error's `Display` representation to stderr
and exit unsuccessfully on startup failure. The tested storage categories and
credential errors omit the path and supplied bytes. This is observed behavior
of these dependencies and entry points, not a universal property of boxed
errors: introducing a context string containing a path, credential or input
could break the contract.

| Actual path exercised | Required output / state |
|---|---|
| Snapshot `describe` on a canary-filled corrupt image | Fails with `Corrupt`; never prints the image contents or path; input bytes stay unchanged. |
| Study `init` under a nonexistent canary-bearing parent path | Fails with `NotFound`; never prints the path. |
| Both service startups with missing credentials | Fail before readiness; retain an OS error code (`os error`), without exposing the credential directory. |
| Both service startups with a canary-filled malformed private key | Fail before readiness; preserve a key-related diagnostic, without exposing key bytes or paths. The valid key is restored only inside the test fixture. |
| Snapshot finite server receives a plaintext frame containing all canaries | Rejects the session, retires normally after its one accepted connection, prints readiness only and no stderr. Offline status still reports an empty follower and no durable acknowledgment. |
| Live Study service receives `practice` with a canary instead of a number | Prints the integer-parse error without echoing the command or operand. |
| Live Study service receives a plaintext frame containing all canaries | A later local `incoming` command reports `served=0 failed=1 busy=0 error ...`; no canary or unsolicited stderr appears. The service exits through `quit`, and the database remains byte-identical. |

The Study `incoming` report is deferred: network workers update in-memory
counters and the last error; only the operator command writes its output.
Snapshot admission deliberately suppresses peer-controlled handshake/session
errors instead of writing to a potentially blocked stderr. Its one-connection
test consequently establishes rejection and continued authoritative state, not
a peer failure category in the server log. Existing admission/backpressure tests
cover the separate blocked-diagnostic-stream behavior.

The useful categories above remain available at their existing command/phase
boundaries. These CLIs do not currently provide a uniform structured error
record with command, phase and native/verification fields. The command name in
the operator invocation and the Study `incoming` prefix identify the tested
phase; a generic `Corrupt` message alone does not identify its structure offset.
Do not infer more detailed phase attribution from these results.

## Reproduction and bounds

Run the two integration targets with the same compiler as their cache:

```sh
cargo +1.99.1 test --offline --locked --target-dir target/xtask \
  -p weekend-replicate -p study-peer --test service_diagnostics
cargo +1.99.1 clippy --offline --locked --target-dir target/xtask \
  -p weekend-replicate -p study-peer --test service_diagnostics -- -D warnings
```

The tests are
[`weekend-replicate/tests/service_diagnostics.rs`](../apps/weekend-replicate/tests/service_diagnostics.rs)
and [`study-peer/tests/service_diagnostics.rs`](../apps/study-peer/tests/service_diagnostics.rs).
Both import the small shared
[`tests/support/service_diagnostics.rs`](../tests/support/service_diagnostics.rs)
fixture by path. It owns the process guards, stream bounds, deadlines and canary
checks; only the application prefix/certificate DNS name varies. No new crate
or dependency is introduced. Each creates an owned temporary directory with a
unique nonce. On Unix the
directory is mode 0700 and the synthetic private keys are mode 0600. Both streams
are captured to owned files; each observation checks a 16 KiB per-stream bound.
Readiness, deferred observation and process retirement use ten-second deadlines;
the injected loopback connection/write use two-second socket bounds. Each owned
child is killed and reaped if its guard drops, including on an assertion failure.
Temporary credentials and outputs are removed afterward. These are bounded
user-space checks under normal OS process/filesystem behavior, not kernel
liveness or hard disk-usage guarantees.

The checks examine both complete streams after process retirement. They reject
any appearance of the synthetic path, credential or record canary, require the
specific category/counter output above, and independently compare the affected
database/image bytes. A zero exit status or an empty log alone cannot satisfy
the service-state or category checks.

## Qualification scope and remaining work

The initial run on 2026-11-16 used native macOS arm64 and Rust 1.99.1 over
`6eb5f1a884bd75cfd71cdd75e3fa1cf0be597e63` plus the new test files and shared fixture.
Both process tests pass; strict Clippy passes for the two integration targets.
This is working-tree regression evidence, separate from the four-platform
`7e0e44f` package checkpoint. The frozen-source Linux follow-up is recorded
below. Windows execution and final-candidate reruns remain separate
qualifications.

### Frozen-source Linux arm64/ext4 result

On 2026-11-16, both tests passed independently in Docker with Rust
1.99.0 (`b940084d7`, LLVM 23.1.1) and Cargo 1.99.0 (`5f94df478`).
The immutable local image was
`sha256:bda5ef43469a7d63b240adcbc5a5b0599be442bce204d12936f6083224f56b18`.
The runtime reported Linux 7.0.14-linuxkit, `aarch64-unknown-linux-gnu`.
The image was used without pulling or installing anything. Both Cargo
invocations used `--offline --locked` and the existing read-only registry cache.

The source was a clean Git archive of
`6eb5f1a884bd75cfd71cdd75e3fa1cf0be597e63`, tree
`adcf057f8c9254376fb93b4fec0a484fb7aa6c80`. A checked patch added only the two
diagnostic integration tests and their shared fixture. No engine, service,
manifest or lockfile was changed. The archive's 732 regular tracked files plus
the three overlay files produced identical complete source inventories before
and after both builds/tests. The host overlay files also retained their hashes.
This establishes the named source-plus-test-overlay profile, not a test of an
unmodified `6eb5f1a` tree or a new four-platform package checkpoint.

The UUID-owned Docker volume reported ext4 for the copied source, build output,
test databases and `TMPDIR`. The container had networking disabled, two CPUs,
2 GiB memory and 256 PIDs; the tests used only its loopback sockets. A 600-second
container timeout and the existing host xtask supervisor bounded execution.
Each service target passed its one process test, with no skips, in 0.09 seconds
and 0.12 seconds respectively. The container exited zero. All 17 recorded host
commands have successful process reports and verified cleanup. Removal checked
the ownership label before deleting only that container and volume; successful
authoritative Docker inventories then confirmed both UUID names absent.

Retained local evidence is
`target/service-diagnostics-linux-be56fabe-4ae7-4684-a918-71cb46ab337c/`.
It includes the exact archive/patch/overlay, shell runner, compiler/filesystem
metadata, both Cargo stdout/stderr logs, before/after source inventories,
supervisor requests and terminal reports, container state and final Docker
inventories. Build caches were removed with the owned volume.

| Input / evidence | SHA-256 |
|---|---|
| Clean source archive | `0b740ab2b537b1702faf529f31a6f5246bf18d4a4dbdd8b165dd1c9d6a728008` |
| Exact three-file overlay patch | `0c5f638ffb5259ef12479b2648dc6da1f45f14fff15087f4bf43f7b10dd019a2` |
| Snapshot service test | `94d34f9be0b62701ecae3674ad3f89c941dc80ed8bfba99fa2a5e0fc5fa034b1` |
| Study service test | `d3a683014f20d0a0508c93595dcd3648a0414ed7d1031b94c1bc1719eee859e8` |
| Shared fixture | `90f2a83d4b004221eddf51b574cf144d7212bfd77dbcf6e179d2b63465120416` |
| Container shell runner | `fe800b58858bead526248f9aabbec2db29673a78dd5ea34eb2e658ede208c14e` |
| Complete source inventory, both observations | `95e0ccad5d4e2701e331789dedb3db39a3047127fb1f69a6f80d00f0f608f0b0` |
| Final `metadata.json` | `dcaa07e26dc19e933596dfaa7fb8b8e3e4a9b27c6e89386f00cf6b0ee038099f` |

### Remaining scope

The tests cover startup, parser and malformed unauthenticated TLS input. They
do not cover every certificate/OS diagnostic, all authenticated corrupt record
frames, panic/debug output, crash dumps, external service managers, third-party
log sinks or arbitrary user `Record` implementations. Study `show` and normal
acknowledgment/progress commands intentionally expose their documented state;
this drill is not a confidentiality promise for those successful commands.
Process command lines and test fixture files are also outside stdout/stderr
redaction. The test harness never receives real secrets.

The [native diagnostic contract](verification-diagnostics.md) offers a separate
path-free, record-free `StorageFailure` with authoritative error and optional
native operation/category/code and verification stage/offset. These two service
entry points do not automatically capture or print those fields. If deployment
needs uniform structured observability, define a bounded typed diagnostic at
each handled startup/storage/transport boundary, select approved enum/numeric
fields, and test the actual output sink and backpressure policy. Avoid formatting
arbitrary error chains or input values into that record. This design would need
its own review and tests; the current drill does not implement or qualify it.
