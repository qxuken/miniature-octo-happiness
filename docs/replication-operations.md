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
