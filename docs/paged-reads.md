# Reading a v2 file under a memory budget

Every existing collection can use paging, including collections of scalars, bytes, lists and nested maps. Choose a mode each time you open the file. The existing eager mode remains the default for workloads that benefit from repeated in-memory lookups.

```rust
use weekend_db::{Database, DatabaseOptions, Limits, ReadOnlyDatabase, Result};

fn read(path: &std::path::Path) -> Result<()> {
    let options = || DatabaseOptions::from(Limits {
        max_live_bytes: 64 << 20,
        max_entries: 1_000_000,
        ..Limits::default()
    });

    // Strict recovery, followed by normal transactions and snapshots.
    let db = Database::open_file_with_options(path, options().with_paged_reads())?;
    let collection = db.collection_id(1);
    let snapshot = db.snapshot();
    let _value = snapshot.get(collection, b"key")?;
    snapshot.visit(collection, |_key, _value| Ok(()))?;
    drop(snapshot);
    drop(db);

    // Exclusive read-only lease. Explicit check includes every retained root.
    let reader = ReadOnlyDatabase::open_file(path, options())?;
    let _value = reader.snapshot().get(reader.collection_id(1), b"key")?;
    reader.check()
}
```

C and Odin can open a writable, strictly verified paged database with `wdb_file_open_paged(path, len, options, out)`. The existing options descriptor supplies the allocator and limits. A null options pointer selects the defaults. The read-only fast opener and streaming visitor are Rust APIs.

## Integrity and write behavior

| Operation | Checks and storage behavior |
| --- | --- |
| Strict paged open | Verifies header, slots, witnesses, COMMIT, directories and all reachable values across current and retained roots. Stabilizes and witnesses the selected generation before exposing the writer. |
| Read-only open | Fully checks structural objects and single-region values. With a matching witness, checks a multi-region byte object's header, extent, tag and declared length, then defers its payload and padding checks. Without a matching witness, verifies the entire graph at open. Performs zero writes and syncs. |
| Point read | Binary-searches the directory's first keys, reads and checks one directory page, then verifies and materializes the requested value and its children. Returned values are owned and can outlive the file handle. |
| Explicit full check | Rereads current and retained graphs under the same lease and checks the selected identity and root. An unread or retained-only damaged blob causes an error here or when read directly. |
| Scan | `Snapshot::visit` delivers binary-key order, retains at most one previous disk value for consecutive aliases, and propagates callback errors. `scan` and `scan_iter` retain their existing materialization and budget contracts. |
| Commit | An immutable overlay holds upserts and tombstones. Validation checks the base row revision. An append reuses file-local value offsets, including when a materialized value is written back elsewhere. The DATA object is rewritten for a changed directory. Append, root and witness each have their own synchronization barrier. |
| Compact copy | Re-encodes reachable values into the destination, preserves sharing and verifies the resulting current and retained graphs. |

Directory pages are process-local slices of the existing sorted DATA object. One long binary key can occupy a page by itself. Page fences, the overlap index, values, scratch and overlays use the selected allocator and budget. Reading and scanning can therefore return I/O, corruption or budget errors after a successful open. Commit and compaction staging need their own headroom. For a closed database, `storage::inspect_file_with_options` provides strict, quiescent verification without stabilization.

## Measurements

Release build of Rust 1.100.0, Apple M3 Pro, local APFS, warm filesystem cache, 64 MiB reader budget. Open and check columns are medians of five opens of the same file. Point columns are pooled random-key latencies from 1,000 reads per trial for small rows and 64 for blobs. The writable modes include reopen stabilization. Read-only measurements use an in-place full check. Fixtures were constructed outside the timer using the shipping v2 encoder.

| Keys and stored values | Mode | Open ms | Full check ms | Point p50 / p95 µs | Live MiB after open | Peak charged MiB |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 64 distinct 768 KiB blobs | Eager | 35.67 | 31.54 | 0.04 / 0.17 | 48.03 | 48.89 |
| | Strict paged | 37.36 | 31.01 | 498 / 624 | 0.10 | 1.60 |
| | Read-only | 0.16 | 30.77 | 473 / 511 | 0.10 | 1.60 |
| 10,000 distinct 16-byte values | Eager | 43.99 | 41.85 | 0.08 / 0.38 | 5.50 | 5.60 |
| | Strict paged | 40.30 | 36.80 | 6.54 / 7.29 | 0.71 | 1.30 |
| | Read-only | 36.63 | 36.43 | 6.38 / 7.00 | 0.71 | 2.01 |
| 1,000,000 keys sharing one 16-byte value | Eager | Limit | Limit | — | — | — |
| | Strict paged | 43.68 | 40.10 | 6.67 / 7.71 | 0.64 | 27.62 |
| | Read-only | 40.22 | 40.12 | 6.54 / 7.42 | 0.64 | 28.26 |

