# Current codec arithmetic and cursor decisions

This R1 batch starts at `edec275384c7cf1d13cf9e0a05d9933f646cc436`.
It reviews alignment and the cursor used by durable-image decoding. Production
codec behavior and serialized bytes are unchanged. The historical
[codec ledger](codec.md) retains its original source versions; its rows are not
silently repinned to the current file.

## Directed decisions and propagation scope

`storage::codec::tests::alignment_and_cursor_boundaries_preserve_failed_take_position`
supplies the existing controls. The new
`cursor_zero_limits_and_short_tags_preserve_the_expected_position` test adds
empty input, every 0–7-byte integer prefix through both count and key readers,
zero limits and values, consumed-prefix state on Limit, and the largest integer
on qualified 64-bit ABIs.

| Decision | Condition | False control | True control | Required result |
|---|---|---|---|---|
| CODEC-ALIGN-CAP | `value > MAX_IMAGE_BYTES` | align(cap) | align(cap+1) | Exact cap succeeds; rounded result above cap is Limit |
| CODEC-COUNT-MAX | `n > maximum` | count(3,3), count(0,0) | count(4,3), count(1,0) | Equality succeeds; excess returns Limit after consuming the valid tag |
| CODEC-CURSOR-DONE | `pos == bytes.len()` | Fresh nonempty cursor | Empty cursor or fully consumed cursor | Unconsumed bytes return Corrupt; exact completion succeeds |

Each predicate has one condition and both outcomes. The three tables are
directed source-level pairs, not compiler-measured MC/DC. Existing tests also
cover alignment overflow, zero/exact-region rounding, successful and failed
take, short integer reads and key limits/truncation. All failed `take` calls
preserve position. Count/key operations consume a valid length tag before a
later limit or payload failure; whole-cursor rollback is not their contract.

The five E rows below identify actual propagation sites and their scoped
success/error controls. They do not imply every transitive std or caller branch
was independently varied. Cursor's checked addition and `Option::filter`
predicate are delegated primitives; their overflow/bounds outcomes are tested
but are not additional compiler-verified predicates in this AST denominator.
The `usize::try_from(u64)` site is D: the largest value converts on the qualified
64-bit targets, while unsupported narrower representations retain a separate
portability obligation. No fabricated slices or invalid raw values are used.

The checksum-table G row is included deliberately as an unresolved example.
A historical G row must remain an individual gap; it cannot become evidence
merely by acquiring an exact source pin. A reconciler regression rejects the
previous promotion, and a guard mutation restores that bug to demonstrate
detection. Independently reviewed non-gap evidence can still qualify a record.

## Mutation controls

Reuse the existing bounded runner and nonempty baselines. The new controls
alter alignment overflow/cap handling, take equality and propagation, integer
and count propagation, count equality, key propagation, cursor completion,
and historical-G classification. Every detected mutant must compile and fail
an actual test with Cargo exit 101; timeouts, compiler errors and cleanup
failures cannot pass. Sources must be restored after every attempt.

```sh
cargo +1.99.1 test --offline --locked --all-features -p weekend-db --lib storage::codec::tests::alignment_and_cursor_boundaries_preserve_failed_take_position -- --exact
cargo +1.99.1 test --offline --locked --all-features -p weekend-db --lib storage::codec::tests::cursor_zero_limits_and_short_tags_preserve_the_expected_position -- --exact
cargo +1.99.1 xtask mutations --case r1-codec- --case r1-inventory-historical-gap --toolchain 1.99.1 --output target/codec-arithmetic-debug
cargo +1.99.1 xtask mutations --case r1-codec- --case r1-inventory-historical-gap --release --toolchain 1.99.1 --output target/codec-arithmetic-release
```

The remaining codec records stay in the individual G queue. The source table
below binds only this batch; it does not claim whole-codec qualification or
close compiler, generated-code, native or release gates. Platform and frozen
test results are added after their actual completion.

## Current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `34baf455a1e1ad7e8d76c932ed417fe60c324bbeaad64efd0ca398205012472d` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| G | weekend-db::weekend_db | src/storage/codec.rs:40:5– | 1217..1222 | while | crc_tables | Checksum table generation remains outside this batch |
| E | weekend-db::weekend_db | src/storage/codec.rs:95:29– | 3219..3220 | try_propagation | align | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| P | weekend-db::weekend_db | src/storage/codec.rs:96:5– | 3226..3228 | if | align | Named directed source pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:806:35– | 26797..26798 | try_propagation | take | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| E | weekend-db::weekend_db | src/storage/codec.rs:812:43– | 26973..26974 | try_propagation | int | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| E | weekend-db::weekend_db | src/storage/codec.rs:815:43– | 27104..27105 | try_propagation | count | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| D | weekend-db::weekend_db | src/storage/codec.rs:815:73– | 27134..27135 | try_propagation | count | Qualified 64-bit conversion; narrower ABI is delegated |
| P | weekend-db::weekend_db | src/storage/codec.rs:816:9– | 27145..27147 | if | count | Named directed source pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:823:36– | 27338..27339 | try_propagation | key | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| P | weekend-db::weekend_db | src/storage/codec.rs:827:9– | 27411..27413 | if | done | Named directed source pair |
