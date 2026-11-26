# Sequenced coordinator startup and admission

Initialization publishes three revisions. The first two create the format and
data collections. The third puts the format marker and zero counter in one
transaction. The simulator injects an I/O failure at every operation from the
successful trace, verifies the exact trace prefix and checks both loss and
persistence of pending writes on reopen.

| Reopened revision | Durable state | Startup admission |
| --- | --- | --- |
| 0 | Neither collection exists | `NotFound` |
| 1 | Empty format collection | `UnsupportedFormat` |
| 2 | Both collections are empty | `UnsupportedFormat` |
| 3 | Format marker and counter value 0 | Accepted |

For each of the three publications, faults through the append sync yield `Io`.
Subsequent faults through the witness sync yield `OutcomeUnknown`. The live
handle keeps the previous acknowledged revision and rejects requests with
`RecoveryRequired`. Where the root sync itself fails, the simulator's two crash
modes select the adjacent revisions. After reopen, admission does not write
either the rejected or accepted image.

| Format marker | Counter | Admission |
| --- | --- | --- |
| Absent | Unsigned 0 | `UnsupportedFormat` |
| Unknown version bytes | Unsigned 0 | `UnsupportedFormat` |
| Unsigned value | Unsigned 0 | `Corrupt` |
| Current bytes | Absent | `NotFound` |
| Current bytes | Bytes | `Type` |
| Current bytes | Unsigned 0 | Accepted |

The marker's size limit and the first and second draft writes' write-count
limits each fail at revision 2. After crash and reopen both collections remain
empty. The scalar allocation for the counter, request receipt decisions, and
other targets keep their own ledger entries. The final seeding commit is inside
an assertion macro. The simulator exercises it, and the source inventory
retains the macro's separate blind spot.
