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
expected predecessor. The two controls independently force an unchecked
request to perform a comparison and suppress the mismatch rejection for an
exact request. These provide an enumerated source disposition for the guard
and its pattern and short-circuit records. The validation table records
the outcomes the code actually evaluates.

| Predecessor mode | Comparison result | Outcome |
| --- | --- | --- |
| Unchecked | Masked | Continue |
| Exact | Equal | Continue |
| Exact | Different | `Conflict` |

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/sequenced.rs` | `7c0ded9fd3380747be331b51a468a4d17254017fb438a6a1c5f8293bc31deebb` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:262:8– | 8211..8214 | pattern_test | submit | Pattern presence is the same single test as its enclosing prior-receipt if. The prior-receipt control observes both. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:263:9– | 8250..8255 | match | submit | A lower nonzero sequence retires, equality repeats the receipt, and a higher eligible sequence commits. The sequence-order control changes that order. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:277:5– | 8798..8800 | if | submit | The predecessor-mode table and the two independent guard controls exercise every reachable outcome. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:277:8– | 8801..8804 | pattern_test | submit | Unchecked skips the comparison. Exact binds the expected value for both matching and mismatching receipts. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/sequenced.rs:277:8– | 8801..8879 | short_circuit_and | submit | Unchecked masks the comparison. Exact evaluates both outcomes. |
