# Allocator source decision audit

This source audit covers the `src/memory.rs` allocator migration on Rust 1.100.
Its 42 AST records include the conditional allocation-claim code for
`fault-injection`. The scope is the library wrapper and its tests. The standard
allocator's implementation and compiler MC/DC remain outside this audit.

P denotes direct predicate controls, C overlapping syntax, E scoped execution or
error propagation, and I an invariant exclusion.
All source records have dispositions below. The [live budget ledger](../memory-budget-decisions.md)
remains the detailed arithmetic, allocation rollback and ownership specification.

## M1: independent configuration bounds

`tests/allocation.rs::allocation_limits_reject_each_independent_invalid_configuration`
uses valid controls and individually invalid depth zero, depth above 64, entries,
writes, observations above one million, and bytes/value/transaction sizes above
isize::MAX. The eight-condition OR has baseline all false and eight independent
single-true cases. The enclosing if and six intermediate OR trees are duplicate
syntax, not additional obligations. Zero/default maximum live budgets are valid
and tested separately; there is no missing max_live_bytes configuration predicate.

## M2: shared allocation, reference count and destruction

`memory::tests::null_system_allocation_drops_inputs_and_allows_retry` injects
`AllocError` before calling `Global`, then permits success. Value destruction
occurs once and subsequent allocation works. `allocation_ownership_transfers_or_drops_exactly_once`
controls failed claims, raw/Box/Shared transfers, two aliases versus last drop,
and ZST box bypass under an exhausted fault budget.
Both kinds of nonzero handle use the same fallible `Global` allocation path.
An initialized allocation becomes a `Box<T>` and the raw FFI path consumes
that box with `Box::into_raw`. Destruction reconstructs the box so deallocation
also runs if a destructor unwinds. Foreign-handle bytes stay outside the live
core budget.
`last_shared_owner_destroys_once_after_cross_thread_accesses` exercises synchronized
final destruction. `shared_clone_aborts_at_the_refcount_threshold_in_a_subprocess`
compares MAX-1 to MAX without corrupting the parent: the child restores real
counts on the successful path and verifies abort separately. No forged pointer
or oversized real allocation is used. The existing bounded Loom refcount model
and its weakened-ordering mutant are delegated concurrency evidence (MEM-V3),
not a proof of every scheduling/platform implementation.

## M3: capacity, layout, claim and admission

`memory::live_budget_tests::charged_reserve_capacity_count_and_layout_decision_pairs`
controls count addition, available capacity, layout validity and rejected growth.
`growth_reserves_both_buffers_and_rolls_back_failures` covers initial and growth
allocator failure, cap equality/one byte short and failed claims. Failed initial
allocation leaves live bytes and capacity at zero. Failed growth preserves the
old bytes and charge. `admission_limit_and_overflow_decision_pairs`
controls checked addition and finite cap equality independently as sequential
guards. `allocation_layout_tests::admission_overflow_rolls_back_exactly` covers
maximum admitted charge plus one, and rolls back synthetic state on drop.
`reservation_checks_capacity_overflow_and_budget_independently` covers external
reserve no-growth/growth, count overflow, Vec byte-layout rejection and the
fault-budget match arms unlimited/zero/positive. External capacity is deliberately
excluded from live-byte accounting; this is not a missing accounting branch.

`aligned_growth_partial_iteration_and_null_shared_cleanup` and
`shared_clones_keep_one_charge_and_unwind_reclaims_allocation` check over-alignment,
partial iteration, normal/unwinding destruction and exact charge lifetimes.
`zero_sized_elements_and_output_transfer` covers ZST storage and verifies that
transfer to an external `Vec` keeps the same address and releases its charge.
`usage_bounds_peak_during_admission_update_window` and
`concurrent_reservations_cannot_exceed_cap` check telemetry/concurrent admission.
`geometric_growth_is_bounded_and_uses_logarithmic_allocation_claims` and
`geometric_growth_exact_fallback_preserves_budget_and_failure_atomicity` cover
overflow, the maximum and spare capacity, rounded versus exact fallback, and
OOM propagation without fallback. The detailed ledger records truth pairs for
the reachable wrapper predicates.

