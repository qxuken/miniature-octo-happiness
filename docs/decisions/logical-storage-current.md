# Logical export and restore after allocator and encoder changes

The [operations audit](operations.md) defines O1 and O2 and the original
status of each logical-storage decision. The current inventory matches
the original on all 152 owners, decision kinds and alternatives. All
condition expressions match for the 148 rows below. The current offsets
account for the source edits. Delegated rows keep their original limits.

Four changed propagation sites have individual mutation controls. The map
writer borrows ordered entries using encoder scratch, then writes each key
and value. The restore preflight encodes a fresh physical image to check
representability before creating a destination.

The exported buffer uses Global. `export_logical_on` constructs its own
`Memory::new(limits)`; the default allocator handle has no custom provider.
For this case `ChargedVec::into_external` transfers the existing pointer,
length and capacity directly into the returned vector. The fallible copy
inside that helper is reachable for a custom provider in other callers.
An exploratory mutation that translated a transfer error survived the
Global refusal sweep on both hosts, consistent with this caller invariant.
Earlier allocation failures are still exercised by the read-only export
test. The transfer is outside the AST decision count.

| Source | SHA-256 |
| --- | --- |
| `src/storage/logical.rs` | `754297e6d5ed285ef4bcbe69621d59baab4d14e6664b43ad2ac34c938d514836` |

