# Packed directory pages

Format 3 stores each collection in sorted, immutable leaves. Records of every supported value type use the same transaction, lookup, ordered visit and snapshot APIs. Changing one record can share nearly the entire collection with a retained revision.

## Creating a file

```rust
use weekend_db::{Database, DatabaseOptions, Limits};

let limits = Limits { max_entries: 1_000_000, max_live_bytes: 64 << 20,
                      max_writes: 2048, ..Limits::default() };
let options = DatabaseOptions::from(limits).with_packed_pages();
let db = Database::create_file_with_options(path, options)?;
let collection = db.create_collection()?;
let mut tx = db.begin();
tx.put(collection, b"key", &db.bytes(b"value")?)?;
tx.commit()?;
```

C and Odin expose `wdb_file_create_packed` with the same path and database-options arguments as the options-based creation API. Ordinary open detects the file format. Default creation still writes v2. Opening or compacting an existing v2 database preserves v2. Both formats currently enforce the 64 MiB physical image ceiling independently of the engine's live allocation budget.

## Disk layout and publication

The file header selects format 3. Object envelopes, alignment, root slots, commit descriptors and durable witnesses follow the existing publication protocol. Integers in structural records use little endian.

| Object | Payload |
| --- | --- |
| Leaf (kind 4) | Ordered rows, each with a u32 key length, key bytes, u64 record revision, u32 encoded value length and encoded value. |
| Branch (kind 5) | Between 1 and 128 u64 leaf offsets in key order. |
| DATA (kind 2) | u64 data revision, next collection ID and collection count. In ID order, each collection carries its ID, content revision, record count, branch count and the ordered branch offsets, all u64. |
| COMMIT (kind 3) | Names the current DATA and retained DATA roots using the existing descriptor encoding. |

A leaf packs whole rows into the 4,032 payload bytes available in a 4 KiB object. A single larger row occupies its own aligned extent. Values carry their type tag. Booleans use one payload byte, numbers use eight, and bytes, lists and maps use u32 lengths or counts. Maps store sorted keys. Each row encodes its own value, including nested values.

The writer sorts the transaction's edits, merges them into the affected leaves and reuses untouched offsets. It writes new leaves, affected branches, the top-level catalog and the commit descriptor. It syncs the append, writes and syncs the root slot, then writes and syncs the witness before acknowledging the commit. Retained roots still reference their original pages. Splits, oversized records, more collections, or larger catalogs can increase the append. Compaction copies each shared branch and leaf once.

Open verifies the reachable graph, including retained-only leaves and their inline values. It keeps page boundaries in memory. Lookup selects a leaf and decodes the requested value. Ordered visits stream the leaves. A separate full check rereads the reachable image. Read-only open also requires a matching witness. Images with invalid page checksums, key order, overlapping objects or invalid child pointers fail verification.

## Million-key measurement

An ordinary writer inserted 1,000,000 four-byte big-endian keys and 1,000,000 distinct 16-byte payloads in transactions of up to 2,048 records. After closing and reopening, the test retained revision 490, then replaced the value for key 500,000 with the seven bytes `changed` in revision 492. Both writer and reader used a 64 MiB engine budget.

The file grew from **50,171,904 to 50,188,288 bytes**. The **16,384-byte append** comprised one leaf, one branch, one DATA and one COMMIT. The normal slot and witness overwrites added another 8,192 bytes of writes at existing offsets. The regression checks the four object kinds in a separate multibranch fixture.

| Release measurement | macOS arm64 / APFS | Docker Linux arm64 / ext4 |
| --- | ---: | ---: |
| Bulk insert | 8.54 s | 3.73 s |
| Initial writable reopen | 123.69 ms | 104.56 ms |
| One-key commit, including durability barriers | 6.46 ms | 0.46 ms |
| Read-only reopen with retained revision | 119.16 ms | 102.70 ms |
| Explicit full check | 119.86 ms | 104.82 ms |
| Point lookup p50 / p95 | 4.13 / 4.79 µs | 4.13 / 5.08 µs |
| Visit and compare both complete snapshots | 199.49 ms | 164.24 ms |
| Reader live / peak engine bytes | 2,053,016 / 4,416,120 | 2,053,008 / 4,416,112 |
| Bulk writer peak engine bytes | 3,344,688 | 3,344,680 |

These are single warm runs on one development host. Point timings cover 1,001 current-root lookups; the test also compares each key in the retained root. Memory numbers count engine allocations. Process RSS and filesystem cache are outside that budget.

After reopening, the test compares every key and value in order in both snapshots, including the original 16 bytes at the edited key in the retained root. A verified backup occupies **42,168,320 bytes** and passes the same two complete scans after its own reopen. Smaller simulator tests exercise each publication I/O failure, allocation refusal and retry, retained-only corruption, invalid references, nested values and oversized rows.

```sh
cargo +1.100.0 test -p weekend-db --release --all-features --locked --test packed_directory
cargo +1.100.0 test -p weekend-db --release --all-features --locked --test packed_directory \
  million_distinct_packed_values -- --ignored --exact --nocapture
```

Native Windows runtime qualification, a v2-to-v3 migration, larger file offsets, and equivalent million-key coverage for distributed paths remain open.

## Verification checkpoint

At `a48cb71`, the macOS workspace passed 1,124 tests in each of debug and release. The million-key native test ran separately in release. The Docker Linux release run passed 302 tests including the million-key case, core unit tests and the allocation, compatibility, durability, paging and retention suites. C11 and C++17 consumers created and reopened a packed file using the macOS shared library. The Windows x64 MSVC cross-target check passed.

The source inventory reconciled 11,281 decisions across 230 files. The tests detected all 19 changed or new mutation controls in both macOS profiles. Forcing every branch to rebuild broke the exact append assertion. Allowing repeated leaf key ranges admitted a resealed invalid branch and broke the recovery test.
