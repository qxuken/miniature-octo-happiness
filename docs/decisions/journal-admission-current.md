# Journal write admission

The regression starts with a durable receipt for value 20 at revision 4.
Each rejected call preserves the entire local image. With an eligible intent,
a lost reply leaves the new intent and the prior receipt together on reopen.

| Local state | Requested revision | Remote revision | Result | Commands sent |
| --- | ---: | ---: | --- | --- |
| Idle | 3 | — | Below local floor | None |
| Idle | u64::MAX | — | No successor revision | None |
| Idle | 4 | 3 | Remote behind local floor | One read |
| Idle | 5 | 4 | Remote behind requested revision | One read |
| Idle | 4 | 5 | Conflict with remote value | One read |
| Idle | 4 | 4 | Persist intent before sending request | One read, one write |
| Pending | 4 | — | Existing intent requires resume | None |

Once local admission succeeds, the requested revision is at least the local
floor. Comparing the remote revision with the requested revision therefore
also rejects any remote below that floor. The prior compound check repeated
the first comparison. Its two behind cases retain the same public error and
perform no local save. Sequence overflow is excluded by receipt validation:
the stored sequence plus the positive initial revision must fit in u64.

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/journal.rs` | `5c4153f888d12a15975f7699e8bdaee03daf613f8f7c3f184df3aeca2f3e3df9` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:152:30– | 4337..4338 | try_propagation | set | Valid persisted receipts have sequence + initial revision representable. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:153:12– | 4351..4422 | short_circuit_or | set | Both operands have independent admission pairs. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:156:80– | 4608..4609 | try_propagation | set | Read-only coordinator preflight propagates transport errors. |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:157:61– | 4671..4672 | try_propagation | set | Preflight response must parse as a canonical value reply. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/journal.rs:169:11– | 5074..5075 | try_propagation | set | Failed durable staging never sends journal-set; reopening follows the publication barrier. |
