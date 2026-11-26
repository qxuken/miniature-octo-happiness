# Linux supervisor process identity

The supervisor decodes Linux process identity from the bytes of a procfs
stat record. A fixture exercises command names containing parentheses,
non-UTF-8 bytes and a newline, checks field 22 against the full unsigned
range, and verifies that a later field does not displace it. A live check
compares the process table and the observer's own stat record.

Twelve controls preserve the first malformed field's error. Missing
delimiters or required fields retain their exact diagnostic. Invalid
UTF-8 outside the command name retains the UTF-8 error. Invalid or
out-of-range integers retain their parse error. Each failure changes
the first affected field while retaining the preceding fields.

| Decision | Accepted or reaped case | First condition false | Second condition false |
| --- | --- | --- | --- |
| Delimiters are ordered and followed by a space | Ordinary valid row | End precedes start with a trailing space | Correct order without the space |
| Reported PID matches and is positive | PID 42 matches | Reported 43 for requested 42 | Both values zero |
| State has one alphabetic ASCII byte | R | RR | 8 |
| Reaped identity has parent zero and group minus one | Parent 0, group -1 | Parent 1, group -1 | Parent 0, group 0 |
| Live parent and group are nonnegative | Parent 1, group 42 | Parent -1, group 42 | Parent 1, group -1 |

The independent delimiter, PID, state and sign pairs exercise guards in
the existing error-checking macro. The generated branches retain their
separate compiler coverage scope. The reaped test also verifies a live
identity with parent and group zero. A negative group with a nonzero
parent yields an error, so a malformed row cannot silently disappear
as if reaped. PID and state guards run before later field conversions.

Both source representations of the reaped decision and the twelve
propagation records have named source dispositions. The 23 controls
also cover removing the individual macro predicates and choosing the
first closing delimiter instead of the last one.

| Source | SHA-256 |
| --- | --- |
| `xtask/src/supervise.rs` | `0e4c2f9c5df44fd47c4ff1abbe27439d5e201928cbef9d54d572620ccb9e8a41` |
