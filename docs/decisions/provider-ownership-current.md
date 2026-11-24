# Allocator provider ownership and final release

An `AllocatorHandle` with a selected provider owns a reference to the
provider's self-allocated control block. A default handle dispatches to Global
without a provider block. Cloning a selected handle keeps one more reference
without allocating. The final selected handle moves the provider out, frees
the block through that provider and then drops the provider. For C callbacks,
dropping the provider releases its retained context.

## Directed cases

| Operation | Comparison | Result |
| --- | --- | --- |
| Provider bootstrap | Selected provider grants or refuses the first request | Success owns one provider block. Refusal returns `OutOfMemory`, releases the input provider and leaves no blocks. |
| Allocator dispatch | Default handle versus the selected provider with a refusal pending | The default database proceeds without calling the selected provider. The selected database returns `OutOfMemory` at the refusal and can retry. |
| Clone | Default versus a selected handle | The default uses Global. The selected clone shares the one provider block, uses it after the original is dropped and frees it after the last owner. |
| Retain threshold | Count immediately below `isize::MAX` versus at it | The child below the threshold clones successfully. The child at the threshold aborts. Both paths use the existing subprocess fixture and its native termination check. |
| Box and shared allocation | Zero versus nonzero box layout, and nonzero grant versus refusal | A zero-sized box bypasses the selected provider. Nonzero box, raw handle and shared allocation refusals destroy the input once and restore the allocator baseline. |
| Shared drop | First of two owners versus final owner | The first drop leaves the charge held. The final drop clears the charge, including when value destruction unwinds. |

For provider drop, there are three feasible cases. With a default handle the
second test is unevaluated. With a selected handle it distinguishes the final
decrement from earlier decrements.

| Provider present | Final strong reference | Free provider block |
| --- | --- | --- |
| No | Unevaluated | No |
| Yes | No | No |
| Yes | Yes | Yes |

The four-worker test holds one block per worker and drops the original handle
while all workers still have their blocks. It observes the five live blocks
and the still-retained context before the workers are released. After the
workers free their blocks and drop their handles, exactly five frees and one
context release have occurred. The release callback checks that zero blocks
are live at the point of release. This includes the provider block itself.

## Bounded ordering check

The shared ownership model uses the same `retain_ref` and `release_ref`
atomic orderings as both production handle types. It checks the last owner's
visibility of both earlier workers' accesses before joining the threads.
`xtask atomic-model` now runs a fail-required Relaxed-decrement control.
Loom uses three threads, preemption bound two, and a 1,000-branch bound,
without a separate runtime or permutation cutoff. The model uses an independent
Loom `Arc` for storage. Native tests and Miri exercise actual allocations and
raw pointers. The extra model mode adds one tooling inventory record.

## Source binding

| Source | SHA-256 |
| --- | --- |
| `src/memory.rs` | `e6ebf06ad09de067f4ad341c7f517532cd9d8ca0c33fb41cc4575b092c8062cf` |

E records scoped execution. C records related syntax for the same case.
Provider drop uses scoped execution because the last-reference condition is
unevaluated for a default handle. The independent single-condition and
allocation-error cases use named controls in the [manifest](current-scope.json).

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/memory.rs:154:9– | 5493..5498 | match | as_allocator | Default and selected provider dispatch. |
| C | weekend-db::weekend_db | src/memory.rs:164:12– | 5793..5796 | pattern_test | clone | The pattern inside the named clone decision. |
| E | weekend-db::weekend_db | src/memory.rs:173:9– | 5982..5984 | if | drop | Default, nonfinal and final owner paths. The final-free control omits deallocation and the context callback. |
| C | weekend-db::weekend_db | src/memory.rs:173:12– | 5985..5988 | pattern_test | drop | Provider presence inside the enclosing drop decision. |
| C | weekend-db::weekend_db | src/memory.rs:173:12– | 5985..6069 | short_circuit_and | drop | Same three feasible cases as the enclosing drop decision. |

The ownership controls can retain an extra reference or omit cleanup in an
isolated test process. They avoid a premature free through a live pointer.

## Verification

Frozen source `ab467f8eee1d4cdd215e99eccda234e503f38e08` used Rust 1.100.0.
The library, allocation, allocator-routing and concurrency suites passed
with all engine features enabled. Eight new source controls and the existing
threshold control failed their intended tests in each profile.

| Host | Debug tests | Release tests | Debug controls | Release controls |
| --- | ---: | ---: | ---: | ---: |
| macOS arm64 | 241 | 241 | 9/9 | 9/9 |
| Docker Linux aarch64 on ext4 | 251 | 251 | 9/9 | 9/9 |

Both hosts passed the bounded model campaign, including the new Relaxed
decrement control and the existing FFI and budget controls. Miri `708c94ef0`
with strict provenance passed nine allocator fixtures on macOS, including
the concurrent release test. Engine Clippy passed on both hosts and for the
Windows MSVC cross target. Native Windows execution remains open. The source
inventory now records 9,376 open entries (1,993 runtime, 26 driver, and 7,357
tooling). The scoped cases above do not measure compiler MC/DC.

```sh
cargo +1.100.0 xtask mutations --case r1-provider-ownership- \
  --case memory-abort-threshold --toolchain 1.100.0 \
  --output target/r1/provider-ownership-debug
cargo +1.100.0 xtask atomic-model --toolchain 1.100.0
```

Repeat the mutations with `--release` and a separate output path.
