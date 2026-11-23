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
The caller's exact failure propagation at the helper boundary remains open.

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

The source rows comprise 8 directed pairs, 20 execution
or propagation rows, 2 invariant rows, 1 coupled row
and one open row. The inventory reconciles 10,582 decisions, with 31 fewer
open records. The remaining 9,801 open records include 2,431 runtime
records. The tree and value decisions and helper-caller propagation remain
in batch 4. Native publication
barriers and lower storage wrappers retain their own batch 5 scope.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/engine.rs` | `17c2cb6848cb59efd25829a5bd228b01d8230863b0be83a2a05efad08bcd2008` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/engine.rs:741:69– | 25735..25736 | try_propagation | charge | A real shared DAG reaches aggregate usize overflow under a 1 MiB live cap. The old write remains usable. |
| P | weekend-db::weekend_db | src/engine.rs:816:9– | 28382..28384 | if | put | Local values are assigned. A value from a different database returns ForeignStore. |
| E | weekend-db::weekend_db | src/engine.rs:825:28– | 28756..28757 | try_propagation | assign | Assign propagates Closed before staging a write. |
| C | weekend-db::weekend_db | src/engine.rs:827:9– | 28829..28831 | if | assign | The if shares the existing-write pattern outcome. |
| P | weekend-db::weekend_db | src/engine.rs:827:12– | 28832..28835 | pattern_test | assign | An existing write can be replaced at capacity. A distinct write needs a slot. |
| P | weekend-db::weekend_db | src/engine.rs:830:27– | 28921..28964 | short_circuit_and | assign | The same collection and key reuse the write. Each individual mismatch consumes a new slot. |
| I | weekend-db::weekend_db | src/engine.rs:837:37– | 29233..29234 | try_propagation | assign | Draft bytes include the old weight. The remainder and new weight are each at most isize::MAX. |
| P | weekend-db::weekend_db | src/engine.rs:838:13– | 29248..29250 | if | assign | An excessive replacement keeps the previous overlay. A smaller one succeeds. |
| P | weekend-db::weekend_db | src/engine.rs:845:9– | 29479..29481 | if | assign | The first write fits and a distinct second write exceeds capacity. |
| G | weekend-db::weekend_db | src/engine.rs:848:50– | 29630..29631 | try_propagation | assign | Direct helper tests reach both arithmetic guards. The caller residual remains open for a legal input or reviewed invariant. |
| E | weekend-db::weekend_db | src/engine.rs:849:39– | 29671..29672 | try_propagation | assign | A new oversized write propagates the logical budget error and leaves the key absent from the overlay. |
| E | weekend-db::weekend_db | src/engine.rs:850:36– | 29709..29710 | try_propagation | assign | Refusing the write key keeps the completed point observation and the previous write. |
| E | weekend-db::weekend_db | src/engine.rs:851:49– | 29760..29761 | try_propagation | assign | Refusing the write vector also releases the already copied write key. |
| E | weekend-db::weekend_db | src/engine.rs:911:9– | 31880..31883 | for_iteration | validate | Empty, matching and changed collection dependency sets exercise the loop. |
| P | weekend-db::weekend_db | src/engine.rs:912:13– | 31934..31936 | if | validate | An unrelated change validates. A changed or removed observed collection conflicts. |
| E | weekend-db::weekend_db | src/engine.rs:916:9– | 32078..32081 | for_iteration | validate | Empty, matching, modified and removed point dependencies exercise the loop. |
| P | weekend-db::weekend_db | src/engine.rs:921:13– | 32286..32288 | if | validate | The same revision validates. Replacing or removing the observed point conflicts. |
| E | weekend-db::weekend_db | src/engine.rs:930:54– | 32653..32654 | try_propagation | publish | A durable failure poisons the writer. The next commit propagates RecoveryRequired. |
| E | weekend-db::weekend_db | src/engine.rs:932:35– | 32730..32731 | try_propagation | publish | A stale observed record or collection returns Conflict before publication. |
| E | weekend-db::weekend_db | src/engine.rs:933:78– | 32810..32811 | try_propagation | publish | The last revision is usable. The next commit returns Exhausted before applying edits. |
| E | weekend-db::weekend_db | src/engine.rs:935:9– | 32883..32886 | for_iteration | publish | Empty and mixed writes exercise the loop. Deleting before inserting allows replacement at capacity. |
| I | weekend-db::weekend_db | src/engine.rs:943:40– | 33187..33188 | try_propagation | publish | Validation under the writer lease has checked the dependency for every write. The loop never removes a collection. |
| E | weekend-db::weekend_db | src/engine.rs:945:25– | 33239..33244 | match | publish | The durable fixture replaces values and removes an existing record. The retained snapshot keeps the original row. |
| E | weekend-db::weekend_db | src/engine.rs:953:18– | 33519..33520 | try_propagation | publish | Refusing a record path copy leaves current and retained roots intact before any publication I/O. |
| E | weekend-db::weekend_db | src/engine.rs:954:63– | 33584..33585 | try_propagation | publish | Refusing a deletion path copy leaves current and retained roots intact before any publication I/O. |
| E | weekend-db::weekend_db | src/engine.rs:957:91– | 33777..33778 | try_propagation | publish | Refusing the updated collection path drops the private record tree. |
| P | weekend-db::weekend_db | src/engine.rs:959:29– | 33818..33820 | if | publish | A read-only commit advances publication revision. A nonempty edit advances the data revision as well. |
| E | weekend-db::weekend_db | src/engine.rs:968:11– | 34063..34064 | try_propagation | publish | Refusing the assembled head drops all private edits before publication. |
| E | weekend-db::weekend_db | src/engine.rs:970:57– | 34175..34176 | try_propagation | publish | Durable candidate encoding refusal returns OutOfMemory before writes or syncs. |
| E | weekend-db::weekend_db | src/engine.rs:976:22– | 34429..34430 | try_propagation | commit | A second commit or a commit after abort returns Closed. |
| E | weekend-db::weekend_db | src/engine.rs:978:22– | 34490..34495 | match | commit | Success becomes Committed. Conflict and Io become Aborted. Preparation failure and OutcomeUnknown become Failed. |
| E | weekend-db::weekend_db | src/engine.rs:990:22– | 34891..34892 | try_propagation | abort | A second abort or abort after completion returns Closed. |
