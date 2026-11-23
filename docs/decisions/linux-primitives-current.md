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
| `src/storage/linux.rs` | `512931ec540b580a4cf83706878f73aeae4358e74fefd1e0b15fd74a3fcf0220` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/linux.rs:61:5– | 2484..2489 | match | os_error | Exercise each mapped errno, the default raw error and an error with no raw value. |
| P | weekend-db::weekend_db | src/storage/linux.rs:76:9– | 3147..3149 | if | acquire_open_lock | A real competing description independently fails while the initial and later acquisitions succeed. |
| E | weekend-db::weekend_db | src/storage/linux.rs:100:5– | 3885..3888 | for_iteration | identity_with | One or three complete identities are collected. Three zeros exhaust the attempt budget. |
| P | weekend-db::weekend_db | src/storage/linux.rs:103:9– | 3967..3972 | while | identity_with | Visit until the exact buffer fills, including a final one-byte suffix. |
| P | weekend-db::weekend_db | src/storage/linux.rs:106:17– | 4116..4118 | if | identity_with | Consult errno only on the negative callback and preserve the operation and code. |
| E | weekend-db::weekend_db | src/storage/linux.rs:112:15– | 4333..4334 | try_propagation | identity_with | Retry an interruption and propagate the terminal error on a distinct case. |
| P | weekend-db::weekend_db | src/storage/linux.rs:113:13– | 4348..4350 | if | identity_with | Both zero and excessive progress independently reject. An exact positive count advances. |
| C | weekend-db::weekend_db | src/storage/linux.rs:113:16– | 4351..4382 | short_circuit_or | identity_with | Same measured predicates and source range as the enclosing if. |
| P | weekend-db::weekend_db | src/storage/linux.rs:118:9– | 4481..4483 | if | identity_with | A complete nonzero identity returns. A complete zero identity consumes another attempt. |
| P | weekend-db::weekend_db | src/storage/linux.rs:127:5– | 4825..4827 | if | exchange | Swap both inodes or reject ENOENT while preserving the one existing name. |
| P | weekend-db::weekend_db | src/storage/linux.rs:146:5– | 5227..5229 | if | status | A zero status succeeds and a nonzero status captures native errno. |
| P | weekend-db::weekend_db | src/storage/linux.rs:159:13– | 5580..5582 | if | blob | A negative query or fill captures errno. Nonnegative progress retains its size. |
| E | weekend-db::weekend_db | src/storage/linux.rs:166:45– | 5767..5768 | try_propagation | blob | A terminal query refusal stops before allocation or fill. |
| P | weekend-db::weekend_db | src/storage/linux.rs:169:5– | 5932..5934 | if | blob | 64 KiB reaches allocation. One byte larger rejects first. |
| E | weekend-db::weekend_db | src/storage/linux.rs:173:43– | 6082..6083 | try_propagation | blob | Refuse the exact requested capacity and preserve live allocator bytes. |
| E | weekend-db::weekend_db | src/storage/linux.rs:175:50– | 6161..6162 | try_propagation | blob | Preserve terminal fill refusal after a successful query. |
| P | weekend-db::weekend_db | src/storage/linux.rs:176:5– | 6168..6170 | if | blob | Accept exactly the queried byte count and reject both smaller and larger fills. |
| E | weekend-db::weekend_db | src/storage/linux.rs:209:7– | 7176..7177 | try_propagation | names | Preserve the selected allocator refusal during real xattr name collection. |
| E | weekend-db::weekend_db | src/storage/linux.rs:255:40– | 8690..8691 | try_propagation | copy_metadata_with | Refuse the source-list allocation first and retry successfully. |
| E | weekend-db::weekend_db | src/storage/linux.rs:256:38– | 8730..8731 | try_propagation | copy_metadata_with | Refuse the destination-list allocation after collecting the source names. |
| E | weekend-db::weekend_db | src/storage/linux.rs:272:5– | 9166..9169 | for_iteration | copy_xattrs | Visit the destination attribute names, including the stale attribute. |
| P | weekend-db::weekend_db | src/storage/linux.rs:273:9– | 9208..9210 | if | copy_xattrs | The matching name continues to value copy. The destination-only attribute is removed. |
| E | weekend-db::weekend_db | src/storage/linux.rs:279:5– | 9433..9436 | for_iteration | copy_xattrs | Copy the shared source attribute to the destination. |
| E | weekend-db::weekend_db | src/storage/linux.rs:282:11– | 9600..9601 | try_propagation | copy_xattrs | Refuse the value allocation after collecting both lists and retry to completion. |

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
