# Retain historical snapshots explicitly

The current committed state is always available, while clients explicitly
retain and release historical snapshots by ID; retention survives reopening.
This gives applications control over undo history instead of requiring every
commit to remain accessible forever. Later reclamation must preserve every
retained snapshot as well as any state still needed by active readers or
transactions; releasing retention is not itself immediate reclamation.
