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
| `src/storage/file.rs` | `a671d59cc6d2ca075ad49084901aa1d39a1c45532c4cc5a9c7c077eab3176c4f` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:268:37– | 9045..9046 | try_propagation | acquire_open_lease | A second Linux opener reports Busy at Lock while the first lease remains held. macOS acquires its lease during open. |
| E | weekend-db::weekend_db | src/storage/file.rs:400:9– | 13332..13334 | if | event | The no-hook operation succeeds. A poisoned callback prevents action and healthy callbacks count events. |
| C | weekend-db::weekend_db | src/storage/file.rs:400:12– | 13335..13338 | pattern_test | event | The optional callback is present or absent. |
| E | weekend-db::weekend_db | src/storage/file.rs:401:57– | 13421..13422 | try_propagation | event | A poisoned callback mutex returns Panic before performing the action. |
| E | weekend-db::weekend_db | src/storage/file.rs:467:49– | 15281..15282 | try_propagation | compact_file | Invalid limits return Invalid before acquisition. The ordinary compaction regressions exercise accepted limits. |
| E | weekend-db::weekend_db | src/storage/file.rs:484:64– | 15913..15914 | try_propagation | compact_file_with_hook_on | Invalid limits or refusal at request one reject setup. |
| E | weekend-db::weekend_db | src/storage/file.rs:485:37– | 15952..15953 | try_propagation | compact_file_with_hook_on | Refusal at request two or a zero cap rejects the nonempty callback. |
| E | weekend-db::weekend_db | src/storage/file.rs:487:59– | 16043..16044 | try_propagation | compact_file_with_hook_on | Refusal at request three drops the callback and observer budget. |
| E | weekend-db::weekend_db | src/storage/file.rs:490:71– | 16194..16195 | try_propagation | compact_file_with_hook_on | Refusal at request four drops the prepared observer before file I/O. |
