# Allocator source decision audit

The inventory contains 37 production AST records in `src/memory.rs`: 20 `?`,
eight ifs, seven overlapping ORs, one match and one loop. This is a source audit,
not a compiler MC/DC percentage or a claim that allocator internals are covered.
No production changes or duplicate allocator tests were needed for this tranche.

P denotes direct predicate controls; C overlapping syntax; E execution/error
propagation evidence, not every transitive callee outcome; I an invariant exclusion.
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

`memory::tests::null_system_allocation_drops_inputs_and_allows_retry` forces the
thread-local raw allocator result to null, then permits success; value destruction
occurs once and subsequent allocation works. `allocation_ownership_transfers_or_drops_exactly_once`
controls failed claims, raw/Box/Shared transfers, two aliases versus last drop,
and ZST box bypass under an exhausted fault budget.
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
`growth_reserves_both_buffers_and_rolls_back_failures` covers empty allocation,
nonempty realloc, cap equality/one byte short, failed claim and null realloc;
old bytes and live charge survive failure. `admission_limit_and_overflow_decision_pairs`
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
`zero_sized_elements_and_output_transfer` covers ZST storage and explicit transfer
into external ownership. `usage_bounds_peak_during_admission_update_window` and
`concurrent_reservations_cannot_exceed_cap` check telemetry/concurrent admission.
The detailed ledger records exact truth pairs and separates synthetic arithmetic
states from physically realizable public cases.

## M4: copied bytes and invariants

`copy_external` rejects a slice length above isize::MAX defensively, but a valid
Rust byte slice must already satisfy that bound. The true branch cannot be tested
with a legal input; fabricating such a slice would be UB and is excluded.
Empty/nonempty copy success and allocation rejection are covered by ownership
and public allocation-failure tests. `ChargedVec::extend` is exercised with empty
and populated lists through existing value/tree tests; its loop delegates iterator
implementation. Actual system exhaustion/allocator metadata behavior remains an
allocator contract, independently of the injected null return.

## Macro and generated boundaries

The inventory excludes three derive expansions (`MemoryUsage`, `Limits`,
`Memory`) and four production assert macros: reserve-before-push (line 237),
reserve-before-extend (249), reserve-before-resize (259), and nonzero raw layout
(446). Their false outcomes violate internal caller preconditions; normal
mutations use reserved capacity, and ZST boxed values bypass raw. These are
invariant obligations, not public rejection promises. The test-only thread_local
seam is excluded with cfg(test), rather than counted as production syntax.
Generated traits, atomics, Option/Result combinators,
Vec/iterator code, global allocator implementation and standard-library unwind
mechanics are outside the AST denominator. The budget ledger explicitly audits
checked-add/filter closure predicates even though they are not separate AST ifs.

## Source records

Source SHA-256: `56b3852ee4c3fc970cccf60a98cf5a3db4145c5298960f6d7200f48755b731dd`.
Location is line:column; hash is the first 12 source-token SHA-256 digits.

| Location | Kind / owner | Disposition | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 73:9 | `if` / `validate` | C | M1 | `935f68319d4f` |
| 73:12 | `short_circuit_or` / `validate` | C | M1 | `142fafc1b97a` |
| 73:12 | `short_circuit_or` / `validate` | C | M1 | `eba9b457ca90` |
| 73:12 | `short_circuit_or` / `validate` | C | M1 | `081a4c0c0476` |
| 73:12 | `short_circuit_or` / `validate` | C | M1 | `63c86f5c7fa5` |
| 73:12 | `short_circuit_or` / `validate` | C | M1 | `d65ce18fed56` |
| 73:12 | `short_circuit_or` / `validate` | C | M1 | `65f7071c1b02` |
| 73:12 | `short_circuit_or` / `validate` | P | M1 | `0d3ce3c67265` |
| 108:39 | `try_propagation` / `allocate` | E | M2 | `8a8de823d5ed` |
| 131:9 | `if` / `clone` | P | M2 | `935f68319d4f` |
| 153:9 | `if` / `drop` | P | M2 | `935f68319d4f` |
| 203:75 | `try_propagation` / `reserve` | E | M3 | `8a8de823d5ed` |
| 204:9 | `if` / `reserve` | P | M3 | `935f68319d4f` |
| 207:74 | `try_propagation` / `reserve` | E | M3 | `8a8de823d5ed` |
| 208:46 | `try_propagation` / `reserve` | E | M3 | `8a8de823d5ed` |
| 209:20 | `try_propagation` / `reserve` | E | M3 | `8a8de823d5ed` |
| 217:13 | `if` / `reserve` | P | M3 | `935f68319d4f` |
| 224:74 | `try_propagation` / `reserve` | E | M3 | `8a8de823d5ed` |
| 241:9 | `for_iteration` / `extend` | E | M4 | `10c22bcf4c76` |
| 334:42 | `try_propagation` / `new` | E | M1 | `8a8de823d5ed` |
| 341:10 | `try_propagation` / `new` | E | M3 | `8a8de823d5ed` |
| 354:17 | `match` / `claim` | P | M3 | `4945a70fa7f9` |
| 360:45 | `try_propagation` / `claim` | E | M3 | `8a8de823d5ed` |
| 370:21 | `try_propagation` / `shared` | E | M3 | `8a8de823d5ed` |
| 371:70 | `try_propagation` / `shared` | E | M3 | `8a8de823d5ed` |
| 375:70 | `try_propagation` / `reserve_external` | E | M3 | `8a8de823d5ed` |
| 376:9 | `if` / `reserve_external` | P | M3 | `935f68319d4f` |
| 377:25 | `try_propagation` / `reserve_external` | E | M3 | `8a8de823d5ed` |
| 379:49 | `try_propagation` / `reserve_external` | E | M3 | `8a8de823d5ed` |
| 384:9 | `if` / `copy_external` | I | M4 | `935f68319d4f` |
| 388:53 | `try_propagation` / `copy_external` | E | M4 | `8a8de823d5ed` |
| 400:39 | `try_propagation` / `admit` | E | M3 | `8a8de823d5ed` |
| 430:44 | `try_propagation` / `copy` | E | M4 | `8a8de823d5ed` |
| 436:9 | `if` / `boxed` | P | M3 | `935f68319d4f` |
| 441:50 | `try_propagation` / `boxed` | E | M3 | `8a8de823d5ed` |
| 444:21 | `try_propagation` / `raw` | E | M3 | `8a8de823d5ed` |
| 448:92 | `try_propagation` / `raw` | E | M3 | `8a8de823d5ed` |

Totals: C=7, E=21, I=1, P=8; 37 records.

## Executed validation for this tranche

On macOS/aarch64, all-feature debug and release runs passed 14 FFI unit tests,
14 memory unit tests (including the abort subprocess), five `allocation`, five
`live_budget`, and two `live_budget_decisions` tests: 40 per profile. Four new FFI
tests also passed `cargo +nightly miri test --offline -p weekend-db --all-features
--lib ffi::decision_tests::`. Strict all-feature library/test Clippy passed.
The rebuilt debug cdylib passed `python3 tools/ffi_smoke.py
 target/verification-diagnostics/debug/libweekend_db.dylib`.

Runs used `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/verification-diagnostics`.
The inventory completed without parse failures. No production code changed;
only the appended FFI test module is new Rust source. Existing Miri, sanitizer,
Loom and mutation results cited above are prior evidence, not newly rerun claims.
This tranche does not claim Windows execution, exhaustive native failures,
new mutations, compiler MC/DC percentages or completed project assurance.
