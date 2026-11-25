# Journal state encoding and local publication

The fixed-size journal entry stores the coordinator identity, client slot and
two optional payloads. The decode fixture round-trips empty and populated
states and changes one field at a time to check invalid sizes, magic, flags,
identity, slot, receipt encoding, absent payload bytes and intent bounds.
A persisted receipt must have a positive sequence and a representable
sequence plus the initial revision. Consequently, a decoded receipt cannot
exhaust the next sequence while admitting a pending intent.

The storage fault fixtures fail each local staging operation, reopen the
database, and check whether the publication barrier made the intent durable.
A failed stage never sends the write. A separate sweep starts after the
remote commit and verifies pending versus receipted recovery on each side
of local publication. The checks exercise the save operation end to end;
underlying allocation and engine error paths keep their delegated scope.

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/journal.rs` | `5c4153f888d12a15975f7699e8bdaee03daf613f8f7c3f184df3aeca2f3e3df9` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:49:9– | 1191..1193 | if | encode | Receipt payload is present or absent. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:49:12– | 1194..1197 | pattern_test | encode | Receipt payload is present or absent. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:53:9– | 1334..1336 | if | encode | Pending expected revision and value are present or absent. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:53:12– | 1337..1340 | pattern_test | encode | Pending expected revision and value are present or absent. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:62:12– | 1637..1678 | short_circuit_or | decode | Part of the independently varied length, magic and reserved-bit header guard. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:70:20– | 2019..2021 | if | decode | Receipt flag selects present or absent layout. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:71:63– | 2105..2106 | try_propagation | decode | Version or receipt-field validation can reject the payload. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:78:23– | 2275..2277 | if | decode | Pending flag selects present or absent layout. |
| B | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:95:9– | 2770..2772 | if | decode | Pending range is evaluated only for a present intent; sequence exhaustion is excluded after receipt validation. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:95:12– | 2773..2776 | pattern_test | decode | Pending range is evaluated only for a present intent; sequence exhaustion is excluded after receipt validation. |
| B | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:95:12– | 2773..2943 | short_circuit_and | decode | Pending range is evaluated only for a present intent; sequence exhaustion is excluded after receipt validation. |
| B | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:96:17– | 2817..2942 | short_circuit_or | decode | The sequence overflow operand cannot independently reject a valid receipt. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:115:43– | 3233..3234 | try_propagation | load | Catalog read delegates to the snapshot. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:116:45– | 3279..3280 | try_propagation | load | Missing journal row maps to UnsupportedFormat. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:117:77– | 3358..3359 | try_propagation | load | Journal row must be bytes and State::decode must accept the encoding. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:117:79– | 3360..3361 | try_propagation | load | Journal row must be bytes and State::decode must accept the encoding. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:122:64– | 3507..3508 | try_propagation | open | File reopen propagates engine errors. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:128:28– | 3707..3708 | try_propagation | create | Initial state is published using the normal save path. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:137:44– | 3942..3943 | try_propagation | save | Encoded payload allocation can fail before publication. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:138:10– | 3954..3955 | try_propagation | save | The journal row is staged in one transaction. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:139:20– | 3976..3977 | try_propagation | save | The in-memory state changes only after commit succeeds. |
