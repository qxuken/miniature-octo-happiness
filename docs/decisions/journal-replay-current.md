# Coordinator client receipt replay and checkpoint reporting

A write from slot 7 commits value 20 at revision 4 and loses its reply. A
different writer then commits value 99 at revision 5. After reopening both
databases, checking the journal reports `pending-committed 5 99` without
writing either database. Resuming returns `committed 4`, records the original
receipt and preserves all coordinator bytes. Another local restart returns
the same revision without contacting the coordinator.

The head-receipt guard rejects a slot receipt that contradicts the value at
its own revision. Keeping the receipt fixed at revision 4 for value 20 gives
these independent pairs.

| Checkpoint revision | Checkpoint value | Result |
| --- | --- | --- |
| 4 | 20 | The pending receipt matches. |
| 4 | 99 | Reject the inconsistent head value. |
| 5 | 99 | The earlier receipt still confirms the pending write. |

The journal unit tests also exercise malformed replies, busy retry, stale
conflicts and failed local receipt publication. The native writer integration
suite exercises restore, restart and incompatible histories. Delegated rows
retain their callee-specific failure scope; no follower acknowledgment is
part of the coordinator receipt.

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/journal.rs` | `cfed6dae622427a4bcde92e7c4e51160da68b5f795a0f3157b4a7e8ad673ea96` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:174:9– | 5249..5252 | let_else | resume | Pending and idle/replayed local state. |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:180:45– | 5524..5525 | try_propagation | resume | Admitted pending intent has a representable next sequence. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:192:35– | 5932..5933 | try_propagation | resume | Transport error preserves the saved intent for retry. |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:193:79– | 6013..6014 | try_propagation | resume | Pending admission excludes u64::MAX as the expected revision. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:205:15– | 6396..6397 | try_propagation | resume | Local receipt publication can fail after remote commit. |
| B | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:206:16– | 6414..6416 | if | resume | The revision is comparable only for a parsed conflict. Older, equal, newer and malformed replies are covered. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:206:19– | 6417..6420 | pattern_test | resume | The revision is comparable only for a parsed conflict. Older, equal, newer and malformed replies are covered. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:206:19– | 6417..6513 | short_circuit_and | resume | The revision is comparable only for a parsed conflict. Older, equal, newer and malformed replies are covered. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:212:15– | 6628..6629 | try_propagation | resume | Clearing a newer conflict uses the same durable journal save. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:224:11– | 7059..7060 | try_propagation | check | A failed checkpoint request propagates to the caller. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:225:9– | 7070..7072 | if | check | Coordinator error reply is returned before checkpoint parsing. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:229:9– | 7236..7239 | let_else | check | Only the four-word checkpoint form proceeds. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:238:50– | 7596..7597 | try_propagation | check | Canonical checkpoint revision required. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:239:44– | 7642..7643 | try_propagation | check | Canonical checkpoint value required. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:240:9– | 7653..7655 | if | check | A checkpoint below the initialized revision rejects. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:243:22– | 7791..7793 | if | check | Empty slot and encoded receipt replies. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:246:68– | 7912..7913 | try_propagation | check | Fixed-width receipt decoding and revision bounds. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:246:80– | 7924..7925 | try_propagation | check | Fixed-width receipt decoding and revision bounds. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:247:16– | 7942..7996 | short_circuit_and | check | Both operands are covered by the head-receipt table. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:256:9– | 8309..8311 | if | check | Older restored coordinator is behind the journal floor. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:261:9– | 8490..8492 | if | check | Pending and completed journal states. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:261:12– | 8493..8496 | pattern_test | check | Pending and completed journal states. |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:263:43– | 8607..8608 | try_propagation | check | Pending state was admitted with a representable next sequence. |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:266:81– | 8770..8771 | try_propagation | check | Pending state excludes u64::MAX as its expected revision. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:268:13– | 8800..8802 | if | check | A matching remote receipt acknowledges this exact intent. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:272:9– | 8938..8940 | if | check | Divergent receipt reports mismatch. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:279:22– | 9248..9253 | match | check | Aligned, pending-eligible and pending-stale fixtures. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:281:29– | 9329..9356 | match_guard | check | Compare the eligible revision with a newer revision from another writer. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:317:5– | 10310..10313 | let_else | parse_state | Exact word count required for value or conflict. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:322:5– | 10512..10514 | if | parse_state | Response kind must match the requested operation. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:325:35– | 10634..10635 | try_propagation | parse_state | Both numeric fields use the canonical decimal parser. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:325:61– | 10660..10661 | try_propagation | parse_state | Both numeric fields use the canonical decimal parser. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:329:35– | 10753..10754 | try_propagation | canonical_number | Integer parse errors propagate. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:330:5– | 10760..10762 | if | canonical_number | Decimal text must round-trip without signs or leading zeros. |
