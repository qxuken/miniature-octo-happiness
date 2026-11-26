# Read-only replica recovery envelope

The existing verification fixture first recovers a witnessed valid image.
It then changes the expected identity, revision and end independently, or
clears only the witness. All four changes return Corrupt without writing the
image. The read trace contains only length and read operations. This proof
targets the replica recovery guard reached by reopening a captured export
or installing a verified follower image.

The replication reopen test injects I/O failure at each read operation of
a successful export reopen. Every failed operation propagates Io, preserves
the original image and leaves progress unacknowledged.

| Source | SHA-256 |
| --- | --- |
| `src/storage/mod.rs` | `c7df5222e6d03401a600c5e624c36e3abc52e5a8341adf87d04af18ad66ef08f` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/mod.rs:405:49– | 14123..14124 | try_propagation | recover_replica | The injected read error propagates without publication. |
| C | weekend-db::weekend_db | src/storage/mod.rs:408:8– | 14280..14357 | short_circuit_or | recover_replica | Nested short-circuit expression within the four-operand guard. |
| C | weekend-db::weekend_db | src/storage/mod.rs:408:8– | 14280..14397 | short_circuit_or | recover_replica | Nested short-circuit expression within the four-operand guard. |
| C | weekend-db::weekend_db | src/storage/mod.rs:408:8– | 14280..14429 | short_circuit_or | recover_replica | Nested short-circuit expression within the four-operand guard. |
