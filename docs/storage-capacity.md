# Migrating and growing packed files

Existing v2 and v3 files keep their format when opened or compacted. The migration commands preserve store identity, record revisions, retained roots and the next collection and retention IDs. Close database handles before running them.

```sh
weekend-admin migrate-to old.wdb new.wdb v3
weekend-admin migrate old.wdb 1GiB
```

The first command creates and verifies a separate destination on macOS, Linux or Windows. A failed copy may leave a partial destination. The second uses the existing macOS/Linux atomic exchange and directory sync protocol to install the verified copy at the original name. Windows supports copying to a new name. Keep space for the old and new images during migration. A downgrade must fit all reachable roots at the target capacity.

Choose `v3` for packed 64 MiB files or `64MiB`, `256MiB`, `1GiB`, or `unbounded` for format 4. Rust can select the latter at creation with `DatabaseOptions::with_file_capacity(storage::FileCapacity::GiB1)`. C and Odin provide `wdb_file_create_with_capacity` with capacity in bytes and zero for unbounded growth. Memory limits still apply. File offsets are bounded by the host's signed offset range, and the existing per-value and collection limits apply.

Format 4 extends the packed layout with a capacity in the checksummed header and a zero legacy object-count field. Top-level byte values larger than 4,032 bytes occupy separate kind-6 objects. Leaf tag 8 stores the object's offset, byte length and expected object CRC as three little-endian u64s. A witnessed read-only open verifies the descriptor and object header and reads the payload on first access. Writable open and explicit full check verify every reachable payload, including retained-only bytes. Nested values retain the inline packed encoding. Recovery indexes reachable extents in sorted blocks and checks neighboring intervals for overlap. Fresh offline copies stream into their unpublished destination, remapping shared offsets across roots.

## Qualification

At storage checkpoint `0601618`, the 64 MiB reader opened a densely written **4,328,767,488-byte** native macOS file in **3.97 ms**, fully checked it in **2.72 s** and peaked at **2,116,669 engine bytes** in the measured run. An 8 GiB sparse-offset fixture used **32,312 engine bytes**. The fixed-capacity tests fill the last 16 KiB with one commit and verify the first refused commit leaves the preceding revision readable.

The one-follower workload exercised a **1,014,595,584-byte** primary, including rejection by the default 64 MiB transfer policy, explicit admission, restart, verification of current and retained values, backup and restore. Its measured native macOS reader peaked at **1,194,920 engine bytes**. Separate million-key coverage compares every row of both generations. A local commit and the follower's installation receipt remain separate events.

The migration process-interruption test terminates a child before and after 11 copy and replacement operations, then reopens the canonical file and retries. Linux qualification also fills a bounded ext4 filesystem and checks the native ENOSPC result and retained data after retry. Raw and structured fuzz targets include deterministic v3 and v4 seeds. These runs exercise process interruption and simulated failures; actual device power-loss qualification remains on the production roadmap.

[Native capacity tests](../tests/storage_capacity.rs) · [Packed directory format](packed-directory.md)

[Packaged warm and cold reader measurements](capacity-package-bench.md) · [External blob budget and relocation regressions](../tests/packed_blobs.rs)
