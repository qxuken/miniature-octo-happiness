# Merge local changesets with point-read validation

Superseded for the initial transaction contract by
[ADR 0007](0007-merge-stable-record-assignments.md), which defines record scopes,
conservative observation validation, and the simpler milestone sequence.

The first concurrency milestone permits two local transactions based on the
same snapshot to merge compatible changesets, while publication remains
coordinated. Reject the second commit if an intervening transaction wrote a
key it writes or changed a key it read; publish none of a rejected
transaction's changes. This preserves compatible edits without adopting
CRDT-specific assignment semantics, while conservatively rejecting stale
read-dependent decisions; demonstrate the contract with interleaved handles
before introducing parallel threads.
