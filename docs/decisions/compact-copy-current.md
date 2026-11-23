# Compact copy, headroom and verification

The public compact-copy path requires a byte-backed source, an unpoisoned
writer and state lock, and a healthy store. The source is read only for its
physical length. An I/O failure while reading length leaves the source
healthy and does no destination I/O. Retrying the same destination succeeds.

The shared length fixture enumerates valid, short and over-cap values.
For both compaction and headroom, the short and over-cap predicates reject
independently. Rejection poisons the headroom path, while a failed compact
copy leaves the source healthy. Both operations check an existing poison
before querying the medium. Headroom also rounds an unaligned unreachable
tail up to the next region. Above-cap lengths are filtered first, and the
image cap is region-aligned, so rounding an admitted length cannot fail.

## Copy fault and verification matrix

The fixture keeps three retained IDs, including two that share a retained
revision. It compares current and retained rows, revision counters, image
bytes and lease ownership. It faults every destination callback. A positive short read or write
continues with the remaining bytes. The first failing callback stops
further I/O and the destination lease can be reacquired before a simulated
crash. The source remains byte-for-byte the same with its writer usable.

| Destination stage | Error on failure | Reopened destination |
| --- | --- | --- |
| Length, initial metadata, append or append flush | Io | Rejected |
| Root write | OutcomeUnknown | Rejected |
| Root flush | OutcomeUnknown | Complete copy only if the pending root survives the failed flush or crash |
| Witness write or flush | OutcomeUnknown | Complete copy |
| Read-only verification after the final flush | Io for injected I/O refusals | Complete copy |
| All writes, flushes and verification succeed | Copy stats returned | Complete copy |

An early durable append prefix may extend the sparse image past the
minimum length while its header stays volatile. Dropping pending writes
in that case returns UnsupportedFormat. The other incomplete states in
this sweep return Corrupt. Every successful reopen yields the exact image
of an uninterrupted compact copy. The golden identity and DATA histories
exercise compatibility with the current encoder.

Verification starts after destination publication and only reads the
destination. The cache fixture requires both serialization caches to be
released before verification and checks that this preserves the compact
bytes. The envelope fixture varies identity, revision, end and selected
witness independently. All four match in the success case. Each mismatch
alone rejects, and all four case pairs preserve the medium.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/engine.rs` | `c1a9f0d9f416442840397a04b3b5d1ac7b3332b5d6dec98495d346d1420dea6d` |
| `src/storage/mod.rs` | `68916e5c5a0e7003a5e77a047e40f6af8de5c8a569c384e254e26f2851d0b457` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/engine.rs:287:50– | 8786..8787 | try_propagation | compact_copy | Writer mutex poison stops the copy with RecoveryRequired. |
| E | weekend-db::weekend_db | src/engine.rs:289:35– | 8844..8845 | try_propagation | compact_copy | A database without a byte store reports Invalid. |
| E | weekend-db::weekend_db | src/engine.rs:290:78– | 8924..8925 | try_propagation | compact_copy | State lock poison stops the copy before either medium is touched. |
| E | weekend-db::weekend_db | src/engine.rs:297:10– | 9135..9136 | try_propagation | compact_copy | Preserve destination errors before verification. |
| E | weekend-db::weekend_db | src/engine.rs:298:31– | 9168..9169 | try_propagation | compact_copy | A complete publication with a refused verification read still reports its error. |
| E | weekend-db::weekend_db | src/storage/mod.rs:121:21– | 4334..4335 | try_propagation | space | A poisoned store rejects before querying physical length. |
| E | weekend-db::weekend_db | src/storage/mod.rs:122:51– | 4387..4388 | try_propagation | space | Report a length refusal and preserve usability for retry. |
| P | weekend-db::weekend_db | src/storage/mod.rs:123:9– | 4398..4400 | if | space | The lower and upper bounds reject independently and poison. A valid length reports headroom. |
| C | weekend-db::weekend_db | src/storage/mod.rs:123:12– | 4401..4469 | short_circuit_or | space | Same predicates and source range as the preceding pair. |
| I | weekend-db::weekend_db | src/storage/mod.rs:127:62– | 4617..4618 | try_propagation | space | The admitted length is within a region-aligned cap, so rounding cannot fail. |
| E | weekend-db::weekend_db | src/storage/mod.rs:152:21– | 5415..5416 | try_propagation | encode_compact_copy | A poisoned store stops before the first source or destination callback. |
| E | weekend-db::weekend_db | src/storage/mod.rs:153:49– | 5466..5467 | try_propagation | encode_compact_copy | A refused source length avoids destination I/O and allows retry. |
| P | weekend-db::weekend_db | src/storage/mod.rs:154:9– | 5477..5479 | if | encode_compact_copy | Short or over-cap source length independently rejects. A valid source can be copied. |
| C | weekend-db::weekend_db | src/storage/mod.rs:154:12– | 5480..5544 | short_circuit_or | encode_compact_copy | Same predicates and source range as the preceding pair. |
| E | weekend-db::weekend_db | src/storage/mod.rs:164:10– | 5801..5802 | try_propagation | encode_compact_copy | A failed destination publication propagates before independent verification. |
| E | weekend-db::weekend_db | src/storage/mod.rs:181:98– | 6414..6415 | try_propagation | create_compact | A failed fresh-image creation stops the copy. |
| E | weekend-db::weekend_db | src/storage/mod.rs:191:63– | 6766..6767 | try_propagation | verify | The destination recovery error propagates after completed publication. |
| P | weekend-db::weekend_db | src/storage/mod.rs:192:9– | 6777..6779 | if | verify | Independently vary the identity, revision, committed end and witness of the selected root. |
| C | weekend-db::weekend_db | src/storage/mod.rs:192:12– | 6780..6869 | short_circuit_or | verify | The identity and revision prefix is exercised by the envelope pairs. |
| C | weekend-db::weekend_db | src/storage/mod.rs:192:12– | 6780..6914 | short_circuit_or | verify | The identity, revision and end prefix is exercised by the envelope pairs. |
| C | weekend-db::weekend_db | src/storage/mod.rs:192:12– | 6780..6949 | short_circuit_or | verify | Same predicates and source range as the envelope pair. |

## Verification at fa87e4b

- Rust 1.100.0 passed 242 tests per profile on macOS arm64 and 250 per
  profile on Docker Linux arm64 with ext4. Both golden histories passed in
  every profile. The frozen source was
  `fa87e4b4667c43290d99cd2d23ea310f1301d02a`.
- Both macOS profiles detected all 22 controls. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-compact-copy-` and
  repeat with `--release`. The first candidate exposed a missed guard
  because the public API already rejects poison before calling the private
  headroom method. The direct fixture exercises that guard and confirms
  healthy length and poisoned early return.
- Three focused Miri tests, strict macOS and Windows MSVC Clippy, and
  ten mutation-runner tests passed.
- The inventory maps 21 additional source records. It reports 10,581
  decisions and 9,592 open records, including 2,222 runtime.
