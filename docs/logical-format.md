# Logical snapshot format 1

Logical export/import provides a format-versioned migration boundary for
verified quiescent images. It preserves store identity, current and retained
capture revisions, data revisions, collection and record revisions, and
next collection/retained IDs. Values are inline. Physical offsets, page
alignment, unreachable objects and allocation sharing are absent.

Use `storage::export_logical_on` or `export_logical_file` to obtain bytes,
and `import_logical_on` or `import_logical_file` to restore. Both take
`Limits`. File import creates a new destination only after complete
parsing and physical representability checks. Generic import requires an
empty destination. Failed installation can leave a partial newly created
destination, which the caller must inspect or remove. Existing destination
files are never overwritten.

```sh
cargo run --bin weekend-admin -- export source.wdb snapshot.logical
cargo run --bin weekend-admin -- import snapshot.logical restored.wdb
cargo run --bin weekend-admin -- check restored.wdb
```

Export takes the same exclusive source lease as inspection and never
writes or syncs the source. Close writable handles first. It verifies
current and retained graphs before export. A readable unwitnessed root
can be exported without stabilizing the source. A successful import
publishes and verifies the selected logical state durably in the new
file. Restored identity represents the same lineage. Independent
replicated forks require separate enrollment and identity rules.

## Wire layout

Integers below are unsigned 64-bit little-endian unless explicitly
specified otherwise. Counts always count entries, and byte lengths count
bytes. The whole stream ends with a CRC64/ECMA-182 of every preceding
byte, using polynomial `0x42f0e1eba9ea3693`, non-reflected input, and zero
initial and final XOR. Checksums detect damage and provide no authenticity.

| Sequence | Representation |
|---|---|
| Magic | Eight bytes `WDBLOG01` |
| Logical format version | Integer 1 |
| Store identity | 16 nonzero-as-a-whole bytes |
| Current capture revision | Integer |
| Current data revision | Integer |
| Next retained ID | Nonzero integer |
| Retained count | Integer, at most 64 |
| Each retained root | ID, capture revision, data revision |
| Data table count | Integer, at most 65 |
| Each data entry | Data revision, next collection ID, collection count, collections |
| Final checksum | CRC64 integer |

Retained IDs strictly increase from a minimum of 1 and remain below the
next retained ID. A retained capture revision is below the current
capture revision. Each data revision is no greater than its capture
revision or the current data revision. A data table entry exists exactly
once per referenced data revision, sorted in ascending order. Every entry
must be referenced by at least one current or retained root. Sharing a
data revision always selects the same directory and next collection ID.
The current next collection ID is at least every retained next collection
ID. Empty revision-zero images are representable.

Each collection contains its ID, content revision, record count and
records. Collection IDs strictly increase from 1 and remain below the
data entry's nonzero next collection ID. Content revisions are nonzero
and no greater than that data revision. Each record contains a key byte
length, key bytes, write revision and one value. Keys strictly increase
in unsigned byte lexicographic order. Write revisions are nonzero and no
greater than the collection content revision.

A value starts with a one-byte tag.

| Tag | Payload |
|---|---|
| 0, none | Integer zero |
| 1, boolean | Integer 0 or 1 |
| 2, signed integer | Two's-complement 64-bit representation |
| 3, unsigned integer | Integer |
| 4, float | IEEE-754 binary64 bits, preserving NaN payloads |
| 5, bytes | Byte length followed by bytes |
| 6, map | Entry count, then key byte length, key bytes and value for each entry |
| 7, list | Entry count followed by values in list order |

Map keys strictly increase in unsigned byte lexicographic order. Empty
keys and byte strings are valid. Unknown tags, duplicate/out-of-order
keys or IDs, missing or extra data revisions, inconsistent revisions,
invalid scalar encodings, truncation, checksum mismatches and trailing
bytes reject. Unknown magic or logical format versions return
`UnsupportedFormat`. Structural errors return `Corrupt`. Resource bounds
return `Limit` and fallible allocation failures return `OutOfMemory`.

## Bounds and compatibility

Streams are limited to 64 MiB including checksum, and at most 16,384
inline value nodes across all data entries. Configured depth, byte,
entry and value-weight bounds apply during parsing. Depth is counted
from 1 for each record's top-level value. Every data entry and value
must also fit the current physical format when encoded with sharing
removed. Export performs this representability check before returning,
so highly shared physical images can return `Limit` when their expanded
logical representation exceeds the envelope. The format provides bounded
whole-image export/import. It does not promise streaming or online work.

The version here belongs to the logical format independently of physical
file format v2 and the Rust/C ABI. Format 1 restores into the current
physical codec. A future physical codec can retain this logical reader,
but an unsupported future logical version needs an explicit converter
and fixtures before support is claimed. Schema metadata and unknown
application fields are ordinary values and retain their bytes and keys.
No application schema migration or salvage is inferred from a damaged
source.

## Evidence

`tests/logical.rs` covers exact export/import/export replay, all value
tags including a NaN payload, nested shared inputs, retained root gaps,
revision preservation, and continued allocation of collection and
retained IDs. It rejects every truncation of its fixture, repaired-CRC
invalid root/count fields, unsupported versions, trailing bytes and depth
limits without touching the destination. Native subprocesses exercise
CLI export/import, verify source bytes unchanged, preserve existing
destinations, and leave no destination after invalid input. Debug and
release tests passed on arm64 macOS. These tests are fixtures and parser
regressions, with long-running fuzzing and future-version conversion
qualification remaining separate work.

The checked-in `tests/fixtures/logical-v1.bin` is a version-1 compatibility
fixture exported from the existing retained-v2 seed. Its current revision
4 has key 0 equal to 21, while retained ID 1 captures revision 2 with key 0
equal to 11. A dedicated test imports it and reproduces the exact logical
bytes, then checks both states. Preserve this fixture across format
changes instead of regenerating it from a new writer.
