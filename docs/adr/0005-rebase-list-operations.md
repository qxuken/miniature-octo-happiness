# Rebase explicit local list operations

Superseded for the initial roadmap by
[ADR 0007](0007-merge-stable-record-assignments.md). List rebasing is an optional
later experiment. The original decision and its limits are retained below as
historical context, not instructions for the first implementation.

Capture list insert, remove, and replace operations in changesets and transform
their positions through intervening committed operations. Preserve enough
history for active transactions instead of relying on snapshot differences
alone to reconstruct edit intent. This supports finer merging than whole-list
conflicts, at the cost of a separate transformation algorithm and explicit
overlap rules; it does not commit the project to distributed CRDT semantics.

Accepted limits on this design: only live local transactions, bounded history
with explicit too-old/limit errors, and single-element insert/remove/replace.
Same-gap insertions follow commit order; competing modifications of the same
existing element conflict. Transactions do not survive restart, and old retained
snapshots are not automatically writable branches.
