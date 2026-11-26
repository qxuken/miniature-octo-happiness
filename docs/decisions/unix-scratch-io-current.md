# Unix compaction scratch and positioned I/O

The scratch name uses one fallible reservation through the location's
allocator. It holds the prefix, 32 lowercase hex digits, and the
terminated suffix. Allocation precedes the entropy request and directory
duplication. Failure to reserve or obtain entropy frees temporary storage.
The name stays relative to the held parent directory descriptor.

| Check | Controlled cases | Result |
| --- | --- | --- |
| Scratch reservation | First request refused or allowed | Refusal returns OutOfMemory before entropy. One reservation fits the complete name. |
| Entropy callback | Io refusal or a deterministic 16-byte result | Preserve the refusal and free the allocation, or encode exactly two hex digits per byte |
| Exclusive creation | Absent name or occupied name | Create the candidate or return AlreadyExists with Create and EEXIST. Keep the existing candidate bytes after refusal. |
| Cleanup unlink | Candidate present or absent | Remove the candidate alone, or return NotFound with Remove and ENOENT |
| Positioned read | Offset within file, offset at EOF, unrepresentable offset | Return only the available bytes, return zero, or report Read and EINVAL. Bytes outside the reported prefix stay intact. |
| Positioned write | Claimed read-write descriptor or claimed read-only descriptor | Apply the write at the requested offset, or report Write and EBADF and keep the bytes |

The scratch fixture exercises production name preparation with a
deterministic identity callback. Creation, collision and unlink use the
native descriptor-relative calls. The positioned I/O fixture uses the
public FileStore API on a real file. The compaction suite also checks
canonical and retained state at each existing publication observation.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `a671d59cc6d2ca075ad49084901aa1d39a1c45532c4cc5a9c7c077eab3176c4f` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:333:10– | 11363..11364 | try_propagation | scratch | Propagate one selected allocator refusal before calling entropy. |
| D | weekend-db::weekend_db | src/storage/file.rs:334:9– | 11374..11377 | for_iteration | scratch | Encode exactly 16 bytes in order. The successful iterator has a fixed nonzero cardinality. |
| E | weekend-db::weekend_db | src/storage/file.rs:334:36– | 11401..11402 | try_propagation | scratch | Propagate a terminal identity failure and release the completed reservation. |
| D | weekend-db::weekend_db | src/storage/file.rs:335:13– | 11417..11420 | for_iteration | scratch | Use both nibbles in order and encode them as lowercase hex. |
| P | weekend-db::weekend_db | src/storage/file.rs:352:9– | 11981..11983 | if | remove | Preserve unlink success and an absent-leaf native error independently. |

## Verification

The frozen test source was `d31aba8a9c4cf7ade8b4c77b9b752cc00aedd354`
with Rust 1.100.0. The engine and storage suites passed 277 tests per
profile on macOS arm64 and 288 per profile on Docker Linux aarch64/ext4.
Both image fixtures matched their golden bytes in each run. The xtask
mutation contracts passed 18 tests and strict Clippy passed on both
hosts.

Each host detected all 12 controls in both profiles, in two disjoint
shards. The controls that change the positioned I/O offsets, EOF count
or native operation labels exercise data and error propagation through
the native adapter. They do not bind new Boolean decision records to
those delegated calls.

The inventory records five new source dispositions and rebinds 70
prior rows. The two scratch reservations become one. The queue has
9,447 open records, including 2,078 runtime, 26 driver and 7,343 tooling.
Descriptor exhaustion, metadata failures at the remaining call sites,
unsupported Linux architecture admission and deployed-device behavior
remain open.

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --lib \
  storage::file::decision_tests
cargo +1.100.0 xtask mutations --case r1-unix-scratch-io- --toolchain 1.100.0 \
  --output target/unix-scratch-io-debug
cargo +1.100.0 xtask mutations --case r1-unix-scratch-io- --toolchain 1.100.0 \
  --release --output target/unix-scratch-io-release
```
