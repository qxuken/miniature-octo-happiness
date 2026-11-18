# Recovery VALUE object decisions

The VALUE reader admits each reference, reuses verified shared values and
decodes the payload. The [DATA ledger](codec-data-current.md) covers the
directory that supplies the top-level value reference.

| Decision | Cases and outcomes |
|---|---|
| Depth and backward reference | The existing traversal fixture opens at its full depth and rejects before reading its leaf when the depth limit is one lower. A forward reference rejects. Removing only the forward guard survives that test because the later cached or uncached extent check rejects the same request. The combined guard and its OR retain B. |
| Cache lookup | Shared values decode once and retain node identity. Removing reuse makes the previously occupied object's second decode fail. An unaligned reference still rejects even when its region's aligned start was cached. |
| Cached end | A successful parent object and a cached child occupy disjoint aligned regions. The preceding reference check ensures the child starts before the parent. The cached child therefore ends at or before that parent. The end check retains I, scoped to the public traversal invariant. The isolated removal probe survives. |
| Tag and scalar kind | The roundtrip covers all five scalar kinds plus bytes, map and list. An unknown tag with a complete one-word payload rejects. The malformed boolean case normalizes Invalid to Corrupt. Injected allocation failure preserves OutOfMemory. |
| Payload fields and ordering | Every truncated prefix and one trailing byte reject for the fixture's integer, bytes, map and list. A repeated map key rejects. More map or list entries than the configured bound return Limit. Forward map and list children reject. |
| Allocation propagation | The existing shared-value recovery sweep exercises scalar, blob, map and list allocation, child insertion and the value cache reservation. Failures preserve the source and a sufficient allowance reopens with shared node identity. |

The DATA and VALUE tests share a retained-history fixture. The malformed
copies verify exact errors, no writes or syncs and complete byte preservation.
Revision 4 opens with its distinct current rows and retained ID 1 at revision
2 in the healthy fixture. Clearing the newest slot in each malformed copy
lets revision 3 open with its own rows and the earlier retained snapshot.

The first debug prototype detected 35 of 37 changes in their intended tests.
The forward guard and cached end probes survived and are recorded separately
from the 35 registered controls. The aligned cache-offset check has a
supplemental control for the delegated expression, outside the syntax
inventory. The two tag switches have scoped alternative coverage. Compiler
MC/DC, delegated calls and the remaining cache rules retain their own scope.

With the shared fixture, the two debug prototype shards detect all 29 DATA
and all 35 VALUE controls. Each intended test fails in the corresponding
variant, and all 218 command retirements verify. Frozen qualification is
still pending.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `34baf455a1e1ad7e8d76c932ed417fe60c324bbeaad64efd0ca398205012472d` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| B | weekend-db::weekend_db | src/storage/codec.rs:946:9– | 31709..31711 | if | value_inner | Depth pair observed; forwarded offset omission masked by later checks |
| B | weekend-db::weekend_db | src/storage/codec.rs:946:12– | 31712..31767 | short_circuit_or | value_inner | Depth pair observed; forwarded offset omission masked by later checks |
| C | weekend-db::weekend_db | src/storage/codec.rs:956:9– | 32207..32209 | if | value_inner | Coupled with cache pattern test |
| P | weekend-db::weekend_db | src/storage/codec.rs:956:12– | 32210..32213 | pattern_test | value_inner | Directed source pair above |
| I | weekend-db::weekend_db | src/storage/codec.rs:957:13– | 32279..32281 | if | value_inner | Nonoverlapping aligned objects under selected traversal |
| E | weekend-db::weekend_db | src/storage/codec.rs:962:56– | 32466..32467 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:964:31– | 32554..32555 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:965:21– | 32577..32582 | match | value_inner | Scoped tag and scalar alternatives |
| E | weekend-db::weekend_db | src/storage/codec.rs:967:40– | 32651..32652 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:968:28– | 32681..32686 | match | value_inner | Scoped tag and scalar alternatives |
| P | weekend-db::weekend_db | src/storage/codec.rs:976:21– | 32994..32996 | if | value_inner | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:981:19– | 33153..33154 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:983:84– | 33252..33253 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:983:86– | 33254..33255 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:985:72– | 33347..33348 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:988:17– | 33464..33467 | for_iteration | value_inner | Scoped iteration over decoded entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:989:70– | 33553..33554 | try_propagation | value_inner | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:990:21– | 33576..33578 | if | value_inner | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:994:56– | 33791..33792 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:994:77– | 33812..33813 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:995:56– | 33870..33871 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:997:47– | 33937..33938 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1000:72– | 34043..34044 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1002:17– | 34106..34109 | for_iteration | value_inner | Scoped iteration over decoded entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:1003:56– | 34181..34182 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1003:77– | 34202..34203 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1004:54– | 34258..34259 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1006:49– | 34327..34328 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1010:22– | 34420..34421 | try_propagation | value_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1012:64– | 34503..34504 | try_propagation | value_inner | Scoped propagation above |
