# C input, output and options boundaries

This ledger extends the [handle accounting audit](ffi-handle-accounting-current.md).
It exercises the live C entry points and uses direct helper tests where a
public caller already validates the owner.

## Directed controls

| Decision | Cases and observations |
| --- | --- |
| Export status | A successful commit returns its revision. An ordinary error returns its ABI code. A panicking adapter returns Panic, clears the revision and requires recovery. Reopening sees the previous committed revision. |
| Output and ownership | A missing output returns Invalid. A valid output is cleared before a later rejection. A foreign snapshot returns ForeignStore with ID zero. The same owner can retain it. |
| Input span | An empty span can have a null address. One byte and exactly the configured two-byte cap round trip. Three bytes return Limit and leave a null output. A null address with a nonzero in-limit length returns Invalid. |
| Byte output | A missing required-size pointer returns Invalid. The package-version export reaches the shared size guard directly. Empty, singleton and three-byte values report their actual sizes. Short buffers stay untouched. Exact and larger buffers receive the bytes and keep their unwritten suffix. |
| Database options | Null options select defaults. A null allocator uses Global and preserves supplied limits. Invalid limits return before a provider allocation. Invalid callback descriptors and provider allocation refusal preserve their respective statuses and clean up the output and context. |
| Native pathname | A valid one-byte name and the exact 1 MiB cap are admitted by the conversion helper. Null, empty and embedded-NUL spans return Invalid. One byte above the cap returns Limit before a read. |

For the buffer guard, A is a null destination and B is nonzero capacity.
The pair `(F,T)/(T,T)` changes A and the pair `(T,F)/(T,T)` changes B.
The tests observe Invalid only at `(T,T)` for this guard. Capacity can
then fail separately with Buffer. For the path guard, A is a null address
and B is zero length. The valid name gives `(F,F)` and comparing it with
`(T,F)` and `(F,T)` independently exercises both rejections. The
short-circuit syntax records share the corresponding `if` controls.