`ChargedVec` prechecks the requested layout and admits the full new buffer
before consuming an allocation claim. `Vec::try_reserve_exact` and the private
adapter to `Global` then own allocation and growth. The old charge remains live
until growth succeeds, including when `Global::grow` can extend in place. The
adapter delegates deallocation with the same layout. Its test-only fault seam
can return an error before either Global call, leaving an existing allocation
owned by the original vector.

## M4: copied bytes and invariants

`copy_external` rejects a slice length above isize::MAX defensively, but a valid
Rust byte slice must already satisfy that bound. The true branch cannot be tested
with a legal input; fabricating such a slice would be UB and is excluded.
Empty/nonempty copy success and allocation rejection are covered by ownership
and public allocation-failure tests. `ChargedVec::extend` is exercised with empty
and populated lists through existing value/tree tests; its loop delegates iterator
implementation. Actual system exhaustion/allocator metadata behavior remains an
allocator contract, independently of the injected error return.

## Macro and generated boundaries

The inventory excludes the derive expansions and four production assertions
for reserved capacity before push, extend and resize, and for the nonzero
handle layout. These guard private caller invariants. ZST boxes bypass the
handle allocator. The `cfg(test)` fault predicate is outside the inventory.
Generated traits, atomics, combinators, standard-library growth and unwind
mechanics are outside the AST denominator. The budget ledger also audits the
checked-add and filter predicates in the admission closure.

## Source records

| Source | SHA-256 |
| --- | --- |
| `src/memory.rs` | `db8fdd3c84557996cf7a66cfbb50abb33a69ed8840aafe72fb849cf0fccefe3a` |

B marks partial evidence for the initialization wrapper. Tests exercise the
underlying shared allocator's failure and the initialization success path.

