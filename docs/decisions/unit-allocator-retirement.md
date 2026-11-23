# Unit allocation counter retirement

Allocation failure tests now use the allocator that owns the requested
block. Engine unit tests configure the test provider. Public Rust output
tests refuse Global requests in their standalone test binary. The engine
budget tracks live and peak bytes, and the fault-injection feature
continues to expose I/O and publication controls.

| Boundary | Observed result |
|---|---|
| Tree map insertion, list replacement and list removal | Each real provider refusal preserves the original roots, budget charge and provider live bytes. The sweep reaches a successful retry. |
| Nested-map encoding scratch | The source row limit and encoder live-byte limit reject before the provider is called. Refusal of an admitted request reports OutOfMemory. Retrying preserves ordered keys and shared value identity. |
| External Vec | Capacity reuse makes zero requests. Count overflow returns Limit and a representable count with excessive byte layout returns OutOfMemory. A legal grow requests storage once and stays outside the live-byte cap. |
| Charged growth | Appending 4,096 bytes takes 13 allocation and growth requests. Rejected or refused growth preserves the original buffer and charge. The rounded request retries at exact size on Limit. A real provider refusal propagates. |
| Shared, Box and raw ownership | Failed requests destroy each input once. Zero-sized boxes make zero provider requests. The last shared owner frees the value once, and raw and boxed ownership return provider usage to baseline. |

External Vec capacity reuse is delegated to `Vec::try_reserve_exact`.
The previous capacity mutant depended on the removed synthetic claim.
Its replacement changes the reported error at the engine's count-overflow
guard, which is checked independently of provider refusal and layout
rejection.

## Verification

At `fdb6cf7`, Rust 1.100.0 passed 421 core and typed tests with two explicit
ignores per profile on macOS arm64. Docker Linux arm64/ext4 passed 205
library, allocation, public Rust output, retention and compaction tests
per profile. Debug and release ran both golden histories. Strict Clippy
passed on macOS and for Windows MSVC. The macOS run with all features
disabled passed 136 library tests. Miri passed 21 memory tests, both the
tree and nested-map regressions, and all five allocator-routing tests.

At `6bd2928`, all 22 selected mutation controls were detected in each
macOS profile. The six reports bind that clean source and account for 186
verified command retirements. The runner's ten mutation tests passed.

```sh
cargo +1.100.0 xtask mutations --toolchain 1.100.0 \
  --case memory- --case live-budget --case r1-alloc- --case r1-encode-value-map-entries
```

Use the same command with `--release` for the optimized profile. The
inventory reconciles 10,582 decisions after removal of eight records.
The 9,899 open records include 2,529 runtime records.
