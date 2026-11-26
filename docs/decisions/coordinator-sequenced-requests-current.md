# Sequenced coordinator requests and receipt lookup

| Boundary | Observed behavior |
| --- | --- |
| Read dispatch | The six read commands return the documented reply. Missing and extra words and an unknown command return `Invalid`. Successful and rejected reads preserve exact durable bytes. |
| Store identity | Each journal command preserves `ForeignStore` for a foreign identity before parsing its remaining operands. A malformed identity and a database without a storage identity return `Invalid`. |
| Write operands | Each command rejects invalid slot, sequence, revision or value independently. Slot 1024 and a value above `u64::MAX` reject. Both valid commands can advance the same slot. |
| Journal predecessor layout | A dash permits an initial request. A valid explicit receipt permits the next request. Malformed hex and a valid-width receipt with the wrong version both return `Invalid`. |
| Receipt lookup | A missing collection returns `NotFound`, an empty slot returns `receipt 0` and a stored non-byte receipt returns `Corrupt`. |
| Checkpoint lookup | The current counter and receipt are reported together. A malformed receipt returns `Corrupt`. If the counter is missing while the receipt is corrupt, the result is `NotFound`. |
| Hexadecimal representation | The known bytes encode as lowercase hex and decode exactly. Both width mismatches and invalid characters in either nibble reject. |
| Initialization scalar allocation | The allocator refuses the next allocation after creating both collections and staging the format row. The writer returns `OutOfMemory`. Both collections reopen empty at revision 2. |

The write fixtures verify the previous counter, receipt and durable bytes
after the rejected requests. The receipt-history controls keep the length
and version valid while varying the reserved sequence, revision relationship,
sequence prefix limit and acknowledged head. The two shorter `||` records
are evaluations within that same four-condition decision.

## Qualification

Frozen source `11a32db4b6e38ec2356a1794e54f5c346c81be83` ran with Rust
1.100.0 and locked dependencies. Each debug and release profile on macOS
arm64 and Docker Linux arm64/ext4 passed 43 coordinator unit tests, 16
coordinator integration tests and 53 standalone fuzz-library tests. Two
macOS and three Linux extended workloads remained opt-in. Strict Clippy
passed in both Cargo targets.

Each campaign detected all 28 new controls and six existing receipt
controls in the coordinator binary. Fresh extraction verified the intended
failure assertions, 112 supervised commands and 1,020 restored source
files per campaign. Three Windows x64 test executables cross-linked per
profile. Native execution is pending for this source.

The 34 remaining runtime records in the sequenced module have source
dispositions, including the six scoped rows below. Repository reconciliation
leaves 192 runtime and 7,458 tooling gaps. The standalone library also runs
the five new fixtures. Its source records retain their own tooling ledger.
[Frozen source and evidence checksums](coordinator-sequenced-requests-11a32db.sha256)

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/sequenced.rs` | `4c8b7d2c5408e5b19d57943f317d3bad7efa1da3188cb6538027d9c259f4f2c5` |
| `apps/write-coordinator/src/native/sequenced.rs` | `05b48dac35ba15d3dd70ccb32c39cfec7913f3ae81d25e8792af52738cfa7ad2` |

The second source pin reflects sharing the test allocator and checksum
modules with the legacy protocol. The production bytes and positions
are unchanged.

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:73:12– | 2275..2369 | short_circuit_or | from_bytes | The zero-sequence fixture skips the revision check. A nonzero sequence reaches it, and a mismatching revision rejects. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:73:12– | 2275..2527 | short_circuit_or | from_bytes | Earlier violations skip the sequence-prefix bound. The valid relationship reaches it, and the prefix case independently rejects. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:107:5– | 3385..3388 | for_iteration | hex | Empty input returns empty output. The fixed 16-byte input visits every byte and stops after both nibbles per byte. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:119:9– | 3722..3727 | match | unhex::digit | The fixed identity tests decimal and lowercase-letter digits. An invalid character in either nibble returns Invalid. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:126:5– | 3907..3910 | for_iteration | unhex | The 16-byte identity round trip fills every byte and terminates. The invalid-nibble cases return through the respective high and low paths. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:182:5– | 5686..5691 | match | handle | The read fixture covers all six read shapes and the fallback. The write-field fixture covers both write shapes. |
