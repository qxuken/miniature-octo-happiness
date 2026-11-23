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
| `src/storage/simulator.rs` | `f52d26ee5547b04e900282b05095eee6d9d01e3faf9fe5955f22bc466206ab50` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/storage/simulator.rs:94:79– | 2673..2674 | try_propagation | from_image_with_allocator | Budget control block refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:96:40– | 2763..2764 | try_propagation | from_image_with_allocator | Volatile image copy refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:97:39– | 2804..2805 | try_propagation | from_image_with_allocator | Durable image copy refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:105:12– | 3037..3038 | try_propagation | from_image_with_allocator | Shared disk block refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:136:49– | 4170..4171 | try_propagation | crash | Volatile copy refusal after survivor copy |
| E | weekend-db::weekend_db | src/storage/simulator.rs:160:59– | 4906..4907 | try_propagation | event | First-event refusal keeps the next fault |
| E | weekend-db::weekend_db | src/storage/simulator.rs:182:51– | 5662..5663 | try_propagation | copy | Buffer reservation reports allocator refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:196:45– | 6167..6168 | try_propagation | survive | Initial survivor copy refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:303:52– | 9617..9618 | try_propagation | write | Tear prefix copy refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:304:60– | 9679..9680 | try_propagation | write | Pending buffer refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:306:65– | 9811..9812 | try_propagation | write | Volatile growth refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:310:68– | 10063..10064 | try_propagation | write | Durable growth refusal |
| E | weekend-db::weekend_db | src/storage/simulator.rs:335:62– | 10819..10820 | try_propagation | sync | Durable copy refusal |

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
