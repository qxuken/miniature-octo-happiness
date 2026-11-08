# Inspect, back up and restore

Close every database handle on the source file before these operations.
The source is opened read-only and held under the same exclusive lease as
a writer. Inspection verifies the selected current graph and every
retained graph. A damaged newest graph is an error even when an older
graph is readable.

```sh
cargo run --bin weekend-admin -- check state.wdb
cargo run --bin weekend-admin -- backup state.wdb backup.wdb
cargo run --bin weekend-admin -- restore backup.wdb restored.wdb
```

`check` reports format, store identity, revision, data revision, witness
status, physical and committed bytes, and retained roots. It performs no
source write or sync. An unwitnessed root can be readable. Inspection
reports that condition without making a commit durability acknowledgment.

Backup validates the source before creating the destination. It copies
the current graph and explicitly retained history, preserving IDs and
revisions. The destination is synchronized and verified before success.
Restore uses the same protocol. Both require a new destination and
reject an existing path. A failure can leave a partial new destination.
Inspect that file before deciding whether to remove it, and preserve the
original source until the restored application state has been checked.

The recovery point is the selected source revision under the lease.
When the source was cleanly closed after an acknowledged commit, that
includes the latest acknowledged commit. Copying a readable unwitnessed
root makes that selected state durable in the destination. Time and space
depend on the reachable graph, including retained roots. Provision room
for a complete copy alongside the source and measure restore time for
the application before assigning a recovery-time objective.

Store identity is preserved. A restored file replaces that lineage.
Two independently writable restored copies need a separate replication
enrollment and actor-identity procedure. Destination permissions follow
the existing adapter creation policy and process umask. Source ACLs are
not copied.

The Rust APIs are `storage::inspect_on`, `backup_on`, `inspect_file`,
`backup_file`, and `restore_file`. Custom `ByteStore` destinations must be
empty and honor the lease and synchronization contract. The inspection
and backup functions retain the source lease throughout verification and
copying. `tests/inspection.rs` sweeps source and destination I/O failures
and checks the restored current and retained rows. Its native test runs
the CLI in independent processes and verifies a read-only source.

Full schema/index verification, logical migration, salvage and detailed
corruption offsets remain tracked in the production roadmap. Use the
read-only verifier before migration and preserve the original image.

Logical format 1 export/import is documented in [logical-format.md](logical-format.md).
Use `weekend-admin export SOURCE NEW_EXPORT` and
`weekend-admin import EXPORT NEW_DATABASE`. The parser verifies format,
checksum, bounds and logical invariants before creating a destination.
