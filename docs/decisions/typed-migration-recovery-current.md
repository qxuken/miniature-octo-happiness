# Typed migration admission and interrupted publication

Selected-ID migration preserves caller order and stages row changes, unique
claims and the application's progress row in the same transaction. The test
uses a version-2 catalog and two selected version-1 rows. The third row is
outside the batch. A retained view captures the version-1 schema and the
original rows, including an application-unknown field.

## Admission and rollback cases

| Decision | Independent inputs and outcome |
| --- | --- |
| Batch exceeds write or observation limit | The same 17 IDs pass admission with both limits at 1,024. Lowering either limit alone to 16 returns `Limit` before conversion. |
| Batch contains zero or duplicates | An empty batch passes admission and returns zero. At the same call site, `[1, 0]` and `[1, 1]` return `InvalidId` before any callback. The first sequence detects rejection deferred until lookup of ID zero. |
| ID scratch reservation fails | A selected allocator refuses the first request after a progress update was staged. The call returns `OutOfMemory` before conversion. Catching it still closes the write and rolls back progress. A new write can retry. |
| Catalog version does not match the requested model | Both migration APIs reject the older model with `UnsupportedVersion`, before its callback. The current catalog reaches row validation. |
| Stored row is ahead of the target | Version 3 rejects against a version-2 target before conversion. Version 1 reaches the callback. |
| Stored row already matches the target | Version 2 skips conversion. Version 1 invokes it once. |
| Selected row is absent | A valid first ID can stage its update. A missing second row returns `NotFound` and rolls back the earlier update and progress. |
| Conversion or unique claim fails | A callback error and a competing new claim keep their respective statuses. Neither publishes the prior staged progress or migration. |

The reversed input `[2, 1]` invokes the callback in that order. The sorted
copy serves admission and duplicate detection. The empty list returns zero.
The thirteen `r1-typed-migration-` controls exercise these cases independently.

## Publication and resume matrix

The uninterrupted baseline migrates IDs `2, 1`, changes both unique slugs
and writes progress 2. The test confirms both new claims and compares exact
catalog, schema, primary, index and progress rows. Each interrupted run
begins from the same durable image and refuses one I/O call in the successful
trace. The trace prefix must match exactly through the refused call. The
failed process's current view still has revision 12 and requires recovery.

| Failed call | API status | Reopens with `DropAll` | Reopens with `PersistAll` |
| --- | --- | ---: | ---: |
| Length, append writes or append sync, steps 0–26 | `Io` | 12 | 12 |
| Root write, step 27 | `OutcomeUnknown` | 12 | 12 |
| Root sync, step 28 | `OutcomeUnknown` | 12 | 13 |
| Witness write or sync, steps 29–30 | `OutcomeUnknown` | 13 | 13 |

All 62 cases compare the exact rows for the selected revision. Revision 12
still resolves the old slugs and progress 0. Revision 13 resolves the new
slugs and progress 2. Both preserve the unknown field, the unselected third
row and the earlier retained view. Retrying the old root converts both IDs.
Retrying the new root skips both callbacks and preserves its rows. This matrix
uses simulated I/O refusal and the two stated persistence policies.

## Source binding

| Source | SHA-256 |
| --- | --- |
| `crates/weekend-db-typed/src/store.rs` | `1ba12c605448cd6fbf5b718f6920f189ec334f0d5d0e054603256d626f05d217` |

The manifest binds ten new named source controls. The two OR expressions
share the evidence of their enclosing decisions. The loop row records scoped
execution and the input-order control.

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:846:13– | 29453..29456 | for_iteration | migrate_ids | Empty, resumed and two-item batches visit the expected IDs in caller order. |

## Verification

Frozen source `7fcd99265ae0012a64f00e3f82bbf69ef7d07f16` used Rust 1.100.0.
On macOS arm64 and Docker Linux aarch64/ext4, debug and release each passed
66 typed tests with one existing ignored doctest. The publication fixture
verified the same 62 cases on each profile and host. All thirteen controls
failed their intended tests. Clippy passed on both hosts and for the Windows
MSVC cross target. Strict-provenance Miri `708c94ef0` passed the ID scratch
refusal fixture on macOS.

The same-object compiler condition reports used nightly `708c94ef0`, with
independent runtime profiles for the two selected tests. Each operand pair
comes from one instantiation of `migrate_ids`. Matching LLVM tools produced
the reports with empty stderr.

| Fixture | Condition | Source position | True | False |
| --- | --- | --- | ---: | ---: |
| `migration_batch_limits_independently_prevent_callbacks` | Write limit exceeded | 825:16 | 1 | 2 |
| Same fixture | Observation limit exceeded | 825:49 | 1 | 1 |
| `migration_batches_reject_invalid_ids_and_roll_back_callbacks_and_progress` | Zero ID present | 832:16 | 2 | 2 |
| Same fixture | Duplicate ID present | 832:46 | 1 | 1 |

The compiler rejected the `coverage-options=mcdc` flag. The table reports
condition counts from `coverage-options=condition`. The directed inputs and
isolated mutations supply the source-level independence evidence.

The inventory has 9,352 open records (1,969 runtime, 26 driver and 7,357
tooling). Native Windows execution and release-version and device
qualification retain their separate gates.

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db-typed
cargo +1.100.0 test --offline --locked --all-features -p weekend-db-typed \
  --test models interrupted_migration_batches_reopen_records_claims_and_progress_together \
  -- --exact --nocapture
cargo +1.100.0 xtask mutations --case r1-typed-migration- \
  --toolchain 1.100.0 --output target/r1/typed-migration-debug
```

Repeat with `--release` and a separate mutation output directory.

For the condition reports, compile the `models` and `runtime_decisions`
integration tests with `-C instrument-coverage -Z coverage-options=condition`
in `RUSTFLAGS` and `--no-run --message-format=json`. Use the executable from
the matching `compiler-artifact` entry. Run one listed fixture per executable
with `--exact` and its own `LLVM_PROFILE_FILE`, then use matching
`llvm-profdata merge -sparse` and `llvm-cov show --show-branches=count
--show-instantiations` against that executable alone.
