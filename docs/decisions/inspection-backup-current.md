# Inspection and backup verification

The [original operations audit](operations.md) defines group O3. The
current inventory still contains eighteen decisions. Eight propagation
sites have named controls. The retained-metadata allocation keeps its
existing [refusal evidence](rust-output-allocation.md). The nine rows
below retain the original evidence scope.

The copy now releases source caches before building the destination,
then releases the destination's serialization caches before reading it
back. The verification test checks that these allocations have retired
while the source lease is still held. A separate fault at the first
header read after the destination's final sync tests the public result
and the independently reopenable state of the copy.

| Source | SHA-256 |
| --- | --- |
| `src/storage/inspection.rs` | `f4c8b4b51570019de30e45a58285f28104f18c2e8b1a0c7cd9391e3b1d06fbf6` |

| Status | Target | Position | Bytes | Kind | Owner | Original evidence group |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/inspection.rs:34:5– | 1067..1070 | for_iteration | information | O3 |
| D | weekend-db::weekend_db | src/storage/inspection.rs:56:34– | 1940..1941 | try_propagation | inspect_on | O3 |
| D | weekend-db::weekend_db | src/storage/inspection.rs:110:34– | 3668..3669 | try_propagation | backup_on | O3 |
| D | weekend-db::weekend_db | src/storage/inspection.rs:118:54– | 4063..4064 | try_propagation | inspect_file | O3 |
| D | weekend-db::weekend_db | src/storage/inspection.rs:130:34– | 4572..4573 | try_propagation | backup_file | O3 |
| D | weekend-db::weekend_db | src/storage/inspection.rs:131:78– | 4652..4653 | try_propagation | backup_file | O3 |
| D | weekend-db::weekend_db | src/storage/inspection.rs:131:86– | 4660..4661 | try_propagation | backup_file | O3 |
| D | weekend-db::weekend_db | src/storage/inspection.rs:132:63– | 4725..4726 | try_propagation | backup_file | O3 |
| D | weekend-db::weekend_db | src/storage/inspection.rs:133:60– | 4787..4788 | try_propagation | backup_file | O3 |
