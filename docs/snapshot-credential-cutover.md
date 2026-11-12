# Snapshot replication credential cutover

The private snapshot CLI supports an **operator-controlled stop/restart cutover**
of its exact pinned leaf credentials. Enrollment (stream and database identity),
selected snapshot, retained roots, and durable receipt persist independently of
the TLS configuration. This qualification adds no protocol or runtime API.

## Procedure and acknowledgment boundaries

1. Record the current follower receipt with an authenticated poll. Close the
   server before offline inspection or changing the credential paths selected
   by its startup command. Keep source writes quiescent while CLI operations
   hold their native leases.
2. Generate fresh private credential directories with `weekend-replicate keygen`.
   Transfer only each endpoint's public `cert.der` to the other endpoint using
   the operator's trusted channel. Retain restrictive private-key permissions.
3. Stop the old endpoint. Change its configured credential directory and exact
   peer pin together, and change the client's credential directory and server
   pin. Restart `serve` with the **same** follower directory, stream, and file
   identity. Configuration here means the explicit CLI arguments selected by
   the operator; the CLI does not maintain a transactional configuration file.
4. Poll using the new pair. A successful poll reports the already persisted
   follower receipt; it does not create a source commit or ship new data.
5. Ship any newer source state, then poll again. Treat only the fresh successful
   authenticated follower receipt as confirmation of follower durability.
   A local source commit or completed export is a separate event.

A partial cutover is expected to reject authentication. A client presenting a
fresh leaf to a server still pinning its old leaf fails. An old server pin fails
against a fresh server leaf. A fresh server pin does not authorize an old client
once the server has switched its client pin. Match both directions before
retrying. A failed exchange does not imply rollback or removal of an already
installed snapshot.

If the operator is interrupted after selecting new server credentials and peer
pin but before syncing, reopen the existing follower with the intended startup
arguments, inspect/poll its persisted receipt, then retry. Do not initialize a
new enrollment as a shortcut. The test kills the new endpoint after readiness
but before any client exchange; it does not simulate a torn credential-file
write, a kill during TLS parsing, or a power failure.

There is no live reload, overlapping old/new pin window, CA trust deployment,
automatic credential distribution, automatic rollback, or dynamic roster. An
operator reverting to old credentials reauthorizes that old identity and must
make that policy decision explicitly. General routing/configuration journaling
and secret distribution remain outside this example.

## Native process test

`apps/weekend-replicate/tests/credential_rotation.rs` runs actual CLI child
processes against temporary native files and loopback TLS. It exercises:

- Valid old credentials/pins and an initial shipped revision 3, preserving
  retained root 1 at revision 2.
- A local source advance to revision 4 while successful polls still report the
  exact revision-3 follower receipt.
- Client-only leaf and incorrect server-pin failures against an active old
  endpoint, each followed by a successful matching control.
- Server-only leaf replacement: a stale server pin fails and a matching fresh
  server pin with the still-authorized old client succeeds.
- Coordinated new server leaf/client pin startup, process kill before sync,
  offline reopen, and a second startup. Old client and old server pin each
  fail against that reachable endpoint; matching new credentials succeed. A
  stale-client `ship --attempts 1` completes a local revision-4 capture but
  fails authentication without a follower receipt. Offline reopening still
  selects exactly revision 3 before a valid ship is attempted.
- Shipping revision 4 after the interruption, repeated polls, and another
  independent endpoint restart, all with the exact expected durable receipt.

The oracle checks all 32,768 bytes of the current row and retained row, current
and retained revisions, the complete retained-ID list, and scans proving each
view contains exactly one key, `row`. It compares the full
selected snapshot bytes with the exported image. The receipt digest is
independently recomputed over the entire export with `/usr/bin/shasum -a 256`
on macOS or `/usr/bin/sha256sum` on Linux, through the same bounded child runner. Failed/partial cutovers keep
that image and the manifest bytes unchanged. Repeated polling/reopening after
successful cutover leaves the manifest bytes unchanged and source revision 4;
there is no extra commit hidden by an equal logical value. Raw file comparisons
occur after endpoint shutdown; the selected image is independently reopened.

Client commands have 40-second completion deadlines and servers have 10-second
readiness deadlines. RAII guards kill/reap every child on success and assertion
failure, and temporary directories are removed after children close. Fresh
nonce directories are created exclusively with mode 0700 **before** a cleanup
guard acquires ownership. A second test verifies that failed admission of an
existing directory preserves its sentinel file. Output is
redirected to fixture files rather than undrained pipes. This test is scoped to
macOS/Linux: native Windows key generation is not qualified here, and this file
is cfg-excluded on Windows rather than reporting a Windows pass.

## Qualification and reproducibility

Frozen base: `a56083c3e518abd5fe9196c51beb7bb4bce9d023`, extracted from a Git archive
with Mise Python 3.14.8's `filter='data'`. The only build overlay is the new test,
SHA256 `a32827ea88243a16c362c10825607e4a13b892f4a0703083defa27444ffcc272`.
The archive SHA256 is
`4618ce15671fcd30c8e241008f194199f73a3bd27d56ef827980c428525d227a`.
All 607 extracted/overlaid source-file hashes were checked after qualification;
the repository test still matched the frozen overlay. No runtime or manifest
change was needed.

On both macOS arm64 and Linux aarch64, the following passed (two tests in each profile: one native integration scenario
and one admission regression, with actual matching debug/release CLI executables):

```sh
cargo test --offline --locked -p weekend-replicate --test credential_rotation
cargo test --offline --locked -p weekend-replicate --test credential_rotation --release
cargo clippy --offline --locked -p weekend-replicate --test credential_rotation -- -D warnings
```

Both used Rust 1.99.1 (`58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM 23.1.1),
`CARGO_INCREMENTAL=0`, and the original lockfile. Host orchestration used Mise
Python 3.14.8. Linux used immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`
with networking disabled, 3 GiB memory, 256 PIDs, two CPUs, a read-only source
and registry mount, and a private Docker volume for copied sources, build and
native temporary files. `stat -f` reported the ext2/ext3 filesystem family.
The UUID container and volume were removed after logs/binary hashes were
captured; live Docker inventories confirmed both absent. No dependency download,
remote CI, repository visibility change, or publication occurred.

Evidence is in `target/credential-cutover-snapshot-a56083c/`: the archive,
full source binding, exact commands, compiler output, CLI binary hashes, logs,
and the preserved orchestration script. Its hash index SHA256 is
`6559533a7ff495eb3f823ac16c9e219a30de919e0a54f03e05b27dff0d9eeeee`.
The earlier passing, less comprehensive test is retained under
`target/credential-cutover-snapshot-a56083c-initial/` with its original source
hash. The strengthened scenario before the cleanup-ownership fix is separately
retained at `target/credential-cutover-snapshot-a56083c-pre-ownership-fix/`
under its original test/index hashes. Review found that constructing a cleanup
guard before successful directory creation could remove a preexisting path
after failed admission. The final two-test evidence qualifies that fix as well
as the scan, rejected-upload, and independent digest controls; no earlier run
is relabeled.

This is finite native loopback qualification of a specific manual cutover. It
does not establish cross-host deployment correctness, routing atomicity,
credential expiry/renewal scheduling, physical power-loss behavior, or a native
Windows rotation gate. Source commit and follower durability acknowledgments
remain separate throughout.