| Status | Target | Position | Bytes | Kind | Owner | Original evidence group |
| --- | --- | --- | --- | --- | --- | --- |
| P | weekend-db::weekend_db | src/storage/logical.rs:21:9– | 654..656 | if | bytes | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:29:55– | 899..900 | try_propagation | bytes | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:37:37– | 1146..1147 | try_propagation | blob | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:42:9– | 1269..1271 | if | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:45:42– | 1386..1387 | try_propagation | value | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:46:9– | 1397..1402 | match | value | O1 |
| I | weekend-db::weekend_db | src/storage/logical.rs:48:64– | 1520..1521 | try_propagation | value | O1 |
| I | weekend-db::weekend_db | src/storage/logical.rs:49:53– | 1577..1578 | try_propagation | value | O1 |
| I | weekend-db::weekend_db | src/storage/logical.rs:50:54– | 1641..1642 | try_propagation | value | O1 |
| I | weekend-db::weekend_db | src/storage/logical.rs:51:51– | 1695..1696 | try_propagation | value | O1 |
| I | weekend-db::weekend_db | src/storage/logical.rs:52:54– | 1762..1763 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:55:47– | 1912..1913 | try_propagation | value | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:56:17– | 1931..1934 | for_iteration | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:63:37– | 2157..2158 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:63:46– | 2166..2167 | try_propagation | value | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:64:17– | 2185..2188 | for_iteration | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:64:44– | 2212..2213 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:65:54– | 2269..2270 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:65:56– | 2271..2272 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:72:37– | 2444..2445 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:73:23– | 2469..2470 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:75:67– | 2587..2588 | try_propagation | data | O1 |
| I | weekend-db::weekend_db | src/storage/logical.rs:78:78– | 2754..2755 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:82:11– | 2831..2832 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:84:43– | 2933..2934 | try_propagation | data | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:85:9– | 2944..2947 | for_iteration | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:86:25– | 3006..3007 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:87:42– | 3050..3051 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:89:69– | 3170..3171 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:93:15– | 3313..3314 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:95:43– | 3417..3418 | try_propagation | data | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:96:13– | 3432..3435 | for_iteration | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:97:31– | 3493..3494 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:98:42– | 3537..3538 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:99:42– | 3581..3582 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:109:34– | 3919..3920 | try_propagation | export_logical_on | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:110:51– | 3972..3973 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:111:63– | 4037..4038 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:117:24– | 4171..4172 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:118:24– | 4197..4198 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:119:38– | 4237..4238 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:120:40– | 4279..4280 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:121:45– | 4326..4327 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:122:44– | 4372..4373 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:123:57– | 4431..4432 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:125:66– | 4537..4538 | try_propagation | export_logical_on | O2 |
| E | weekend-db::weekend_db | src/storage/logical.rs:127:5– | 4606..4609 | for_iteration | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:128:28– | 4676..4677 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:129:39– | 4717..4718 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:130:44– | 4763..4764 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:135:34– | 4979..4980 | try_propagation | export_logical_on | O2 |
| E | weekend-db::weekend_db | src/storage/logical.rs:136:5– | 4986..4989 | for_iteration | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:137:32– | 5044..5045 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:139:42– | 5094..5095 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:142:32– | 5284..5285 | try_propagation | export_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:153:82– | 5556..5557 | try_propagation | take | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:158:43– | 5692..5693 | try_propagation | int | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:161:43– | 5823..5824 | try_propagation | count | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:161:71– | 5851..5852 | try_propagation | count | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:162:9– | 5862..5864 | if | count | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:169:56– | 6066..6067 | try_propagation | blob | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:171:30– | 6126..6127 | try_propagation | blob | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:175:9– | 6225..6227 | if | value | O1 |
| C | weekend-db::weekend_db | src/storage/logical.rs:175:12– | 6228..6289 | short_circuit_or | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:178:31– | 6370..6371 | try_propagation | value | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:179:9– | 6384..6389 | match | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:188:38– | 6689..6690 | try_propagation | value | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:190:21– | 6778..6780 | if | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:198:40– | 7010..7011 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:202:66– | 7162..7163 | try_propagation | value | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:205:17– | 7288..7291 | for_iteration | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:206:42– | 7345..7346 | try_propagation | value | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:207:21– | 7368..7370 | if | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:213:54– | 7640..7641 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:214:57– | 7699..7700 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:220:66– | 7907..7908 | try_propagation | value | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:222:17– | 7970..7973 | for_iteration | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:223:54– | 8039..8040 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:224:54– | 8095..8096 | try_propagation | value | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:232:34– | 8322..8323 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:233:30– | 8354..8355 | try_propagation | data | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:234:9– | 8365..8367 | if | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:237:58– | 8487..8488 | try_propagation | data | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:240:9– | 8573..8576 | for_iteration | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:241:32– | 8620..8621 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:242:37– | 8659..8660 | try_propagation | data | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:243:13– | 8674..8676 | if | data | O1 |
| C | weekend-db::weekend_db | src/storage/logical.rs:243:16– | 8677..8708 | short_circuit_or | data | O1 |
| C | weekend-db::weekend_db | src/storage/logical.rs:243:16– | 8677..8724 | short_circuit_or | data | O1 |
| C | weekend-db::weekend_db | src/storage/logical.rs:243:16– | 8677..8746 | short_circuit_or | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:247:66– | 8902..8903 | try_propagation | data | O1 |
| E | weekend-db::weekend_db | src/storage/logical.rs:250:13– | 9020..9023 | for_iteration | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:251:38– | 9077..9078 | try_propagation | data | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:252:17– | 9096..9098 | if | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:258:41– | 9335..9336 | try_propagation | data | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:259:17– | 9354..9356 | if | data | O1 |
| C | weekend-db::weekend_db | src/storage/logical.rs:259:20– | 9357..9390 | short_circuit_or | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:262:42– | 9500..9501 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:270:18– | 9741..9742 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:280:14– | 10029..10030 | try_propagation | data | O1 |
| D | weekend-db::weekend_db | src/storage/logical.rs:287:15– | 10200..10201 | try_propagation | data | O1 |
| P | weekend-db::weekend_db | src/storage/logical.rs:299:5– | 10407..10409 | if | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:302:5– | 10486..10488 | if | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:306:5– | 10612..10614 | if | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:306:8– | 10615..10706 | short_circuit_or | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:309:5– | 10765..10767 | if | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:317:35– | 11012..11013 | try_propagation | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:318:5– | 11039..11041 | if | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:321:32– | 11137..11138 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:322:37– | 11176..11177 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:323:31– | 11209..11210 | try_propagation | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:324:5– | 11216..11218 | if | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:324:8– | 11219..11259 | short_circuit_or | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:327:53– | 11356..11357 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:329:35– | 11432..11433 | try_propagation | decode | O2 |
| E | weekend-db::weekend_db | src/storage/logical.rs:331:5– | 11465..11468 | for_iteration | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:332:30– | 11514..11515 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:333:36– | 11552..11553 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:334:32– | 11586..11587 | try_propagation | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:335:9– | 11597..11599 | if | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:335:12– | 11600..11643 | short_circuit_or | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:335:12– | 11600..11679 | short_circuit_or | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:335:12– | 11600..11710 | short_circuit_or | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:335:12– | 11600..11746 | short_circuit_or | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:346:57– | 11934..11935 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:348:34– | 12008..12009 | try_propagation | decode | O2 |
| E | weekend-db::weekend_db | src/storage/logical.rs:349:5– | 12015..12018 | for_iteration | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:350:41– | 12079..12080 | try_propagation | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:351:9– | 12090..12092 | if | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:351:12– | 12093..12311 | short_circuit_or | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:352:17– | 12144..12310 | short_circuit_and | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:359:9– | 12380..12382 | if | decode | O2 |
| C | weekend-db::weekend_db | src/storage/logical.rs:359:12– | 12383..12489 | short_circuit_and | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:366:5– | 12593..12595 | if | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:372:31– | 12805..12806 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:377:7– | 12938..12939 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:379:43– | 13024..13025 | try_propagation | decode | O2 |
| E | weekend-db::weekend_db | src/storage/logical.rs:380:5– | 13031..13034 | for_iteration | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:384:35– | 13222..13223 | try_propagation | decode | O2 |
| P | weekend-db::weekend_db | src/storage/logical.rs:385:9– | 13233..13235 | if | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:395:15– | 13573..13574 | try_propagation | decode | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:422:6– | 14226..14227 | try_propagation | install | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:431:34– | 14495..14496 | try_propagation | import_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:432:36– | 14533..14534 | try_propagation | import_logical_on | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:437:61– | 14798..14799 | try_propagation | export_logical_file | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:446:34– | 15110..15111 | try_propagation | import_logical_file | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:447:36– | 15148..15149 | try_propagation | import_logical_file | O2 |
| D | weekend-db::weekend_db | src/storage/logical.rs:448:50– | 15200..15201 | try_propagation | import_logical_file | O2 |
