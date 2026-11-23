# Simulator allocator failure propagation

`SimulatedDisk::from_image_with_allocator` selects storage for the simulator's
two images, pending writes, and event trace. The database can use a separate
allocator. Existing factories select Global. Public `trace` and
`durable_image` copies still use plain Rust `Vec`.

The constructor test refuses each of the four allocations and checks cleanup.
The first-event test verifies that refusal leaves the deferred fault armed.
Crash and sync tests preserve the prior bytes and lease before a successful
retry. Sparse tear tests reject each required allocation before modifying
either image or queuing the write. The database tests inject into the backend
while leaving engine allocation available and check recovery classification,
reopened state, and the preceding acknowledged image.

| Source | SHA-256 |
|---|---|
| `src/storage/simulator.rs` | `af1a89b919143469a2f722695fefc1d9013e332e84803de2e216f6e9eaa29416` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/storage/simulator.rs:94:79– | 2673..2674 | try_propagation | from_image_with_allocator | Budget control block refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:96:40– | 2763..2764 | try_propagation | from_image_with_allocator | Volatile image copy refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:97:39– | 2804..2805 | try_propagation | from_image_with_allocator | Durable image copy refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:105:12– | 3037..3038 | try_propagation | from_image_with_allocator | Shared disk block refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:136:49– | 4170..4171 | try_propagation | crash | Volatile copy refusal after survivor copy |
| E | weekend-db::weekend_db | src/storage/simulator.rs:164:59– | 5056..5057 | try_propagation | event | First-event refusal keeps the next fault |
| E | weekend-db::weekend_db | src/storage/simulator.rs:186:51– | 5812..5813 | try_propagation | copy | Buffer reservation reports allocator refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:200:45– | 6317..6318 | try_propagation | survive | Initial survivor copy refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:307:52– | 9767..9768 | try_propagation | write | Tear prefix copy refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:308:60– | 9829..9830 | try_propagation | write | Pending buffer refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:310:65– | 9961..9962 | try_propagation | write | Volatile growth refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:314:68– | 10213..10214 | try_propagation | write | Durable growth refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:339:62– | 10969..10970 | try_propagation | sync | Durable copy refusal |

These rows record tested propagation. The public output tests and the
remaining legacy hook users retain their own assurance work.

## Executed checks

Rust 1.100.0 debug and release runs passed on macOS arm64 and Docker Linux
arm64/ext4. The macOS core and typed run passed 419 tests with two explicit
ignores per profile. Docker Linux passed 245 selected core and storage tests
per profile. Both storage golden-history tests ran in every profile. The
macOS durability and storage-decision targets also passed 34 tests per
profile without `fault-injection`. Strict Rust Clippy passed on macOS and
for the Windows MSVC target.

The reconciled inventory binds these 13 propagation rows on the current
source. It records 10,582 decisions and 9,904 individual gaps, including
2,544 runtime records. Native Windows execution retains its hosted gate.
