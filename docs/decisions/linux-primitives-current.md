# Linux native primitives, entropy and metadata copying

The native Linux fixtures use real ext4 files and owned descriptors.
They test the following success and failure outcomes.

| Operation | Native success | Native refusal |
| --- | --- | --- |
| Exclusive flock | Claim and later reacquire the inode | A second description receives Busy with raw EAGAIN and a Lock diagnostic |
| File sync | Sync the claimed regular file | A socket descriptor returns Io with raw EINVAL and a Sync diagnostic |
| Atomic exchange | Swap exact inode identities and bytes | A missing partner reports NotFound with raw ENOENT and leaves the original name in place |
| Metadata copy | Apply the source mode and attribute value and remove a target-only attribute | A selected allocation refusal at either name list or the first value returns OutOfMemory without changing file contents |

The metadata fixture verifies the copied value and missing attribute with
direct kernel reads. A retry of each allocation refusal completes on the
same destination and releases all temporary allocator charges. The scope
covers data preservation and the requested metadata. The operation can
apply some metadata to its candidate before returning an error. Real
ownership mismatch, kernel xattr mutation refusal and metadata read
failure still need separate qualification.

## Controlled entropy and bounded metadata reads

The entropy scripts deliver 5, 10 and 1 bytes, proving the final byte is
requested and only the remaining suffix is overwritten. The EINTR case
repeats the same suffix and leaves no terminal diagnostic. Terminal errno
survives, including a refusal after a partial fill. Zero progress and
oversized reports each reject, while exact remaining capacity succeeds.
The complete-draw controls accept on the first or third attempt and
reject after three complete zero identities.

Metadata blob controls distinguish the query and fill phases. A negative
callback preserves its native diagnostic. An interrupted query retries.
The bounded allocation accepts 64 KiB and rejects one byte more before
allocating. It propagates genuine allocator refusal separately. Both
a short and an oversized fill reject, and an exact fill returns its
complete bytes. These scripts exercise wrapper decisions. Device-level
flush guarantees remain a deployment gate.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/linux.rs` | `d9ef4abcfcbb9a0d048f67b38b81d23bdae27241b70ad99f4f212ac3dc7040ea` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/linux.rs:61:5– | 2478..2483 | match | os_error | Exercise each mapped errno, the default raw error and an error with no raw value. |
| P | weekend-db::weekend_db | src/storage/linux.rs:76:9– | 3141..3143 | if | acquire_open_lock | A real competing description independently fails while the initial and later acquisitions succeed. |
| E | weekend-db::weekend_db | src/storage/linux.rs:100:5– | 3879..3882 | for_iteration | identity_with | One or three complete identities are collected. Three zeros exhaust the attempt budget. |
| P | weekend-db::weekend_db | src/storage/linux.rs:103:9– | 3961..3966 | while | identity_with | Visit until the exact buffer fills, including a final one-byte suffix. |
| P | weekend-db::weekend_db | src/storage/linux.rs:106:17– | 4110..4112 | if | identity_with | Consult errno only on the negative callback and preserve the operation and code. |
| E | weekend-db::weekend_db | src/storage/linux.rs:112:15– | 4327..4328 | try_propagation | identity_with | Retry an interruption and propagate the terminal error on a distinct case. |
| P | weekend-db::weekend_db | src/storage/linux.rs:113:13– | 4342..4344 | if | identity_with | Both zero and excessive progress independently reject. An exact positive count advances. |
| C | weekend-db::weekend_db | src/storage/linux.rs:113:16– | 4345..4376 | short_circuit_or | identity_with | Same measured predicates and source range as the enclosing if. |
| P | weekend-db::weekend_db | src/storage/linux.rs:118:9– | 4475..4477 | if | identity_with | A complete nonzero identity returns. A complete zero identity consumes another attempt. |
| P | weekend-db::weekend_db | src/storage/linux.rs:127:5– | 4819..4821 | if | exchange | Swap both inodes or reject ENOENT while preserving the one existing name. |
| P | weekend-db::weekend_db | src/storage/linux.rs:146:5– | 5221..5223 | if | status | A zero status succeeds and a nonzero status captures native errno. |
| P | weekend-db::weekend_db | src/storage/linux.rs:159:13– | 5574..5576 | if | blob | A negative query or fill captures errno. Nonnegative progress retains its size. |
| E | weekend-db::weekend_db | src/storage/linux.rs:166:45– | 5761..5762 | try_propagation | blob | A terminal query refusal stops before allocation or fill. |
| P | weekend-db::weekend_db | src/storage/linux.rs:169:5– | 5926..5928 | if | blob | 64 KiB reaches allocation. One byte larger rejects first. |
| E | weekend-db::weekend_db | src/storage/linux.rs:173:43– | 6076..6077 | try_propagation | blob | Refuse the exact requested capacity and preserve live allocator bytes. |
| E | weekend-db::weekend_db | src/storage/linux.rs:175:50– | 6155..6156 | try_propagation | blob | Preserve terminal fill refusal after a successful query. |
| P | weekend-db::weekend_db | src/storage/linux.rs:176:5– | 6162..6164 | if | blob | Accept exactly the queried byte count and reject both smaller and larger fills. |
| E | weekend-db::weekend_db | src/storage/linux.rs:184:7– | 6418..6419 | try_propagation | names | Preserve the selected allocator refusal during real xattr name collection. |
| E | weekend-db::weekend_db | src/storage/linux.rs:203:40– | 7333..7334 | try_propagation | copy_metadata | Refuse the source-list allocation first and retry successfully. |
| E | weekend-db::weekend_db | src/storage/linux.rs:204:38– | 7373..7374 | try_propagation | copy_metadata | Refuse the destination-list allocation after collecting the source names. |
| E | weekend-db::weekend_db | src/storage/linux.rs:205:5– | 7380..7383 | for_iteration | copy_metadata | Visit the destination attribute names, including the stale attribute. |
| P | weekend-db::weekend_db | src/storage/linux.rs:206:9– | 7444..7446 | if | copy_metadata | The matching name continues to value copy. The destination-only attribute is removed. |
| E | weekend-db::weekend_db | src/storage/linux.rs:215:5– | 7732..7735 | for_iteration | copy_metadata | Copy the shared source attribute to the destination. |
| E | weekend-db::weekend_db | src/storage/linux.rs:221:11– | 8002..8003 | try_propagation | copy_metadata | Refuse the value allocation after collecting both lists and retry to completion. |

## Verification at 355447a

- Rust 1.100.0 passed 272 tests per profile on Docker Linux arm64/ext4
  and 265 per profile on macOS arm64, including both golden histories.
  The frozen source was `355447a27f24a2ee4ba90741059b87c7e54d7444`.
- Both Linux profiles detected all 31 host-tagged controls. Reproduce
  on Linux with `cargo xtask mutations --toolchain 1.100.0 --case r1-linux-primitive-`
  and repeat with `--release`. Four shards per profile ran on ext4 from
  the exact bundled commit.
- The four native Linux primitive tests also passed under UID 1000 in
  both profiles. Strict Clippy passed on Linux, macOS and for Windows
  MSVC. Eleven mutation-runner tests passed on both Unix hosts.
- Linux lists 651 R1 controls and macOS lists 632. Removing each host's
  native-only cases yields the same 620 names in order.
- The inventory maps 24 more runtime records. It reports 10,582
  decisions and 9,489 open records, including 2,120 runtime.
