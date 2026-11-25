# Typed lookup, repair and migration decisions

Lookup checks the catalog’s unique-field list before invoking application
encoding. The admission test observes the error for a declared field without a unique index and
the application's distinct error for a registered index. Catching the latter
inside a write still aborts the staged primary row and index claim. A fresh
read sees the original revision and the retained checkpoint resolves the
original owner.

The kind guard has three operands: declared kind differs, encoded value is
non-null, encoded kind differs. The existing typed fixture uses declared-wrong
null, declared-correct null, wrong non-null and correct non-null inputs.
These give pairs for all three operands, including the null exemption.
The stale-default fixture leaves a physically missing primary field behind a
real claim. The corrupt-owner fixture separately supplies a primary with
different bytes. Each rejects with `IndexInconsistent`.

## Failure scopes

- [Physical indexes, corruption and retained state (T3)](typed-runtime.md)
  covers lookup misses, decoding and stale claims. Absent fields skip
  encoding, explicit null has no key and physical scalar values produce one.
- [Index allocator and repair fixtures](typed-index-allocator-current.md)
  exercise scans, sorting, missing and mismatched owners, duplicate claims
  and failure atomicity. A loop row records execution over its fixture.
- [Unique-claim replacement](typed-unique-claims-current.md) covers both old
  and new key sets, unchanged claims, duplicate ownership and final writes.
- [Migration and recovery](typed-migration-recovery-current.md) covers both
  traversal modes, an earlier staged row, callback failure, allocation
  refusal and retained roots. Delegated propagation rows identify the
  operation exercised; individual error injection at every site remains open.

