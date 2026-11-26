# Sequenced coordinator submission

The coordinator binary and standalone fuzz library run the same submission
fixtures. Rejections check the acknowledged rows. The durable cases also
compare the image.

| Decision | Result |
| --- | --- |
| Reserved sequence zero | An existing slot returns `Invalid` for zero. A lower nonzero sequence returns the retirement boundary. |
| Prior receipt | An empty slot accepts sequence 1. An occupied slot distinguishes a retired request, an exact retry and a higher sequence. The exact retry returns its original revision. |
| Predecessor | With the same initial receipt and proposed write, an unchecked request and an exact matching predecessor commit. A different exact predecessor returns `Conflict` at the old revision. |
| Counter read | A missing row returns `NotFound`. A nonnumeric row returns `Type`. A later valid counter permits a fresh submission. |
| Prior receipt decode | A malformed prior receipt returns `Corrupt`. The counter and receipt stay at the prior revision. |
| Revision addition | A validated logical import with acknowledged revision `u64::MAX` rejects a new submission with `Exhausted`. Its two collections keep their original data revision. |
| Selected allocator | Refusal after the receipt observation returns `OutOfMemory` at scalar construction. The prior receipt and live-byte usage survive, and the next attempt commits. |
| Draft budget | Observation, counter staging, receipt construction and receipt staging each return `Limit` in separate cases. The durable head remains at revision 4 and the retained revision-3 counter remains zero. |

The budget cases reopen at revision 4. With limits restored, retry commits
revision 5 and a repeat returns the same receipt and leaves the image byte
identical. The existing retirement fault sweep checks the append, root and
witness barriers, reopening at revision 4 or 5 with a complete counter and
receipt. The commit inside the assertion remains a separate macro blind spot.

`Predecessor::Unchecked` masks the comparison because there is no bound
expected predecessor. One control forces an unchecked request to perform a
comparison. The other reverses the comparison for exact requests. These
provide an enumerated source disposition for the guard
and its pattern and short-circuit records. The validation table records
the outcomes the code actually evaluates.

| Predecessor mode | Comparison result | Outcome |
| --- | --- | --- |
| Unchecked | Masked | Continue |
| Exact | Equal | Continue |
| Exact | Different | `Conflict` |

## Qualification

Frozen source `8ac75037511e61aeb08ca6c880446db2c217a4bc` ran with Rust
1.100.0 and locked dependencies. Each debug and release profile on macOS
arm64 and Docker Linux arm64/ext4 passed 38 coordinator unit tests, 16
coordinator integration tests, four logical-image tests and 48 standalone
fuzz-library tests. Two macOS and three Linux extended workloads remained
opt-in. Clippy with warnings denied passed in both Cargo targets.

Each campaign detected 13 coordinator and 13 standalone mutations.
Fresh extraction verified the intended failure assertions, 96 supervised
commands and 1,018 restored source files per campaign. The two coordinator
tests and the standalone fuzz-library executable cross-linked for Windows
x64/MSVC in both profiles. Native execution is pending.

All 15 remaining runtime source records in `submit` have a disposition.
The standalone target gains 10 named records. Its five pattern, ordering
and predecessor guard records remain in the tooling queue under the
current evidence categories. Repository reconciliation leaves 226 runtime
and 7,458 tooling gaps.
[Frozen source and evidence checksums](coordinator-sequenced-submit-8ac7503.sha256)

## Coordinator source records

The second fingerprint binds these submission source ranges after adding the
request and lookup fixtures at `2ea321e`.

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/sequenced.rs` | `7c0ded9fd3380747be331b51a468a4d17254017fb438a6a1c5f8293bc31deebb` |
| `apps/write-coordinator/src/native/sequenced.rs` | `4c8b7d2c5408e5b19d57943f317d3bad7efa1da3188cb6538027d9c259f4f2c5` |
| `apps/write-coordinator/src/native/sequenced.rs` | `05b48dac35ba15d3dd70ccb32c39cfec7913f3ae81d25e8792af52738cfa7ad2` |

The second source pin reflects sharing the test allocator and checksum
modules with the legacy protocol. The production bytes and positions
are unchanged.

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:262:8– | 8211..8214 | pattern_test | submit | Pattern presence is the same single test as its enclosing prior-receipt if. The prior-receipt control observes both. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:263:9– | 8250..8255 | match | submit | A lower nonzero sequence retires, equality repeats the receipt, and a higher eligible sequence commits. The sequence-order control changes that order. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:277:5– | 8798..8800 | if | submit | The predecessor-mode table and the two independent guard controls exercise every reachable outcome. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:277:8– | 8801..8804 | pattern_test | submit | Unchecked skips the comparison. Exact binds the expected value for both matching and mismatching receipts. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:277:8– | 8801..8879 | short_circuit_and | submit | Unchecked masks the comparison. Exact evaluates both outcomes. |
