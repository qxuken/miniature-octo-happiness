# Foreign allocator descriptor and layout decisions

The selected allocator validates its callbacks before taking ownership of the
context. `allocate` and `deallocate` are required. The context callbacks are
either both present or both absent. The application keeps a borrowed context
alive for all calls. A retained context is released after the last free.

## Decision pairs

| Descriptor | Allocate absent | Deallocate absent | Context callbacks mismatched | Outcome |
| --- | --- | --- | --- | --- |
| All callbacks | False | False | False | Accept and retain once |
| Without allocate | True | False | False | Invalid before retain or allocation |
| Without deallocate | False | True | False | Invalid before retain or allocation |
| Without retain | False | False | True | Invalid before retain or allocation |
| Without release | False | False | True | Invalid before retain or allocation |
| Without either context callback | False | False | False | Accept without context callbacks |

The table lists each input's logical values. Rust short-circuits later
operands once the rejection result is known. The first three rows also
cover the nested OR for the two essential callbacks. The final row pairs
with the first for both the retain and release pattern tests. The fixture
observes context counts before and after dropping the adapter.

| Layout | Allocate | Deallocate | Observation |
| --- | --- | --- | --- |
| Size 0, alignment 64 | Bypasses the callback | Bypasses the callback | Aligned handle of length zero. The pending refusal is untouched. |
| Size 3, alignment 64 | Calls provider | Calls provider for the successful retry | First request returns `AllocError`. Retry returns the requested length and alignment. The free reduces live blocks to zero before context release. |

The eight named `r1-foreign-allocator-` controls independently omit the
essential callback checks, accept each unpaired context descriptor, omit
retain or release, and send the over-aligned zero-byte allocation or free
to the callback. The two zero-byte controls use an observer that can count
the unexpected call without freeing a pointer it did not allocate.

## Source binding and verification

| Source | SHA-256 |
| --- | --- |
| `src/memory.rs` | `e6ebf06ad09de067f4ad341c7f517532cd9d8ca0c33fb41cc4575b092c8062cf` |

The [inventory manifest](current-scope.json) binds eight source records
to these tests and controls. The full OR expression shares the enclosing
descriptor decision's evidence. The retain and release pattern records
share their respective callback tests.

The frozen test source `9822b4c8a9385cc2ea70e7ca30e3bfc1201e83fb` passed on
Rust 1.100.0 with all engine features enabled. Both profiles ran the library,
allocation, allocator-routing and concurrency suites. On each host and profile,
the two unmodified baseline tests passed and all eight controls failed the
intended test.

| Host | Debug tests | Release tests | Debug controls | Release controls |
| --- | ---: | ---: | ---: | ---: |
| macOS arm64 | 240 | 240 | 8/8 | 8/8 |
| Docker Linux aarch64 on ext4 | 250 | 250 | 8/8 | 8/8 |

Clippy passed on both hosts and for the `x86_64-pc-windows-msvc` cross target.
Miri `708c94ef0` with strict provenance passed all eight portable allocator
fixtures on macOS, including the existing cross-thread provider and retained
value lifetime tests. Native Windows execution remains open. The inventory
contains 9,387 open records (2,005 runtime, 26 driver, 7,356 tooling).

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db \
  --lib memory::test_allocator::foreign_
cargo +1.100.0 xtask mutations --case r1-foreign-allocator- \
  --toolchain 1.100.0 --output target/r1/foreign-allocator-debug
```

Repeat the tests and controls with `--release` and a separate output path.
The provider's internal allocation and the shared-owner refcount schedules
retain their own qualification scope. The earlier
[in-memory](ffi-generated-exports-current.md) and
[file](ffi-file-factories-current.md) checks cover the delivered C ABI.
