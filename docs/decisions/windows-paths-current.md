# Windows pathname preparation

The Windows adapter validates the original UTF-16 input, prepares and opens
the parent, reads the canonical parent from its handle, and validates the
canonical drive and volume before opening the final leaf. Create and reopen
share this preparation. The selected allocator owns the buffers throughout.

The adapter enforces a cap of 32,767 UTF-16 units including NUL. Both the normalized
parent with its added prefix and the canonical parent with its appended
leaf must fit the cap. The append check counts a separator only when one is
needed. An oversized result returns Limit before reservation or mutation.
OutOfMemory during leaf assembly keeps the original terminated parent intact.

| Decision | Changes that exercise it | Expected result |
| --- | --- | --- |
| Original length | 32,766 and 32,767 units before NUL | Accept the first and refuse the second with Limit |
| Original shape | Ordinary input, embedded NUL alone, invalid trailer alone | Refuse NUL and invalid trailers independently with Invalid |
| Leaf character | Ordinary space or Unicode, control alone, reserved ASCII alone | Refuse control and reserved ASCII independently with Invalid |
| Path query | Zero, fit, exact buffer size, larger requirement | Report the native error on zero. A fit retains NUL. Both short results retry. |
| Query growth cap | Requirement at cap and requirement one above cap | The first retries. The second returns Limit before a second query. |
| Parent fast path | Verbatim alone, NT alone, neither, short drive path, equally short relative path, drive path of 248 units | Each recognized prefix skips normalization. An unprefixed path skips only when it is both short and drive-shaped. |
| Normalized shape | DOS drive, device, verbatim, NT, UNC, and relative | Add or replace the right prefix, or reuse the existing output |
| Normalized parent cap | NUL-terminated result with prefix exactly at cap and one above cap | Only the former succeeds |
| Final leaf cap | Parent with and without a final separator, exact fit and one over | Only an exact fit succeeds. An over-cap refusal keeps the parent. |
| Selected allocation | Refuse the input, initial query, growth, prefix or leaf reservation | Report OutOfMemory and reclaim scratch. A leaf retry reuses the original parent. |
| Canonical drive | Valid shapes, wrong shape, non-byte and non-alphabetic drive units | Return an uppercase byte only for a supported drive shape |

