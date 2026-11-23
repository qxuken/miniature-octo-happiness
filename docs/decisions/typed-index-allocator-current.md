# Allocator routing for typed index maintenance

The regression observed eight calls through Global during a read-only index
check with a selected allocator. That operation constructed application-output
vectors for both scans and allocated the expected-key vector through Global.
On the new path, checking, rebuilding and checking the draft again each use
the selected database allocator. The regression observes zero Global calls
for the checked read and for the repair closure.

## Ownership and limits

`Snapshot::scan_iter` shares the owned, binary-ordered scan path with the
transaction iterator. It owns the buffers independently of the snapshot and
the database handle. The core test refuses Global requests, confirms that
live bytes increase while holding the scan and reads the exact binary keys
after dropping both handles. The allocation sweep verifies that successful
and refused scans return to the same provider bytes and live charge on drop.

Index maintenance uses both iterators. It consumes primary rows, then moves
the matching index keys from the already ordered shared-collection scan into
selected-allocator scratch. Shared-collection observation still applies for
transaction conflict detection. Matching keys retain their charges until the
repair state is dropped. The expected and stored scratch vectors use the
selected allocator outside the live-byte cap. This can make tight live-byte
limits reject index maintenance earlier. The transaction-byte scan preflight
still applies to the full scanned collections.

| Case | Observation |
| --- | --- |
| Matching and foreign table prefixes | The repair state contains only entries for the selected table. The other table's claim stays readable. |
| Missing and orphaned claims | Check returns one of each. Repair stages exactly the needed index changes while the primary rows stay identical. The retained view still reports the earlier discrepancies. |
| Refusal when reserving the stored-entry vector | `OutOfMemory` closes the repair draft. The primary and index rows preserve their pre-repair state. |
| Refusal during snapshot scan construction | `OutOfMemory` reaches the caller and provisional allocations and charges are released. The snapshot revision and records remain readable. |

The seven named controls exercise the two scratch allocator choices, both
scan routes, the table filter, and both scoped error propagations.

## Source binding

| Source | SHA-256 |
| --- | --- |
| `src/engine.rs` | `fe60c081c3e844455b4d8c2761a5459d67781763b6487e08e3f21c81dd6c64d9` |
| `crates/weekend-db-typed/src/store.rs` | `1ba12c605448cd6fbf5b718f6920f189ec334f0d5d0e054603256d626f05d217` |

The three new named source controls bind the prefix condition and the two
error propagations. The shared index scan visits matching and foreign keys. The repair state takes
ownership of the matching keys directly. Source movement in
the typed crate shifts the prior unique-claim selector locations while
preserving their predicates and byte-identical function body.

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:577:5– | 18874..18877 | for_iteration | index_state | The shared-collection scan visits both matching and other table entries and terminates. |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:913:16– | 31732..31735 | pattern_test | put_raw | Existing-claim pattern evidence from the unique-claim ledger. |

The new snapshot iterator follows the original engine decisions below.
Their original source slices and enclosing function bodies match the prior
ledger source byte for byte. Their earlier observation/scan and
assignment/commit evidence carries forward at the recorded scope.

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/engine.rs:771:9– | 26918..26920 | if | observe | An empty observation list misses. An existing identical point is reused at capacity. |
| I | weekend-db::weekend_db | src/engine.rs:784:37– | 27368..27369 | try_propagation | observe | A valid Rust byte slice has at most isize::MAX bytes. Adding the fixed Point layout fits usize on supported ABIs. |
| C | weekend-db::weekend_db | src/engine.rs:799:9– | 27795..27797 | if | get | The enclosing if shares the overlay lookup pattern decision. |
| I | weekend-db::weekend_db | src/engine.rs:806:38– | 28021..28022 | try_propagation | get | A successful observation resolved the same immutable base collection. |
| C | weekend-db::weekend_db | src/engine.rs:826:9– | 28771..28773 | if | assign | The if shares the existing-write pattern outcome. |
| I | weekend-db::weekend_db | src/engine.rs:836:37– | 29175..29176 | try_propagation | assign | Draft bytes include the old weight. The remainder and new weight are each at most isize::MAX. |
| I | weekend-db::weekend_db | src/engine.rs:878:53– | 30763..30764 | try_propagation | scan_records | The preceding collection observation resolved the same immutable base. |
| E | weekend-db::weekend_db | src/engine.rs:879:9– | 30790..30793 | for_iteration | scan_records | Empty and mixed overlays use the base and both write passes. The BTreeMap checks ordered results at capacity. |
| E | weekend-db::weekend_db | src/engine.rs:889:23– | 31105..31110 | match | scan_records | Mixed overlays remove and insert. Deleting before inserting admits the full collection fixture. |
| E | weekend-db::weekend_db | src/engine.rs:910:9– | 31821..31824 | for_iteration | validate | Empty, matching and changed collection dependency sets exercise the loop. |
| E | weekend-db::weekend_db | src/engine.rs:915:9– | 32019..32022 | for_iteration | validate | Empty, matching, modified and removed point dependencies exercise the loop. |
| E | weekend-db::weekend_db | src/engine.rs:934:9– | 32824..32827 | for_iteration | publish | Empty and mixed writes exercise the loop. Deleting before inserting allows replacement at capacity. |
| I | weekend-db::weekend_db | src/engine.rs:942:40– | 33128..33129 | try_propagation | publish | Validation under the writer lease has checked the dependency for every write. The loop never removes a collection. |
| E | weekend-db::weekend_db | src/engine.rs:944:25– | 33180..33185 | match | publish | The durable fixture replaces values and removes an existing record. The retained snapshot keeps the original row. |
| E | weekend-db::weekend_db | src/engine.rs:977:22– | 34431..34436 | match | commit | Success becomes Committed. Conflict and Io become Aborted. Preparation failure and OutcomeUnknown become Failed. |

## Verification

Frozen source `2ea387df4dd68f24223d0c48ec356c1201081228` used Rust 1.100.0.
Both profiles passed the typed package suite and the core library,
allocation, allocator-routing, engine decision and retention suites.
All seven controls failed their intended tests in both profiles on both
hosts. Clippy passed on both hosts and for the Windows MSVC cross target.
Strict-provenance Miri `708c94ef0` passed both new regression fixtures.

| Host | Typed tests per profile | Selected core tests per profile |
| --- | ---: | ---: |
| macOS arm64 | 64 | 255 |
| Docker Linux aarch64 on ext4 | 64 | 265 |

The typed package retains one existing ignored doctest. The added public
snapshot iterator reuses the existing scan budget and order. Earlier
transaction iterator evidence is pinned to its own source occurrence.
The four new inventory records have scoped dispositions. The queue totals
9,365 open records. Native Windows execution remains open.

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db-typed
cargo +1.100.0 test --offline --locked --all-features -p weekend-db \
  --lib --test allocation --test allocator_routing --test engine_decisions --test retention
cargo +1.100.0 xtask mutations --case r1-typed-index-allocator- \
  --toolchain 1.100.0 --output target/r1/typed-index-allocator-debug
```

Repeat with `--release` and a separate mutation output directory.
