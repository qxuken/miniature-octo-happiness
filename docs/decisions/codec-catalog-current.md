# Recovery descriptor and retained catalog decisions

This batch maps the selected COMMIT payload and retained-catalog records in
`recover`. The [object reader ledger](codec-graph-current.md) covers the
extent and first read boundaries separately. Production codec bytes and
earlier source bindings are preserved.

| Decision | Cases and outcomes |
|---|---|
| Descriptor end and birth | A normal object ends at the selected slot end and has the slot revision. A smaller birth revision returns Corrupt before reading DATA. The end comparison is dominated on this path. Slot admission sets `descriptor + REGION == end`, and a successful object spans at least one region and stays within its bound. The combined guard and its OR retain B. |
| Descriptor revision | The selected slot, payload revision and current DATA revision agree. Change the payload revision or move the DATA revision above it independently. Each change returns Corrupt before reading DATA. The valid case and either single change supply the two source pairs. |
| Next retained ID | An empty catalog with next ID 1 opens revision 2. Changing only that ID to zero returns Corrupt. Clearing the selected slot opens revision 1. The previous nonempty-catalog test still rejected zero when the guard was removed because the later entry-ID comparison also failed. |
| Retained entry | The REC-V5 fixtures isolate five conditions. The entry ID is zero or repeats the previous ID, reaches the next ID, the captured revision reaches the current revision, or the declared DATA revision exceeds the captured or current DATA revision. Each case returns Corrupt, and the previous root opens when selected on its own. The last two cases check rejection before the retained DATA read. |
| Retained next collection ID | With the same valid retained DATA reference, a next-collection value greater than current returns Corrupt. The lower value opens with the retained snapshot. |
| Read, field and payload propagation | Current and retained DATA errors and COMMIT object errors stop open. Truncating the checksummed COMMIT before each of its words returns Corrupt. Adding one zero byte beyond the complete declared payload also returns Corrupt. The valid fixture opens revision 5 with retained IDs 1 and 2 and distinct rows. With the newest slot cleared, every malformed case opens revision 4 and retains exactly ID 1. The count above the cap returns Limit in the REC-V5 fixture. |
| Catalog loop | The empty-catalog fixture exercises zero entries. The previous root in the truncation test exercises one. The current root exercises two, and the existing retention-limit test reopens the full supported catalog. This is scoped iteration evidence. |
| Allocation propagation | The existing raw recovery allocation sweep reaches current head, entry vector, retained head and DATA cache reservation with a retained image. Each injected failure returns OutOfMemory and preserves the bytes. A later allowance recovers the full current and retained graph. |

The new integration tests make the field and end conditions observable through
public open. They use real consecutive published slots, independently compute
a valid descriptor checksum and preserve the opaque bytes outside the changed
fields. Recovery failure with the selected slot present preserves the complete
image. Separately selecting the older slot demonstrates that rejection comes
from the chosen graph. See [REC-V4 and REC-V5](../../DESIGN.md#rec-v4-verify-the-selected-graph-before-opening).

The first local debug prototype detected 22 of 23 changes. The single
survivor omitted the next-ID guard. The empty catalog regression detects that
omission. The final working-source debug run detects all 27 controls, including
four allocation propagation controls. The prototype checks that the named
test fails for each control. Frozen and hosted qualification remain pending.

The eight repeated `cursor.int()` calls have identical owner and expression
selectors. The ledger distinguishes them by exact byte spans and a full-file
hash. Each corresponding mutation selects a single occurrence and the audit
requires the same named truncation regression to fail. The remaining thirteen
controls have unique selectors in the reconciler.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `34baf455a1e1ad7e8d76c932ed417fe60c324bbeaad64efd0ca398205012472d` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/storage/codec.rs:1206:68– | 41614..41615 | try_propagation | recover | Scoped propagation above |
| B | weekend-db::weekend_db | src/storage/codec.rs:1207:9– | 41625..41627 | if | recover | Descriptor end dominated by admitted slot and object bounds |
| B | weekend-db::weekend_db | src/storage/codec.rs:1207:12– | 41628..41678 | short_circuit_or | recover | Descriptor end dominated by admitted slot and object bounds |
| E | weekend-db::weekend_db | src/storage/codec.rs:1211:36– | 41819..41820 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1212:36– | 41857..41858 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1213:41– | 41900..41901 | try_propagation | recover | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1214:9– | 41911..41913 | if | recover | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1214:12– | 41914..41967 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:1217:35– | 42054..42055 | try_propagation | recover | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1218:9– | 42065..42067 | if | recover | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1221:57– | 42189..42190 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1223:66– | 42302..42303 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1228:11– | 42426..42427 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1231:41– | 42566..42567 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1233:9– | 42610..42613 | for_iteration | recover | Scoped iteration over zero, one, multiple and capped entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:1234:34– | 42663..42664 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1235:49– | 42714..42715 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1236:38– | 42754..42755 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1237:54– | 42810..42811 | try_propagation | recover | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1238:13– | 42825..42827 | if | recover | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1238:16– | 42828..42878 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1238:16– | 42828..42927 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1238:16– | 42828..42989 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1238:16– | 42828..43047 | short_circuit_or | recover | Overlapping syntax under the documented parent pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:1248:77– | 43275..43276 | try_propagation | recover | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1249:13– | 43290..43292 | if | recover | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1256:15– | 43576..43577 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1263:22– | 43737..43738 | try_propagation | recover | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1265:62– | 43849..43850 | try_propagation | recover | Scoped propagation above |
