# Transaction assignment and commit preparation decisions

The assignment sweep stages one edit and then refuses each request for a
second edit. The first two requests capture the second point. The next
two copy its key and grow the write vector. Allowing all four requests
succeeds. A refusal preserves the first write and leaves the draft active.
Only a completed point observation stays charged and influences a later
conflict. The test checks draft bytes, provider bytes, live budget and
both published and pinned snapshot values.

An existing write can be replaced at the write cap while the provider
refuses every request. The identity controls separate collection and key
mismatches. Replacement computes the logical charge by subtracting the
old weight and adding the new one. The old weight is already included in
draft bytes, and both the remainder and new weight are bounded by
isize::MAX. An excessive replacement preserves the existing charge.

The existing shared-DAG fixture reaches real aggregate transaction charge
overflow under a 1 MiB live cap. The two standalone write-charge arithmetic
controls also detect wrapping for the input sum and then the fixed overhead.
The first helper case uses out-of-domain arithmetic inputs deliberately.
The [write-charge bound](write-charge-current.md) removes the earlier helper
propagation site and records the corresponding arithmetic controls.

The durable sweep uses the prior two replacements and deletes an existing
record that remains visible in the retained snapshot. It refuses each
preparation allocation through candidate encoding. Every failure reaches
Failed, preserves the durable image and both roots, and issues zero writes
or syncs. Dropping and reopening checks the same current and retained rows.
The completed attempt commits the new values and removes the record from
the current snapshot. Existing I/O tests verify the separate Io and
OutcomeUnknown transitions.

## Verification at c1d2e58

- Rust 1.100.0 debug and release passed 231 tests per profile on macOS arm64
  and 238 on Docker Linux arm64/ext4. This covers core, assignment, retention,
  transaction and durability tests, including both golden histories.
- Three directed assignment and arithmetic tests passed under Miri.
  Strict Clippy passed on macOS and for Windows MSVC. Ten mutation-runner
  tests passed.