| Disposition | Location | Bytes | Kind | Owner | Evidence | Token hash |
| --- | --- | --- | --- | --- | --- | --- |
| E | `src/memory.rs:34:33–34:34` | `987..988` | `try_propagation` | `allocate` | M2 | `8a8de823d5ed` |
| E | `src/memory.rs:48:33–48:34` | `1348..1349` | `try_propagation` | `grow` | M3 | `8a8de823d5ed` |
| C | `src/memory.rs:93:9–93:11` | `2805..2807` | `if` | `validate` | M1 | `935f68319d4f` |
| C | `src/memory.rs:93:12–94:35` | `2808..2862` | `short_circuit_or` | `validate` | M1 | `142fafc1b97a` |
| C | `src/memory.rs:93:12–95:44` | `2808..2906` | `short_circuit_or` | `validate` | M1 | `eba9b457ca90` |
| C | `src/memory.rs:93:12–96:43` | `2808..2949` | `short_circuit_or` | `validate` | M1 | `081a4c0c0476` |
| C | `src/memory.rs:93:12–97:49` | `2808..2998` | `short_circuit_or` | `validate` | M1 | `63c86f5c7fa5` |
| C | `src/memory.rs:93:12–98:52` | `2808..3050` | `short_circuit_or` | `validate` | M1 | `d65ce18fed56` |
| C | `src/memory.rs:93:12–99:58` | `2808..3108` | `short_circuit_or` | `validate` | M1 | `65f7071c1b02` |
| P | `src/memory.rs:93:12–100:64` | `2808..3172` | `short_circuit_or` | `validate` | M1 | `0d3ce3c67265` |
| E | `src/memory.rs:129:45–129:46` | `4168..4169` | `try_propagation` | `allocate` | M2 | `8a8de823d5ed` |
| P | `src/memory.rs:158:9–158:11` | `5145..5147` | `if` | `clone` | M2 | `935f68319d4f` |
| P | `src/memory.rs:180:9–180:11` | `5873..5875` | `if` | `drop` | M2 | `935f68319d4f` |
| E | `src/memory.rs:230:75–230:76` | `7463..7464` | `try_propagation` | `reserve` | M3 | `8a8de823d5ed` |
| P | `src/memory.rs:231:9–231:11` | `7474..7476` | `if` | `reserve` | M3 | `935f68319d4f` |
| E | `src/memory.rs:234:74–234:75` | `7619..7620` | `try_propagation` | `reserve` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:235:46–235:47` | `7667..7668` | `try_propagation` | `reserve` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:236:20–236:21` | `7689..7690` | `try_propagation` | `reserve` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:239:45–239:46` | `7791..7792` | `try_propagation` | `reserve` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:247:75–247:76` | `8295..8296` | `try_propagation` | `reserve_geometric` | M3 | `8a8de823d5ed` |
| P | `src/memory.rs:248:9–248:11` | `8306..8308` | `if` | `reserve_geometric` | M3 | `935f68319d4f` |
| P | `src/memory.rs:251:9–251:11` | `8384..8386` | `if` | `reserve_geometric` | M3 | `935f68319d4f` |
| E | `src/memory.rs:258:9–258:14` | `8594..8599` | `match` | `reserve_geometric` | M3 | `4945a70fa7f9` |
| P | `src/memory.rs:259:34–259:52` | `8680..8698` | `match_guard` | `reserve_geometric` | M3 | `748e6158ff2c` |
| E | `src/memory.rs:271:9–271:12` | `9085..9088` | `for_iteration` | `extend` | M4 | `10c22bcf4c76` |
| E | `src/memory.rs:367:42–367:43` | `11897..11898` | `try_propagation` | `new` | M1 | `8a8de823d5ed` |
| B | `src/memory.rs:374:10–374:11` | `12148..12149` | `try_propagation` | `new` | M3 | `8a8de823d5ed` |
| P | `src/memory.rs:387:17–387:22` | `12546..12551` | `match` | `claim` | M3 | `4945a70fa7f9` |
| E | `src/memory.rs:393:45–393:46` | `12758..12759` | `try_propagation` | `claim` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:403:21–403:22` | `13085..13086` | `try_propagation` | `shared` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:404:70–404:71` | `13157..13158` | `try_propagation` | `shared` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:408:70–408:71` | `13374..13375` | `try_propagation` | `reserve_external` | M3 | `8a8de823d5ed` |
| P | `src/memory.rs:409:9–409:11` | `13385..13387` | `if` | `reserve_external` | M3 | `935f68319d4f` |
| E | `src/memory.rs:410:25–410:26` | `13438..13439` | `try_propagation` | `reserve_external` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:412:49–412:50` | `13530..13531` | `try_propagation` | `reserve_external` | M3 | `8a8de823d5ed` |
| I | `src/memory.rs:417:9–417:11` | `13646..13648` | `if` | `copy_external` | M4 | `935f68319d4f` |
| E | `src/memory.rs:421:53–421:54` | `13819..13820` | `try_propagation` | `copy_external` | M4 | `8a8de823d5ed` |
| E | `src/memory.rs:433:39–433:40` | `14228..14229` | `try_propagation` | `admit` | M3 | `8a8de823d5ed` |
| E | `src/memory.rs:471:44–471:45` | `15587..15588` | `try_propagation` | `copy` | M4 | `8a8de823d5ed` |
| P | `src/memory.rs:477:9–477:11` | `15794..15796` | `if` | `boxed` | M3 | `935f68319d4f` |
| E | `src/memory.rs:486:21–486:22` | `16127..16128` | `try_propagation` | `allocate_handle` | M2 | `8a8de823d5ed` |
| E | `src/memory.rs:491:45–491:46` | `16314..16315` | `try_propagation` | `allocate_handle` | M2 | `8a8de823d5ed` |

Totals B=1, C=7, E=23, I=1, P=10 across 42 records.

## Rust 1.100 qualification

All runs used the pinned `src/memory.rs` above. The native debug and release
runs enabled all features. The Linux source, build and scratch files were on
the Docker VM's ext4 volume.

| Platform | Library tests per profile | Allocation, budget and retention integration tests per profile |
| --- | ---: | ---: |
| macOS arm64 | 159 passed | 19 passed |
| Docker Linux arm64 | 165 passed | 19 passed |

The library runs include the COMMIT and retained-catalog encoder cases.
The DATA and VALUE-identity fixtures emitted seven and ten images respectively.
All 17 matched the `459e4c4` baseline byte for byte on both platforms and in
both profiles. The common sorted SHA-256 manifest has digest
`fea361f3bb2f84c4543030d16fa16ab7aaff99f783a9a48ed5b2f9b09121f009`.

Miri `708c94ef09` passed 15 memory tests and four FFI decision tests on macOS.
The abort subprocess case runs only under the native harness. Strict Clippy
passed for all core crate targets on both native platforms and when checking
the `x86_64-pc-windows-msvc` target from macOS.

The growth instrumentation counts the previous and new charges at the
replacement statement. The focused growth test emitted the same counters
from disposable builds of `459e4c4` and the allocator implementation.
