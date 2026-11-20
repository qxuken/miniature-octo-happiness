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
2 in the healthy fixture, both with the witness present and with it cleared.
The malformed copies also clear that witness. Clearing the newest slot in
each malformed copy lets revision 3 open with its own rows and the earlier
retained snapshot.

The first debug prototype detected 35 of 37 changes in their intended tests.
The forward guard and cached end probes survived and are recorded separately
from the 35 registered controls. The aligned cache-offset check has a
supplemental control for the delegated expression, outside the syntax
inventory. The two tag switches have scoped alternative coverage. Compiler
MC/DC, delegated calls and the remaining cache rules retain their own scope.
The storage maintainers own review of the partial, coupled and invariant
dispositions. Recheck these proofs if traversal order, extent validation or
cache insertion changes.

With the shared fixture, the two debug prototype shards detect all 29 DATA
and all 35 VALUE controls. Each intended test fails in the corresponding
variant, and all 218 command retirements verify. The frozen run below
verifies the witness-held-constant fixture on both platforms.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `d0ec5a37605a1221aebfa6cf5cc53f1a7314139b19d477d4e104807a32cbf373` |

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

## Frozen qualification at 1450e64

Source `1450e643e36539e5840a0392af882e4d6319e2d5` passed both profiles on
Rust 1.99.1. The earlier `e2e98e5` run was cancelled after the witness control
review and receives no frozen qualification credit.

| Host | Inventory tests per profile | Core tests per profile | xtask tests per profile | R1 controls per profile |
|---|---|---|---|---|
| macOS arm64/APFS | 32 passed | 346 passed, 2 explicit ignores | 242 passed, 2 explicit ignores | 186/186 detected |
| Docker Linux arm64/ext4 | 32 passed | 353 passed, 2 explicit ignores | 271 passed, 1 explicit ignore | 186/186 detected |

Formatting and strict all-target Clippy passed for the core, xtask and
inventory packages on both hosts. The core tests include allocation failures,
exact bytes, crash schedules and current and retained roots. Docker ran
without networking, using the pinned image and vendor inputs from the
[graph batch](codec-graph-current.md#frozen-qualification-at-4d2e3eb).
Source, Cargo and runtime files lived on ext4. The audited export preceded
removal of the owned containers and volume.

The three local shards ran 62 controls each per profile. The audits verified
2,544 command retirements and 784 copied and restored source files and 347
declared inputs per campaign. They checked the intended test failures for
all 35 VALUE controls and preserved all 142 earlier codec bindings. The two
forward-reference B rows and cached-end I row retain their scope. Both
report hashes agree across platforms and profiles.

| Report | SHA-256 |
|---|---|
| inventory.json | `582399287d7f57b511987e56fd33fe4731b7eddd0f9abee70d91fea49805236f` |
| review.json | `2abe4a86552d1992e5884656919444c84d081e113a9c663c369b93779289ba29` |

Local proof `target/archived-proof/r1-value-1450e64.tar.gz` SHA-256 is
`986995b90653582e7b476afe916cb8101103aed46f5846ad00beeb0386720138`.
Fresh extraction verifies 17,137 indexed payloads and replays both source
audits using the included Git bundle. The archive contains recipes, locked
vendor inputs, command reports and restored source copies.

The [hosted matrix](../r1-ci-shards.md#value-reader-shard-count) uses six
Unix shards. Actionlint 1.7.12 and the actual CLI listings in both profiles
verify the complete 31/31/31/31/31/31 selection. Hosted execution and native
Windows keep separate source scope.
