# Persistent StudyProgress credential cutover

`apps/study-peer/tests/credential_rotation.rs` qualifies a stopped-endpoint TLS credential/pin replacement for the persistent two-actor StudyProgress CLI. It changes no runtime, wire format, actor roster, schema, or database publication code.

## Qualified behavior

The test runs the actual Cargo-built `study-peer` executable's `init` and interactive `run` commands. The Unix fixture also exercises `keygen`. The Windows fixture generates synthetic certificates and PKCS#8 keys in its temporary directory. Both actors use native database files, collection ID `09090909090909090909090909090909`, policy generation 7, and fixed actor identities `[1;16]` / `[2;16]`. Four synthetic self-signed identities provide old/new credentials for A and B. Configuration replacement copies the selected identity into the active directory and renames a staged public pin over the old file while endpoints are stopped. [Rust file rename](https://doc.rust-lang.org/std/fs/fn.rename.html)

The finite scenario checks:

1. Old credentials and old pins exchange successfully. Independent local edits converge at 12 practice minutes, goal 40, limit 50, completed lessons `[1,2]`. Each peer reports acknowledgments, and at least two further acknowledgments per peer occur without a revision change.
2. With both CLI processes stopped, public database APIs durably retain an old snapshot on **each** database. Public `Replica<StudyProgress>` reads capture the entire old record, including its causal state, plus retained ID and revision.
3. A switches to its new credential and B's new pin while B still uses its old credential and A's old pin. Both endpoints become ready and remain responsive. A commits 11 more minutes/lesson 3/goal 80/limit 45; B commits 13 more minutes/lesson 4/goal 70/limit 20. Their distinct states remain at 23 and 25 minutes. Both peers report certificate/alert errors with zero acknowledgments; connection-refused/time-out reports cannot satisfy this oracle. Neither published revision changes during rejection.
4. Both endpoints stop, and B's configuration changes to the matching new identity/pin. A starts with pushing disabled, acknowledges a local 17-minute practice commit, then is killed before any successful new-credential exchange. Its counterpart is stopped. Reopening the CLI preserves A's 40-minute state and acknowledged local revision; B's independent 25-minute state also survives.
5. Coordinated new credentials/pins exchange successfully. Automatic max/min resolution yields goal 80 and session limit 20; the grow-only counter totals 53 and completed lessons are `[1,2,3,4]`. Repeated acknowledgments occur without another commit.
6. After quiescent shutdown, A deliberately restarts with its stale old credential/pin against B's new configuration using the **same database files**. Both live endpoints reject the mismatch, acknowledge nothing, and preserve current state/revisions. Restoring fresh A credentials/pins succeeds, again with no extra commit.
7. After all CLI processes stop, public core reads verify the complete current StudyProgress records are equal, including causal metadata. Every semantic field is checked: total 53, goal `[80]`, limit `[20]`, completed `[1,2,3,4]`, and an empty planned set. Both retained historical records, IDs and original revisions still match their pre-cutover values.

`local <revision>` is a local commit acknowledgment. `peer <count> ack <total>` is a successful remote exchange report. The test keeps these separate: a successful offline edit never satisfies the peer oracle, and a TLS failure must not erase the acknowledged local edit.

## Process and credential handling

CLI listeners request port zero. Test-owned TCP relays keep their allocated loopback ports for the entire scenario and point to each new CLI listener after its `ready` line. The relays forward opaque TLS bytes; they do not terminate TLS or replace application admission. No released port is assumed available for a restart. Explicit blocking mode on accepted relay streams avoids inheriting the listener's nonblocking mode on macOS.

Readiness/reply waits are eight seconds, one-shot commands have ten-second deadlines, and convergence/rejection phases have twenty-second deadlines. The scenario's transport uses the application's existing absolute TLS budget. Timeout paths kill and reap their child before failing; one-shot commands and peer processes have cleanup guards. Intentional interruption kills and reaps A before reopening its database. Relay reads/writes have four-second timeouts, and relay connections are joined on shutdown.

Generated keys remain in temporary fixture directories. Unix credential directories use mode 0700 and active key files use 0600. Windows uses the ACL inherited by the test directory. Evidence contains configuration labels, SHA-256 fingerprints of **public certificates**, process output, source/tool/artifact hashes and filesystem records. The test requires four distinct certificate fingerprints. Cleanup removes temporary credential and database trees. Linux copyback selects only the proof subtree.

The mixed old/new scenario is a combined credential-and-pin configuration rejection. It is not independent MC/DC evidence for each certificate, chain, or exact-leaf-pin decision. The narrower TLS decision evidence remains in [TLS handshake admission](tls-handshake-decisions.md).

## Original POSIX qualification