- Both macOS profiles detected all 33 controls. The ten
  clean-source reports verify 292 command retirements. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-assign-commit-
  --case r1-engine-validation- --case r1-alloc-commit-preparation` and repeat
  with `--release`. This includes 30 new changes and three existing controls.

At this checkpoint the 31 dispositions left 9,801 open records, including
2,431 runtime records. The current table keeps 31 rows with source spans
rebound to the bounded write charge. The old helper propagation row was
retired with its source operation.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/engine.rs` | `c1a9f0d9f416442840397a04b3b5d1ac7b3332b5d6dec98495d346d1420dea6d` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/engine.rs:740:69– | 25677..25678 | try_propagation | charge | A real shared DAG reaches aggregate usize overflow under a 1 MiB live cap. The old write remains usable. |
| P | weekend-db::weekend_db | src/engine.rs:815:9– | 28324..28326 | if | put | Local values are assigned. A value from a different database returns ForeignStore. |
| E | weekend-db::weekend_db | src/engine.rs:824:28– | 28698..28699 | try_propagation | assign | Assign propagates Closed before staging a write. |
| C | weekend-db::weekend_db | src/engine.rs:826:9– | 28771..28773 | if | assign | The if shares the existing-write pattern outcome. |
| P | weekend-db::weekend_db | src/engine.rs:826:12– | 28774..28777 | pattern_test | assign | An existing write can be replaced at capacity. A distinct write needs a slot. |
| P | weekend-db::weekend_db | src/engine.rs:829:27– | 28863..28906 | short_circuit_and | assign | The same collection and key reuse the write. Each individual mismatch consumes a new slot. |
| I | weekend-db::weekend_db | src/engine.rs:836:37– | 29175..29176 | try_propagation | assign | Draft bytes include the old weight. The remainder and new weight are each at most isize::MAX. |
| P | weekend-db::weekend_db | src/engine.rs:837:13– | 29190..29192 | if | assign | An excessive replacement keeps the previous overlay. A smaller one succeeds. |
| P | weekend-db::weekend_db | src/engine.rs:844:9– | 29421..29423 | if | assign | The first write fits and a distinct second write exceeds capacity. |
| E | weekend-db::weekend_db | src/engine.rs:848:39– | 29612..29613 | try_propagation | assign | A new oversized write propagates the logical budget error and leaves the key absent from the overlay. |
| E | weekend-db::weekend_db | src/engine.rs:849:36– | 29650..29651 | try_propagation | assign | Refusing the write key keeps the completed point observation and the previous write. |
| E | weekend-db::weekend_db | src/engine.rs:850:49– | 29701..29702 | try_propagation | assign | Refusing the write vector also releases the already copied write key. |
| E | weekend-db::weekend_db | src/engine.rs:910:9– | 31821..31824 | for_iteration | validate | Empty, matching and changed collection dependency sets exercise the loop. |
| P | weekend-db::weekend_db | src/engine.rs:911:13– | 31875..31877 | if | validate | An unrelated change validates. A changed or removed observed collection conflicts. |
| E | weekend-db::weekend_db | src/engine.rs:915:9– | 32019..32022 | for_iteration | validate | Empty, matching, modified and removed point dependencies exercise the loop. |
| P | weekend-db::weekend_db | src/engine.rs:920:13– | 32227..32229 | if | validate | The same revision validates. Replacing or removing the observed point conflicts. |
| E | weekend-db::weekend_db | src/engine.rs:929:54– | 32594..32595 | try_propagation | publish | A durable failure poisons the writer. The next commit propagates RecoveryRequired. |
| E | weekend-db::weekend_db | src/engine.rs:931:35– | 32671..32672 | try_propagation | publish | A stale observed record or collection returns Conflict before publication. |
| E | weekend-db::weekend_db | src/engine.rs:932:78– | 32751..32752 | try_propagation | publish | The last revision is usable. The next commit returns Exhausted before applying edits. |
| E | weekend-db::weekend_db | src/engine.rs:934:9– | 32824..32827 | for_iteration | publish | Empty and mixed writes exercise the loop. Deleting before inserting allows replacement at capacity. |
| I | weekend-db::weekend_db | src/engine.rs:942:40– | 33128..33129 | try_propagation | publish | Validation under the writer lease has checked the dependency for every write. The loop never removes a collection. |
| E | weekend-db::weekend_db | src/engine.rs:944:25– | 33180..33185 | match | publish | The durable fixture replaces values and removes an existing record. The retained snapshot keeps the original row. |
| E | weekend-db::weekend_db | src/engine.rs:952:18– | 33460..33461 | try_propagation | publish | Refusing a record path copy leaves current and retained roots intact before any publication I/O. |
| E | weekend-db::weekend_db | src/engine.rs:953:63– | 33525..33526 | try_propagation | publish | Refusing a deletion path copy leaves current and retained roots intact before any publication I/O. |
| E | weekend-db::weekend_db | src/engine.rs:956:91– | 33718..33719 | try_propagation | publish | Refusing the updated collection path drops the private record tree. |
| P | weekend-db::weekend_db | src/engine.rs:958:29– | 33759..33761 | if | publish | A read-only commit advances publication revision. A nonempty edit advances the data revision as well. |
| E | weekend-db::weekend_db | src/engine.rs:967:11– | 34004..34005 | try_propagation | publish | Refusing the assembled head drops all private edits before publication. |
| E | weekend-db::weekend_db | src/engine.rs:969:57– | 34116..34117 | try_propagation | publish | Durable candidate encoding refusal returns OutOfMemory before writes or syncs. |
| E | weekend-db::weekend_db | src/engine.rs:975:22– | 34370..34371 | try_propagation | commit | A second commit or a commit after abort returns Closed. |
| E | weekend-db::weekend_db | src/engine.rs:977:22– | 34431..34436 | match | commit | Success becomes Committed. Conflict and Io become Aborted. Preparation failure and OutcomeUnknown become Failed. |
| E | weekend-db::weekend_db | src/engine.rs:989:22– | 34832..34833 | try_propagation | abort | A second abort or abort after completion returns Closed. |
