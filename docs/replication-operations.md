# Operating a passive snapshot follower

A primary commits locally before any network exchange. `Primary::ship`
returns a separate `DurableAck` only after the follower verifies the image
and durably selects it in its manifest. Applications persist and display
these two outcomes separately. After a dropped reply, reconnect and poll
or retry the same immutable export.

The caller supplies an authenticated duplex channel, transfer deadlines,
admission control and retry backoff. Enrollment binds a stream ID to the
primary file identity. It does not authenticate the transport. Use a new
stream ID when restoring or forking primary history.

## Restart and diagnostics

Open the dedicated private follower directory with `Follower::open` and
the expected enrollment. Opening verifies the authoritative manifest and
its selected image before exposing progress. An unselected image never
provides fallback. Keep the directory path stable while a follower is open.

`Follower::status()` reports enrollment, durable acknowledgment, selected
slot, physical manifest bytes, the manifest admission limit and whether
maintenance or recovery is required. If recovery is required, the report
omits the durable acknowledgment. The last verified read view can remain
available through `replica()`, while `progress()` returns
`RecoveryRequired`. Close and reopen before serving further requests.
Corrupt selected images require investigation and an explicit fresh seed.

The replica limit supplied to `Follower::create` or `Follower::open` applies to
each decoded image. During installation the follower keeps the selected graph
in memory while it verifies the candidate and durably publishes the manifest.
The manifest and TLS connection use separate memory. Account for both image
graphs when sizing a follower that accepts large snapshots.
[Linux installation and memory measurements](follower-installation-profile.md#installation-timing-and-memory)

## Manifest growth and maintenance

Native followers admit new image installations only while the physical
manifest has at least four regions of space below the 1 MiB admission
ceiling. The current manifest selection record needs three appended
regions. The extra region preserves alignment headroom. The check runs
before staging creation and before sending Ready. Rejected offers return
`Limit`. Queries, stale offers and exact duplicates still return their
existing durable acknowledgment without extending the manifest.

On macOS and Linux, call the consuming `compact_manifest()` method during
a quiescent maintenance window. It closes the follower lease, invokes the
existing verified atomic file replacement protocol, then reopens and
verifies the selected image. The selected revision and digest survive
compaction. A cooperating process can acquire the lease during the
handoff, producing `Busy`. Any failure consumes the old handle. Recover
through `Follower::open` before further operations and poll progress.

Windows currently uses fresh-directory reseeding for this maintenance
workflow because native in-place compact replacement is unavailable.
The admission limit bounds additional growth under normal library use.
It does not shrink historical manifests automatically or limit files
modified outside the private-directory contract.

### Offline CLI maintenance

Stop `serve` and other users of the follower directory first. Use the original
configured enrollment from `describe`; these commands acquire the manifest lease:

```sh
weekend-replicate cleanup-inactive FOLLOWER_DIR STREAM_HEX FILE_IDENTITY_HEX
weekend-replicate compact-manifest FOLLOWER_DIR STREAM_HEX FILE_IDENTITY_HEX
weekend-replicate status FOLLOWER_DIR STREAM_HEX FILE_IDENTITY_HEX
```

`cleanup-inactive` prints `maintenance removed_inactive=N` separately from the
verified follower durability line. It is safe to repeat after reopening; removal
is not a durable namespace-deletion acknowledgment. `compact-manifest` is available
on macOS/Linux and reports before/after manifest bytes and maintenance-required,
then the verified durable receipt. It consumes/reopens the follower internally;
a lease competitor or I/O error can interrupt maintenance. After any error, keep
serving stopped, reopen with `status`, verify the authoritative receipt and retry
maintenance only after recovery succeeds. Never select another image as fallback.

These commands can report an I/O error while writing their output. If
initialization, cleanup or compaction reached its storage operation, an
output failure can leave the operation complete. Reopen with `status` to
verify the selected receipt before retrying. Structured output errors
use the `local_command` phase.

The [capped ext4 fault drill](follower-installation-profile.md#ext4-writeback-errors-during-follower-maintenance)
verified that either recovered canonical manifest reports the exact
selected receipt. It also reproduced an inactive file returning after
a successful cleanup count when the directory transaction failed.

On Windows `compact-manifest` returns an explicit unsupported diagnostic before
opening or modifying any path. Follow [explicit reseeding](#explicit-reseeding)
into a fresh directory, verify its receipt, and only then change the service's
directory while maintaining quiescence. This command does not delete the old
follower or automate a cutover. Cleanup remains available on Windows.

## Interrupted staging and cleanup

`cleanup_inactive()` requires a recovered manifest and verifies the
selected image again before removing anything. It deletes only the
reserved unselected `replica-0.wdb` or `replica-1.wdb` regular-file entries.
If no image has ever been selected, both reserved staging paths can be
removed. It returns the count of removed entries. Other names are left
alone, and directories or symlinks in reserved staging positions produce
`Invalid`.

Cleanup is idempotent. Interrupted cleanup can leave a staging path
present, so run it again after reopening. It does not acknowledge durable
namespace deletion. Transfer retry also replaces its inactive staging
slot. Cleanup never chooses an older image after selected-image corruption.
The dedicated directory remains private to cooperating processes. Hostile
concurrent namespace edits are outside this contract.

An authenticated Linux process-kill drill verified the same boundary with a
35 MiB image and 31 retained roots. Killing after the staged image sync reopened
the old revision. Killing after manifest publication reopened the new revision,
and a fresh poll and duplicate ship recovered its exact receipt.
[Large-snapshot interruption results](follower-installation-profile.md#process-interruption-at-the-two-durability-boundaries)

## Explicit reseeding

1. Recover the primary and construct `Primary` with the intended enrollment.
2. Export a verified snapshot and keep the export alive for retries.
3. Create a new empty directory and call `Follower::create` with the expected
   enrollment and replica limits.
4. Ship the export, then reopen and poll the replacement follower to verify
   its durable acknowledgment.
5. Switch the application's configured follower directory after verification.
   Retain the previous directory until the application's rollback policy
   permits removing it.

These operations never reset an existing enrollment or delete the previous
follower directory. Replacing a follower for the same primary history can
reuse its enrollment. A restored or forked primary history requires a new
stream ID and explicit enrollment on both sides.

## Evidence

The native lifecycle test inflates a closed manifest with an abandoned tail,
checks admission failure before an inactive slot is created, retries an
already installed image at the cap, compacts and installs the next revision.
It checks idempotent cleanup, preservation of unrelated entries, reopening
and fresh-directory reseeding to the same acknowledgment.

The cleanup test removes an interrupted initial upload, rejects a directory
in the inactive slot and preserves inactive bytes when verification of the
selected image fails. The manifest fault test injects an error at every
native compaction hook, reopens the exact selected revision and digest,
and retries the same export. Existing simulator tests continue to exercise
image sync, manifest append/root/witness barriers, lost replies and retries.

### Naturally exhausted manifest and fresh-directory rotation

The native rotation test grows a real follower manifest through successive
installs until it reaches its admission threshold. A poll and an exact
duplicate return the old receipt. A newer offer returns `Limit` on both
ends before altering any follower file. Cleanup then removes only the
inactive slot. The replacement directory first receives the exact old
export under the same enrollment, then accepts the previously rejected
next export. Reopening both directories compares current state, retained
revision 2 and the selected receipts. The old manifest and selected image
remain byte-identical throughout reseeding.

Run the release fixture against the native file adapter. Set
`WEEKEND_MANIFEST_ROTATION_EVIDENCE` to a fresh output directory to copy
the capped follower before cleanup, the old follower after rotation, and
the replacement. The fixture uses test-owned TCP loopback. The companion
CLI integration test exercises authenticated CLI maintenance.

```sh
cargo test --offline --locked --release -p weekend-db --features replication --test replication \
  native_manifest_growth_and_reseed_preserve_receipts -- --exact --ignored --nocapture
cargo test --offline --locked --release -p weekend-replicate --test cli \
  native_cli_enrollment_ship_restart_poll_and_bounded_server_lifetime -- --exact
```

#### Recorded native rotation result

PR head `61efe2d` passed on native Windows x64 with fixed NTFS and
Rust 1.99.1. The hosted checkout was test merge
`0f998401b7923adadea2315de663caabf89a26b5`. The app's Windows CLI test
also passed its explicit compaction refusal at the cap and its cleanup
and receipt checks.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43738101373/job/132885068399)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43738101373/artifacts/14171297883).

The first `maintenance_required` status followed 81 successful
installs. The manifest length was 1,044,480 bytes, with a 1,048,576-byte
ceiling. The capped follower preserved its revision-84 receipt after a
poll, exact duplicate and rejected revision-85 offer. The replacement
reopened at revision 85 with a 73,728-byte manifest. The copied proof
shows that cleanup removed only the inactive slot and that the
replacement's initial image matched the capped follower's selected
image byte for byte. Read-only inspection of both final selected
images verified witnessed format-2 roots at 84 and 85 and retained
ID 1 at revision 2.

macOS/APFS and Docker Linux/ext4 passed the same natural-growth and
CLI profiles. The installations, file lengths and revisions matched
the Windows observations. File identities and image hashes were
generated separately. The Linux Cargo lock stayed unchanged and the
owned Docker container and volume were removed.

| Evidence | SHA-256 |
|---|---|
| Windows ZIP | `2ed13a8d063730085bdd56c63b6386370318b0b5b5d47a7c98c891e2b79b39a7` |
| Windows rotation log | `68aa61f973a07357786730949076ba11b38ac91358ab484442bd82d5cf92a5df` |
| Windows CLI log | `9089f1da3dc69ff0cad60e9b738c96e862267b926342de233e479d6b677865e8` |
| Windows old selected image at revision 84 | `fce3b49745faf02207a3f1ffd24be174d299b61ec9a33c4425f8259aac11d009` |
| Windows replacement image at revision 85 | `61ca0edca794f1c67fc0ffd8d60acbed96f20560f9fc784b6ced64ce57140768` |
| macOS rotation log | `06f0ed9bd895e9c8906b6869397eb126636f83f73458ef0394e4b4a4bea1edce` |
| Docker Linux test log | `fd489391c9ac0b2bc5df596e0f48037c37eb613a8072492a1bc1d9897bf0ce07` |

## Transport deployment contract

Complete transport authentication before passing a channel to
`serve_authenticated`, `Primary::ship` or `Primary::poll`. For mutual TLS,
verify the configured peer certificate or trusted identity and bind that
peer to the expected Enrollment in application configuration. The embedding
application owns certificate rotation, revocation and endpoint configuration.
The [stopped-endpoint cutover guide](snapshot-credential-cutover.md) gives the
tested native Mac/Linux leaf/pin replacement procedure, including partial
configuration rejection, interruption, retained history and receipt checks.
Enrollment IDs and snapshot digests support consistency checks inside the
protocol. Authorization comes from the authenticated transport and that
configuration binding.

Set read and write deadlines before entering an exchange. Close the channel
on completion or error, reconnect with bounded backoff, and cap the number
of concurrent connections and queued exports. Keep the exact exported image
for retries until a durable acknowledgment covers it. On primary restart,
reconstruct Primary from its recovered local history and poll the follower.
If the primary history was restored or forked, explicitly configure a new
stream and a fresh follower directory.

The snapshot fuzz targets exercise the public sender/follower APIs using
in-process byte channels. `snapshot_frames` checks reply decoding and
monotonic acknowledgment acceptance against a separate byte-level oracle.
`snapshot_install_schedule` injects truncated uploads, corrupt payloads,
malformed enrollment/version/type frames, dropped Ready/terminal replies,
replay and cleanup across native follower reopen. It checks the exact
selected revision, current value and retained value after every step.
Storage power-loss and manifest-barrier faults remain covered by the
simulator suite. These harnesses do not perform transport authentication.

## Authenticated native CLI

The private `weekend-replicate` package supplies one-primary/one-follower native
snapshot transport. It uses rustls mutual TLS, an exact configured leaf
certificate on each endpoint, and the separate ALPN `weekend-snapshot/1`.
A completed TLS handshake with no pending handshake writes is required in
addition to both checks before a
channel reaches the core replication API: a parsed certificate and ALPN alone
do not prove the peer sent Finished.
Certificates identify the transport peers; the explicit stream/file enrollment
binds that peer connection to one database lineage. The application has no
certificate rotation, revocation, discovery or multi-tenant authorization service.
All Cargo packages remain unpublished.

Build with `cargo build --locked -p weekend-replicate --release`. The primary
Linux server/macOS client deployment can use these commands on their respective
hosts (substitute paths and addresses):

```sh
# Each host generates its own credentials; never copy key.der to the peer.
weekend-replicate keygen /private/local-credentials
# Exchange cert.der over a trusted administrative channel and verify its identity.
# Place the remote certificate at /private/remote-cert.der.

# Choose a fresh stream ID once, and persist it in deployment configuration.
STREAM=$(openssl rand -hex 16)
# Close the source writer briefly to acquire the required exclusive inspection lease.
weekend-replicate describe /data/primary.wdb "$STREAM"
# Output includes a directly copyable command:
# weekend-replicate follower-init EMPTY_DIR STREAM_HEX FILE_IDENTITY_HEX

# On the follower, create a private empty directory and paste the reported IDs.
mkdir -m 700 /data/follower
weekend-replicate follower-init /data/follower STREAM_HEX FILE_IDENTITY_HEX
weekend-replicate serve /data/follower STREAM_HEX FILE_IDENTITY_HEX \
  0.0.0.0:9443 /private/local-credentials /private/remote-cert.der

# On the primary, acquire a quiescent snapshot, then release its file lease
# before connecting. The original writer may resume during network transfer.
weekend-replicate ship /data/primary.wdb STREAM_HEX /data/new-export.wdb \
  192.0.2.10:9443 /private/local-credentials /private/remote-cert.der --attempts 3
```

If `follower-init` fails or is interrupted, use a fresh private empty
directory for the next enrollment attempt. A recovered manifest can have
a valid storage root before the follower enrollment row exists. If that
row survived, a recovered follower can start and report `follower empty`.
The first durable snapshot receipt follows a verified shipment.
[Ext4 enrollment and fresh-directory recovery drill](follower-installation-profile.md#interrupted-follower-enrollment-on-ext4)

`describe` uses read-only inspection and prints the current verified local
revision, stream and file identity without stabilizing an unwitnessed root.
`ship` opens/recover-verifies the primary, captures a verified immutable export,
then drops the primary database before any network connection. The export has
its own lease. The source writer must be closed for capture, but is not locked
through the transfer. `ship` prints `local captured_revision=...` separately
from `follower durable_revision=... stream=... digest=...`. Creating a local
export is not evidence of follower installation. New local commits can advance
beyond the captured revision while that image is in transit.

Each client exchange starts one absolute 30-second socket-I/O budget before
TCP connect; the same deadline covers TLS handshake and all protocol reads and
writes. The server starts its budget when it accepts the socket. Successful
partial progress does not renew the deadline, and the application stream
checks it even for plaintext already buffered by TLS. A maximum 64 MiB image
requires roughly 2.2 MiB/s before overhead, so a slower path may time out.
Local verification, filesystem reads and durable flushes are not interruptible
by this socket deadline and may finish after it. A response can therefore time
out after the image became durable. This is not a 30-second wall-clock bound on
all local storage work.
[Buffered I/O and stalled-upload results](shared-app-tls.md#deadline-checks-on-buffered-tls-io)

### Bounded follower admission

`serve` handles up to four concurrent TLS handshakes and queues up to four
authenticated channels. One follower worker owns the manifest and handles
applications in queue order. The handshake and time spent waiting in the
queue share the accepted socket's original deadline. Excess connections at
either limit close before a snapshot protocol reply. A failed connection
provides no durable progress.

`--max-exchanges N` stops accepting after N TCP connections, including failed
authentication and excess connections. The server drains admitted work.
When manifest publication requires recovery, the follower stops taking
queued work, admission closes the listener and cancels stalled handshakes,
and the process exits with the storage error. Pending handshakes check
the cancellation signal in 100 ms I/O waits within the original socket
deadline. Restart `serve` to recover and verify the selected image. TLS
configuration is still validated before binding a listener or opening the
follower.

The listener and follower worker avoid writing peer errors to stderr.
An authenticated request can still receive its protocol error, and the
client reports failed or unconfirmed attempts. The server's startup line
and terminal CLI errors use ordinary process output.

The application caps are separate from the operating system listen backlog.
A stalled authenticated upload occupies the follower worker until its deadline
or in-progress local work completes. The Linux saturation profile covers
two full handshake deadline cycles and a full authenticated queue.
Per-peer fairness and deployment capacity require separate qualification.
[Month replay with a stalled handshake](live-replication-month.md#bounded-admission-with-a-stalled-tls-handshake)
[Linux saturation and cancellation](follower-overload.md)

### Unconfirmed transfer, restart and retry

The default is at most three attempts (`--attempts 1..3`) with 250 ms between
attempts. They all retain the **same** in-memory Export and file lease, use
fresh authenticated connections, and let the core reject inconsistent receipts.
Configuration/protocol admission errors are not treated as successful progress.
After an unconfirmed transfer, the CLI reports that follower outcome may be
uncertain and directs the operator to poll, rather than infer rollback:

```sh
# Stop the follower service first: status is offline and takes its manifest lease.
weekend-replicate status /data/follower STREAM_HEX FILE_IDENTITY_HEX
# Restart serve, then poll from the primary (close its writer briefly for the lease).
weekend-replicate poll /data/primary.wdb STREAM_HEX \
  192.0.2.10:9443 /private/local-credentials /private/remote-cert.der
```

`status` cannot run while `serve` holds the follower manifest lease. Both
`describe` and `poll` require the primary writer to release its exclusive lease
briefly. `poll` reconstructs Primary from recovered local history and releases that file
lease before connecting. It prints the verified local revision separately from
the follower's exact durable revision/digest (or `follower empty`). A restart
does not persist the sender's previous in-memory progress watermark. The
follower's recovered manifest remains authoritative. A restored/forked primary
requires new stream enrollment; do not reinterpret it as the old live writer.

The CLI flushes the local verified revision before it dials. It reports
the follower receipt after the authenticated poll. If receipt output fails,
run a fresh poll or reopen follower status to recover the same durable
receipt. [Snapshot poll output qualification](decisions/snapshot-command-output-current.md)

Sampled polling tracks the verified revision gap and the age of the last
durable observation across transient failures. Each sample uses the same
primary lease rules and reconnects over authenticated TLS.
[Sampled progress diagnostics](replica-progress.md#sampled-polling)

To confirm the exact old capture after restart, compare both the captured
revision and the polled digest with the saved completed export:

```sh
shasum -a 256 /data/new-export.wdb
# Linux alternative: sha256sum /data/new-export.wdb
```

A newer acknowledgment is newer progress, not proof that the exact old image
(including its retained roots) remains selected. Preserve the old export for
independent inspection when needed.

Export files are create-new and are retained after exit, including failure.
The `ship` command captures and sends in one invocation. Use the separate
capture workflow below to keep enough local state to resend the exact image
after a sender restart. Remove export files only when their leases are clear
and your retention policy permits.

### Restartable captures

Use two fresh paths in a trusted private directory. Capture first, then
send the saved capture in a separate process. The same send command also
resumes an interrupted or unconfirmed attempt.

```sh
weekend-replicate capture /data/primary.wdb STREAM_HEX \
  /data/export-42.wdb /data/export-42-descriptor.wdb
weekend-replicate ship-captured /data/primary.wdb STREAM_HEX \
  /data/export-42.wdb /data/export-42-descriptor.wdb \
  192.0.2.10:9443 /private/local-credentials /private/remote-cert.der
```

`capture` completes and verifies the export before committing its local
descriptor. The descriptor records enrollment, revision, length and the
SHA-256 of the exact bytes. It uses one binary row in a separate Weekend
database. File creation precedes the descriptor commit, so an interrupted
capture can leave a partial export, an incomplete descriptor or a usable
pair. A usable pair contains a valid selected descriptor and a matching
complete witnessed image. A partial pair can be inspected or cleaned up
offline. Use fresh paths for the next capture.

`ship-captured` reconstructs the primary under its local file lease.
The recovered primary must cover the captured revision and match the
stream and database identity. The sender releases the primary before
connecting. It verifies the saved export's physical length, SHA-256,
matching witness and complete current and retained graph under an exclusive
read-only file lease, which it holds through all attempts. A changed export,
wrong enrollment or incomplete descriptor is rejected before connecting.
Keep the files together and protect them from local modification. The
descriptor is a local verification record. Authentication uses the
configured TLS peer credentials.

`capture` prints only local capture information. A successful
`ship-captured` prints the captured revision and the follower's separate
durable acknowledgment. If the follower already selected the exact image,
it returns the same revision and digest without another install. It can
instead report a newer durable revision covered by the recovered primary.
That newer receipt describes the currently selected image. It carries no
proof that the older image is still selected. The restartable workflow
uses the existing snapshot wire protocol and the same enrollment policy.
When restoring or forking a primary, configure a fresh stream and follower.

The send commands exit on a capture-announcement write or flush error
before connecting. If stdout fails after the follower installs the export,
the send command exits with an output error. The follower may already have
a durable receipt. Poll over a fresh authenticated connection to retrieve
it. With structured diagnostics, these output errors report `local_command`
and category `io`. A local `capture` that fails only while printing can
leave a usable pair of files. The next `ship-captured` verifies the pair
before contacting the follower.

For embedded Rust, persist `Export::descriptor()` before transfer, then
reconstruct `Primary` and use `Primary::reopen_export_file` or
`Primary::reopen_export` with the saved descriptor. The application owns
descriptor persistence, path lifetime and authenticated channels.

### Credential handling and platform scope

Configuration paths/directories are trusted and must not be mutated by hostile
processes. Certificate/pin reads are capped at 64 KiB and PKCS#8 private keys at
16 KiB. Empty, oversized and nonregular files are rejected; type is checked
before opening to reject configured FIFOs without blocking, and rechecked on
the opened handle. This does not defend a hostile namespace race between those
checks. The opened Unix private-key handle must also have owner-only permissions before
any key bytes are read; public certificates/pins need not be private. Credentials
and key contents are never logged.

Unix key generation creates a new 0700 directory and a create-new 0600 key.
It refuses an existing directory rather than overwriting credentials. Interrupted
generation may leave partial configuration for explicit cleanup. Native Windows
transport is supported, but `keygen` fails closed: externally provision DER
certificates (SAN `weekend-replicate`, suitable for client/server authentication)
and PKCS#8 `key.der` in an ACL-protected directory. It does not fall back to
potentially world-readable private-key creation. Protect follower/export
directories using the host's access controls as well.

### Transport test evidence

On arm64 macOS, the initial CLI suite exercises real rustls/native-file transfer,
wrong client and server identities, matching certificates with absent/wrong
ALPN, authenticated enrollment mismatch, and a dropped terminal reply followed
by reopen and same-export retry. Authentication failures return from the
transport stage before calling core and leave the manifest byte-identical with
no staging file. Captured revision 4 remains the shipped revision while the
primary is reopened and advanced to 5. The follower reopens revision 4/value 2
and retained revision 2/value 1, with the same digest and no manifest growth
on duplicate retry. A real CLI process fixture enrolls, ships, restarts and
polls using `--max-exchanges 1`. Deadline tests distinguish direct private-clock
control from native socket progress. A fragmented real TLS exchange sends the
correct certificate and ALPN but withholds Finished, and must still fail admission; credential tests check caps, FIFO rejection
and Unix modes. These tests do not simulate physical power loss or expand the
checkpoint 8556dc1 source-decision coverage denominator. New CLI source decisions
and dependency/native fault obligations are separate from that audit.


The initial arm64 macOS debug/release runs passed nine unit/native/TLS tests
and one CLI process test per profile, plus strict all-target Clippy. Test logs
are `/tmp/replicate-tests.log`, `/tmp/replicate-release.log` and
`/tmp/replicate-clippy.log`. These initial local results do not claim Windows key-generation support.

Independent qualification used the exact archived git tree
`a0aad9f9ff95a2998ca623b654c452ae02658b6b`; application and test code remain
byte-identical to that tree after these documentation updates. Native arm64
macOS and aarch64 Linux on an ext4 Docker volume each passed nine app tests plus
one CLI process test in debug and release. macOS also passed strict app Clippy
and the full Rust/C/Odin workspace check. Optional Linux Clippy was unavailable
in the pinned image. An authenticated macOS-to-Linux transfer installed follower
revision 13 with retained root 1 at revision 11. After follower restart and
primary advancement to revision 14, fresh TLS polling still reported revision 13
and the exact export digest
`70b916bc69f94a1cccac78417093ec238d8185a444543a4cb84d2a5d2b9733ea`.
Evidence: `target/independent-replication-qualification-rerun-a0aad9f9ff95/evidence.json`.
This is bounded native/cross-OS functional evidence, not physical power-loss,
mixed-version or long-duration network qualification.

## Mixed-checkpoint core wire qualification

The separate [dcd2402 ↔ 315068f report](snapshot-compatibility.md) records native
macOS wire-v1 exchange in both directions, with each archived core built against
its original locked dependencies. Durable revision 4, retained root 1 at revision
2 and independently hashed export receipts survive reopen/retry while the primary
has already committed revision 5. Both receivers reject a poll header whose
version byte is changed to 255 without changing any follower-file bytes. This is
not an upload-interruption test. The fixture authorizes its own private loopback
peers; it does not qualify mixed-version CLI TLS, certificate rotation, revocation,
or trust-policy changes. Exact builds, fixture hashes and scope are in the report.

## Verified backup and fresh-stream restore

The [follower backup/restore runbook](follower-backup-restore.md) selects the
manifest-verified image while the service stays closed, verifies its former
receipt, copies/restores to new paths, and seeds an explicitly new stream and
follower directory. Frozen `65d142e` passed the native macOS CLI/admin procedure
with current revision 4 and retained root 1 at revision 2. Wrong enrollment was
rejected both by offline status and an authenticated upload; all old follower
files and the original primary remained unchanged. Existing restore destinations
were also rejected unchanged. This bounded test does not inject an abandoned
inactive-slot sentinel, implement automatic failover/fencing, or qualify power
loss. The runbook records exact builds, receipts and application-data oracles.

Snapshot and study now compile the same private [TLS implementation](shared-app-tls.md)
under distinct profiles. Snapshot retains its SAN, ALPN, deadline and exact-pin
semantics. Study requires coordinated CLI upgrades for its new required ALPN;
snapshot wire version and acknowledgment rules are unchanged.

The [actual mixed-version snapshot CLI gate](snapshot-cli-upgrade.md) qualifies
315068f/365daf6 release binaries in both sender directions on Mac/Linux,
including persisted enrollment, retained data, failed transfer/retry and exact
durable receipts. This named pre-1 pair does not establish compatibility for
other versions or the newer maintenance commands.

## Live export and polling from an embedded primary

An application that owns the open primary can capture and poll through the
same `Database` handle it uses to commit. Keep one `Primary` per configured
follower so successive polls keep checking the accepted receipt. Pass an
authenticated channel with a deadline for each exchange.

```rust
use weekend_db::{Database, Limits, Result};
use weekend_db::replication::{DurableAck, Export, Primary, ReplicaObservation};
use std::{io::{Read, Write}, path::Path};

fn capture(
    primary: &mut Primary,
    db: &Database,
    export_path: &Path,
    copy_limits: Limits,
) -> Result<Export> {
    primary.export_live_file(db, export_path, copy_limits)
}

fn ship(
    primary: &mut Primary,
    export: &mut Export,
    authenticated: impl Read + Write,
) -> Result<(u64, DurableAck)> {
    let captured_revision = export.revision();
    let follower = primary.ship(export, authenticated)?;
    Ok((captured_revision, follower))
}

fn poll(
    primary: &mut Primary,
    db: &Database,
    authenticated: impl Read + Write,
) -> Result<ReplicaObservation> {
    primary.poll_live(db, authenticated)
}
```

Capture waits for any in-flight publication, then pins one acknowledged head,
its collection cursor and the retained roots listed at that revision. The source
locks are released before the destination is created, encoded, synced and
verified. The destination has its own memory budget. Pinned source values stay
charged to the source until encoding releases the pinned roots. Budget or I/O
failure can leave a partial destination for the caller to discard. For retries
or a sender restart, keep the completed export and its exact descriptor as in
[restartable captures](#restartable-captures).

`poll_live` samples the primary revision before writing the poll request. A
later local commit may advance the primary while the request is in flight.
`ReplicaObservation` gives that local sample and the follower's durable receipt
as separate values. `revision_gap()` counts local revisions, including metadata
commits. An empty follower has no durable receipt or gap. The file-based CLI
still uses an exclusive lease to open the primary. Use the embedded methods in
the application that holds that lease to keep writing during capture and poll.

### Verification

At `d5fdbb5`, a native test pauses the destination after its first append sync.
The source commits a new value, releases its old retained root, saves another
root and creates a collection while the destination stays paused. The first
export matches the quiescent revision-4 export byte for byte and includes
retained revision 2. Live polling reports local revision 8 and follower revision
4. The next export installs revision 8. After reopening, the follower reports
gap zero and exposes the replacement retention catalog and the latest value.

Simulator controls check that a copy-budget failure and an injected destination
sync error leave source memory use, durable bytes and the sender's local bound
unchanged. Retrying produces the same descriptor. Polling controls reject a
foreign source before channel I/O, preserve the accepted follower receipt and
reject a later regression. Existing local and follower durability barriers still
apply.

Native macOS arm64/APFS passed 660 workspace tests with all features and six
documented ignores. Strict workspace Clippy passed. ThreadSanitizer
(`nightly-2026-11-12`) passed 16 replication and concurrency tests with the
manifest-growth test left to its separate maintenance qualification. Docker
Linux arm64/ext4 passed 191 core and selected integration tests, one maintenance
ignore and strict core Clippy from the exact archived source. The suites included
allocation, compaction, retention, storage decisions, and both golden byte
histories. Windows MSVC x64 strict cross-Clippy passed.

At PR head `c9746c1`, the native Windows x64/fixed-NTFS minimum-Rust job
ran `live_export_pins_old_roots_while_the_native_source_changes` successfully
in debug and release on Rust 1.99.0. The job ran from clean PR test merge
`c02e66c14cb62b55d4029a43058cdce7bb3a2761`. The engine, storage and test
blobs matched the branch. The archive SHA-256 is
`c63b1e624e28579fb3f2ef4c0cf4e19e79953953f8c7a739a43f24ba113dc5d4`.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43777168601/job/133005969663)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43777168601/artifacts/14187137921).

Run the native overlap test and the source-preservation controls from the
archived checkout:

```sh
cargo test --locked -p weekend-db --all-features --lib replication::tests
cargo test --locked -p weekend-db --all-features --test replication \
  live_export_pins_old_roots_while_the_native_source_changes -- --exact
```

| Evidence under `target/live-replication-qualification/` | File |
|---|---|
| Source archive for `d5fdbb52f4f7ef77abb905195bbd613a5a65f45a` | `source.tar` (`e51ffdd3ccf26f816791b5a444c3e55d469add9e07703126f534bb4c5f8428f5`) |
| Native workspace and strict Clippy | `macos-workspace.log`, `macos-clippy.log` |
| Native replication and concurrency under ThreadSanitizer | `macos-tsan.log` |
| Docker Linux ext4 run and toolchain | `linux.log` |
| Windows MSVC x64 cross-Clippy | `windows-cross-clippy.log` |

The [authenticated Mac-to-Linux month workload](live-replication-month.md)
exercises the embedded primary against the actual TLS follower CLI, including
31 installs, four dropped replies and all 31 retained roots after reopen.
