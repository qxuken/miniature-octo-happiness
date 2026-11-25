# Native file replacement and scratch custody

Native compaction first validates and locks the source. It rejects a
hard-linked source before creating a scratch file. It then publishes the
scratch image, verifies it, copies file metadata, flushes metadata and
verifies the image again. Both directory entries must still name the
inodes held by the operation. In separate controls, replacing either
entry with a byte-identical file on a different inode rejects with Invalid
before exchange.

The native test uses the original and successful compacted files as byte
oracles. It checks the precise callback trace for failure before and after
each action, and confirms that a cooperating opener sees Busy throughout
the successful operation. It compares canonical and scratch inode
identities, bytes and presence before reopening. Reopen checks current
and retained rows. Each fixture includes an unrelated scratch-shaped
sibling whose bytes survive.

| Failed hook | Public status | Canonical file | Operation's scratch file |
| --- | --- | --- | --- |
| Before the scratch is created | Io | Original | Absent |
| After scratch creation and before exchange | Io | Original | Partial or complete candidate |
| Immediately before exchange | OutcomeUnknown | Original | Complete candidate |
| After exchange and before old-file unlink | OutcomeUnknown | Compacted | Original |
| After old-file unlink through final sync | OutcomeUnknown | Compacted | Absent |

These rows describe the running native filesystem. For example, an error
after exchange has executed can report OutcomeUnknown even when the
compacted file already occupies the canonical name. A target publication
error during candidate preparation is remapped from OutcomeUnknown to Io
while the source name is still authoritative. The translation preserves
other failures.

## Namespace model

The model checks both sides of each of the six barriers. O means original
canonical and candidate scratch. X means compacted canonical and original
scratch. C means compacted canonical with no scratch.

| Most recent completed action | Durable names | Flushed names | Volatile names |
| --- | --- | --- | --- |
| None | O | O | O |
| Exchange | O | O | X |
| Directory sync | O | X | X |
| Replacement file sync | X | X | X |
| Old-file unlink | X | X | C |
| Cleanup directory sync | X | C | C |
| Cleanup file sync | C | C | C |

The model reopens the resulting canonical image for all three persistence
choices. Every image matches the expected current and retained rows.
The existing first-error test also checks the exact prefix and returned
error independently of the namespace model. Physical power-loss and
deployment filesystem qualification remain open.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `0584ba7e5eb42f5d949ceb6408f17c04059c53403d10ee9534efb53e4f212390` |
| `src/storage/replacement.rs` | `758de2d4ff064a05b11d046097206602d2f2504f9748d2d0e99ec3693b12ebdf` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:404:32– | 13524..13525 | try_propagation | call | A failed before callback prevents the action. |
| E | weekend-db::weekend_db | src/storage/file.rs:406:31– | 13588..13589 | try_propagation | call | A failed after callback returns an error after the action executed. |
| E | weekend-db::weekend_db | src/storage/file.rs:490:39– | 16223..16224 | try_propagation | compact_file_impl | Reject an empty path before opening a source. |
| E | weekend-db::weekend_db | src/storage/file.rs:491:62– | 16287..16288 | try_propagation | compact_file_impl | Open a valid source and propagate missing or leased source errors. |
| P | weekend-db::weekend_db | src/storage/file.rs:494:5– | 16424..16426 | if | compact_file_impl | Reject a two-link source before scratch creation, then accept the same source with one link. |
| E | weekend-db::weekend_db | src/storage/file.rs:504:6– | 16724..16725 | try_propagation | compact_file_impl | A source recovery error stops before scratch creation. |
| E | weekend-db::weekend_db | src/storage/file.rs:526:29– | 17484..17485 | try_propagation | compact_file_impl | A verification error for a published scratch image leaves the source name alone. |
| P | weekend-db::weekend_db | src/storage/file.rs:530:13– | 17563..17565 | if | compact_file_impl | Remap OutcomeUnknown from target publication to Io and preserve ordinary errors. |
| E | weekend-db::weekend_db | src/storage/file.rs:535:11– | 17684..17685 | try_propagation | compact_file_impl | A preparation error stops before copying metadata. |
| E | weekend-db::weekend_db | src/storage/file.rs:538:7– | 17800..17801 | try_propagation | compact_file_impl | Metadata-copy refusal stops before metadata sync or exchange. |
| E | weekend-db::weekend_db | src/storage/file.rs:539:73– | 17875..17876 | try_propagation | compact_file_impl | Metadata-sync refusal stops before verification and exchange. |
| E | weekend-db::weekend_db | src/storage/file.rs:542:28– | 18060..18061 | try_propagation | compact_file_impl | Verify again after copying and syncing metadata. A failed read stops publication. |
| E | weekend-db::weekend_db | src/storage/file.rs:544:36– | 18149..18150 | try_propagation | compact_file_impl | A replacement canonical inode independently rejects. |
| E | weekend-db::weekend_db | src/storage/file.rs:546:7– | 18193..18194 | try_propagation | compact_file_impl | A replacement of either inode independently prevents exchange. |
| E | weekend-db::weekend_db | src/storage/file.rs:550:40– | 18430..18435 | match | compact_file_impl | Verify all six stage dispatches with independent before and after observations. |
| E | weekend-db::weekend_db | src/storage/file.rs:564:40– | 19167..19168 | try_propagation | compact_file_impl | A failed namespace action reports OutcomeUnknown with the observed inode and scratch state. |
| E | weekend-db::weekend_db | src/storage/replacement.rs:14:5– | 419..422 | for_iteration | publish | The six actions execute in the listed order. The fixed array contains six entries. |
| E | weekend-db::weekend_db | src/storage/replacement.rs:22:20– | 600..601 | try_propagation | publish | An error at any stage stops at that action and preserves the precise callback status. |

## Verification at 23eb940

- Rust 1.100.0 passed 251 tests per profile on macOS arm64 and 259 per
  profile on Docker Linux arm64 with ext4. Both golden histories passed
  in every profile. The frozen source was
  `23eb94005e4ae919b50b8f975cc521afd915524c`.
- Both macOS profiles detected all 21 selected controls. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-native-replace- --case replacement-errors`
  and repeat with `--release`. The selection reuses the namespace
  first-error mutation.
- Miri passed the focused namespace propagation test. Strict Clippy
  passed on macOS and for Windows MSVC. Ten mutation-runner tests passed.
- The fuzz library's three namespace replay and model tests passed in
  both macOS profiles with `cargo test --locked --manifest-path fuzz/Cargo.toml --lib namespace`
  and its `--release` variant.
- The inventory maps 18 more runtime records. It reports 10,581 decisions
  and 9,574 open records, including 2,204 runtime.
