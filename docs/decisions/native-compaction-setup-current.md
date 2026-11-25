# Unix compaction setup and observer failure

Ordinary compaction rejects invalid limits before opening the source. The
fault-injection entry uses separate budgets for the observer and compaction.
The fixture refuses each of the first four allocation requests in order:
observer budget, callback box, shared callback and compaction budget. Each
refusal returns OutOfMemory, runs no callbacks, preserves the original file
and returns allocator bytes to baseline. A zero live-byte limit rejects
the nonempty callback with Limit. Compaction with valid limits succeeds
and the original revision reopens.

A poisoned observer returns Panic before the operation closure runs. The
same operation with an absent observer succeeds. The held native lease
case reports Busy with a Lock diagnostic on Linux and Open on macOS,
where the open acquires the lease atomically.

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `0584ba7e5eb42f5d949ceb6408f17c04059c53403d10ee9534efb53e4f212390` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:265:37– | 8924..8925 | try_propagation | acquire_open_lease | A second Linux opener reports Busy at Lock while the first lease remains held. macOS acquires its lease during open. |
| E | weekend-db::weekend_db | src/storage/file.rs:397:9– | 13211..13213 | if | event | The no-hook operation succeeds. A poisoned callback prevents action and healthy callbacks count events. |
| C | weekend-db::weekend_db | src/storage/file.rs:397:12– | 13214..13217 | pattern_test | event | The optional callback is present or absent. |
| E | weekend-db::weekend_db | src/storage/file.rs:398:57– | 13300..13301 | try_propagation | event | A poisoned callback mutex returns Panic before performing the action. |
| E | weekend-db::weekend_db | src/storage/file.rs:464:49– | 15160..15161 | try_propagation | compact_file | Invalid limits return Invalid before acquisition. The ordinary compaction regressions exercise accepted limits. |
| E | weekend-db::weekend_db | src/storage/file.rs:481:64– | 15792..15793 | try_propagation | compact_file_with_hook_on | Invalid limits or refusal at request one reject setup. |
| E | weekend-db::weekend_db | src/storage/file.rs:482:37– | 15831..15832 | try_propagation | compact_file_with_hook_on | Refusal at request two or a zero cap rejects the nonempty callback. |
| E | weekend-db::weekend_db | src/storage/file.rs:484:59– | 15922..15923 | try_propagation | compact_file_with_hook_on | Refusal at request three drops the callback and observer budget. |
| E | weekend-db::weekend_db | src/storage/file.rs:487:71– | 16073..16074 | try_propagation | compact_file_with_hook_on | Refusal at request four drops the prepared observer before file I/O. |
