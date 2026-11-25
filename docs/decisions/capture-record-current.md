# Local capture descriptor qualification

The record has an eight-byte magic followed by the stream identity, file
identity, little-endian revision, little-endian length and image digest.
The golden row uses successive bytes from 0 through 79 after the magic,
so swapping either identity or either number changes the expected bytes.
The native regression reads the row through the database API and separately
loads the typed descriptor. It compares the original file after duplicate
creation and checks missing files, missing collections and rows, a lease held
by another database handle, wrong types, short and long rows and wrong magic.

A second regression persists six individually changed descriptors for one
real export. Reopening rejects the wrong stream, wrong file identity or
future revision as ForeignStore, an older claimed revision or changed digest
as Corrupt, and zero length as Limit. The genuine descriptor reopens the
same image. The primary, descriptor and export bytes are checked after
verification. A descriptor records a local capture; durable follower progress
is observed separately.

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native/capture_record.rs` | `223a36ac490ec4448b301f3f4a777e4ba3d198b62b9b0da9abfbe19da5656954` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:24:7– | 1064..1065 | try_propagation | store | A fresh path succeeds. Reusing the existing path returns AlreadyExists and preserves the original bytes. |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:25:48– | 1114..1115 | try_propagation | store | The collection creation transaction uses normal engine publication and allocation errors. |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:27:50– | 1199..1200 | try_propagation | store | The fixed payload is allocated and staged through the database transaction; allocation failures remain delegated. |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:27:52– | 1201..1202 | try_propagation | store | The fixed payload is allocated and staged through the database transaction; allocation failures remain delegated. |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:28:56– | 1259..1260 | try_propagation | store | The descriptor becomes available after the database commit; commit error outcomes follow storage publication. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:35:7– | 1460..1461 | try_propagation | load | Missing file and held lease produce NotFound and Busy; a closed valid file reopens. |
| B | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:40:13– | 1586..1588 | if | load | An absent collection maps NotFound to UnsupportedFormat. Other snapshot lookup errors are delegated. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:45:11– | 1724..1725 | try_propagation | load | An empty database rejects before row access; the initialized collection supplies a valid lookup. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:46:41– | 1766..1767 | try_propagation | load | An existing empty collection lacks the row; adding the row allows decoding. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:47:59– | 1827..1828 | try_propagation | load | A numeric value rejects as Corrupt. A byte value reaches layout verification. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:48:5– | 1834..1836 | if | load | Valid width and magic admit the record; wrong width alone and wrong magic alone reject. |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native/capture_record.rs:48:8– | 1837..1879 | short_circuit_or | load | Valid width and magic admit the record; wrong width alone and wrong magic alone reject. |
