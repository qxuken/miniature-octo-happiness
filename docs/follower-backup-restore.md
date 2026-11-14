# Back up a verified follower and explicitly seed a new stream

A follower receipt identifies one verified snapshot, not necessarily the primary's
latest local commit. Restoring that snapshot creates an explicitly reconciled
branch of history. This procedure does not implement automatic failover, recover
unshipped primary commits, resume a stale primary under its old stream, or grant
permission to run two primary writers for one enrollment.

## Preconditions and authoritative selection

Stop the follower service and all other users of its dedicated directory. Keep it
quiescent through status, hashing and backup. Stop any primary writer whose file
will be inspected. Keep the old primary/follower artifacts intact for comparison
and reconciliation; decide explicitly which history is authoritative before
switching application traffic. TLS peer authentication and trusted local paths
are still required; see [replication operations](replication-operations.md).

```sh
weekend-replicate status /private/old-follower OLD_STREAM FILE_IDENTITY
```

Status opens and verifies the authoritative manifest and its selected image,
then closes its lease on exit. Require `requires_recovery=false`, a durable
revision/stream/digest and `selected_slot=Some(0)` or `Some(1)`. An empty follower
has no backup source. An error or recovery-required state stops this procedure;
do not guess a slot by file size, timestamp, revision-looking bytes, or existence.
The selected path is exactly `/private/old-follower/replica-0.wdb` or
`replica-1.wdb` from that verified status. Never substitute an inactive slot.

Record the complete receipt and independently hash the selected file, with no
live follower lease. Its SHA-256 must equal the receipt digest. On macOS use
`shasum -a 256 SELECTED_IMAGE`; on Linux use `sha256sum SELECTED_IMAGE`. If it does
not match, stop and investigate. This comparison binds the source bytes to the
receipt; it does not replace the library's graph verification.

## Verified copy and restore to new paths

```sh
weekend-admin backup SELECTED_IMAGE /private/new-backup.wdb
weekend-admin check /private/new-backup.wdb
weekend-admin restore /private/new-backup.wdb /private/new-primary.wdb
weekend-admin check /private/new-primary.wdb
```

Both destination paths must be new. Backup/restore verify the source, preserve
its file identity and recoverable retained roots, and create a verified copy.
They reject an existing destination; they do not replace it. A failed operation
may leave its new partial destination for inspection. Do not treat its existence
as success or overwrite an unrelated file to retry.

The verified copy can repack physical layout. Therefore the original receipt's
digest belongs to the selected source image, not necessarily to the new backup or
restored file bytes. Record each new file's own hash and inspect current and
retained revisions; use application-level reads to verify required records and
indexes. Do not invent a follower receipt for a local copy. Retained roots may
represent older application schema states and require the corresponding reader.

## New stream and fresh follower

Choose a new nonzero random 16-byte stream identifier, persist it with the new
configuration, and use an entirely new private follower directory. Preserve the
restored file identity reported by `describe`; backup does not change it.

```sh
NEW_STREAM=$(openssl rand -hex 16)
weekend-replicate describe /private/new-primary.wdb "$NEW_STREAM"
mkdir -m 700 /private/new-follower
weekend-replicate follower-init /private/new-follower "$NEW_STREAM" FILE_IDENTITY
weekend-replicate serve /private/new-follower "$NEW_STREAM" FILE_IDENTITY \
  127.0.0.1:9443 /private/follower-credentials /private/primary-cert.der

# In another process, with the restored primary writer closed for capture:
weekend-replicate ship /private/new-primary.wdb "$NEW_STREAM" \
  /private/new-export.wdb 127.0.0.1:9443 \
  /private/primary-credentials /private/follower-cert.der --attempts 1
```

Use your deployment address and separately provisioned/pinned credentials; never
copy private keys between peers. The `ship` output separates local capture from
the new follower's durable receipt. Require the new receipt's stream to be
NEW_STREAM, its revision to match the captured snapshot and its digest to match
the immutable export. Stop the new service, run offline status, and verify that
reopening reports the identical receipt and expected current/retained data.
A failed or missing reply is not proof of rollback: reconcile with recovered
status/poll before retrying. Only then make an explicit application configuration
switch. The library does not retire the old stream or fence a previously running
primary on your behalf.

Using NEW_STREAM to open the old follower or offering a NEW_STREAM snapshot to
its authenticated service must reject with ForeignStore. That is an enrollment
check, not a certificate failure. Do not edit the old manifest/enrollment to make
it accept the restored history.

## Reproducible qualification

```sh
python3 tools/follower-backup-restore.py --source 65d142e \
  --output target/follower-backup-restore-NEW
```

The output directory must be new. This private macOS/Linux harness archives the
chosen commit, builds the existing `weekend-replicate` and `weekend-admin` with
`--offline --locked`, and leaves the original Cargo.lock unchanged. It overlays
only the existing compatibility-driver fixture and an explicit example entry;
that fixture seeds and checks current/retained rows that admin intentionally does
not print. No CLI/core runtime edits or publication occur. The native process
workflow uses exact-leaf mTLS credentials generated in its private output directory.
All commands, logs, source archive/fixture/runner hashes, copied binary hashes,
lockfile and toolchain are retained. Builds, handshakes and process lifetimes are
bounded. A failed run exits unsuccessfully and does not write verified evidence.
The harness supports native macOS/Linux key generation, not Windows key provisioning.

Frozen source `65d142e6ae85e477f0abb21e2ab04d67393db1f3` passed on native arm64 macOS
with rustc 1.99.1 / LLVM 23.1.1, using Python `-O` so success does not depend on
Python assertions. The seeded current root was revision 4 (`key=21`,
`new="current"`), with retained root 1 at revision 2 (`key=11`, no `new` key).
The old follower's selected image matched its exact receipt; verified backup and
restore preserved all these rows. Existing-destination restore rejected with
AlreadyExists and preserved its bytes. Both offline wrong-enrollment status and
an authenticated wrong-stream upload rejected with ForeignStore; every old follower
file stayed unchanged. The new follower reopened with the new-stream receipt and
identical current/retained contents. The original primary and all old follower
files remained byte-identical at the end.

The old stream was `9aede2cf9490aa7c19aac60f128eeacb`, the new stream
`17b5648411de5f7ef268892db060fb06`, and each durable receipt reported revision 4.
For this small already compact image both exact export digests happened to equal
`ae6a2a9f26e00f41ed6a589309597177db076bef7554920dbf32dcbfc9124ab7`;
that equality is not required for other layouts. Full evidence is
`target/follower-backup-restore-reviewed/evidence.json` with adjacent commands,
logs, images and original lockfile.

## Remaining operational boundaries

There is no atomic CLI command spanning status selection and backup. The operator
must maintain quiescence between these commands; restarting the service in between
could change the authoritative slot. There is no automatic primary fencing,
promotion or application schema/index reconciliation. The harness does not test
power loss, interruption during copy, long-duration operation, Windows, remote
host trust rotation, or recovery of commits absent from the selected follower.
Existing storage fault tests and platform qualification remain separate evidence.
