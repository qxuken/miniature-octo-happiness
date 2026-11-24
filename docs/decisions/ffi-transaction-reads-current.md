# C transaction and snapshot reads

The exported transaction reads see the draft. Snapshot reads remain on the
captured revision. The test seeds one row with value 17 at revision 2, captures
a snapshot, deletes that row and commits revision 3. The current snapshot has
no row. The captured snapshot still has the row and can materialize it in a
scan. The scan remains usable after the snapshot is released, and a value
returned from the scan remains usable after the scan is released.

## Export outcomes

| Input | Result |
| --- | --- |
| Existing row at the exact configured byte limit | The transaction and captured snapshot return an owned value. |
| Absent row or collection | Lookup returns `NotFound` and a null handle. Scanning or deleting from an absent collection propagates `NotFound`. |
| Key one byte over the limit | Lookup and delete return `Limit`. The lookup output is null, and the transaction can still read and delete the original row. |
| Draft deletion | Draft lookup returns `NotFound` and the draft scan succeeds with zero rows. The committed deletion advances to revision 3. Calling a read or scan on the closed transaction returns `Closed` with a null handle. |
| Allocation refusal during captured scan | `OutOfMemory` and a null scan handle. Selected-allocator bytes return to the earlier count and retry returns the captured row. |
| Scan index past the last row | `Bounds` and a null value handle. |
| Allocation refusal while acquiring a scan value | `OutOfMemory` and a null handle. Bytes and handle count return to the earlier counts. Retry produces an owned value. |
| Close attempt with only the returned value still live | `Busy`. Releasing the value permits close and returns the selected allocator's live block count to zero. |

The C and C++ consumers also check failed delete, missing-row and scan
statuses, the captured snapshot, and a scan that survives release of its
transaction. One past the last row they observe `Bounds` and a null handle,
then retrieve the original row from that scan.

## Source binding

The inventory identifies these six `export!` invocations as macro blind spots.
The ten named `r1-ffi-transaction-reads-` controls compile the bodies and check
status mapping, the delete operation and allocation refusal at the ABI.

| Export | Byte span | Token hash prefix | Controls |
| --- | --- | --- | --- |
| `wdb_snapshot_get` | 14740..15109 | `d29870db1ed5` | Missing row and inner lookup error |
| `wdb_snapshot_scan` | 15111..15359 | `0f28c26bb59e` | Propagate scan error |
| `wdb_tx_get` | 15682..15989 | `d770a7000bd8` | Missing row and inner lookup error |
| `wdb_tx_delete` | 16281..16481 | `9c1eaaf2d973` | Propagate delete error and install the draft deletion |
| `wdb_tx_scan` | 16483..16711 | `7650528d2450` | Propagate scan error |
| `wdb_scan_value` | 17473..17711 | `04a48507c20a` | Index bounds and handle-allocation error |

The status boundary is still exercised by the earlier success, ordinary-error
and caught adapter-panic cases. The file hash includes the new test. The parsed
status dispatcher's location and production body are unchanged.

| Source | SHA-256 |
| --- | --- |
| `src/ffi.rs` | `badceb85abf4de706b52d6ee8d03836712055e8e5a3b93ca8b7cca2fbf963e29` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/ffi.rs:42:5– | 1604..1609 | match | boundary | Successful reads, lookup errors and the prior adapter panic fixture. |

## Verification

Frozen source `e34a9c264c4e161f4d88d1a864edc48d1f6c0ec6` used Rust 1.100.0.
The library, allocation, allocator-routing and concurrency suites passed in
debug and release with all engine features enabled. All ten controls failed
the intended tests in both profiles on both hosts. Strict-provenance Miri
`708c94ef0` passed the new fixture on macOS. Clippy passed on both hosts and
for the Windows MSVC cross target.

| Host | Debug tests | Release tests |
| --- | ---: | ---: |
| macOS arm64 | 242 | 242 |
| Docker Linux aarch64 on ext4 | 252 | 252 |

The release packages use feature mask `3` and `panic=unwind`. The C11 and
C++17 consumers ran before and after relocation. The ABI probe verified the
source identity of each loaded shared library. The package manifests bind
the library, both consumers and the C input to the same clean source tree.

| Host | Shared library SHA-256 | Package manifest SHA-256 |
| --- | --- | --- |
| macOS arm64 | `7ba6304cf811af6daeab8ff7bda5e3fa646a5fcead2e296394a0f26bcb7a4352` | `5b528ea83f58e9e83a785b611f613a0f2cf7e4fb006aee2fe68e2932c7c94741` |
| Linux aarch64 | `4745475d4c2197e8d5e6cf6e293fe01b53da45dd391e1b3bf3ad6bb1c638ede3` | `ead3f611836f0f31a2c36542a39ce7af9f05dab33ae4b6daeb3a836d20d8f690` |

Native Windows and the remaining generated export paths retain their own
qualification gates. The supplemental macro evidence adds no parsed source
decisions and leaves the inventory at 9,376 open records.

```sh
cargo +1.100.0 xtask mutations --case r1-ffi-transaction-reads- \
  --toolchain 1.100.0 --output target/r1/ffi-transaction-reads-debug
cargo +1.100.0 xtask package --features crdt,replication \
  --output target/r1/ffi-transaction-reads-package
```

Repeat the mutations with `--release` and a separate output path.
