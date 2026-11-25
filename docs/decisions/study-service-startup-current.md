# Study service startup

The native service starts either with a collection ID or with a policy
file. The process test supplies equivalent enrollment through `run`
and `run-policy`. Both reach readiness, report the same default Study
record at revision 2 and quit without changing the database bytes. An
unrecognized command at the same arity exits without reaching readiness.

| Stage | Independent failure probes |
| --- | --- |
| Direct enrollment | An unrecognized actor returns the enrollment error. |
| Policy enrollment | An unreadable actor argument is rejected before the policy is read. A missing policy preserves the I/O error. An unsupported policy preserves the engine error. A valid policy with an unrecognized actor preserves the actor error. |
| Credentials and pin | Missing credentials report the original I/O error even when the pin is malformed. A missing pin reports its own error. A malformed pin reports the rustls error before the database is opened. |
| Opening the file | A missing database reports `NotFound` before an invalid listener address is parsed, and the path remains absent. |
| Local and remote addresses | Each non-Unicode argument and each syntactically invalid address preserves its separate error. An occupied listener reports the same native error kind and OS code as a direct bind. A peer-address failure allows immediate rebinding of the successfully opened listener. |
| Opening the replica | A valid policy with the other configured actor returns `ForeignStore` on the existing actor A file. |

The focused test checks the original bytes and acquires the file lease
again after every refusal. Reopening reports revision 2. The listener
flags and address query inside the service, native worker failures and
the full peer status matrix remain open.
