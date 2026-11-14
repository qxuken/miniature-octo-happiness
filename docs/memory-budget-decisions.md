# Live allocation budget: scoped decision evidence

This ledger covers the new core live-byte admission and charged-capacity wrapper
in `src/memory.rs`, plus public rollback boundaries. It extends the existing
[allocator ledger](../DESIGN.md#mem-v1-independent-configuration-bounds), without
claiming whole-allocator or whole-library MC/DC. The
[source inventory](decision-inventory.md) remains an unreviewed syntactic
inventory; these cases are not automatically matched to its records.

The budget counts requested core allocation layouts, including in-flight
reservations and conservative old-plus-new capacity during growth. The allocator
may grow a buffer in place with `realloc`; admission still reserves the complete
new capacity while the old capacity remains charged. This preserves the same cap
and peak policy whether realloc moves or extends the buffer in place. It excludes allocator overhead,
backend storage, application-output buffers, FFI handles and the fixed budget
control block. `usize::MAX` is the default cap, not a bypass of checked arithmetic.
Zero is a valid cap, but a database needs a nonzero initial head allocation.
Allocation failures return `OutOfMemory`; cap/count/layout rejections return
`Limit`. Peak usage includes admitted reservations that subsequently fail.

## Admission decisions and pairs

Let `U` be currently charged bytes, `B` requested bytes, and `L` the cap.
`Memory::admit` implements two sequential single-condition decisions:

1. `A`: `U.checked_add(B)` produces a sum.
2. When A succeeds, `C`: that sum is at most `L`.

There is no evaluated C when A fails. Do not invent a `C=true` value for an
overflowing sum or claim a unique-cause compound pair across that masked branch.
Each executable guard has a direct false/true pair; this is the applicable
single-condition MC/DC interpretation for the sequential checks.

| Guard | Accepted control | Rejected comparison | Result and rollback |
|---|---|---|---|
| A, fixed request | `U=MAX-1, B=1, L=MAX` | `U=MAX, B=1, L=MAX` | `Limit` for overflow; live remains MAX, not zero/wrapped |
| C, finite equality boundary | `U=7, B=1, L=8` | `U=7, B=1, L=7` | Exact equality accepted; rejection leaves live at 7 |
| Zero request/cap | `U=0, B=0, L=0` | `U=0, B=1, L=0` | Zero admission succeeds without charging; nonzero fails |
| Default maximum cap | `U=0, B=MAX, L=MAX` | Same held charge followed by `B=1` | No special unlimited bypass; overflow still rejected |

`memory::live_budget_tests::admission_limit_and_overflow_decision_pairs`
executes the first three rows and the fixed-request overflow pair.
The huge `U` rows use private admission guards, not impossible real allocations.
They are a model of reachable arithmetic state rather than a claim that a process
can allocate MAX bytes. All held guards are dropped and usage returns to zero.
The finite equality pair changes only C's truth while arithmetic remains valid.
The default-cap row reuses
`allocation_layout_tests::admission_overflow_rolls_back_exactly`.

Public integration test
`initial_head_requires_exact_live_layout_budget` measures the platform's initial
head layout H, then compares caps H-1/H and zero. H succeeds, H-1 and zero fail
with `Limit`. A further value allocation at cap H fails without changing live
usage or revision. `value_node_admission_failure_releases_already_allocated_payload`
measures the complete eight-byte-value allocation T, compares T-1/T, and confirms
that the earlier payload is reclaimed if the later node admission fails. Peak
retains the intermediate payload charge. Exact-cap construction, alias/drop and
retry preserve value contents and return live usage to the initial head baseline.

## Capacity, allocation and ownership decisions

`ChargedVec::reserve` checks count addition, available capacity and layout before
admission and allocation. These are sequential guards too, not one fabricated
compound Boolean expression.

| Guard/behavior | Decision pair or independent control | Evidence and state |
|---|---|---|
| `len.checked_add(extra)` | Fixed nonempty vector with representable extra versus overflowing extra | `Limit`; old length, contents and usage unchanged |
| `needed <= capacity` | Reserve within capacity while fault budget is zero versus growth with fault budget zero | No-growth succeeds without claiming; growth reports `OutOfMemory`, contents and live charge unchanged |
| `Layout::array::<T>(needed)` | Representable byte layout versus representable count with byte layout exceeding isize bound | Layout failure reports `Limit` before allocation |
| Empty/nonempty backing capacity | `capacity==0` allocates; existing nonzero capacity reallocates | `growth_reserves_both_buffers_and_rolls_back_failures` starts with empty bytes and then grows them; `aligned_growth_partial_iteration_and_null_shared_cleanup` repeatedly grows over-aligned elements |
| Growth cap | Eight-byte old buffer + sixteen-byte new buffer: cap24 versus cap23 | `growth_reserves_both_buffers_and_rolls_back_failures`: cap24 succeeds, cap23 fails; successful growth live16/peak24 |
| Allocation claim succeeds/fails | Same growth and cap24, fault injection disabled versus zero remaining claims | Same test: failed claim releases new admission, retains old live8 |
| System reallocator returns pointer/null | Same growth/cap24, thread-local allocator fault off/on | Same test: null returns `OutOfMemory`, original Vec and contents/live8 remain; success replaces bookkeeping without dropping the old pointer |
| Shared allocator returns pointer/null | Over-aligned input, system fault off/on | `aligned_growth_partial_iteration_and_null_shared_cleanup`: failed Shared allocation drops input once and restores live0 |
| Last Shared reference | Drop one of two aliases versus final alias | `shared_clones_keep_one_charge_and_unwind_reclaims_allocation`: charge persists after first drop and releases after final drop |
| Element destructor normal/unwinds | Normal value drop versus deliberately panicking destructor | Same test: Shared and consuming iterator reclaim all allocations/charges through unwind |
| Iterator exhausted/partially consumed | Return first element then drop iterator with remaining elements | `aligned_growth_partial_iteration_and_null_shared_cleanup`: buffer remains charged until iterator drop, remaining elements destroyed once, returned element separately owned |
| Zero-sized element capacity | Three unit elements at zero cap versus nonzero bytes at finite cap | `zero_sized_elements_and_output_transfer`: ZST path allocates/charges zero; no zero-sized raw allocation is attempted |
| Explicit output transfer | Charged internal bytes versus `into_external()` result | Same test: bytes unchanged, core charge released at documented output ownership boundary |

`memory::live_budget_tests::charged_reserve_capacity_count_and_layout_decision_pairs`
executes the first three capacity/count/layout rows. Remaining unqualified unit
test names in this table refer to `memory::live_budget_tests`, except
`aligned_growth_partial_iteration_and_null_shared_cleanup` in
`memory::allocation_layout_tests`.

The private count/capacity/layout tests exercise the wrapper directly because the
public collection limits prevent practical construction of these counts. Array
layout rejection is feasible for non-ZST element types; for a ZST,
`Vec::capacity()==usize::MAX` prevents the allocation branch. An allocation of
zero bytes from this branch is therefore infeasible under the wrapper invariant.

The existing fault-injection remaining-budget decision (`MAX`, zero, positive)
is reused from `memory::tests::reservation_checks_capacity_overflow_and_budget_independently`
and `allocation_ownership_transfers_or_drops_exactly_once`. It is distinct from
the new byte cap: MAX here is a sentinel for an allocation-count test mechanism,
whereas `max_live_bytes=MAX` still performs checked addition.

## Concurrency and composed rollback

`concurrent_reservations_cannot_exceed_cap` holds eight simultaneous 32-byte
requests against cap32: exactly one succeeds, peak32 and live0 after drop.
`concurrent_drafts_share_the_database_budget` checks the same budget across real
transaction drafts. These test contention and ownership; they do not prove every
CAS retry interleaving. Atomic update internals are delegated to the standard
library rather than counted as our source-level decisions.

`usage_bounds_peak_during_admission_update_window` models the interval after
updating live but before updating peak and verifies the reported peak is at least
the sampled live count. A second modeled query after an old owner drops also
retains that observed high water, before the admitting thread resumes. The
query uses atomic fetch_max to persist its sample; a load-only clamp could
otherwise report a decreasing peak. Counters are sampled separately, not a linearizable
snapshot of the whole database.

The five tests in `tests/live_budget.rs` cover value aliases surviving database
drop, pinned snapshots, draft destruction, failed commit preserving the head,
concurrent drafts, and budget rejection during reopening/backup/logical export/
import. The source image is unchanged and failed destination operations leave an
empty image. These are composed behavior checks, not additional MC/DC conditions
for every storage-publication branch.

## Explicit exclusions and residual work

- `Shared::clone` abort threshold and original refcount ordering retain their
  existing MEM-V3 evidence; no new whole-refcount proof is asserted here.
- `push`, `extend_from_slice`, and `resize` assertions enforce internal
  reserve-before-growth invariants. Their panic outcomes require violating the
  wrapper contract; those misuse paths are not claimed covered by the admission
  pairs. Every shipped call site still needs to preserve those invariants.
- Trait-generated drop glue, `Vec` deallocation during unwind, allocator internals,
  compiler lowering, `AtomicUsize` implementation and slice sorting are not
  expanded into conditions. Miri/layout tests support the ownership assumptions,
  but do not constitute MC/DC for those dependencies.
- Reconstructing the original `Layout::array(capacity)` uses an `expect`: failure
  is infeasible for a nonzero buffer created by this wrapper because its exact
  layout was validated at allocation and capacity cannot grow through a slice.
  Realloc preserves alignment and old contents on null; success is followed only
  by non-unwinding Vec metadata replacement/forget before ownership is restored.
- Count/layout failure and allocation-null paths are distinct decisions. Tests
  do not claim a meaningful allocator invocation after a rejected layout or cap.
- No aggregate RSS bound, allocator fragmentation bound, or process-wide budget
  covering typed/replication/app/native allocations follows from this ledger.
- This tranche records scoped source-level pairs and behavioral checks. It adds
  no new compiler-instrumented MC/DC percentage. A subsequent safe mutation
  replay is recorded below.

## Reproduction and source identity

Use a separate target directory when other assurance jobs run concurrently:

```sh
CARGO_TARGET_DIR=target/budget-decisions cargo test --offline -p weekend-db --lib memory::
CARGO_TARGET_DIR=target/budget-decisions cargo test --offline -p weekend-db --test live_budget --test live_budget_decisions
CARGO_TARGET_DIR=target/budget-decisions cargo test --offline --release -p weekend-db --lib memory::
CARGO_TARGET_DIR=target/budget-decisions cargo test --offline --release -p weekend-db --test live_budget --test live_budget_decisions
```

On 2026-11-09, before the realloc optimization, 14 `memory::` unit tests and seven public tests (five existing,
two added here) passed in debug and release using the commands above. The final
same-type valid/invalid layout control was also rerun separately in both profiles.
Strict Clippy passed for the new integration target. Independent review previously
ran the over-alignment/partial-iteration/null-Shared and admission-overflow cases
under nightly Miri; that is scoped ownership evidence, not a whole-suite Miri run.

After the realloc optimization, all seven `memory::live_budget_tests` passed
again in debug and release. Both `memory::allocation_layout_tests` passed under
nightly Miri on the final realloc implementation (over-alignment, growth, partial
iteration, null Shared cleanup and overflow). The allocator-versus-reallocator
branch and unchanged conservative capacity policy are reflected above.

Final source snapshot SHA-256 values (uncommitted tranche following `1ca5e5a`):

| Source | SHA-256 |
|---|---|
| `src/memory.rs` | `56b3852ee4c3fc970cccf60a98cf5a3db4145c5298960f6d7200f48755b731dd` |
| `tests/live_budget.rs` | `87c1d079dccfbaac67dacfb54e54d78f11da818b017a4d8d9edd0d6b9e9bb81f` |
| `tests/live_budget_decisions.rs` | `d57f2af87a5e1cc928eee832397792f342153dedbc25351784904f5b71e0393b` |

Regenerate/reconcile these hashes after edits before reusing the stated pairs as
evidence for a different source revision. No whole-project MC/DC claim follows.


## Safe guard mutation replay

`python3 tools/mutations.py --case live-budget` replayed five isolated mutations:
changing cap equality to strict inequality, removing the cap filter, wrapping
instead of checking admission addition, suppressing released-byte accounting,
and removing persistence and clamping of the sampled peak. Each passed its baseline and failed its
named budget tests after mutation. The runner rejects compile errors and zero
matched tests as evidence. Results are in `target/mutations-live-budget`. These
mutations change admission/telemetry semantics without invalidating pointers or
object ownership; they do not constitute an unsafe-code mutation campaign.

The final telemetry follow-up first reproduced a decreasing observed peak
(8 then 4) while an admitting thread was paused. Querying now persists its
sampled high water with fetch_max. All seven admission tests passed again in
debug/release, the modeled window passed under Miri, and the updated peak
mutation was caught in `target/mutations-live-budget-peak-final`. The source
hash above includes this correction; other layout/ownership paths are unchanged.
