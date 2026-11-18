# Recovery DATA directory decisions

The DATA reader decodes a collection directory and the ordered records in
each collection. It also reuses a decoded directory when a retained snapshot
refers to the same object. The [COMMIT ledger](codec-catalog-current.md)
covers the caller, including its current and retained references.

| Decision | Cases and outcomes |
|---|---|
| Cached directory lookup | A repeated offset reuses the verified directory, with one header read and one full read in total. A distinct offset and revision reads the second object. A repeated offset with a different revision rejects. The cached end comparison is partial because both current and retained recovery use the same selected descriptor bound and only successful reads enter the cache. |
| Revision uniqueness | The same revision at a second offset rejects before that offset is read. The distinct-offset and distinct-revision control reads the second object and recovers. |
| DATA header fields | A DATA payload revision different from the COMMIT reference, an object birth revision different from the DATA payload, and a zero next collection ID each reject. The zero-ID case uses an empty directory, where an entry ID check cannot mask loss of the header guard. |
| Collection fields | Increasing IDs, each below the next ID, and positive revisions no later than the DATA revision allow recovery. Duplicate IDs, an ID at the next ID, revision zero and a future revision reject separately. The fixtures include an empty collection. |
| Record fields | Sorted records recover. Equal or decreasing keys reject. The selected-graph test isolates zero and future write revisions. The graph traversal test verifies that a forward value reference to its own DATA directory issues no further read of that directory. |
| Exact length and limits | Every truncated prefix of a 128-byte DATA payload and an added trailing byte return Corrupt. Collection count, record count and key length one above their configured bounds return Limit. The checksummed fixtures isolate DATA admission within the selected graph. |
| Allocation and insertion | The existing recovery allocation sweep with a retained snapshot reaches record insertion, collection insertion and the DATA cache reservation. Each injected failure returns OutOfMemory and preserves the durable image. A sufficient allowance recovers the rows. |

The new integration test opens revision 4 and verifies the current rows and
retained ID 1 at revision 2. It makes separate source copies for each DATA
change and clears the newest witness to enforce selected-graph rejection.
The failure must preserve the complete candidate and perform no write or
sync. With the newest slot removed, the same image independently opens
revision 3 with its prior rows and the retained catalog.

The cached extent comparison remains partial on the public recovery path.
The collection and record loops use zero, one or multiple entries across
the existing fixtures. This supports scoped iteration evidence. Compiler
MC/DC and delegated iterator and allocation internals retain their own scope.

The working-source debug campaign detects all 29 DATA controls and checks
that each intended test fails. The eight repeated field and count reads
bind to their byte spans and use per-occurrence mutations. The batch
preserves the earlier 178 mutations and the pinned production codec.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `15be88e7381e9ea806ee9c9219505526b1cc2d30c79b3bce877ea65ec28f71ea` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| C | weekend-db::weekend_db | src/storage/codec.rs:1037:9– | 35197..35199 | if | data_inner | Overlapping syntax under the documented parent pair |
| P | weekend-db::weekend_db | src/storage/codec.rs:1037:12– | 35200..35203 | pattern_test | data_inner | Directed source pair above |
| B | weekend-db::weekend_db | src/storage/codec.rs:1038:13– | 35288..35290 | if | data_inner | Cached end dominated by earlier read under the same descriptor bound |
| B | weekend-db::weekend_db | src/storage/codec.rs:1038:16– | 35291..35350 | short_circuit_or | data_inner | Cached end dominated by earlier read under the same descriptor bound |
| P | weekend-db::weekend_db | src/storage/codec.rs:1043:9– | 35506..35508 | if | data_inner | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1050:55– | 35732..35733 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1052:36– | 35825..35826 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1053:43– | 35870..35871 | try_propagation | data_inner | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1054:9– | 35881..35883 | if | data_inner | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1054:12– | 35884..35940 | short_circuit_or | data_inner | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1054:12– | 35884..35964 | short_circuit_or | data_inner | Overlapping syntax under the documented parent pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:1057:64– | 36080..36081 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1060:9– | 36166..36169 | for_iteration | data_inner | Scoped iteration over zero, one and multiple entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:1061:34– | 36219..36220 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1062:48– | 36269..36270 | try_propagation | data_inner | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1063:13– | 36284..36286 | if | data_inner | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1063:16– | 36287..36345 | short_circuit_or | data_inner | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1063:16– | 36287..36386 | short_circuit_or | data_inner | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1063:16– | 36287..36433 | short_circuit_or | data_inner | Overlapping syntax under the documented parent pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:1071:76– | 36611..36612 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1074:13– | 36720..36723 | for_iteration | data_inner | Scoped iteration over zero, one and multiple entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:1075:66– | 36813..36814 | try_propagation | data_inner | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1076:17– | 36832..36834 | if | data_inner | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1080:50– | 37029..37030 | try_propagation | data_inner | Scoped propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:1081:17– | 37048..37050 | if | data_inner | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1081:20– | 37051..37107 | short_circuit_or | data_inner | Overlapping syntax under the documented parent pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:1084:52– | 37227..37228 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1084:65– | 37240..37241 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1092:18– | 37487..37488 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1101:14– | 37746..37747 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1103:22– | 37780..37781 | try_propagation | data_inner | Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1105:69– | 37868..37869 | try_propagation | data_inner | Scoped propagation above |

## Frozen qualification at 0c1969a

Source `0c1969ad4a115c20be3a10fe791d7f8c795ba644` passed in both
profiles on Rust 1.99.1.

| Host | Inventory tests per profile | Core tests per profile | xtask tests per profile | Controls per profile |
|---|---|---|---|---|
| macOS arm64/APFS | 32 passed | 345 passed, 2 explicit ignores | 242 passed, 2 explicit ignores | 151/151 detected |
| Docker Linux arm64/ext4 | 32 passed | 352 passed, 2 explicit ignores | 271 passed, 1 explicit ignore | 151/151 detected |

Both hosts passed formatting and strict all-target Clippy for the core,
xtask and inventory packages. The core suite includes exact serialization,
allocation, crash recovery and retained roots. Docker used the pinned
offline image and vendor inputs from the [graph batch](codec-graph-current.md#frozen-qualification-at-4d2e3eb).
Source, Cargo home, builds, temporary files and runtime evidence resided on
ext4. The exported audit passed before both containers and the owned volume
were removed.

The three local shards ran 51, 50 and 50 controls per profile. The audits
verify **2,108 command retirements**, **783 copied and restored source files
and 347 inputs per campaign**, the intended named test failures for all 29
DATA controls and preservation of the prior 110 codec bindings. The two
cached-end B rows retain their partial scope. Both report hashes match
across platforms and profiles.

| Report | SHA-256 |
|---|---|
| inventory.json | `978ac78a04a81ca179e26569692983f420e4f6e1f01f8854d3f75180a9691580` |
| review.json | `7995e8e0ef1f53424d4584ed5d79af89976d144acb1291638a55142bf07ac66e` |

Local proof `target/archived-proof/r1-data-0c1969a.tar.gz` SHA-256 is
`23cb44d800b672d509af158b80041d5ae011499161912ec9c0ff1c808c1f164d`.
Fresh extraction verifies **15,817 indexed payloads** and replays both
audits from the included Git bundle. The archive also contains the locked
vendor inputs, recipes, reports and restored source copies.

The [hosted matrix](../r1-ci-shards.md#data-directory-shard-count) uses five
Unix shards. Actionlint 1.7.12 and the actual CLI listings in both profiles
verify the complete 31/30/30/30/30 selection. Hosted execution and native
Windows retain a separate source scope.
