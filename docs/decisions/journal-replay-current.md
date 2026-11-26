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
| `apps/write-coordinator/src/native/journal.rs` | `76aa147ef70e8f423870bded014438c2d8ad683e0a13c67c238c421fce96d35c` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:174:9– | 5216..5219 | let_else | resume | Pending and idle/replayed local state. |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:180:45– | 5491..5492 | try_propagation | resume | Admitted pending intent has a representable next sequence. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:192:35– | 5899..5900 | try_propagation | resume | Transport error preserves the saved intent for retry. |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:193:79– | 5980..5981 | try_propagation | resume | Pending admission excludes u64::MAX as the expected revision. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:205:15– | 6363..6364 | try_propagation | resume | Local receipt publication can fail after remote commit. |
| B | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:206:16– | 6381..6383 | if | resume | The revision is comparable only for a parsed conflict. Older, equal, newer and malformed replies are covered. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:206:19– | 6384..6387 | pattern_test | resume | The revision is comparable only for a parsed conflict. Older, equal, newer and malformed replies are covered. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:206:19– | 6384..6480 | short_circuit_and | resume | The revision is comparable only for a parsed conflict. Older, equal, newer and malformed replies are covered. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:212:15– | 6595..6596 | try_propagation | resume | Clearing a newer conflict uses the same durable journal save. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:224:11– | 7026..7027 | try_propagation | check | A failed checkpoint request propagates to the caller. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:225:9– | 7037..7039 | if | check | Coordinator error reply is returned before checkpoint parsing. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:229:9– | 7203..7206 | let_else | check | Only the four-word checkpoint form proceeds. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:238:50– | 7563..7564 | try_propagation | check | Canonical checkpoint revision required. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:239:44– | 7609..7610 | try_propagation | check | Canonical checkpoint value required. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:240:9– | 7620..7622 | if | check | A checkpoint below the initialized revision rejects. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:243:22– | 7758..7760 | if | check | Empty slot and encoded receipt replies. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:246:68– | 7879..7880 | try_propagation | check | Fixed-width receipt decoding and revision bounds. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:246:80– | 7891..7892 | try_propagation | check | Fixed-width receipt decoding and revision bounds. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:247:16– | 7909..7963 | short_circuit_and | check | Both operands are covered by the head-receipt table. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:256:9– | 8276..8278 | if | check | Older restored coordinator is behind the journal floor. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:261:9– | 8457..8459 | if | check | Pending and completed journal states. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:261:12– | 8460..8463 | pattern_test | check | Pending and completed journal states. |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:263:43– | 8574..8575 | try_propagation | check | Pending state was admitted with a representable next sequence. |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:266:81– | 8737..8738 | try_propagation | check | Pending state excludes u64::MAX as its expected revision. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:268:13– | 8767..8769 | if | check | A matching remote receipt acknowledges this exact intent. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:272:9– | 8905..8907 | if | check | Divergent receipt reports mismatch. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:279:22– | 9215..9220 | match | check | Aligned, pending-eligible and pending-stale fixtures. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:281:29– | 9296..9323 | match_guard | check | Compare the eligible revision with a newer revision from another writer. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:316:5– | 10255..10258 | let_else | parse_state | Exact word count required for value or conflict. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:321:5– | 10457..10459 | if | parse_state | Response kind must match the requested operation. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:324:35– | 10579..10580 | try_propagation | parse_state | Both numeric fields use the canonical decimal parser. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:324:61– | 10605..10606 | try_propagation | parse_state | Both numeric fields use the canonical decimal parser. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:328:35– | 10698..10699 | try_propagation | canonical_number | Integer parse errors propagate. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:329:5– | 10705..10707 | if | canonical_number | Decimal text must round-trip without signs or leading zeros. |
