# Coordinator legacy commit and recovery

The coordinator binary and standalone fuzz library exercise the same
legacy command parser and transaction path through separate Cargo targets.
The added tests preserve typed errors and inspect the acknowledged state
before and after refusal.

| Decision | Input and outcome |
| --- | --- |
| Collection lookup, counter lookup and caller propagation | A missing collection and a missing row in an existing collection both return NotFound through get, set and set-id. The revision stays unchanged. |
| Parsed expected revision and value | Invalid integers in either position return Invalid for each write command. A matching revision publishes one value and a stale revision returns the existing revision and value. |
| Draft and value limits | A plain update with a zero transaction-byte budget, a counter dependency beyond the observation limit, an oversized receipt value and a second draft write beyond the write limit all return Limit. The counter stays at zero and the receipt remains absent. |
| Recovery guard | A definite or ambiguous I/O failure preserves the acknowledged counter and makes subsequent commands return RecoveryRequired until reopening. |

The durable case begins at revision 4 with a prior receipt from
revision 3 and a retained root at that revision. The clean trace
identifies append, root and witness syncs. Each recorded I/O fails
under both discard-all and persist-all simulator crash policies.
The writer returns Io through append sync and OutcomeUnknown later.
A fresh open sees revision 4 with the old value and absent new receipt,
or revision 5 with the new value and receipt together. The root sync
case distinguishes which pending writes survive.

Retry returns the original revision if the receipt survived and
commits once otherwise. Another crash and fresh open check revision 5,
the same retry result and the exact retained receipt bytes and value.
The simulated fault sweep adds behavioral evidence for the commit
sites and their caller. The named mutations cover source decisions
that are enumerated by the inventory.