The frozen baseline was `a56083c3e518abd5fe9196c51beb7bb4bce9d023`, archived without a live-checkout dependency. The only source overlay was this new integration test. Archive SHA-256:

```text
4618ce15671fcd30c8e241008f194199f73a3bd27d56ef827980c428525d227a
```

Final integration-test SHA-256:

```text
b5a68c2b71ffe850532d1104f947326e9f7f140f0614541691ff94a94e986948
```

All four executions passed one integration scenario, with strict integration-target Clippy also passing on each platform:

| Platform | Debug | Release | Proof subtree |
| --- | --- | --- | --- |
| macOS arm64 | passed | passed | `target/credential-cutover-study-a56083c/native-proofs/` |
| Linux arm64, private ext4 volume | passed | passed | `target/credential-cutover-study-a56083c/linux-proofs/` |

Host tools were Mise Rust/Cargo 1.99.1 and Python 3.14.8. Linux used the already-local immutable Docker image `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`, Rust/Cargo 1.99.1 and its bundled Python/OpenSSL. Builds used `--locked --offline`, isolated Cargo targets, and `CARGO_INCREMENTAL=0`. The Docker container used `--pull never`, `--network none`, 2 GiB memory, 2 CPUs and 256 PIDs. Source and captured locked registry dependencies entered through read-only mounts and were copied into the private volume. `findmnt` recorded ext4 on `/dev/vda1` under the UUID Docker volume for build/runtime data; no database ran on a host bind mount. Only the named UUID container and volume were removed, and both absences were verified.

The aggregate gate record is `target/credential-cutover-study-a56083c/evidence-index.json`. `source-manifest.json` is the initial frozen input inventory; final verified status is recorded in the aggregate/platform proof records. Command vectors, exit codes, durations, CLI/test binaries, public fingerprints, per-step events, compiler/tool versions and binary hashes are retained. Source files were checked against the frozen inventory before and after qualification. Native proof SHA-256 is `bab2c88d8b98d0335f9cdc8a2995e9920e2420d7abd2a711d6eae828597c0fe0`; Linux proof SHA-256 is `137bf3495cf4d9694078ce2ab6f379de20df36871b75e8b81ab59569cbad85cd` (each hashes its platform `evidence.json`). Early fixture failures are retained separately and are not passing qualification records.

## Portable fixture qualification

Public certificate fingerprints use `sha2` from the workspace lockfile. All
16 `study-peer` tests passed in both profiles on native macOS arm64/APFS and
Docker Linux arm64/ext4. Live mismatched identities exchanged no peer
acknowledgments. Peer A recovered its acknowledged local edit and both peers
converged to 53 minutes with their original retained roots. Repeat exchanges
left the local revisions stable within each run. Strict all-target Clippy
passed on both platforms and the Windows MSVC test executable cross-linked.
Native Windows execution is pending.

| Local artifact | SHA-256 |
|---|---|
| Integration test source | `1d1f96b5247f0f1974783774165be868a01da99ea7282a9a7f934284043c46b3` |
| macOS debug events | `a3c39b857f87d4a7c3088ff9bedc03dc687fc52893ed0a67c97dccc54706bff9` |
| macOS release events | `bc6ef1360632f2775977af1fe6c1431f180f1ebf6c08329b91d482d8c4ea83b8` |
| Linux debug events | `39cbbb256fadb51e183c938fedc8ecf0a041d6d329287837f10dbd871aaeb808` |
| Linux release events | `56763fa1c5f394d2709357b4fc1c103928697b34d78c10b8c41e3c7e20b1cb0d` |
| Windows MSVC debug test executable | `b3adb6672a7c5a4f657b0040639ac1336eb139e0aadca4d8bb947731b3c62122` |

To run the integration test locally:

```sh
mise exec rust@1.99.1 -- \
  cargo test --offline --locked -p study-peer --test credential_rotation
mise exec rust@1.99.1 -- \
  cargo test --offline --locked --release -p study-peer --test credential_rotation
```

An optional `STUDY_CUTOVER_EVIDENCE` environment variable selects a **new** log directory with an existing parent. Otherwise logs and generated credentials live in the test's temporary namespace.

## Operator boundary

For this fixed-roster deployment, stop both endpoints and establish quiescent database leases before replacing local credential files and reciprocal peer pins. Start both with the coordinated configuration, inspect local state, then enable exchange and require explicit peer acknowledgments plus full-state convergence. Preserve offline commits across an incomplete cutover; do not infer synchronization from a successful local command. A stale configuration must be rejected by the active fresh peer before the cutover is considered complete.

This drill covers interruption **after complete configuration replacement and an acknowledged local commit, before successful exchange**. Torn configuration replacement, live reload, device power loss, actor enrollment and generalized PKI are separate qualification profiles. On Windows, provision production credentials in an ACL-protected directory before starting either peer.
