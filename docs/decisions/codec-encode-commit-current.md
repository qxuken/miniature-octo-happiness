# COMMIT and retained-catalog encoder decisions

The COMMIT descriptor writes the head revision, current DATA offset, DATA
revision, next retained ID and catalog count. Each retained entry records
its ID, captured revision, DATA offset and DATA revision. The encoder stages
referenced DATA before appending the descriptor, then finalizes both caches
before returning a publication candidate.

The directed byte test checks an empty catalog on a metadata-only head and
a two-entry catalog with gaps in its IDs. One retained entry requires new
DATA staging and the other shares the current DATA offset. The test checks
exact little-endian payload bytes, physical object order, kind, length and
birth revision. Its allocation sweep checks each refusal, the acknowledged
cache entries and live charges, then verifies the successful retry.

The budget test uses cached DATA for every root. A charged buffer sets the
headroom for the listed field, in bytes above the fixture’s live charge.
At those boundaries the geometric reserve can fall back to the exact size.
The second retained entry makes the offset write independently reachable
under the budget. Refusals return Limit and release the draft allocations.
Removing the pressure permits encoding with both acknowledged cache identities.

The simulated-disk retention test sweeps allocation failures through the
public retain operation. Each refusal records zero writes or syncs, preserves
the durable image byte for byte, and keeps the revision, IDs and live charge
at their baseline.
Retry publishes ID 2, and reopening recovers both retained rows.

The existing golden history checks full-image fingerprints, all 64 retained
roots and the compact and backup images. Its two controls substitute the head
revision for the DATA revision or derive the next ID from the live count.
The loop control drops the first retained entry from the encoded payload.
These rows record scoped execution and propagation evidence.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `7b9a71d1a41a71e0cfca7dc61184d247066146f6ceb631a76e042658f8d1967b` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/storage/codec.rs:768:55– | 25659..25660 | try_propagation | encode_with_append | Current DATA staging |
| E | weekend-db::weekend_db | src/storage/codec.rs:770:30– | 25730..25731 | try_propagation | encode_with_append | COMMIT revision, word 1, headroom 0 |
| E | weekend-db::weekend_db | src/storage/codec.rs:771:25– | 25757..25758 | try_propagation | encode_with_append | Current DATA offset, word 2, headroom 8 |
| E | weekend-db::weekend_db | src/storage/codec.rs:772:35– | 25794..25795 | try_propagation | encode_with_append | Current DATA revision, word 3, headroom 24 |
| E | weekend-db::weekend_db | src/storage/codec.rs:773:34– | 25830..25831 | try_propagation | encode_with_append | Next retained ID, word 4, headroom 40 |
| E | weekend-db::weekend_db | src/storage/codec.rs:774:47– | 25879..25880 | try_propagation | encode_with_append | Catalog count, word 5, headroom 48 |
| E | weekend-db::weekend_db | src/storage/codec.rs:775:5– | 25886..25889 | for_iteration | encode_with_append | Empty and populated catalogs, including all 64 retained roots in the golden history |
| E | weekend-db::weekend_db | src/storage/codec.rs:776:76– | 25998..25999 | try_propagation | encode_with_append | Retained DATA staging on a cache miss |
| E | weekend-db::weekend_db | src/storage/codec.rs:777:32– | 26032..26033 | try_propagation | encode_with_append | First retained ID, word 6, headroom 72 |
| E | weekend-db::weekend_db | src/storage/codec.rs:778:43– | 26077..26078 | try_propagation | encode_with_append | First retained revision, word 7, headroom 88 |
| E | weekend-db::weekend_db | src/storage/codec.rs:779:27– | 26106..26107 | try_propagation | encode_with_append | Second retained DATA offset, word 12, headroom 168 |
| E | weekend-db::weekend_db | src/storage/codec.rs:780:48– | 26156..26157 | try_propagation | encode_with_append | First retained DATA revision, word 9, headroom 96 |
| E | weekend-db::weekend_db | src/storage/codec.rs:782:66– | 26230..26231 | try_propagation | encode_with_append | COMMIT object staging |
| E | weekend-db::weekend_db | src/storage/codec.rs:788:41– | 26370..26371 | try_propagation | encode_with_append | VALUE cache finalization |
| E | weekend-db::weekend_db | src/storage/codec.rs:789:51– | 26423..26424 | try_propagation | encode_with_append | DATA cache finalization |
