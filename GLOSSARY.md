# weekend-db

Application state can be read at its current version or at retained historical
versions. These terms describe the versions exposed to a database client.

## Language

**Snapshot**:
A read-only view of application state at a particular version, unaffected by
later changes to that state.
_Avoid_: Moment, database copy

**Retained snapshot**:
A historical snapshot that remains available after closing and reopening the
database until the client explicitly releases its retention.
_Avoid_: Backup

**Base snapshot**:
The snapshot from which a transaction reads and against which it proposes
changes.
_Avoid_: Latest state, when referring to the transaction's starting version

**Changeset**:
The final record assignments and removals proposed by a transaction relative
to its base snapshot.
_Avoid_: Snapshot, byte diff, list operation history

**Record**:
A stable keyed entry whose value may contain nested maps, lists, and scalars.
It is the initial unit of conflicting updates, whether or not it has a schema.
_Avoid_: Native struct, individual nested field

**Transaction**:
A group of reads and proposed changes based on one snapshot. Its changes become
visible together if its commit succeeds.
_Avoid_: Changeset, when also referring to the transaction's reads

**Conflict**:
A reason a transaction cannot commit because an intervening transaction wrote
the same record or invalidated an observation, including conservatively
tracked observations of a collection.
_Avoid_: Corruption, merge winner

**Merge**:
Combining compatible changes from different changesets into one resulting
application state. The compatibility rules are part of the transaction contract.
_Avoid_: Last-writer-wins, unless that specific conflict rule is intended

**Record schema**:
A versioned description of a record type's fields, including identities that
remain stable across compatible changes such as field renaming.
_Avoid_: Native struct layout, engine file format

**Record migration**:
An application-defined transformation of stored records from one schema or
meaning to another.
_Avoid_: File-format conversion, automatic schema inference

