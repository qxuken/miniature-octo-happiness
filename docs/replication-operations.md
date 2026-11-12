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

## Transport deployment contract

Complete transport authentication before passing a channel to
`serve_authenticated`, `Primary::ship` or `Primary::poll`. For mutual TLS,
verify the configured peer certificate or trusted identity and bind that
peer to the expected Enrollment in application configuration. The embedding
application owns certificate rotation, revocation and endpoint configuration.
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
partial progress does not renew the deadline. This requires roughly 2.2 MiB/s
for a maximum 64 MiB image before overhead, and may reject a slower path.
Local verification, filesystem reads and durable flushes are not interruptible
by this socket deadline and may finish after it. A response can therefore time
out after the image became durable. This is not a 30-second wall-clock bound on
all local storage work.

`serve` admits one active connection at a time. Optional `--max-exchanges N`
exits after exactly N accepted connections, including failed authentication;
this provides a deterministic test/service lifetime without fault-injection
options. TLS/configuration is validated before binding a listener or opening
the follower. The OS listen backlog is not an application queue/admission SLA.
A stream of stalled connections can still deny service; deployment firewall,
supervision and rate controls remain necessary. If a manifest needs recovery
after an installation error, the service exits rather than continuing to expose
an old acknowledgment. Restart `serve` to recover and verify the selected image.

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
The CLI does not reopen an arbitrary saved file as an Export. After a primary
process restart, poll first; if another shipment is needed, capture to a **new**
export path. That is a newly captured image, not an automatic retry of the old
file. Remove old export files only when no process holds them and your operational
retention policy permits. A failed export can leave a partial file.

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
