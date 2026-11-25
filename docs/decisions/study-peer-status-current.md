# Study outgoing peer status

An enrolled pair starts with zero acknowledgments. A commits seven practice
minutes at revision 3 while B stays at revision 2. The first connection
attempt reaches a released loopback port. Text output reports zero
acknowledgments and an error. JSON output reports
`connect/io/connection_refused`. The unopened B file keeps its original bytes.

| Observation | Result |
| --- | --- |
| Outgoing work starts disabled | The peer command reports `peer 0 idle`. |
| Pushing is enabled with B stopped | The worker surfaces a connection failure. A's local commit remains independent of the reply. |
| B opens with its configured identity and A retries | The peer acknowledgment advances and reports seven minutes. B reports revision 3 with the same seven minutes. |
| A syncs again | Another acknowledgment arrives and B stays at revision 3. |
| Both peers quit after pushing is disabled | Both children exit successfully. B reopens at revision 3 with seven minutes. |
| B is interrupted after committing and before its reply in the lost-ack test | A reports `peer 0 error Io`. The existing retry test confirms B's published state and converges. |

The probes exercise the active and stopped loop, paused and enabled
pushing, successful and failed outcomes, and text and JSON error replies.
They check the resulting observations and worker exit without measuring
backoff intervals. Native worker join failures and the accepting worker's
operation errors retain their own decision records.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native.rs` | `9ced0c6b1d1ea19a0c2c827410755ad5775e4026c8271d1d7448a9f9822f9c68` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:326:33– | 11742..11747 | match | run_with_io | The same loop observes idle before pushing, a refused connection in each diagnostic mode, and an acknowledged state after retry. |
