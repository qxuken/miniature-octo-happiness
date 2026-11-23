# Generated C exports and delivered allocator callbacks

The [C boundary audit](ffi-boundaries-current.md) covers the helper decisions.
This batch exercises decisions in the compiled export bodies, including
their status values, output sentinels, ownership and ABI-visible state.

The validation-mode test seeds two existing records, then starts two
transactions that edit different records from the same revision. The
later transaction commits revision 3. Records mode permits the earlier
transaction to commit revision 4 and both edits are visible. Whole-database
mode returns Conflict, clears the revision output to zero and keeps the
earlier record at its seeded value. The existing invalid-mode fixture
checks rejection and a null transaction output for values outside 0–1.

The packaged C11 and C++17 consumers exercise allocator callbacks through
the public header. They reject invalid limits and unpaired context callbacks
before any allocation or retain. A successful open keeps the copied options
and callbacks after the caller edits the descriptors. The consumer checks
the byte limit, live-byte telemetry, the size and alignment on every free,
and that the context remains retained until the last free. It injects a
refusal at each request observed in a successful open, then tests a refusal
and retry while acquiring a snapshot. A live retained snapshot and its
materialized scan each keep close Busy until release.

## Compiled export controls

The ranges and hashes below bind the invocation or template source.
These are execution and mutation controls for Rust expansion and the
delivered C ABI. The AST inventory keeps the sites as macro blind spots.

| Site | Byte span | SHA-256 prefix | Observed decision or propagation |
| --- | --- | --- | --- |
| `wdb_database_options_init` | 4863..5076 | `1d5f06ffdd24` | Null rejects. Valid storage receives defaults and a null allocator. |
| `wdb_database_get_limits` | 5078..5262 | `d463de8c9957` | Null output rejects. The selected limits survive caller descriptor changes. |
| `wdb_database_get_memory_usage` | 5264..5465 | `831f12516a6a` | Null output rejects. The getter returns the configured limit and a bounded live/peak sample. |
| `wdb_open_with_options` | 5598..5808 | `38e453ed2c82` | Validation errors clear the handle. Every observed allocation failure unwinds the provider and clears the handle. |
| `wdb_retained_ids` | 9007..9464 | `6a03771908da` | Null destination and nonzero capacity independently control Invalid. The short-buffer result is in elements. |
| `wdb_close` | 9466..9680 | `2f01b3ddb12c` | A live handle returns Busy. The quiescent empty count permits close and releases the allocator context after its last free. |
| `wdb_value_make` | 10032..10695 | `4f4dc5904061` | Bytes require zero scalar bits. The invalid path resets the output. |
| `release` | 10869..11116 | `67bc252f4667` | Null inputs skip release. Live value, snapshot, transaction and scan handles release their ownership. |
| `wdb_value_scalar` | 11456..11896 | `dc63205015bd` | The integer arm returns the two’s-complement bits. Existing scalar tests cover the other kind arms. |
| `wdb_tx_begin` | 15361..15680 | `af3e7ca5be95` | Records and whole-database validation produce different outcomes for disjoint edits. Unrecognized modes reject. |
| `wdb_tx_commit` | 16713..16891 | `bfa5a7bc73d6` | Conflict leaves the revision output zero and preserves the prior committed state. |
| `wdb_scan_key` | 17145..17471 | `7d99f4750f87` | A missing index returns Bounds, clears the required count and preserves the destination. |

## Parsed source binding

The tests also reran the parsed status dispatcher on the current source.
The new full-file pin includes the appended validation-mode fixture.

| Source | SHA-256 |
| --- | --- |
| `src/ffi.rs` | `d2d4dc475602efad06c57016f37ed900d67690ef865c932841b8d5208c31e2c9` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/ffi.rs:42:5– | 1604..1609 | match | boundary | Exported calls exercise success, ordinary errors and caught adapter panic in both profiles. |

## Verification

Frozen source `280754c163f458b31525cd979c3c20904c8f5b1d` used Rust 1.100.0.
The library, allocation, allocator-routing, concurrency and file suites
passed 245 tests per profile on arm64 macOS and 255 per profile on
aarch64 Docker Linux/ext4. Clippy passed on both hosts and for the
Windows MSVC cross target. Miri `708c94ef0` passed the validation-mode
test with strict provenance. All ten named mutations failed the
intended test in debug and release on both hosts.

Both private release packages passed C, C++, Odin and the dynamic ABI
checks before and after relocation. Each C and C++ run observed exactly
four open-allocation refusals and the snapshot allocation refusal and
retry. Manifest hashes bind the exact libraries, C consumers and
allocator fixture to the source. The 134-cell optimized trace matched
the independent oracle for Rust, C and Odin on both hosts.

| Host | Delivered shared library SHA-256 | Package manifest SHA-256 |
| --- | --- | --- |
| macOS arm64 | `f5662535bc5cb8ac3c206ebb8b8c1db87041bf79d743706b958c694b4522f859` | `c268ee362716cafbff10be3fbb379f596e74a5b3d1eade2a4b309e756dd1075b` |
| Linux aarch64 | `1833ea46f8ac85c1f1b08536e558fa4b6a025af02f6b4b8ff931ba97f9e32e1b` | `31e415776ba36355992cde5de2b2e0a65b3bbfc83b014ec10157e41f5e3646f1` |

The common oracle transcript SHA-256 is
`0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3`.
The Linux trace uses the local runner inside Docker. Its offline registry
is available under `$HOME/.cargo` because the runner clears `CARGO_HOME`.
The package builds and traces use `crdt,replication` with `panic=unwind`.

Native Windows, file factories with custom allocator options and the
remaining generated export paths retain their own qualification gates.
The source inventory remains at 9,391 open records (2,014 runtime,
26 driver and 7,351 tooling). The compiled checks here cover supplemental
macro obligations.

```sh
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-generated-c- \
  --output target/generated-c-debug
cargo +1.100.0 xtask package --features crdt,replication --odin --rust-source \
  --output target/generated-c-package
cargo +1.100.0 xtask artifact-trace --package target/generated-c-package \
  --odin "$(command -v odin)" --output target/generated-c-trace
```

Repeat the mutation command with `--release` and a separate output
directory.