| Source | SHA-256 |
| --- | --- |
| `crates/weekend-db-typed/src/store.rs` | `1ba12c605448cd6fbf5b718f6920f189ec334f0d5d0e054603256d626f05d217` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:528:90– | 17039..17040 | try_propagation | record_keys | Schema::load`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:529:25– | 17066..17067 | try_propagation | record_keys | schema.validate`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:531:43– | 17165..17166 | try_propagation | record_keys | reserve`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:532:5– | 17172..17175 | for_iteration | record_keys | Physical missing, explicit null and stored value (T3). |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:533:9– | 17206..17208 | if | record_keys | Physical missing, explicit null and stored value (T3). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:533:12– | 17209..17212 | pattern_test | record_keys | Physical missing, explicit null and stored value (T3). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:533:12– | 17209..17334 | short_circuit_and | record_keys | Physical missing, explicit null and stored value (T3). |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:533:44– | 17241..17242 | try_propagation | record_keys | raw.field`. |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:534:16– | 17258..17261 | pattern_test | record_keys | Physical missing, explicit null and stored value (T3). |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:534:91– | 17333..17334 | try_propagation | record_keys | index_key`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:551:41– | 17747..17748 | try_propagation | index_state | info`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:554:5– | 17837..17840 | for_iteration | index_state | Missing, mismatched, duplicate and orphaned claims in the repair fixtures. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:554:60– | 17892..17893 | try_propagation | index_state | view.scan_index_rows`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:556:38– | 17955..17956 | try_propagation | index_state | row_id`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:557:44– | 18001..18002 | try_propagation | index_state | Record::from_value`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:558:9– | 18012..18015 | for_iteration | index_state | Missing, mismatched, duplicate and orphaned claims in the repair fixtures. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:558:68– | 18071..18072 | try_propagation | index_state | record_keys::<T>`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:561:72– | 18199..18200 | try_propagation | index_state | expected.try_reserve`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:566:5– | 18313..18316 | for_iteration | index_state | Missing, mismatched, duplicate and orphaned claims in the repair fixtures. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:567:9– | 18355..18357 | if | index_state | Missing, mismatched, duplicate and orphaned claims in the repair fixtures. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:577:72– | 18941..18942 | try_propagation | index_state | view.scan_index_rows`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:591:5– | 19348..19351 | for_iteration | index_state | Missing, mismatched, duplicate and orphaned claims in the repair fixtures. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:592:9– | 19385..19390 | match | index_state | Missing, mismatched, duplicate and orphaned claims in the repair fixtures. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:594:26– | 19542..19584 | match_guard | index_state | Missing, mismatched, duplicate and orphaned claims in the repair fixtures. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:619:41– | 20215..20216 | try_propagation | find | info`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:623:94– | 20409..20410 | try_propagation | find | Schema::load`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:625:25– | 20463..20464 | try_propagation | find | descriptor.field`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:626:44– | 20508..20509 | try_propagation | find | descriptor.field`. |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:628:8– | 20569..20656 | short_circuit_or | find | Kind, presence, hit and inconsistent-owner fixtures (T3). |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:628:34– | 20595..20655 | short_circuit_and | find | Kind, presence, hit and inconsistent-owner fixtures (T3). |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:634:5– | 20810..20813 | let_else | find | Kind, presence, hit and inconsistent-owner fixtures (T3). |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:634:87– | 20892..20893 | try_propagation | find | index_key`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:637:5– | 20937..20940 | let_else | find | Kind, presence, hit and inconsistent-owner fixtures (T3). |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:637:68– | 21000..21001 | try_propagation | find | view.get_record`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:640:63– | 21103..21104 | try_propagation | find | id.as_u64`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:641:42– | 21147..21148 | try_propagation | find | get`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:641:75– | 21180..21181 | try_propagation | find | get`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:642:90– | 21272..21273 | try_propagation | find | Schema::load`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:645:36– | 21467..21468 | try_propagation | find | doc.record`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:649:44– | 21600..21601 | try_propagation | find | schema.read_field`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:650:41– | 21642..21643 | try_propagation | find | schema.read_field`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:651:73– | 21717..21718 | try_propagation | find | index_key`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:702:63– | 23748..23749 | try_propagation | rebuild_indexes | index_state`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:704:13– | 23818..23821 | for_iteration | rebuild_indexes | Repair fixtures compare primary bytes, expected owners and retained state. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:705:17– | 23866..23868 | if | rebuild_indexes | Repair fixtures compare primary bytes, expected owners and retained state. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:710:55– | 24102..24103 | try_propagation | rebuild_indexes | w.tx.delete`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:713:13– | 24149..24152 | for_iteration | rebuild_indexes | Repair fixtures compare primary bytes, expected owners and retained state. |
| P | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:719:17– | 24462..24464 | if | rebuild_indexes | Repair fixtures compare primary bytes, expected owners and retained state. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:720:69– | 24557..24558 | try_propagation | rebuild_indexes | w.store.db.unsigned`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:720:71– | 24559..24560 | try_propagation | rebuild_indexes | w.tx.put`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:790:56– | 27267..27268 | try_propagation | migrate | info`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:799:14– | 27564..27565 | try_propagation | migrate | Schema::load`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:801:13– | 27610..27613 | for_iteration | migrate | All-row migration with old and current rows (T4). |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:801:62– | 27659..27660 | try_propagation | migrate | w.tx.scan_iter`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:806:42– | 27843..27844 | try_propagation | migrate | row_id`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:808:18– | 27905..27906 | try_propagation | migrate | w.migrate_record`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:835:56– | 29110..29111 | try_propagation | migrate_ids | info`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:844:14– | 29407..29408 | try_propagation | migrate_ids | Schema::load`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:848:56– | 29553..29554 | try_propagation | migrate_ids | id_key`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:848:58– | 29555..29556 | try_propagation | migrate_ids | w.tx.get`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:856:18– | 29882..29883 | try_propagation | migrate_ids | w.migrate_record`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:894:29– | 31031..31032 | try_propagation | put_raw | id_key`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:897:38– | 31110..31111 | try_propagation | put_raw | self.tx.get`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:899:25– | 31173..31174 | try_propagation | put_raw | self.tx.get`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:903:25– | 31296..31297 | try_propagation | put_raw | old.as_ref`. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:905:51– | 31420..31421 | try_propagation | put_raw | self.keys::<T>`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:907:9– | 31485..31488 | for_iteration | put_raw | Unique-claim replacement, competing owner and rollback fixtures. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:908:41– | 31553..31554 | try_propagation | put_raw | self.tx.get`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:912:9– | 31685..31688 | for_iteration | put_raw | Unique-claim replacement, competing owner and rollback fixtures. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:913:62– | 31778..31779 | try_propagation | put_raw | self.tx.get`. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:923:9– | 32158..32161 | for_iteration | put_raw | Unique-claim replacement, competing owner and rollback fixtures. |
| E | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:928:9– | 32322..32325 | for_iteration | put_raw | Unique-claim replacement, competing owner and rollback fixtures. |
| D | weekend-db-typed::weekend_db_typed | Delegated to `crates/weekend-db-typed/src/store.rs:930:70– | 32476..32477 | try_propagation | put_raw | self.store.db.unsigned`. |