The callback tests exercise production UTF-16 code. They script Win32
return counts and errors. Native Windows file operations, exact DLL
qualification, and the intended NTFS account remain separate evidence.
For the canonical-drive let chain, conversion to an ASCII alphabetic
byte implies successful conversion to a byte. The two latter predicates
therefore cannot vary independently for a non-byte drive. The table marks
the combined decision partial and exercises the non-byte path separately.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/windows_path.rs` | `74eab9df1d4c9aa0314f79e75bd68986429d7863079af68df3795a5b2e35f9fe` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:18:5– | 696..698 | if | validate_path | Reject an original path that leaves no unit for NUL. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:21:5– | 785..787 | if | validate_path | Either condition independently refuses the input. |
| C | weekend-db::weekend_db | src/storage/windows_path.rs:21:8– | 788..881 | short_circuit_or | validate_path | Shares the full NUL and trailer decision. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:29:5– | 1030..1032 | if | validate_leaf | The leaf is accepted only when no code unit is a forbidden ASCII byte. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:30:57– | 1095..1136 | short_circuit_or | validate_leaf | Control and reserved ASCII each independently refuse the leaf. |
| E | weekend-db::weekend_db | src/storage/windows_path.rs:41:58– | 1373..1374 | try_propagation | append_leaf | The missing terminator is an I/O error. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:44:5– | 1496..1498 | if | append_leaf | Exactly one terminated unit above the final cap refuses append. |
| E | weekend-db::weekend_db | src/storage/windows_path.rs:48:41– | 1651..1652 | try_propagation | append_leaf | Preserve OutOfMemory and the terminated parent. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:50:5– | 1674..1676 | if | append_leaf | Append one separator only when the parent lacks a final backslash. |
| E | weekend-db::weekend_db | src/storage/windows_path.rs:70:45– | 2299..2300 | try_propagation | read_path | Propagate either the initial or the growth refusal. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:73:9– | 2381..2383 | if | read_path | Preserve the error supplied for zero. A positive length avoids the error callback. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:76:9– | 2451..2453 | if | read_path | A fit retains NUL. A count equal to the buffer size must grow. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:81:9– | 2588..2590 | if | read_path | The exact cap can be requested. One unit above it stops. |
| E | weekend-db::weekend_db | src/storage/windows_path.rs:96:41– | 3053..3054 | try_propagation | parent_path | Refuse before calling the native query. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:99:5– | 3104..3106 | if | parent_path | Each prefix independently returns the original terminated parent. |
| C | weekend-db::weekend_db | src/storage/windows_path.rs:99:8– | 3107..3166 | short_circuit_or | parent_path | Shares both prefix conditions with the enclosing if. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:103:5– | 3282..3284 | if | parent_path | Both short length and drive shape are needed for the fast return. |
| C | weekend-db::weekend_db | src/storage/windows_path.rs:103:8– | 3285..3425 | short_circuit_and | parent_path | Shares both fast-path conditions with the enclosing if. |
| E | weekend-db::weekend_db | src/storage/windows_path.rs:113:65– | 3572..3573 | try_propagation | parent_path | Preserve both allocation refusal and the supplied native query error. |
| D | weekend-db::weekend_db | src/storage/windows_path.rs:114:43– | 3617..3622 | match | parent_path | Exercise all five match arms, including both alternatives of the existing-prefix arm. |
| P | weekend-db::weekend_db | src/storage/windows_path.rs:125:5– | 4063..4065 | if | parent_path | The terminated parent including its prefix must fit. |
| E | weekend-db::weekend_db | src/storage/windows_path.rs:129:41– | 4211..4212 | try_propagation | parent_path | Preserve OutOfMemory while expanding the normalized prefix. |
| B | weekend-db::weekend_db | src/storage/windows_path.rs:137:5– | 4424..4426 | if | canonical_drive | Accept a verbatim drive with an ASCII letter. Conversion and alphabetic checks have a data dependency. |
| D | weekend-db::weekend_db | src/storage/windows_path.rs:137:8– | 4427..4430 | pattern_test | canonical_drive | The pattern rejects wrong prefix or separator and accepts the required drive shape. |
| B | weekend-db::weekend_db | src/storage/windows_path.rs:137:8– | 4427..4547 | short_circuit_and | canonical_drive | The drive binding prevents an independent conversion when the shape fails. |
| B | weekend-db::weekend_db | src/storage/windows_path.rs:137:8– | 4427..4587 | short_circuit_and | canonical_drive | An ASCII letter succeeds and a digit fails after successful conversion. The conversion predicate is coupled. |
| D | weekend-db::weekend_db | src/storage/windows_path.rs:138:12– | 4509..4512 | pattern_test | canonical_drive | Unicode beyond a byte exercises failed conversion after a shape match. |

## Verification

The test source is `a28cca61163b5e4c7f861a631f1a4512e7e5d269` with Rust
1.100.0. The nine path tests ran in both profiles on arm64 macOS and
aarch64 Docker Linux on ext4. The engine, file, compaction, durability,
headroom, retention, ByteStore, storage decision and allocator-routing
suites passed 274 tests per macOS profile and 285 per Linux profile.
The xtask mutation contract passed 18 tests on each host. Strict Clippy
passed on both hosts and for the `x86_64-pc-windows-msvc` library and tests.
Miri on macOS passed the leaf assembly refusal and retry using nightly
`708c94ef0`.

All 30 named controls were detected on macOS in each profile, in four
disjoint shards. Each control ran a passing baseline and the selected
test reached a failure under the changed source. The two Windows RNG
controls retain their original replacements and are pinned to the new
adapter source at `81c6ce8`. Their native execution follows in CI.

The inventory reviews 10,589 records with 4,980 scanner blind spots.
It records dispositions for all 27 pathname helper records, including
three partial canonical-drive records, and leaves 40 records in the
Win32 adapter open. The overall queue has 9,453 open records, including
2,084 runtime, 26 driver and 7,343 tooling. Native path query, handle,
lease and raw I/O decisions require their own evidence.

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --lib \
  storage::windows_path::tests
cargo +1.100.0 xtask mutations --case r1-windows-path- --toolchain 1.100.0 \
  --output target/windows-path-controls-debug
cargo +1.100.0 xtask mutations --case r1-windows-path- --toolchain 1.100.0 \
  --release --output target/windows-path-controls-release
```