The Windows UTF-8 propagation case is covered by a Windows-only test
assertion for malformed bytes. Native execution of this candidate is
pending. Macro expansion, per-export propagation and exact delivered
artifacts remain supplemental obligations.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/ffi.rs` | `01efa471657057d8909ea82df1b78458ad3a44c4c6b920abd3fcb990ccc22daf` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/ffi.rs:42:5– | 1604..1609 | match | boundary | Success, ordinary error, and caught adapter panic reach the three ABI status arms. |
| P | weekend-db::weekend_db | src/ffi.rs:58:5– | 2185..2187 | if | output | Null rejects. A valid output is reset before an ordinary failure or successful handle publication. |
| P | weekend-db::weekend_db | src/ffi.rs:67:5– | 2409..2411 | if | input | Exactly the configured maximum is admitted. One byte more returns Limit. |
| E | weekend-db::weekend_db | src/ffi.rs:67:32– | 2436..2437 | try_propagation | input | A missing owner stops before constructing a slice. The live-owner inputs proceed. |
| P | weekend-db::weekend_db | src/ffi.rs:70:5– | 2508..2510 | if | input | Zero length admits the empty span. A one-byte value preserves its payload. |
| P | weekend-db::weekend_db | src/ffi.rs:73:5– | 2556..2558 | if | input | At positive in-limit length, a null pointer rejects and valid bytes round trip. |
| E | weekend-db::weekend_db | src/ffi.rs:79:32– | 2805..2806 | try_propagation | make_handle | A missing owner returns Invalid before reserving or publishing. A live owner acquires a handle. |
| P | weekend-db::weekend_db | src/ffi.rs:91:5– | 3186..3188 | if | same | Only matching owners retain a snapshot and publish its ID. |
| P | weekend-db::weekend_db | src/ffi.rs:103:5– | 3397..3399 | if | copy_bytes | A missing size output rejects before a destination write. A valid pointer receives the length. |
| P | weekend-db::weekend_db | src/ffi.rs:109:5– | 3519..3521 | if | copy_bytes | Null and nonzero capacity each independently affect the Invalid guard. |
| C | weekend-db::weekend_db | src/ffi.rs:109:8– | 3522..3555 | short_circuit_and | copy_bytes | Covered by the byte-destination guard pair. |
| P | weekend-db::weekend_db | src/ffi.rs:112:5– | 3604..3606 | if | copy_bytes | Too little capacity preserves the destination. Exact capacity copies and reports the correct size. |
| P | weekend-db::weekend_db | src/ffi.rs:115:5– | 3677..3679 | if | copy_bytes | Empty output performs no copy. A singleton overwrites exactly one byte. |
| P | weekend-db::weekend_db | src/ffi.rs:145:5– | 4454..4457 | let_else | database_options | Null options select defaults and present options preserve their supplied limits. |
| E | weekend-db::weekend_db | src/ffi.rs:148:62– | 4618..4619 | try_propagation | database_options | Invalid limits return before the provider allocates. Valid limits allow provider construction. |
| P | weekend-db::weekend_db | src/ffi.rs:149:5– | 4626..4629 | let_else | database_options | A null allocator preserves custom limits. A present valid callback descriptor routes allocations. |
| E | weekend-db::weekend_db | src/ffi.rs:152:62– | 4787..4788 | try_propagation | database_options | Invalid callbacks leave the output null and the context untouched. Valid callbacks are retained. |
| E | weekend-db::weekend_db | src/ffi.rs:153:65– | 4856..4857 | try_propagation | database_options | Provider allocation refusal returns OutOfMemory and cleans up its context. Successful allocation keeps the context alive. |
| P | weekend-db::weekend_db | src/ffi.rs:184:5– | 5959..5961 | if | path | Null and zero length each independently reject before reading a span. |
| C | weekend-db::weekend_db | src/ffi.rs:184:8– | 5962..5989 | short_circuit_or | path | Covered by the pathname guard pair. |
| P | weekend-db::weekend_db | src/ffi.rs:187:5– | 6038..6040 | if | path | The exact path cap passes. The next length returns Limit. |
| P | weekend-db::weekend_db | src/ffi.rs:191:5– | 6167..6169 | if | path | An embedded NUL rejects while a valid nonempty name reaches platform conversion. |
| G | weekend-db::weekend_db | src/ffi.rs:195:70– | 6322..6323 | try_propagation | path | Windows UTF-8 error propagation awaits a native run. |

## Verification

Frozen source `9f5d81ffb7ec5d4e2bcf0cc24a6d9ead1ca8f6c7` used Rust 1.100.0.
The library, allocation, allocator-routing, concurrency and file suites
passed 244 tests per profile on arm64 macOS and 254 per profile on
aarch64 Docker Linux/ext4. The dynamic C ABI smoke consumer passed on
the debug and release images. Strict Clippy and all 18 mutation-runner
contract tests passed on both hosts. Six focused tests passed under
macOS Miri `708c94ef0` with strict provenance. Windows MSVC cross-target
Clippy passed.

All 23 named source controls reached the intended test failure in debug
and release on both hosts. The baselines passed. The release reports
combine two disjoint shards per host, with source and test identity
checked for every outcome.

The arm64 macOS and aarch64 Docker Linux/ext4 private release packages
verified C, C++, dynamic ABI and Odin consumers before and after
relocation. Both manifests identify the frozen source, runtime feature
mask `3` for CRDT and replication, and `panic=unwind`. The manifest hashes
match the delivered libraries and consumers.

| Host | Delivered shared library SHA-256 | Odin |
| --- | --- | --- |
| macOS arm64 | `2e4fdd045798e626c91f1f68795df2f375cfcb7e7f2a751fcb8530676d8dd1c0` | `dev-2026-10:a22aa1984` |
| Linux aarch64 | `bc25f295b1182e822beaaebc9ddbe601f9884427b2bf6a63f5dfe0821a6c349d` | `dev-2026-10-nightly:84bc3fc` |

Native Windows and other delivered package profiles retain their
qualification gates.

The inventory records 22 resolved runtime gaps in this tranche. The
only remaining parsed decision in `src/ffi.rs` is the Windows UTF-8
error. The repository queue has 9,391 open records (2,014 runtime,
26 driver and 7,351 tooling).

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features \
  --lib --test allocation --test allocator_routing --test concurrency --test files
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-ffi-boundary- \
  --output target/ffi-boundary-debug
cargo +1.100.0 xtask package --features crdt,replication --odin \
  --output target/ffi-boundary-private-package
```

Run the tests and mutation campaign with `--release` and a separate
mutation output directory for the optimized evidence. Build the xtask
from the candidate checkout so the runner uses that checkout's
compile-time manifest directory.
