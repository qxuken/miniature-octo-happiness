# Codec checksums and unchanged recovery decisions

The optimized CRC-64 implementation generates eight lookup tables at compile
time. Its test oracle advances one bit at a time with the ECMA polynomial.
It checks all 256 input bytes in each of eight word positions and compares
split updates from three initial states at every boundary in unaligned data.
The five named mutations change the bit count, tested high bit, final table
row, byte order or incoming state used for trailing bytes. The golden lifecycle
and DATA tests check the bytes emitted by storage users of the checksum.

The [historical codec audit](codec.md) supplies the C1–C6 groups below.
For the 42 reused records, the original file matches its recorded SHA-256
at `8556dc1`. Each complete enclosing function has a unique byte-identical
counterpart in the current source. Within those functions, the records have
the same relative offsets, owner, kind, alternatives and condition bytes.
Mixed and platform-specific dispositions carry over. The selected-graph
tests require corruption of the newest selected graph to reject recovery.

| Source | SHA-256 |
| --- | --- |
| `src/storage/codec.rs` | `593b1dc249ceca770b436133c894b41aad0596c5f20be79abf96cd69644205ee` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/storage/codec.rs:24:5– | 773..778 | while | crc_tables | Compile-time table generation checked through all byte values in each word position. |
| E | weekend-db::weekend_db | src/storage/codec.rs:27:9– | 868..873 | while | crc_tables | Compile-time table generation checked through all byte values in each word position. |
| P | weekend-db::weekend_db | src/storage/codec.rs:28:19– | 902..904 | if | crc_tables | High-bit recurrence checked against an independent bit-at-a-time implementation. |
| E | weekend-db::weekend_db | src/storage/codec.rs:40:5– | 1217..1222 | while | crc_tables | Compile-time table generation checked through all byte values in each word position. |
| E | weekend-db::weekend_db | src/storage/codec.rs:42:9– | 1278..1283 | while | crc_tables | Compile-time table generation checked through all byte values in each word position. |
| E | weekend-db::weekend_db | src/storage/codec.rs:59:5– | 1803..1806 | for_iteration | update_crc | Empty, multiword and partial inputs with incremental splits and nonzero initial states. |
| E | weekend-db::weekend_db | src/storage/codec.rs:71:5– | 2350..2353 | for_iteration | update_crc | Empty, multiword and partial inputs with incremental splits and nonzero initial states. |
| E | weekend-db::weekend_db | src/storage/codec.rs:95:29– | 3219..3220 | try_propagation | align | C1 |
| E | weekend-db::weekend_db | src/storage/codec.rs:806:35– | 26824..26825 | try_propagation | take | C1 |
| E | weekend-db::weekend_db | src/storage/codec.rs:812:43– | 27000..27001 | try_propagation | int | C1 |
| E | weekend-db::weekend_db | src/storage/codec.rs:815:43– | 27131..27132 | try_propagation | count | C1 |
| D | weekend-db::weekend_db | src/storage/codec.rs:815:73– | 27161..27162 | try_propagation | count | 64-bit conversion / target scope |
| E | weekend-db::weekend_db | src/storage/codec.rs:823:36– | 27365..27366 | try_propagation | key | C1 |
| C | weekend-db::weekend_db | src/storage/codec.rs:860:12– | 28286..28367 | short_circuit_or | object | C4 |
| C | weekend-db::weekend_db | src/storage/codec.rs:870:12– | 28658..28727 | short_circuit_or | object | C4 |
| C | weekend-db::weekend_db | src/storage/codec.rs:870:12– | 28658..28773 | short_circuit_or | object | C4 |
| C | weekend-db::weekend_db | src/storage/codec.rs:870:12– | 28658..28820 | short_circuit_or | object | C4 |
| D | weekend-db::weekend_db | src/storage/codec.rs:878:91– | 29023..29024 | try_propagation | object | 64-bit conversion / target scope |
| C | weekend-db::weekend_db | src/storage/codec.rs:893:9– | 29600..29602 | if | object | C4 |
| B | weekend-db::weekend_db | src/storage/codec.rs:893:12– | 29603..29894 | short_circuit_or | object | C4 |
| B | weekend-db::weekend_db | src/storage/codec.rs:894:17– | 29647..29893 | short_circuit_and | object | C4 |
| I | weekend-db::weekend_db | src/storage/codec.rs:898:43– | 29818..29819 | try_propagation | object | C4 |
| I | weekend-db::weekend_db | src/storage/codec.rs:908:33– | 30137..30138 | try_propagation | object | C4 |
| C | weekend-db::weekend_db | src/storage/codec.rs:913:12– | 30341..30431 | short_circuit_or | object | C4 |
| I | weekend-db::weekend_db | src/storage/codec.rs:928:35– | 31076..31077 | try_propagation | object | C4 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39168..39252 | short_circuit_or | recover | C3 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39168..39304 | short_circuit_or | recover | C3 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39168..39362 | short_circuit_or | recover | C3 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39168..39423 | short_circuit_or | recover | C3 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1143:12– | 39168..39480 | short_circuit_or | recover | C3 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1161:5– | 39845..39848 | for_iteration | recover | C3 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1180:9– | 40687..40690 | for_iteration | recover | C3 |
| P | weekend-db::weekend_db | src/storage/codec.rs:1181:16– | 40751..40857 | short_circuit_or | recover | C3 |
| P | weekend-db::weekend_db | src/storage/codec.rs:1182:21– | 40804..40856 | short_circuit_and | recover | C3 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1192:7– | 41123..41124 | try_propagation | recover | C6 + C3 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1207:9– | 41652..41654 | if | recover | C4 |
| P | weekend-db::weekend_db | src/storage/codec.rs:1207:12– | 41655..41705 | short_circuit_or | recover | C4 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1211:36– | 41846..41847 | try_propagation | recover | C4 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1212:36– | 41884..41885 | try_propagation | recover | C4 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1213:41– | 41927..41928 | try_propagation | recover | C4 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1217:35– | 42081..42082 | try_propagation | recover | C4 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1233:9– | 42637..42640 | for_iteration | recover | C5 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1234:34– | 42690..42691 | try_propagation | recover | C5 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1235:49– | 42741..42742 | try_propagation | recover | C5 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1236:38– | 42781..42782 | try_propagation | recover | C5 |
| E | weekend-db::weekend_db | src/storage/codec.rs:1237:54– | 42837..42838 | try_propagation | recover | C5 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1238:16– | 42855..42905 | short_circuit_or | recover | C5 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1238:16– | 42855..42954 | short_circuit_or | recover | C5 |
| C | weekend-db::weekend_db | src/storage/codec.rs:1238:16– | 42855..43016 | short_circuit_or | recover | C5 |