Peak is the maximum requested live core allocation in the opened handle, including reads. The read-only handle also includes its full check. Strict standalone inspection uses a separate budget. These figures exclude process RSS, the kernel cache and application-owned outputs. An eager blob lookup returns an already-resident immutable handle. The paged lookup reads and verifies 768 KiB before returning an equivalent handle.

On a separate copy of the million-key fixture, one ordered visit consumed all 1,000,000 rows in **24 ms**. After retaining the old root, a one-key update committed in **88 ms** with a **59.35 MiB** charged high-water mark. Reopen returned the edited value and the retained snapshot returned the previous value. The file grew from **28,028,928 to 56,041,472 bytes**. These are single-run write and scan observations.

To reproduce locally, set an unused fixture path per case. The edit action creates and removes a separate copy.

```sh
area=$(mktemp -d)
for kind in blobs distinct million; do
  for mode in seed eager paged fast; do
    WDB_PAGING_CASE=$kind WDB_PAGING_ACTION=$mode WDB_PAGING_FILE="$area/$kind.wdb" \
      cargo +1.100.0 test -p weekend-db --offline --locked --release --lib \
        native_read_scaling -- --ignored --nocapture || exit 1
  done
done
WDB_PAGING_CASE=million WDB_PAGING_ACTION=edit WDB_PAGING_FILE="$area/million.wdb" \
  cargo +1.100.0 test -p weekend-db --offline --locked --release --lib \
    native_read_scaling -- --ignored --nocapture
```

## Platform verification

On commit `15047ef`, the macOS workspace passed 1,117 tests in each of debug and release. Docker Linux arm64/ext4 passed 294 release tests covering the core, allocation, durability, paging, retention and compatibility. C11 and C++17 consumers on both platforms loaded the release shared library. Refusing each of the 40 allocations during paged reopen preserved the exact source bytes and released the allocator context. Windows x64 MSVC passed a cross-target type check. Native Windows execution remains outstanding.

Docker Linux used the same fixture encoder, reader budget, trial counts and lookup sequence on an ext4 volume. Timings describe this VM on the development host.

| Fixture | Mode | Open ms | Full check ms | Point p50 / p95 µs | Peak MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| 64 distinct 768 KiB blobs | Eager | 36.11 | 35.33 | 0.04 / 0.29 | 48.89 |
| | Strict paged | 34.83 | 34.41 | 537 / 606 | 1.60 |
| | Read-only | 0.12 | 34.61 | 541 / 612 | 1.60 |
| 10,000 distinct 16-byte values | Eager | 47.09 | 49.68 | 0.25 / 0.50 | 5.60 |
| | Strict paged | 37.50 | 36.75 | 7.00 / 7.79 | 1.30 |
| | Read-only | 37.26 | 37.18 | 7.00 / 7.79 | 2.01 |
| 1,000,000 keys sharing one value | Eager | Limit | Limit | — | — |
| | Strict paged | 47.23 | 47.08 | 6.96 / 8.00 | 27.62 |
| | Read-only | 46.21 | 46.53 | 6.92 / 7.79 | 28.26 |

The Linux million-key visit took 25 ms. The retained one-key update committed in 77 ms, peaked at 59.35 MiB and reopened at the expected current and retained values.

The source inventory reconciled 11,002 decisions across 228 files. All 58 selected mutation controls were detected in debug and release, including independent tests for read-only admission and the matching witness, page integrity, and the existing validation and allocation paths that moved. Whole-project compiler MC/DC and a native Windows paging run remain separate qualification work.

## Capacity boundary

V2 still has a 64 MiB physical image ceiling and a minimum 4 KiB aligned object per independently stored value. The million-key measurement exercises directory scaling and value sharing. A million independently stored values require a different durable layout. Strict verification also temporarily reads an entire DATA object. New edits remain resident until reopen. Existing files can adopt the v2 reader without migration. New files can opt into [format 3 with persisted copy-on-write pages and packed values](packed-directory.md). Sparse high offsets, larger files and conversion of existing v2 files remain open.
