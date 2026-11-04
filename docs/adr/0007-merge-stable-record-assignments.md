# Merge stable record assignments and defer list rebasing

Supersedes ADR 0001 and ADR 0005 for the initial transaction contract.

Transactions keep final put/delete assignments at stable outer record keys and
validate observed generations and write targets before applying the assignments
to the current root under one commit gate. Nested maps/lists are immutable
record values, so independent existing records can merge while edits within
one record conflict; use conservative collection generations for missing reads
and supported scans, accepting false conflicts. Prove a whole-root revision
baseline first, then refine it; this reduces implementation and verification
cost by deferring positional list replay and its history while preserving
meaningful independent-record commits.

## Consequences

- Validate versions independently of value equality, including intervening
  writes that restore an earlier value. Validate and publish atomically.
- All data-dependent transaction observations need a validation rule. No
  serializability claim follows from tracking only successful point lookups.
- Two transactions changing different fields or list elements within one
  record conflict initially. Collection-level checks can also reject unrelated
  inserts or changes after a scan.
- Active transactions remain local and nonrestartable; explicit retained
  history is readable but not automatically a source of arbitrarily old writes.
- The initial design needs neither positional operation transformation nor a
  global list-operation log. Keep immutable maps/lists and durable snapshots.
- Automatic retries, schema enforcement, migration execution, and generated
  indexing are not silently introduced by this change.
