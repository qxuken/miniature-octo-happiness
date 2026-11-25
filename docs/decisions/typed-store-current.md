# Typed store decision evidence on the current source

97 decisions across 29 function names reuse the groups in the [typed runtime audit](typed-runtime.md).
For each row we compared the entire function signature and body against commit
`1ca5e5ab`. Both files contain a single occurrence of that body. The scanner
reported the same target, owner, decision kind, alternatives, condition bytes
and relative source offsets. The rows retain their original evidence strength,
including 77 delegated propagations. Edited functions require separate review.

## Unique index keys

The models test `scalar_boundaries_binary_lookup_and_float_bit_identity_roundtrip`
reads collection 3 directly after inserting one record. It requires exactly five
claims owned by record 1 and constructs their keys independently from the
collection ID, field ID, numeric kind tag and payload. The values cover true,
a negative signed integer, an unsigned integer, a NaN with a specific payload
and binary data containing a zero byte. The allocation sweep
`every_core_allocation_failure_aborts_primary_and_index_updates` checks the
error kind, unchanged revision, primary rows and index rows after a refusal.
The named reserve control is bound separately in the scope manifest.

| Source | SHA-256 |
| --- | --- |
| `crates/weekend-db-typed/src/store.rs` | `1ba12c605448cd6fbf5b718f6920f189ec334f0d5d0e054603256d626f05d217` |

