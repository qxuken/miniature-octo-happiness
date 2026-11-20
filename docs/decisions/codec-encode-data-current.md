# DATA encoder decisions

The DATA encoder reuses a cached offset only when the revision and next
collection counter agree. On a miss it writes numeric collection IDs and
bytewise record keys in ascending order, staging each referenced VALUE first.

The directed test seeds four acknowledged DATA objects and disables
allocation for a cache hit and a counter mismatch. It compares exact DATA
payload bytes for a collection with an empty key, a key with an embedded
NUL and shared VALUE references. The second collection has no records.
Both order controls omit a sort and must fail that byte comparison. The
header test checks DATA kind, payload length and the data revision at birth.

The allocation sweep runs the populated shape and three single-record
variants. Key widths of 1, 40 and 56 bytes move payload growth to the record
revision, collection record count and collection ID. Each refusal returns
`OutOfMemory` and releasing the draft restores the live allocation charge.
The original cache retains all four entries in the same order. A successful
draft is read back through its new indexed entry and a repeated encode
reuses the staged offset.

The internal short collection key establishes the conversion and callback
error path. Public collection keys come from eight-byte IDs. The record
visitor pushes into a reserved buffer and always returns `Ok(())`, so its
propagation row retains I for that invariant. The enclosing cache `if` stays
coupled to its directed pattern pair. Loop, payload and allocation evidence
is scoped to these calls.

The existing `serialization_data_golden_history` regression verifies the
durable image fingerprints, 64 retained roots, reopened snapshots, and
matching compact and backup images. Production codec bytes match the parent.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `d0ec5a37605a1221aebfa6cf5cc53f1a7314139b19d477d4e104807a32cbf373` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| C | weekend-db::weekend_db | src/storage/codec.rs:636:9– | 21659..21661 | if | data | Coupled to the same cache pattern |
| P | weekend-db::weekend_db | src/storage/codec.rs:636:12– | 21662..21665 | pattern_test | data | Borrowed cache is hit without allocation |
| P | weekend-db::weekend_db | src/storage/codec.rs:638:13– | 21756..21758 | if | data | Equal and mismatched next-collection counters |
| E | weekend-db::weekend_db | src/storage/codec.rs:644:40– | 21991..21992 | try_propagation | data | DATA revision field allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:645:37– | 22030..22031 | try_propagation | data | Next-collection field allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:646:51– | 22083..22084 | try_propagation | data | Collection-count field allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:648:67– | 22201..22202 | try_propagation | data | Collection traversal buffer allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:650:83– | 22333..22334 | try_propagation | data | Invalid internal collection key length |
| E | weekend-db::weekend_db | src/storage/codec.rs:653:11– | 22405..22406 | try_propagation | data | Invalid key error from the collection callback |
| E | weekend-db::weekend_db | src/storage/codec.rs:655:9– | 22473..22476 | for_iteration | data | Empty and populated collection maps |
| E | weekend-db::weekend_db | src/storage/codec.rs:656:28– | 22529..22530 | try_propagation | data | Collection ID field allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:657:36– | 22567..22568 | try_propagation | data | Collection revision field allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:658:48– | 22617..22618 | try_propagation | data | Record-count field allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:660:60– | 22728..22729 | try_propagation | data | Record traversal buffer allocation |
| I | weekend-db::weekend_db | src/storage/codec.rs:664:15– | 22862..22863 | try_propagation | data | Record callback always returns Ok after the reserved push |
| E | weekend-db::weekend_db | src/storage/codec.rs:666:13– | 22936..22939 | for_iteration | data | Empty and populated record maps |
| E | weekend-db::weekend_db | src/storage/codec.rs:667:50– | 23011..23012 | try_propagation | data | VALUE child encoding error |
| E | weekend-db::weekend_db | src/storage/codec.rs:668:33– | 23046..23047 | try_propagation | data | Record key length or byte allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:669:40– | 23088..23089 | try_propagation | data | Record revision field allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:670:36– | 23126..23127 | try_propagation | data | Record VALUE offset field allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:674:67– | 23245..23246 | try_propagation | data | DATA staging error |
| E | weekend-db::weekend_db | src/storage/codec.rs:676:71– | 23333..23334 | try_propagation | data | Borrowed DATA cache fork error |
| E | weekend-db::weekend_db | src/storage/codec.rs:677:63– | 23398..23399 | try_propagation | data | DATA index reserve error |
| E | weekend-db::weekend_db | src/storage/codec.rs:679:67– | 23484..23485 | try_propagation | data | DATA cache entry reserve error |
