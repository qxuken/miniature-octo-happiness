# Typed catalog and table registration decisions

The [typed runtime audit](typed-runtime.md) provides the named T1 and T9
witnesses for table-name length, descriptor registration, primary IDs, table
ownership and catalog shape. The catalog test compares one valid registration
with independent replacements of its collection ID, type ID, version, unique
list and unique IDs. Each rejected registration preserves the revision.
An unsigned ID, a zero ID, a repeated ID, a scalar in place of the list and
a Boolean *inside* the list distinguish the successive checks. The last case
must translate the value conversion error to `InvalidCatalog`.

The table registration path now reserves its temporary unique-ID list through
the database allocator and passes the allocator to catalog parsing. The
returned table handle still owns its name key in an ordinary Rust vector.
The delegated rows retain the error-path limits in T5. The duplicate-ID
predicate compares a predecessor only after the first slot; T1 explains the
masked operand. The compound catalog identity predicate has three rejecting
inputs, each changing one field of the valid entry.

| Source | SHA-256 |
| --- | --- |
| `crates/weekend-db-typed/src/store.rs` | `1ba12c605448cd6fbf5b718f6920f189ec334f0d5d0e054603256d626f05d217` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:329:9– | 10088..10090 | if | table | Registration and descriptor controls (T1). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:329:12– | 10091..10126 | short_circuit_or | table | Registration and descriptor controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:333:42– | 10261..10262 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:336:41– | 10394..10395 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:338:53– | 10508..10509 | try_propagation | table | Selected allocator reserves the model’s unique-ID scratch before copying and sorting. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:341:9– | 10603..10606 | for_iteration | table | Registration and descriptor controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:342:42– | 10687..10688 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:342:73– | 10718..10719 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:343:13– | 10733..10735 | if | table | Registration and descriptor controls (T1). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:343:16– | 10736..10765 | short_circuit_and | table | Registration and descriptor controls (T1). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:343:16– | 10736..10813 | short_circuit_or | table | Registration and descriptor controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:350:49– | 11008..11009 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:352:25– | 11101..11102 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:353:9– | 11112..11114 | if | table | Registration and descriptor controls (T1). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:353:12– | 11115..11118 | pattern_test | table | Registration and descriptor controls (T1). |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:354:13– | 11157..11159 | if | table | Registration and descriptor controls (T1). |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:357:13– | 11265..11267 | if | table | Registration and descriptor controls (T1). |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:361:13– | 11418..11423 | match | table | Registration and descriptor controls (T1). |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:363:21– | 11549..11551 | if | table | Registration and descriptor controls (T1). |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:366:21– | 11730..11732 | if | table | Registration and descriptor controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:367:70– | 11832..11833 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:376:24– | 12201..12227 | match_guard | table | Registration and descriptor controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:381:58– | 12409..12410 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:382:23– | 12434..12439 | match | table | Registration and descriptor controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:384:48– | 12541..12542 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:394:26– | 12890..12891 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:395:55– | 12947..12948 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:396:20– | 12969..12970 | try_propagation | table | Registration delegates this failure to key reservation, schema, catalog or transaction operations. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:422:5– | 13932..13934 | if | info | Table provenance and catalog identity (T1, T9). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:426:63– | 14163..14164 | try_propagation | info | Catalog lookup, missing entry and parsing delegate to core read and catalog validation. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:427:59– | 14223..14224 | try_propagation | info | Catalog lookup, missing entry and parsing delegate to core read and catalog validation. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:429:6– | 14261..14262 | try_propagation | info | Catalog lookup, missing entry and parsing delegate to core read and catalog validation. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:430:5– | 14268..14270 | if | info | Table provenance and catalog identity (T1, T9). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:430:8– | 14271..14341 | short_circuit_or | info | Table provenance and catalog identity (T1, T9). |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:152:5– | 3991..3993 | if | id_key | Zero and valid primary IDs (T9). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:172:43– | 4520..4521 | try_propagation | parse | Catalog field access and reservation delegate to value traversal and allocator evidence. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:173:9– | 4531..4533 | if | parse | Catalog shape and identity controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:176:39– | 4656..4657 | try_propagation | parse | Catalog field access and reservation delegate to value traversal and allocator evidence. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:176:41– | 4658..4659 | try_propagation | parse | Catalog field access and reservation delegate to value traversal and allocator evidence. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:177:9– | 4669..4672 | for_iteration | parse | Catalog shape and identity controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:177:30– | 4690..4691 | try_propagation | parse | Catalog field access and reservation delegate to value traversal and allocator evidence. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:179:29– | 4747..4748 | try_propagation | parse | Catalog field access and reservation delegate to value traversal and allocator evidence. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:182:13– | 4841..4843 | if | parse | Catalog shape and identity controls (T1). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:182:16– | 4844..4875 | short_circuit_or | parse | Catalog shape and identity controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:188:46– | 5054..5055 | try_propagation | parse | Catalog field access and reservation delegate to value traversal and allocator evidence. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:189:43– | 5099..5100 | try_propagation | parse | Catalog field access and reservation delegate to value traversal and allocator evidence. |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:190:46– | 5147..5148 | try_propagation | parse | Catalog field access and reservation delegate to value traversal and allocator evidence. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:193:9– | 5189..5191 | if | parse | Catalog shape and identity controls (T1). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:193:12– | 5192..5232 | short_circuit_or | parse | Catalog shape and identity controls (T1). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:193:12– | 5192..5254 | short_circuit_or | parse | Catalog shape and identity controls (T1). |
| D | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:220:72– | 6134..6135 | try_propagation | create_on | ByteStore creation error delegated to engine creation tests. |
