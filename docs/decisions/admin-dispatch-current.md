# Administrative CLI decision audit

The [original command-line audit](command-line.md) defines group A1.
Nineteen rows retain their original owner, decision kind, conditions and
alternatives at the current offsets. The expanded dispatch, export helper
call and Windows compact guard have separate rows. The Unix compact guard
has a matched command and an unknown command with the same arity. Its
mutations reject a supported command or accept the unknown one. Busy-file
compaction and scripted lock and write failures check the propagated errors.

| Source | SHA-256 |
| --- | --- |
| `tools/admin.rs` | `5bf45aecb59283e8d3f5c3192fcc01fdf0abe983219402f2b7daba31a40611fb` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend-admin | tools/admin.rs:24:5– | 717..722 | match | run | Process fixtures exercise dispatch by arity, each supported command and the invalid arm. |
| P | weekend-db::weekend-admin | tools/admin.rs:25:43– | 772..792 | match_guard | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:26:87– | 884..885 | try_propagation | run | A1 |
| P | weekend-db::weekend-admin | tools/admin.rs:35:28– | 1305..1323 | match_guard | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:36:70– | 1398..1399 | try_propagation | run | A1 |
| E | weekend-db::weekend-admin | tools/admin.rs:46:13– | 1807..1810 | for_iteration | run | A1 |
| B | weekend-db::weekend-admin | tools/admin.rs:64:25– | 2460..2480 | match_guard | run | Native Windows rejects compact as unsupported. The new same-arity unknown-command witness awaits native Windows execution. |
| P | weekend-db::weekend-admin | tools/admin.rs:65:43– | 2571..2614 | match_guard | run | A1 |
| C | weekend-db::weekend-admin | tools/admin.rs:65:43– | 2571..2614 | short_circuit_or | run | A1 |
| P | weekend-db::weekend-admin | tools/admin.rs:66:25– | 2644..2646 | if | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:67:77– | 2745..2746 | try_propagation | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:69:78– | 2845..2846 | try_propagation | run | A1 |
| P | weekend-db::weekend-admin | tools/admin.rs:77:43– | 3080..3099 | match_guard | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:78:80– | 3184..3185 | try_propagation | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:79:67– | 3253..3254 | try_propagation | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:80:44– | 3299..3300 | try_propagation | run | Logical CLI export checks delivered bytes. The callee checks lock, write and sync errors with its scripted ByteStore. |
| P | weekend-db::weekend-admin | tools/admin.rs:84:43– | 3424..3443 | match_guard | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:85:86– | 3534..3535 | try_propagation | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:89:52– | 3728..3729 | try_propagation | run | A1 |
| D | weekend-db::weekend-admin | tools/admin.rs:90:81– | 3811..3812 | try_propagation | run | A1 |
| P | weekend-db::weekend-admin | tools/admin.rs:218:5– | 8078..8080 | if | main | A1 |
| C | weekend-db::weekend-admin | tools/admin.rs:218:8– | 8081..8084 | pattern_test | main | A1 |
