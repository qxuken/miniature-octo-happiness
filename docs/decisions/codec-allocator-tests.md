# Codec allocation refusal tests

Fifteen codec tests now exercise the selected allocator through
`TestAllocator`. Each failure sweep allows a fixed number of real
allocation or growth requests and refuses the rest during the operation.
The simulator uses an independent allocator during recovery and creation.

| Boundary | Observed result |
|---|---|
| Borrowed VALUE and DATA caches | Reuse preserves identity and issues zero provider requests. Each fork releases partial allocations and leaves the original index unchanged. |
| Writer, indexes and object boundaries | Spare capacity and rejected bounds issue zero requests. Refused growth preserves the written prefix and allocation charge. |
| VALUE, DATA and COMMIT candidates | Every refusal reports OutOfMemory and reclaims the charge. A successful attempt preserves cache references, record order and retained descriptors. |
| Recovery | Refusals preserve the durable image. The diagnostic keeps the allocation error and selected COMMIT descriptor offset. Successful recovery shares the same decoded VALUE nodes. |
| Fresh image creation | Refused staging leaves the simulator image empty. A successful retry matches contiguous encoding and the expected write boundaries. |

## Verification

Rust 1.100.0 passed the library suite on macOS arm64 (168 tests per
profile) and Docker Linux arm64 on ext4 (175 tests per profile) at
`937d235` in debug and release. Both golden histories ran in every
profile. Strict Clippy passed on macOS and for the Windows MSVC target.
Miri passed the cache fork and rollback test at `ba0acf8`.

The macOS mutation runner rebuilt at `ba0acf8` detected all 17 COMMIT
controls in both profiles. Debug also detected nine controls for graph
allocation, error propagation, writer and key growth, VALUE cache fork,
DATA index reserve and segmented append. The eight reports verify 190
command retirements on that clean source.

The production portion of `src/storage/codec.rs` matches the parent
byte for byte. The current VALUE encoding ledger binds the new file
digest. All 10,590 inventoried decisions and 4,978 blind spots retain
their source spans and classifications.
