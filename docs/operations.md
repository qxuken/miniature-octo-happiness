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

Typed `Read::check_indexes` and `Write::rebuild_indexes` verify or atomically
rebuild indexes from primary records. Explicit complete-root salvage is
documented in [salvage.md](salvage.md). Opt-in [verification diagnostics](verification-diagnostics.md) report the failed
stage and structure-start offset when known. Use the read-only verifier before migration and preserve the
original image.

Logical format 1 export/import is documented in [logical-format.md](logical-format.md).
Use `weekend-admin export SOURCE NEW_EXPORT` and
`weekend-admin import EXPORT NEW_DATABASE`. The parser verifies format,
checksum, bounds and logical invariants before creating a destination.

## Space and I/O diagnostics

For an open byte-backed database, `database.storage_space()` reports the
current revision, physical and committed bytes, aligned append offset and
remaining capacity. It performs one length query under the publication mutex,
without a graph scan. Remaining capacity is append headroom, not reclaimable
space. An abandoned partial tail consumes a complete protected region.
Compact before an expected edit can exhaust this budget, while provisioning
a complete verified copy alongside the authoritative file.

```rust,ignore
let space = database.storage_space()?;
println!("revision {}: {} append bytes remain",
         space.revision, space.remaining_append_bytes);

match weekend_db::storage::with_native_diagnostics(||
    weekend_db::storage::inspect_file(path, config)) {
    Ok(info) => println!("verified revision {}", info.revision),
    Err(failure) => {
        // Use failure.error for retry/reopen decisions.
        // Native context is optional and contains no file path or record data.
        eprintln!("{:?}: {:?}", failure.error, failure.native);
    }
}
```

Native categories distinguish permission, disk-full and quota failures from
other errors. The diagnostic describes the first terminal syscall error in the
synchronous scope, which can include cleanup; it does not change the engine's
durability classification. No last-error lookup is needed. Nested scopes and
threads are isolated. Custom adapters and operations on another thread may
have no native context. Convenience `_with_diagnostics` variants are available
for file create/open, supported in-place compaction and transaction commit.
