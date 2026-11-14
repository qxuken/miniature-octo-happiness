# Engine source decision audit

This classifies every production `src/engine.rs` record from the
[source inventory](../decision-inventory.md): **141 records**, comprising 98 `?`,
23 `if`, six `&&`, five pattern tests, four loops, three matches, one `||`, and
one let-else. Logical and if-let records overlap their enclosing decisions;
these are not 141 independent Boolean obligations or a compiler MC/DC percentage.
Cfg branches are a source union, including native and replication variants.

New evidence is in [engine_decisions.rs](../../tests/engine_decisions.rs) and the
engine's private counter/poison/arithmetic tests. The only production refactor
extracts the existing checked new-write sum into a
private `write_charge` helper used by real assignment, preserving observation
timing, order of checks and error behavior. Existing scoped
[TX-V1/TX-V2 evidence](../../DESIGN.md#tx-v1-whole-database-revision-check)
remains applicable. The appendix maps every record to these statuses:

| Status | Meaning |
|---|---|
| P | Direct predicate truth controls/compound independence pairs identified below |
| E | Referenced execution, enum, loop or propagation evidence; not every transitive error variant |
| C | Duplicate syntactic record of an already classified condition |
| I | Error outcome coupled away by an earlier check/invariant, with rationale |
| D | Delegated platform/replication evidence; wrapper-specific failure attribution remains open |
| G | Explicit remaining test/proof gap |

## T1: validation and observations

`tests/transactions.rs::existing_record_edits_follow_validation_mode` holds the
same draft and varies A=`WholeDatabase`, B=`base != current`: (T,T) conflicts,
(F,T), (T,F), (F,F) commit. First/second isolate A; first/third isolate B. B is
masked when A is false; the fixture establishes its conceptual value.
`record_and_collection_validation_distinguish_changes_and_removal` checks a point
accepts another record's edit but rejects its replacement/deletion or collection
removal; scan/absence accepts another collection's edit but rejects its own
collection's edit/removal. It checks revision, immutable snapshots and terminal
state. `collection_identity_and_unrelated_catalog_changes` covers unrelated
catalog changes and non-reused IDs; existing ABA/write-skew tests remain useful.

New `point_observation_matches_collection_and_key_independently` caps observations
at one. For A=`same collection`, B=`same key`, (T,T) reuses an existing point;
(F,T) and (T,F) require a new observation and fail Limit. These isolate both
conjunction operands. Repeated absence in one collection reuses its observation;
another collection fails at the same cap. Existing/missing points exercise both
let-else outcomes; below-cap/at-cap cases control both observation-limit guards.

## T2: overlays, filters and data revisions

New `overlay_lookup_and_replacement_match_both_identity_components` caps writes
at one. (Same collection,same key)=(T,T) reads/replaces the staged value; (F,T)
and (T,F) read the original and cannot add another write. Same-key replacement
still succeeds at capacity. `bounded_observations_writes_and_empty_user_keys`
retains independent empty-key/write-limit controls.

New `scan_filters_collection_and_value_presence_independently` stages a delete
and a replacement in each of two collections. For each filter A=matching
collection, B=required value presence, (T,T) is included, (F,T) excluded, and
(T,F) excluded by that filter but handled by its complement. Both collections'
results detect leakage and exercise both match arms. These are masking pairs;
the fixture defines B even when A skips it. Existing
`final_assignments_can_replace_records_at_collection_limit` verifies deletes
precede inserts in scan and publication. New
`read_only_commit_advances_publication_but_not_data_revision` pairs empty writes
with accepted no-op deletion: both advance publication, only the latter advances
data revision. These cases also cover empty/nonempty write iterations.

## T3: terminal states and cleanup

New `all_terminal_states_reject_reads_edits_and_second_completion` compares active
drafts with Committed, explicitly Aborted and Failed (entry-limit) drafts.
Get/contains/scan/count/put/delete/commit/abort all reject Closed after termination,
without changing revision. Put uses a same-store value to isolate the active
guard: foreign-value validation intentionally runs first.

The commit match has successful, Conflict/Io and other-error controls: TX-V2
covers Conflict->Aborted; `tests/durability.rs::publication_failures_reopen_the_revision_that_survived`
covers Io->Aborted and OutcomeUnknown->Failed at every publication boundary;
the new entry-limit case covers Limit->Failed. This is enum-arm evidence.
New `abort_and_failed_commit_clear_owned_payloads_before_draft_drop` confirms
abort and failed publication clear 4 KiB of staged payloads while the draft is
still alive. Reserved vector capacity remains until drop; dropping the draft
returns live usage to baseline and no uncommitted row is visible.

## T4: publication and poison

New `absent_and_poisoned_storage_reject_publication_and_compaction` compares
memory-only, healthy byte-backed and I/O-poisoned databases. Missing storage
rejects storage-space/compaction with Invalid. I/O poison blocks publication,
catalog/retention changes, space and compaction, while acknowledged reads remain.
Private `engine::tests::mutex_poison_preserves_reads_and_blocks_all_writer_gates`
adds caught mutex poisoning, mutable compaction access and replication-head gates.
For requires_recovery A=mutex poison, B=storage poison: (F,F)->false,
(T,F)->true, (F,T)->true isolate each disjunction operand. B is masked in (T,F)
and false by the memory-only fixture. Existing
`tests/concurrency.rs::pinned_reader_proceeds_during_io_and_adapter_panic_fails_closed`
adds actual adapter panic during append/root/witness sync and reopen checks.

State publication storage None/Some is controlled by memory and durable tests.
Retention None/Some is controlled by
`tests/retention.rs::stale_record_commits_preserve_current_retention_without_resurrection`:
record commits preserve current retention, retain/release install the prepared
catalog. `retain_release_faults_recover_whole_catalogs_at_every_io_boundary`
checks failed durability cannot install a partial in-process catalog.

## T5: retention and exhaustion

`tests/retention.rs::retention_limit_foreign_roots_and_monotonic_ids` controls
own/foreign snapshots, 63->64 accepted retention versus the next rejected retain,
release and non-reused IDs. `historical_retain_release_and_live_lifetimes_survive_reopening`
covers present/missing lookup and release. New
`retained_id_output_allocation_failure_does_not_change_catalog` compares zero and
normal allocation budget for ID materialization without changing revision/catalog.

Private `every_revision_increment_accepts_last_revision_then_rejects_wrap`
pairs MAX-1/MAX for each revision increment in create/drop collection,
retain/release and commit. MAX is published once, then Exhausted leaves revision,
catalog and live usage unchanged. `final_catalog_and_retention_ids_are_assigned_once`
pairs next-ID MAX-1/MAX. `store_identity_exhaustion_is_isolated_in_a_subprocess`
pairs process identity MAX-1/MAX without altering the parent's global counter.
Existing individual exhaustion tests remain. These use private state setup,
not impractically many public calls; no production seam was added.

## T6: collection, key and byte limits

New `snapshot_collection_and_key_errors_have_valid_controls` compares valid,
foreign and missing IDs across snapshot get/scan/count, transaction get/scan and
catalog removal. Empty key at max_bytes0 succeeds; one byte fails. Existing
`collection_identity_and_unrelated_catalog_changes` pairs foreign/same-store
values in put.

New `observation_and_replacement_byte_limits_preserve_the_draft` rejects both
present-point and absent/scan observations at zero transaction bytes (a reopened
seeded image supplies the point case). Normal assignments succeed; overweight
new writes and replacements fail without changing the old overlay; a subsequent
small replacement commits. This independently controls the charge and replacement
limit comparisons. Arithmetic-overflow guards are classified separately below.

## T7: preparation and propagation

Existing fault sweeps supply successful and injected-failure controls:

- `tests/allocation.rs::every_candidate_allocation_failure_leaves_all_records_and_history_unchanged`:
  candidate graph/head creation and atomic publication.
- `observation_overlay_and_scan_failures_never_publish`: observation, overlay and
  scan allocation paths preserve published records/revision.
- `failed_collection_publication_is_atomic_and_ids_are_not_reused`: catalog
  removal preparation and pinned snapshots.
- `tests/retention.rs::retention_preparation_allocation_failures_are_atomic`:
  retention vector/head preparation and unchanged durable image/catalog.
- `tests/live_budget_decisions.rs::initial_head_requires_exact_live_layout_budget`
  and `tests/allocation.rs::allocation_limits_reject_each_independent_invalid_configuration`:
  head admission and invalid configurations.
- `tests/durability.rs::preparation_and_backend_allocation_failures_preserve_the_acknowledged_state`
  and `publication_failures_reopen_the_revision_that_survived`: storage boundary
  propagation and terminal outcomes.

E means these referenced tests support the behavior, not that every callee error
variant was instrumented at every `?`. Empty/nonempty collection/point loops are
covered by observation-free/read-only and point/scan drafts in T1-T3; iteration
outcomes are not added to a Boolean MC/DC percentage.

## T8: delegation, infeasibility and residual gaps

Native wrappers delegate to limits validation, RNG and file adapters. Existing
`tests/files.rs::{file_reopen_retention_abort_and_live_reader_lifetimes,creation_is_exclusive_and_aliases_cannot_bypass_locking,malformed_files_are_rejected_without_repair_or_truncation}`
and diagnostics suites remain their evidence. Native identity generation remains explicitly delegated, including its error
path; this tranche adds no production RNG injection interface. Linux handles
unsupported targets, syscall errors/short reads and three zero identities; macOS
uses arc4random_buf and rejects three zero identities; Windows handles BCrypt
errors and three zero identities. No existing shared identity-source seam was
found. Existing native helper/diagnostics tests do not prove these platform RNG
branches executed; that residual native obligation remains D, not a tested pair. Replication wrappers delegate to
`src/replication/follower/tests.rs::{image_verification_failures_preserve_the_reported_revision,interrupted_image_and_manifest_barriers_recover_exactly_one_image}`
and primary tests. D rows remain open for wrapper-specific failure attribution.

I dispositions:

- The new database in decode_replica has not escaped before its mutex acquisition;
  poison is infeasible. Fresh create/open write-state unwraps have the same rationale.
- Second base-collection lookups in transaction get/scan cannot fail after the
  observation checked the same immutable base. Publication's current collection
  lookup cannot fail after validation under the held mutex: all writes have a
  dependency, removed collections already conflict, and that loop never drops
  collections.
- `key.len()+size_of::<Point>()` cannot overflow on supported 32/64-bit targets:
  key length <=isize::MAX and fixed Point size is far below isize::MAX.
  Replacement subtraction cannot underflow because the old weight is included
  in draft bytes; its following sum is <=2*isize::MAX<usize::MAX.
- Retention fork len+extra is bounded by 64 entries and extra 0/1. It is an invariant,
  not an additional inventory decision; no invalid private state is fabricated.

The two previous numeric gaps are addressed by T9. The first checked sum in the
new-write helper is additionally proved nonoverflowing inside its public input
domain; its defensive out-of-domain test is not presented as a reachable API case.

Method-internal choices (`any`, `find`, `map_or`, `ok_or`, `unwrap_or_else`),
implicit drop/allocator branches and macros are not expanded by the inventory.
Their relevant semantic effects appear above, without claiming a complete lowered
code denominator. All records are classified; native/replication D obligations remain open.

## T9: extreme logical weights and checked arithmetic

Private `shared_dag_reaches_transaction_sum_overflow_without_large_allocations`
builds actual valid Values using only scalar/list constructors. A memoized balanced
DAG represents weight `16*n` with fewer than `4*usize::BITS` distinct cached Values;
core peak stays within an enforced 1 MiB cap. This is a bounded-memory real
transaction test, not a forged weight or enormous slice.

Let H=isize::MAX, M=usize::MAX=2H+1, W=size_of(Write), O=the collection-observation
charge plus one-byte key plus W. The first staged value has weight
`16*floor((H-O)/16)`, leaving draft bytes B within 15 bytes of H. A second valid
value has weight `16*floor(H/16)=H-15`. Its new-write charge is `1+(H-15)+W`;
the test establishes that adding it to B overflows usize. `put` returns Limit,
leaves bytes/one staged write/state unchanged, publishes nothing, and the original
first write can still commit. Separate `charge` controls accept extra 0 and
extra (H-B), reject extra (H-B+1) through the cap comparison without overflow, then
exercise the genuine overflow through the second put. Thus cap equality and
checked-add failure are distinct, rather than conflating their identical errors.

`new_write_charge_checks_each_sum_without_allocating_giant_keys` calls the exact
private helper used by assign. With key K=H-W+1 and value weight H, the first sum
is M-W and the final sum is exactly M (accepted). Increasing only K by one makes
the first sum representable but the final overhead addition overflow (Limit).
Both conceptual arguments are <=H. These are **synthetic arithmetic inputs**;
no giant slice is allocated and no end-to-end giant-key coverage is claimed.
For all valid key/weight inputs <=H, the first sum is <=2H=M-1, so its overflow
is infeasible under the public preconditions. A helper-only (M,1) test exercises
that defensive failure outside the valid API domain; (0,0) accepts W.

Three independent release-mode wrapping-arithmetic mutations in a disposable
worktree were caught by assertions, not compile errors: draft checked_add->wrapping_add
by the real DAG transaction; initial helper sum->wrapping_add by its defensive
case; final overhead sum->wrapping_add by the exact boundary pair. The helper
extraction adds no explicit if/try AST node (the checked operations return through
ok_or), so the appendix remains 141 source records. These supplemental arithmetic
conditions are stated explicitly instead of hiding them from the syntactic
inventory denominator.

## Source-record appendix

Each row names source line/kind in the pinned source and the evidence group above.
Same-line records with different kinds are intentional. Full source byte ranges
and IDs can be regenerated with the inventory utility.

| Line | Kind | Owner / expression | Status | Evidence |
|---:|---|---|---|---|
| 36 | `try_propagation` | `fork`: `mem.reserve(&mut entries, self.entries.len() + extra)` | E | T7 |
| 83 | `if` | `publish`: `let Some(storage) = &mut self.storage` | P | T4 |
| 83 | `pattern_test` | `publish`: `&mut self.storage` | C | T4 |
| 89 | `try_propagation` | `publish`: `storage.persist( mem, &head, next_collection, retention.as_ref().unwrap_or(…` | E | T4/T7 |
| 93 | `if` | `publish`: `let Some(retention) = retention` | P | T4 |
| 93 | `pattern_test` | `publish`: `retention` | C | T4 |
| 181 | `try_propagation` | `storage_space`: `self.publication_state()` | E | T4 |
| 184 | `try_propagation` | `storage_space`: `self.publication_state()? .storage .as_mut() .ok_or(Error::Invalid)` | E | T4 |
| 209 | `try_propagation` | `compact_copy`: `self.state.get_mut().map_err(&#124;_&#124; Error::RecoveryRequired)` | E | T4 |
| 210 | `try_propagation` | `compact_copy`: `state.storage.as_mut().ok_or(Error::Invalid)` | E | T4 |
| 221 | `try_propagation` | `replication_head`: `self.write_state()` | E | T4/T8 |
| 222 | `try_propagation` | `replication_head`: `state.storage.as_ref().ok_or(Error::Invalid)` | E | T4/T8 |
| 223 | `try_propagation` | `replication_head`: `storage.guard()` | D | T8 |
| 237 | `try_propagation` | `decode_replica`: `Self::with_limits(limits)` | D | T8 |
| 239 | `try_propagation` | `decode_replica`: `crate::storage::recover_replica(backend, &db.mem, identity, revision, length)` | D | T8 |
| 240 | `try_propagation` | `decode_replica`: `db.write_state()` | I | T8 |
| 267 | `try_propagation` | `publication_state`: `self.write_state()` | E | T4 |
| 268 | `if` | `publication_state`: `let Some(storage) = &state.storage` | P | T4 |
| 268 | `pattern_test` | `publication_state`: `&state.storage` | C | T4 |
| 269 | `try_propagation` | `publication_state`: `storage.guard()` | E | T4 |
| 277 | `try_propagation` | `with_limits`: `Memory::new(limits)` | E | T7 |
| 280 | `try_propagation` | `with_limits`: `NEXT_STORE .try_update(Ordering::Relaxed, Ordering::Relaxed, &#124;n&#124; …` | E | T5 |
| 285 | `try_propagation` | `with_limits`: `mem.shared(Head { revision: 0, data_revision: 0, collections: Map::new(), })` | E | T7 |
| 304 | `try_propagation` | `create_on`: `Self::with_limits(limits)` | E | T7 |
| 314 | `try_propagation` | `create_on`: `Storage::create( backend, &db.mem, &state.head, state.next_collection, iden…` | E | T7 |
| 322 | `try_propagation` | `open_on`: `Self::with_limits(limits)` | E | T7 |
| 323 | `try_propagation` | `open_on`: `Storage::open(backend, &db.mem)` | E | T7 |
| 336 | `short_circuit_or` | `requires_recovery`: `self.state.is_poisoned() state .storage .as_ref() .is_some_and(Storage::req…` | P | T4 |
| 344 | `try_propagation` | `create_file`: `limits.validate()` | D | T8 |
| 345 | `try_propagation` | `create_file`: `crate::storage::file_identity()` | D | T8 |
| 346 | `try_propagation` | `create_file`: `crate::storage::FileStore::create(path)` | D | T8 |
| 350 | `try_propagation` | `open_file`: `limits.validate()` | D | T8 |
| 351 | `try_propagation` | `open_file`: `crate::storage::FileStore::open(path)` | D | T8 |
| 356 | `if` | `retain_snapshot`: `!std::ptr::eq(self, snapshot.db)` | P | T5 |
| 359 | `try_propagation` | `retain_snapshot`: `self.publication_state()` | E | T4 |
| 360 | `if` | `retain_snapshot`: `state.retention.entries.len() >= MAX_RETAINED_SNAPSHOTS` | P | T5 |
| 363 | `try_propagation` | `retain_snapshot`: `state.head.revision.checked_add(1).ok_or(Error::Exhausted)` | E | T5 |
| 365 | `try_propagation` | `retain_snapshot`: `id.checked_add(1).ok_or(Error::Exhausted)` | E | T5 |
| 366 | `try_propagation` | `retain_snapshot`: `state.retention.fork(&self.mem, 1)` | E | T5/T7 |
| 377 | `try_propagation` | `retain_snapshot`: `self.mem.shared(Head { revision: next, data_revision: state.head.data_revis…` | E | T5/T7 |
| 379 | `try_propagation` | `retain_snapshot`: `state.publish(&self.mem, head, next_collection, Some(retention))` | E | T4/T5 |
| 383 | `try_propagation` | `release_retained`: `self.publication_state()` | E | T4 |
| 389 | `try_propagation` | `release_retained`: `state .retention .entries .iter() .position(&#124;r&#124; r.id == id) .ok_o…` | E | T5 |
| 390 | `try_propagation` | `release_retained`: `state.head.revision.checked_add(1).ok_or(Error::Exhausted)` | E | T5 |
| 391 | `try_propagation` | `release_retained`: `state.retention.fork(&self.mem, 0)` | E | T5/T7 |
| 397 | `try_propagation` | `release_retained`: `self.mem.shared(Head { revision: next, data_revision: state.head.data_revis…` | E | T5/T7 |
| 399 | `try_propagation` | `release_retained`: `state.publish(&self.mem, head, next_collection, Some(retention))` | E | T4/T5 |
| 409 | `try_propagation` | `retained_snapshot`: `state .retention .entries .iter() .find(&#124;r&#124; r.id == id) .ok_or(Er…` | E | T5 |
| 419 | `try_propagation` | `retained_ids`: `self.mem.reserve(&mut ids, state.retention.entries.len())` | E | T5 |
| 433 | `if` | `check_collection`: `id.store == self.store` | P | T6 |
| 440 | `try_propagation` | `create_collection`: `self.publication_state()` | E | T4 |
| 441 | `try_propagation` | `create_collection`: `state.head.revision.checked_add(1).ok_or(Error::Exhausted)` | E | T5 |
| 443 | `try_propagation` | `create_collection`: `id.checked_add(1).ok_or(Error::Exhausted)` | E | T5 |
| 451 | `try_propagation` | `create_collection`: `state.head.collections.set( &self.mem, &id.to_le_bytes(), Collection { revi…` | E | T7 |
| 456 | `try_propagation` | `create_collection`: `self.mem.shared(Head { revision: next, data_revision: next, collections, })` | E | T7 |
| 457 | `try_propagation` | `create_collection`: `state.publish(&self.mem, head, following, None)` | E | T4/T7 |
| 463 | `try_propagation` | `drop_collection`: `self.check_collection(id)` | E | T6 |
| 464 | `try_propagation` | `drop_collection`: `self.publication_state()` | E | T4 |
| 465 | `if` | `drop_collection`: `state.head.collections.get(&id.id.to_le_bytes()).is_none()` | P | T6 |
| 468 | `try_propagation` | `drop_collection`: `state.head.revision.checked_add(1).ok_or(Error::Exhausted)` | E | T5 |
| 472 | `try_propagation` | `drop_collection`: `state .head .collections .remove(&self.mem, &id.id.to_le_bytes())` | E | T7 |
| 477 | `try_propagation` | `drop_collection`: `self.mem.shared(Head { revision: next, data_revision: next, collections, })` | E | T7 |
| 479 | `try_propagation` | `drop_collection`: `state.publish(&self.mem, head, next_collection, None)` | E | T4/T7 |
| 542 | `if` | `key`: `key.len() > db.limits().max_bytes` | P | T6 |
| 561 | `try_propagation` | `get`: `self.db.check_collection(id)` | E | T6 |
| 562 | `try_propagation` | `get`: `key(self.db, k)` | E | T6 |
| 563 | `try_propagation` | `get`: `collection(&self.head, id)` | E | T6 |
| 569 | `try_propagation` | `scan`: `self.db.check_collection(id)` | E | T6 |
| 570 | `try_propagation` | `scan`: `collection(&self.head, id)` | E | T6 |
| 573 | `try_propagation` | `count`: `self.db.check_collection(id)` | E | T6 |
| 574 | `try_propagation` | `count`: `collection(&self.head, id)` | E | T6 |
| 582 | `if` | `active`: `self.state == TransactionState::Active` | P | T3 |
| 589 | `try_propagation` | `charge`: `self.bytes.checked_add(extra).ok_or(Error::Limit)` | E | T9 |
| 590 | `if` | `charge`: `next > self.db.limits().max_transaction_bytes` | P | T6 |
| 597 | `try_propagation` | `observe_collection`: `self.db.check_collection(id)` | E | T6 |
| 598 | `try_propagation` | `observe_collection`: `collection(&self.base, id)` | E | T6 |
| 599 | `if` | `observe_collection`: `self.collections.iter().any(&#124;(c, _)&#124; *c == id)` | P | T1 |
| 602 | `if` | `observe_collection`: `self.collections.len() + self.points.len() >= self.db.limits().max_observat…` | P | T1 |
| 605 | `try_propagation` | `observe_collection`: `self.charge(std::mem::size_of::<(CollectionId, u64)>())` | E | T6 |
| 606 | `try_propagation` | `observe_collection`: `self.db.mem.reserve(&mut self.collections, 1)` | E | T7 |
| 614 | `try_propagation` | `observe`: `self.active()` | E | T3 |
| 615 | `try_propagation` | `observe`: `self.db.check_collection(id)` | E | T6 |
| 616 | `try_propagation` | `observe`: `key(self.db, k)` | E | T6 |
| 617 | `let_else` | `observe`: `collection(&self.base, id)?.records.get(k)` | P | T1/T6 |
| 617 | `try_propagation` | `observe`: `collection(&self.base, id)` | E | T1/T6 |
| 620 | `if` | `observe`: `self .points .iter() .any(&#124;p&#124; p.collection == id && p.key.as_slic…` | P | T1 |
| 623 | `short_circuit_and` | `observe`: `p.collection == id p.key.as_slice() == k` | P | T1 |
| 627 | `if` | `observe`: `self.collections.len() + self.points.len() >= self.db.limits().max_observat…` | P | T1 |
| 633 | `try_propagation` | `observe`: `k.len() .checked_add(std::mem::size_of::<Point>()) .ok_or(Error::Limit)` | I | T8 |
| 634 | `try_propagation` | `observe`: `self.charge( k.len() .checked_add(std::mem::size_of::<Point>()) .ok_or(Erro…` | E | T6 |
| 636 | `try_propagation` | `observe`: `self.db.mem.copy(k)` | E | T7 |
| 637 | `try_propagation` | `observe`: `self.db.mem.reserve(&mut self.points, 1)` | E | T7 |
| 647 | `try_propagation` | `get`: `self.observe(id, k)` | E | T1/T6 |
| 648 | `if` | `get`: `let Some(w) = self .writes .iter() .find(&#124;w&#124; w.collection == id &…` | P | T2 |
| 648 | `pattern_test` | `get`: `self .writes .iter() .find(&#124;w&#124; w.collection == id && w.key.as_sli…` | C | T2 |
| 651 | `short_circuit_and` | `get`: `w.collection == id w.key.as_slice() == k` | P | T2 |
| 655 | `try_propagation` | `get`: `collection(&self.base, id)` | I | T8 |
| 661 | `try_propagation` | `contains`: `self.get(id, k)` | E | T3 |
| 664 | `if` | `put`: `!self.db.mem.same(&value.mem)` | P | T6 |
| 673 | `try_propagation` | `assign`: `self.observe(id, k)` | E | T1/T6 |
| 675 | `if` | `assign`: `let Some(index) = self .writes .iter() .position(&#124;w&#124; w.collection…` | P | T2 |
| 675 | `pattern_test` | `assign`: `self .writes .iter() .position(&#124;w&#124; w.collection == id && w.key.as…` | C | T2 |
| 678 | `short_circuit_and` | `assign`: `w.collection == id w.key.as_slice() == k` | P | T2 |
| 685 | `try_propagation` | `assign`: `self .bytes .checked_sub(old) .and_then(&#124;n&#124; n.checked_add(weight)…` | I | T8 |
| 686 | `if` | `assign`: `bytes > self.db.limits().max_transaction_bytes` | P | T6 |
| 693 | `if` | `assign`: `self.writes.len() >= self.db.limits().max_writes` | P | T2 |
| 696 | `try_propagation` | `assign`: `write_charge(k.len(), weight)` | E | T9 |
| 697 | `try_propagation` | `assign`: `self.charge(extra)` | E | T6 |
| 698 | `try_propagation` | `assign`: `self.db.mem.copy(k)` | E | T7 |
| 699 | `try_propagation` | `assign`: `self.db.mem.reserve(&mut self.writes, 1)` | E | T7 |
| 709 | `try_propagation` | `scan`: `self.active()` | E | T3 |
| 710 | `try_propagation` | `scan`: `self.observe_collection(id)` | E | T1/T6 |
| 711 | `try_propagation` | `scan`: `collection(&self.base, id)` | I | T8 |
| 712 | `for_iteration` | `scan`: `self .writes .iter() .filter(&#124;w&#124; w.collection == id && w.value.is…` | E | T2 |
| 715 | `short_circuit_and` | `scan`: `w.collection == id w.value.is_none()` | P | T2 |
| 719 | `short_circuit_and` | `scan`: `w.collection == id w.value.is_some()` | P | T2 |
| 722 | `match` | `scan`: `&w.value` | E | T2 |
| 730 | `try_propagation` | `scan`: `records.set( &self.db.mem, &w.key, Record { revision: 0, value: value.clone…` | E | T2/T7 |
| 731 | `try_propagation` | `scan`: `records.remove(&self.db.mem, &w.key)` | E | T2/T7 |
| 737 | `try_propagation` | `count`: `self.scan(id)` | E | T3 |
| 740 | `if` | `validate`: `self.validation == Validation::WholeDatabase self.base.revision != current.…` | C | T1 |
| 740 | `short_circuit_and` | `validate`: `self.validation == Validation::WholeDatabase self.base.revision != current.…` | P | T1 |
| 743 | `for_iteration` | `validate`: `&self.collections` | E | T1 |
| 744 | `if` | `validate`: `collection(current, *id).map(&#124;c&#124; c.revision) != Ok(*expected)` | P | T1 |
| 748 | `for_iteration` | `validate`: `&self.points` | E | T1 |
| 753 | `if` | `validate`: `actual != Some(p.revision)` | P | T1 |
| 762 | `try_propagation` | `publish`: `self.db.publication_state()` | E | T4 |
| 763 | `try_propagation` | `publish`: `self.validate(&state.head)` | E | T1 |
| 764 | `try_propagation` | `publish`: `state.head.revision.checked_add(1).ok_or(Error::Exhausted)` | E | T5 |
| 766 | `for_iteration` | `publish`: `self .writes .iter() .filter(&#124;w&#124; w.value.is_none()) .chain(self.w…` | E | T2 |
| 774 | `try_propagation` | `publish`: `collections .get(&w.collection.id.to_le_bytes()) .ok_or(Error::Conflict)` | I | T8 |
| 776 | `match` | `publish`: `&w.value` | E | T2 |
| 784 | `try_propagation` | `publish`: `c.records.set( &self.db.mem, &w.key, Record { revision: next, value: value.…` | E | T2/T7 |
| 785 | `try_propagation` | `publish`: `c.records.remove(&self.db.mem, &w.key)` | E | T2/T7 |
| 788 | `try_propagation` | `publish`: `collections.set(&self.db.mem, &w.collection.id.to_le_bytes(), c)` | E | T2/T7 |
| 790 | `if` | `publish`: `self.writes.is_empty()` | P | T2 |
| 799 | `try_propagation` | `publish`: `self.db.mem.shared(Head { revision: next, data_revision, collections, })` | E | T7 |
| 801 | `try_propagation` | `publish`: `state.publish(&self.db.mem, head, next_collection, None)` | E | T4/T7 |
| 807 | `try_propagation` | `commit`: `self.active()` | E | T3 |
| 809 | `match` | `commit`: `result` | E | T3 |
| 821 | `try_propagation` | `abort`: `self.active()` | E | T3 |

Classification totals: **C: 6**, **D: 8**, **E: 91**, **I: 6**, **P: 30**. Source dispositions are not a coverage percentage.

## Executed checks and source snapshot

On 2026-11-09 with all features, seven engine unit tests and 33 integration tests
(10 new engine decisions, 12 existing transaction, six retention, five allocation)
passed in debug and release. The final byte-limit additions were rerun in both
profiles. Use a separate target directory to avoid artifact collisions:

```sh
CARGO_TARGET_DIR=target/engine-decisions cargo test --offline -p weekend-db --all-features --lib engine::tests
CARGO_TARGET_DIR=target/engine-decisions cargo test --offline -p weekend-db --all-features --test engine_decisions --test transactions --test retention --test allocation
# Repeat the two commands with --release.
```

The existing durability, native and replication references above identify prior
scoped evidence; they are not all rerun by this tranche. Exact SHA-256 snapshot:

| File | SHA-256 |
|---|---|
| `src/engine.rs` | `a21b678fa55b24275df2f2e921ea91e9f14ee3d104ae864bd2e49f311014b1fe` |
| `tests/engine_decisions.rs` | `05c2f7e1a91c9af684e062f8b7d1838e7849dc9278ba2a0bbbf691a35cbc2219` |
| `tests/transactions.rs` | `f4d6aec6b5f63cea148c64b9a31b89f755848b196fb0e38b542e5d4802cdf112` |
| `tests/retention.rs` | `f55a8c487b6576d61773f47fc91b3839ee0b81d11c6fc7e5cfe0adf9de2945cd` |
| `tests/allocation.rs` | `1fe93186d258e8f5a98d2b87f3a7eb34c9a6ca7e0b067833b940dabbfa939a86` |

This completes the source-record classification and closes the prior arithmetic
gaps, **not** every remaining test or MC/DC obligation: the eight D records remain
open, along with
compiler/macro expansion and dependency decisions outside this denominator.

Arithmetic follow-up: nine engine unit tests plus 29 transaction/allocation/engine/
schedule integration tests passed with all features in debug and release. The
final DAG equality/size-bound assertions were rerun in both profiles. Mutation
results are described in T9. RNG delegation remains a native qualification task.

The final real-DAG transaction case also passed nightly Miri; strict all-feature
Clippy passed for the library and engine integration target.
