# Recovery under simultaneous memory and I/O faults

The commit-crash model can arm an I/O fault and cap the live bytes of the
same writable reopen. It measures the read prefix of a budget refusal on a
separate copy. If the scheduled I/O lies in that prefix, the attempt must
return `Io`. Otherwise it must return `Limit` before stabilization. After
each crash the oracle verifies the exact durable bytes, selected revision and
witness. The final open checks both current and retained values, and a further
acknowledged commit must survive another crash.

The native storage test arms the selected allocator and disk independently.
Three successive failed opens on one unwitnessed image exercise an early
read error, allocation refusal during cache construction, and a stabilization
sync error. Each attempt must release the lease and all charged bytes. The
final open restores the original witness and exposes current revision 4 and
retained revision 2.

## Regression and verification

An initial AddressSanitizer run at `49d5757` found a 15-byte input that also
failed the stable replay. The oracle had assumed that one byte below the
unrestricted peak would always reject. Geometric growth can retry at exact
capacity. The corrected oracle verifies a successful capped probe and halves
the cap on a fresh copy until it observes a real refusal. The discovered input
is checked in as `fuzz/seeds/commit_crash/recovery-budget-exact-growth`.

At `34406b2`, all 33 fuzz-library tests pass in debug and release on arm64
macOS and Docker Linux on ext4. The directed replay covers two slot parities,
five I/O positions and four combinations of capped attempts. Strict
fuzz-library Clippy passes on both hosts. At `49d5757` the native storage
fixture and five named mutation controls passed in both profiles on both
hosts. The subsequent oracle correction changed the fuzz workspace and its
inventory bindings.

On the clean corrected commit, macOS AddressSanitizer completed 44,040
commit-crash executions in 121 seconds with no reported finding. The run
used cargo-fuzz 0.13.2, nightly-2026-11-12, seed 4939 and a 41-byte input cap.
The campaign covered the simulator and the generated retained histories.

```sh
cargo +1.100.0 test --offline --locked --manifest-path fuzz/Cargo.toml --lib
cargo +1.100.0 xtask fuzz --target commit_crash --toolchain nightly-2026-11-12 \
  --seed 4939 --max-len 41 --seconds 120 --runs 100000 --output target/compound-recovery
```

## Storage source evidence

At `34406b2`, the change to `src/storage/mod.rs` relative to `e40e0d9`
adds assertions inside the existing test after its original sweep. The six
records below retain the original source positions, conditions and evidence
from the [publication](bytestore-publication-current.md),
[compact-copy](compact-copy-current.md) and
[creation](storage-creation-reopen-current.md) ledgers.

| Source | SHA-256 |
| --- | --- |
| `src/storage/mod.rs` | `c7df5222e6d03401a600c5e624c36e3abc52e5a8341adf87d04af18ad66ef08f` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/mod.rs:303:22– | 10741..10746 | match | persist | Length success continues; refusal poisons with Io before append. |
| I | weekend-db::weekend_db | src/storage/mod.rs:314:41– | 11111..11112 | try_propagation | persist | An admitted length rounds up within the aligned cap and cannot overflow. |
| I | weekend-db::weekend_db | src/storage/mod.rs:135:62– | 4799..4800 | try_propagation | space | The admitted length is within a region-aligned cap, so rounding cannot fail. |
| C | weekend-db::weekend_db | src/storage/mod.rs:200:12– | 6962..7051 | short_circuit_or | verify | The identity and revision prefix is exercised by the envelope pairs. |
| C | weekend-db::weekend_db | src/storage/mod.rs:200:12– | 6962..7096 | short_circuit_or | verify | The identity, revision and end prefix is exercised by the envelope pairs. |
| E | weekend-db::weekend_db | src/storage/mod.rs:243:9– | 8449..8452 | for_iteration | create | On success write all four zero regions in order; each failure stops at its own callback. |
