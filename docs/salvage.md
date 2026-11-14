# Explicit complete-root salvage

`weekend-admin salvage SOURCE NEW_FILE` attempts to copy the highest
independently verified complete root into a new file. It reports the
selected revision, data revision, retained-root count and verified slot
revisions. Every successful report explicitly states that witnesses were
ignored and acknowledged data loss is possible.

Normal open and inspection continue to reject a damaged selected graph.
Salvage is a separate operator-selected recovery action. It takes an
exclusive source lease and never writes or syncs the source. Each of the
two root slots is verified independently through a read-only metadata view
that masks the other slot and both witnesses. The existing codec checks
the header, chosen root, current graph and every retained graph. Salvage
chooses the higher revision among candidates that pass all checks.

The procedure copies whole verified graphs. It does not guess individual
records, reconstruct broken trees or discard a damaged retained root from
an otherwise valid candidate. If neither candidate verifies, it returns
`Corrupt`. Unsupported headers, I/O errors and resource-limit failures
abort the operation. A transient read error never triggers selection of
less data. Missing or damaged witnesses can permit salvage of a root that
ordinary recovery would reject.

The file destination is created exclusively only after source verification.
Existing destinations are never overwritten. A failure during destination
publication may leave a partial new file. Preserve the source, retain that
failure evidence as needed, and use a fresh destination for another attempt.
Verify the output with `weekend-admin check NEW_FILE` and reconcile its
application records with external receipts before switching the application.

The Rust APIs are `storage::salvage_file` and `storage::salvage_on`. The latter
accepts an already supplied empty ByteStore. Both return `SalvageReport`.
The copied database preserves the original file identity, collection IDs,
revision and recoverable retained roots. Treat a salvaged history as an
explicit recovery event. Replicated forks or restored histories require a
new stream enrollment before resuming replication.

Tests cover healthy input, damaged newest graph with intact witnesses,
missing witnesses, missing newest slot with an intact newer witness,
missing both slots, corruption of both graphs, unsupported header, I/O
failure, source byte preservation, existing destination rejection and
native CLI creation. Successful output reopens at the selected revision
and preserves its retained data. Ordinary inspection still rejects the
same damaged newest-root source.
