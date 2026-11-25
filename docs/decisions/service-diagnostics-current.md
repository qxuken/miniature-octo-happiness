# Shared service diagnostics

Both CLI binaries compile this module. Its unit tests run in each binary.
Each CLI also has process scenarios for normal and structured diagnostics.
The status codes and labels below are part of the operator-visible schema.

The formatter has 1,024 bytes including space for the newline. Tests prefill
it to every remaining length through an exact fit for four field layouts.
They verify that output is always a prefix of the expected fields, that an
unsuccessful write preserves the earlier prefix, and that unused bytes stay
zero. The layouts cover native details, an OS code alone, missing values and
the largest verification offset. The capture matrix independently varies
failure and opt-in and checks the original result and reported phase.

| Source | SHA-256 |
| --- | --- |
| `apps/support/service_diagnostics.rs` | `fb80728e4368315a21775a33e08081f321717c05066f9a06f49107c84c6bf4a9` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:257:22– | 10098..10100 | if | capture | The capture matrix varies enabled and failing independently and checks both returned values. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:257:25– | 10101..10136 | short_circuit_and | capture | The capture matrix varies enabled and failing independently and checks both returned values. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:266:5– | 10410..10412 | if | settings_from_args | Only a leading opt-in flag is consumed; the remaining arguments stay intact. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:282:5– | 10891..10896 | match | entry | Both CLI process scenarios include successful commands and failure exits. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:285:13– | 10993..10995 | if | entry | Both CLI process scenarios check the plain diagnostic and structured error paths. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:328:31– | 12231..12232 | try_propagation | write_str | Every byte capacity through exact fit is exercised; a full line rejects another byte and preserves the reserved terminator. |
| I | study-peer::study-peer | apps/support/service_diagnostics.rs:336:91– | 12537..12538 | try_propagation | fmt | The prefix starts empty; write_str appends complete valid Rust strings and advances len only after a successful copy. The initialized prefix is always UTF-8. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:340:5– | 12618..12623 | match | operation_name | Every native operation has an explicit expected schema label. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:359:5– | 13448..13453 | match | stage_name | Every verification stage has an explicit expected schema label. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:372:5– | 14019..14024 | match | optional_number | Signed OS error and unsigned offset values, as well as absent values, render exactly. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:31:9– | 1441..1446 | match | parse | Command table covers every recognized spelling, missing input and the unknown fallback. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:386:10– | 14442..14443 | try_propagation | fields | The field buffer sweep admits complete output and rejects insufficient room for the leading fields. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:387:9– | 14453..14455 | if | fields | Cases cover no native error, an OS code alone, and structured native detail. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:387:12– | 14456..14459 | pattern_test | fields | Cases cover no native error, an OS code alone, and structured native detail. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:388:28– | 14516..14521 | match | fields | Exact native schema labels cover permission, disk full, quota and other. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:398:14– | 14966..14967 | try_propagation | fields | The capacity sweep includes failure writing the native object prefix. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:399:54– | 15022..15023 | try_propagation | fields | The capacity sweep exercises the optional native error number. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:400:31– | 15055..15056 | try_propagation | fields | The capacity sweep includes the closing brace of native detail. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:401:16– | 15073..15075 | if | fields | Absent structured detail selects an OS-code object or null. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:401:19– | 15076..15079 | pattern_test | fields | Absent structured detail selects an OS-code object or null. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:405:14– | 15240..15241 | try_propagation | fields | The capacity sweep includes the OS-code-only object. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:407:34– | 15293..15294 | try_propagation | fields | The capacity sweep includes the null native field. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:409:44– | 15349..15350 | try_propagation | fields | The capacity sweep crosses the verification field key. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:410:9– | 15360..15362 | if | fields | Absent and present verification use different exact payloads. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:410:12– | 15363..15366 | pattern_test | fields | Absent and present verification use different exact payloads. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:415:14– | 15559..15560 | try_propagation | fields | The capacity sweep includes the verification object prefix. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:416:54– | 15615..15616 | try_propagation | fields | The capacity sweep includes an absent offset or the largest unsigned offset. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:426:9– | 15905..15907 | if | json | Only ship and ship-captured include the unconfirmed follower outcome in the command table. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:451:5– | 16781..16783 | if | incoming | Incoming counters render with and without the last diagnostic, including maximum unsigned values. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:451:8– | 16784..16787 | pattern_test | incoming | Incoming counters render with and without the last diagnostic, including maximum unsigned values. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:73:9– | 3349..3354 | match | text | Classifier and name tests exercise engine, I/O, TLS, invalid input and unclassified codes. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:74:36– | 3397..3402 | match | text | All public engine errors retain their public names. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:96:31– | 4434..4439 | match | text | Each named I/O kind and the fallback have exact expected text. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:142:5– | 6131..6133 | if | classify | An engine error selects engine; other concrete errors reach later branches. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:142:8– | 6134..6137 | pattern_test | classify | An engine error selects engine; other concrete errors reach later branches. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:145:12– | 6291..6293 | if | classify | An I/O error selects I/O; a direct TLS error reaches the next branch. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:145:15– | 6294..6297 | pattern_test | classify | An I/O error selects I/O; a direct TLS error reaches the next branch. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:150:9– | 6594..6596 | if | classify | I/O with a known TLS payload uses its TLS code; ordinary I/O and an opaque payload keep the I/O code. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:150:12– | 6597..6600 | pattern_test | classify | I/O with a known TLS payload uses its TLS code; ordinary I/O and an opaque payload keep the I/O code. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:157:12– | 6847..6849 | if | classify | Direct TLS errors select TLS while input and opaque errors reach the remaining checks. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:157:15– | 6850..6853 | pattern_test | classify | Direct TLS errors select TLS while input and opaque errors reach the remaining checks. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:160:12– | 7018..7020 | if | classify | Integer parse error, address parse error and an opaque error independently distinguish the two operands. |
| C | study-peer::study-peer | apps/support/service_diagnostics.rs:160:15– | 7021..7100 | short_circuit_or | classify | Integer parse error, address parse error and an opaque error independently distinguish the two operands. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:167:5– | 7271..7276 | match | tls_code | Direct and I/O-wrapped certificate, invalid-message and general TLS errors have exact labels. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:204:5– | 8532..8534 | if | propagate | Enabled and disabled captures propagate the original result; only enabled errors take the supplied diagnostic. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:205:18– | 8573..8575 | if | propagate | Success clears a previously stored diagnostic; failure can preserve its supplied diagnostic or clear it when absent. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:210:5– | 8740..8742 | if | at | Both capture settings run the operation and preserve its success value or error. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:215:5– | 8853..8858 | match | at | Successful operations clear context; nested failing operations preserve their phase. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:230:5– | 9279..9281 | if | storage | Both real programs execute storage operations with diagnostics enabled and disabled. |
| E | study-peer::study-peer | apps/support/service_diagnostics.rs:234:5– | 9362..9367 | match | storage | Successful database creation or description and missing/corrupt database errors exercise storage capture. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:257:22– | 10098..10100 | if | capture | The capture matrix varies enabled and failing independently and checks both returned values. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:257:25– | 10101..10136 | short_circuit_and | capture | The capture matrix varies enabled and failing independently and checks both returned values. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:266:5– | 10410..10412 | if | settings_from_args | Only a leading opt-in flag is consumed; the remaining arguments stay intact. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:282:5– | 10891..10896 | match | entry | Both CLI process scenarios include successful commands and failure exits. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:285:13– | 10993..10995 | if | entry | Both CLI process scenarios check the plain diagnostic and structured error paths. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:328:31– | 12231..12232 | try_propagation | write_str | Every byte capacity through exact fit is exercised; a full line rejects another byte and preserves the reserved terminator. |
| I | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:336:91– | 12537..12538 | try_propagation | fmt | The prefix starts empty; write_str appends complete valid Rust strings and advances len only after a successful copy. The initialized prefix is always UTF-8. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:340:5– | 12618..12623 | match | operation_name | Every native operation has an explicit expected schema label. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:359:5– | 13448..13453 | match | stage_name | Every verification stage has an explicit expected schema label. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:372:5– | 14019..14024 | match | optional_number | Signed OS error and unsigned offset values, as well as absent values, render exactly. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:31:9– | 1441..1446 | match | parse | Command table covers every recognized spelling, missing input and the unknown fallback. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:386:10– | 14442..14443 | try_propagation | fields | The field buffer sweep admits complete output and rejects insufficient room for the leading fields. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:387:9– | 14453..14455 | if | fields | Cases cover no native error, an OS code alone, and structured native detail. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:387:12– | 14456..14459 | pattern_test | fields | Cases cover no native error, an OS code alone, and structured native detail. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:388:28– | 14516..14521 | match | fields | Exact native schema labels cover permission, disk full, quota and other. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:398:14– | 14966..14967 | try_propagation | fields | The capacity sweep includes failure writing the native object prefix. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:399:54– | 15022..15023 | try_propagation | fields | The capacity sweep exercises the optional native error number. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:400:31– | 15055..15056 | try_propagation | fields | The capacity sweep includes the closing brace of native detail. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:401:16– | 15073..15075 | if | fields | Absent structured detail selects an OS-code object or null. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:401:19– | 15076..15079 | pattern_test | fields | Absent structured detail selects an OS-code object or null. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:405:14– | 15240..15241 | try_propagation | fields | The capacity sweep includes the OS-code-only object. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:407:34– | 15293..15294 | try_propagation | fields | The capacity sweep includes the null native field. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:409:44– | 15349..15350 | try_propagation | fields | The capacity sweep crosses the verification field key. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:410:9– | 15360..15362 | if | fields | Absent and present verification use different exact payloads. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:410:12– | 15363..15366 | pattern_test | fields | Absent and present verification use different exact payloads. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:415:14– | 15559..15560 | try_propagation | fields | The capacity sweep includes the verification object prefix. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:416:54– | 15615..15616 | try_propagation | fields | The capacity sweep includes an absent offset or the largest unsigned offset. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:426:9– | 15905..15907 | if | json | Only ship and ship-captured include the unconfirmed follower outcome in the command table. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:451:5– | 16781..16783 | if | incoming | Incoming counters render with and without the last diagnostic, including maximum unsigned values. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:451:8– | 16784..16787 | pattern_test | incoming | Incoming counters render with and without the last diagnostic, including maximum unsigned values. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:73:9– | 3349..3354 | match | text | Classifier and name tests exercise engine, I/O, TLS, invalid input and unclassified codes. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:74:36– | 3397..3402 | match | text | All public engine errors retain their public names. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:96:31– | 4434..4439 | match | text | Each named I/O kind and the fallback have exact expected text. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:142:5– | 6131..6133 | if | classify | An engine error selects engine; other concrete errors reach later branches. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:142:8– | 6134..6137 | pattern_test | classify | An engine error selects engine; other concrete errors reach later branches. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:145:12– | 6291..6293 | if | classify | An I/O error selects I/O; a direct TLS error reaches the next branch. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:145:15– | 6294..6297 | pattern_test | classify | An I/O error selects I/O; a direct TLS error reaches the next branch. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:150:9– | 6594..6596 | if | classify | I/O with a known TLS payload uses its TLS code; ordinary I/O and an opaque payload keep the I/O code. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:150:12– | 6597..6600 | pattern_test | classify | I/O with a known TLS payload uses its TLS code; ordinary I/O and an opaque payload keep the I/O code. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:157:12– | 6847..6849 | if | classify | Direct TLS errors select TLS while input and opaque errors reach the remaining checks. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:157:15– | 6850..6853 | pattern_test | classify | Direct TLS errors select TLS while input and opaque errors reach the remaining checks. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:160:12– | 7018..7020 | if | classify | Integer parse error, address parse error and an opaque error independently distinguish the two operands. |
| C | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:160:15– | 7021..7100 | short_circuit_or | classify | Integer parse error, address parse error and an opaque error independently distinguish the two operands. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:167:5– | 7271..7276 | match | tls_code | Direct and I/O-wrapped certificate, invalid-message and general TLS errors have exact labels. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:204:5– | 8532..8534 | if | propagate | Enabled and disabled captures propagate the original result; only enabled errors take the supplied diagnostic. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:205:18– | 8573..8575 | if | propagate | Success clears a previously stored diagnostic; failure can preserve its supplied diagnostic or clear it when absent. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:210:5– | 8740..8742 | if | at | Both capture settings run the operation and preserve its success value or error. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:215:5– | 8853..8858 | match | at | Successful operations clear context; nested failing operations preserve their phase. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:230:5– | 9279..9281 | if | storage | Both real programs execute storage operations with diagnostics enabled and disabled. |
| E | weekend-replicate::weekend-replicate | apps/support/service_diagnostics.rs:234:5– | 9362..9367 | match | storage | Successful database creation or description and missing/corrupt database errors exercise storage capture. |