| Status | Target | Location | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:108:9– | 2532..2571 | short_circuit_and | is_consistent | T1/T4/T5 |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:108:9– | 2532..2595 | short_circuit_and | is_consistent | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:146:21– | 3797..3798 | try_propagation | field | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:149:20– | 3899..3900 | try_propagation | uint | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:159:77– | 4199..4200 | try_propagation | row_id | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:160:15– | 4217..4218 | try_propagation | row_id | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:199:35– | 5427..5428 | try_propagation | encode | T1/T4/T5 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:200:9– | 5438..5441 | for_iteration | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:201:56– | 5518..5519 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:201:58– | 5520..5521 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:204:19– | 5565..5566 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:205:60– | 5626..5627 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:205:62– | 5628..5629 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:206:57– | 5686..5687 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:206:59– | 5688..5689 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:207:60– | 5749..5750 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:207:62– | 5751..5752 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:208:41– | 5793..5794 | try_propagation | encode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:213:41– | 5895..5896 | try_propagation | memory | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:226:63– | 6334..6335 | try_propagation | open_on | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:230:72– | 6557..6558 | try_propagation | create_file | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:234:73– | 6779..6780 | try_propagation | open_file | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:240:59– | 7024..7025 | try_propagation | compact_file | T1/T4/T5 |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:246:9– | 7320..7322 | if | initialize | T6 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:249:9– | 7420..7423 | for_iteration | initialize | T1/T4/T5 |
| I | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:250:13– | 7456..7458 | if | initialize | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:250:38– | 7481..7482 | try_propagation | initialize | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:255:60– | 7673..7674 | try_propagation | initialize | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:255:62– | 7675..7676 | try_propagation | initialize | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:256:20– | 7697..7698 | try_propagation | initialize | T1/T4/T5 |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:263:13– | 7862..7864 | if | from_database | T6 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:271:13– | 8114..8117 | for_iteration | from_database | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:272:46– | 8177..8178 | try_propagation | from_database | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:285:43– | 8581..8582 | try_propagation | compact_to | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:288:56– | 8691..8692 | try_propagation | checkpoint | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:291:34– | 8781..8782 | try_propagation | history | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:294:40– | 8891..8892 | try_propagation | release_checkpoint | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:309:52– | 9343..9344 | try_propagation | read_retained | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:319:39– | 9700..9701 | try_propagation | write | T1/T4/T5 |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:320:9– | 9711..9713 | if | write | T10 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:323:26– | 9826..9827 | try_propagation | write | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:404:67– | 13270..13271 | try_propagation | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:405:43– | 13315..13316 | try_propagation | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:406:50– | 13367..13368 | try_propagation | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:409:60– | 13482..13483 | try_propagation | restore_table | T1/T4/T5 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:410:17– | 13501..13504 | for_iteration | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:410:57– | 13541..13542 | try_propagation | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:411:61– | 13605..13606 | try_propagation | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:411:63– | 13607..13608 | try_propagation | restore_table | T1/T4/T5 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:413:17– | 13644..13647 | for_iteration | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:414:58– | 13728..13729 | try_propagation | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:414:86– | 13756..13757 | try_propagation | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:414:88– | 13758..13759 | try_propagation | restore_table | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:441:40– | 14570..14571 | try_propagation | decode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:445:90– | 14746..14747 | try_propagation | decode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:446:41– | 14789..14790 | try_propagation | decode | T1/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:460:29– | 15036..15037 | try_propagation | get | T3/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:461:47– | 15085..15086 | try_propagation | get | T3/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:461:49– | 15087..15088 | try_propagation | get | T3/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:470:29– | 15303..15304 | try_propagation | list | T3/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:471:48– | 15353..15354 | try_propagation | list | T3/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:473:34– | 15419..15420 | try_propagation | list | T3/T5 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:474:5– | 15426..15429 | for_iteration | list | T3/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:475:50– | 15502..15503 | try_propagation | list | T3/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:475:59– | 15511..15512 | try_propagation | list | T3/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:659:62– | 22049..22050 | try_propagation | check_indexes | T3/T5 |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:678:9– | 22754..22756 | if | checked | T6 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:682:9– | 22933..22938 | match | checked | T3/T4/T5 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:688:17– | 23090..23092 | if | checked | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:696:67– | 23410..23411 | try_propagation | check_indexes | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:742:56– | 25406..25407 | try_propagation | insert | T3/T4/T5 |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:746:13– | 25535..25537 | if | insert | T6 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:746:51– | 25573..25574 | try_propagation | insert | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:746:53– | 25575..25576 | try_propagation | insert | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:754:14– | 25856..25857 | try_propagation | insert | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:755:82– | 25940..25941 | try_propagation | insert | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:755:84– | 25942..25943 | try_propagation | insert | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:766:56– | 26254..26255 | try_propagation | update | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:767:61– | 26317..26318 | try_propagation | update | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:768:63– | 26381..26382 | try_propagation | update | T3/T4/T5 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:769:13– | 26396..26398 | if | update | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:772:35– | 26530..26531 | try_propagation | update | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:773:80– | 26612..26613 | try_propagation | update | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:778:56– | 26798..26799 | try_propagation | delete | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:869:71– | 30259..30260 | try_propagation | migrate_record | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:879:57– | 30607..30608 | try_propagation | migrate_record | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:880:10– | 30619..30620 | try_propagation | migrate_record | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:937:29– | 32712..32713 | try_propagation | remove_raw | T3/T4/T5 |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:938:9– | 32723..32726 | let_else | remove_raw | T6 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:938:57– | 32771..32772 | try_propagation | remove_raw | T3/T4/T5 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:941:9– | 32829..32832 | for_iteration | remove_raw | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:941:71– | 32891..32892 | try_propagation | remove_raw | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:941:73– | 32893..32894 | try_propagation | remove_raw | T3/T4/T5 |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:943:13– | 32967..32969 | if | remove_raw | T6 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:943:42– | 32996..32997 | try_propagation | remove_raw | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:946:42– | 33151..33152 | try_propagation | remove_raw | T3/T4/T5 |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:948:44– | 33207..33208 | try_propagation | remove_raw | T3/T4/T5 |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:490:24– | 15839..15844 | match | index_key | Five scalar payloads read back through raw snapshot lookup; T1/T2 cover container rejection and absent keys. |
| I | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:493:47– | 15972..15973 | try_propagation | index_key | The kind arm selects the matching accessor on the same immutable value. |
| I | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:497:36– | 16087..16088 | try_propagation | index_key | The kind arm selects the matching accessor on the same immutable value. |
| I | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:501:36– | 16197..16198 | try_propagation | index_key | The kind arm selects the matching accessor on the same immutable value. |
| I | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:505:36– | 16304..16305 | try_propagation | index_key | The kind arm selects the matching accessor on the same immutable value. |
| I | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:508:40– | 16400..16401 | try_propagation | index_key | The kind arm selects the matching accessor on the same immutable value. |
