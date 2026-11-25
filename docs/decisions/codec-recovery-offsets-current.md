# Codec recovery and encoding evidence after the map encoder change

The source pinned by the linked ledgers differs in one production function:
nested map encoding now uses the encoder’s allocator. Each reused row below belongs
to a different, byte-identical function. Its signature and body occur exactly
once in each source. Shifting the decision by the function’s offset produces
one historical row with the same target, owner, kind and byte range. Its line
and column agree with the bytes in the pinned source. The evidence column
links to that row’s original qualification and retains its disposition.

Three additional records have current-source dispositions. Adding the object
header to a valid Rust slice length cannot overflow usize on the supported
32- and 64-bit targets because slice length is bounded by isize::MAX. Fresh
image and incremental publication tests exercise both empty and borrowed
VALUE and DATA cache choices.

| Source | SHA-256 |
| --- | --- |
| `src/storage/codec.rs` | `593b1dc249ceca770b436133c894b41aad0596c5f20be79abf96cd69644205ee` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| C | weekend-db::weekend_db | src/storage/codec.rs:150:8– | 4893..4952 | short_circuit_or | parse_metadata | [metadata](codec-metadata-current.md): Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:150:8– | 4893..4997 | short_circuit_or | parse_metadata | [metadata](codec-metadata-current.md): Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:150:8– | 4893..5034 | short_circuit_or | parse_metadata | [metadata](codec-metadata-current.md): Overlapping syntax under the documented parent pair |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:5– | 5286..5288 | if | parse_metadata | [metadata](codec-metadata-current.md): Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5469 | short_circuit_or | parse_metadata | [metadata](codec-metadata-current.md): Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5520 | short_circuit_or | parse_metadata | [metadata](codec-metadata-current.md): Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5569 | short_circuit_or | parse_metadata | [metadata](codec-metadata-current.md): Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5627 | short_circuit_or | parse_metadata | [metadata](codec-metadata-current.md): Partial layout guard: dominated cap and coupled alignment atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:164:8– | 5289..5699 | short_circuit_or | parse_metadata | [metadata](codec-metadata-current.md): Partial layout guard: dominated cap and coupled alignment atoms |
| E | weekend-db::weekend_db | src/storage/codec.rs:414:24– | 13864..13869 | match | write_append | [append](codec-append-current.md): Both append layouts |
| E | weekend-db::weekend_db | src/storage/codec.rs:419:9– | 14084..14087 | for_iteration | write_append | [append](codec-append-current.md): One and multiple segments |
| E | weekend-db::weekend_db | src/storage/codec.rs:444:9– | 14770..14775 | match | object | [append](codec-append-current.md): Both staging layouts |
| E | weekend-db::weekend_db | src/storage/codec.rs:447:17– | 14902..14905 | for_iteration | object | [append](codec-append-current.md): Header, payload and padding copies |
| E | weekend-db::weekend_db | src/storage/codec.rs:453:17– | 15106..15109 | for_iteration | object | [append](codec-append-current.md): Header, payload and padding copies |
| E | weekend-db::weekend_db | src/storage/codec.rs:454:21– | 15150..15155 | while | object | [append](codec-append-current.md): Complete and split parts |
| C | weekend-db::weekend_db | src/storage/codec.rs:636:9– | 21686..21688 | if | data | [encode-data](codec-encode-data-current.md): Coupled to the same cache pattern |
| E | weekend-db::weekend_db | src/storage/codec.rs:655:9– | 22500..22503 | for_iteration | data | [encode-data](codec-encode-data-current.md): Empty and populated collection maps |
| I | weekend-db::weekend_db | src/storage/codec.rs:664:15– | 22889..22890 | try_propagation | data | [encode-data](codec-encode-data-current.md): Record callback always returns Ok after the reserved push |
| E | weekend-db::weekend_db | src/storage/codec.rs:666:13– | 22963..22966 | for_iteration | data | [encode-data](codec-encode-data-current.md): Empty and populated record maps |
| E | weekend-db::weekend_db | src/storage/codec.rs:775:5– | 25913..25916 | for_iteration | encode_with_append | [encode-commit](codec-encode-commit-current.md): Empty and populated catalogs, including all 64 retained roots in the golden history |
| B | weekend-db::weekend_db | src/storage/codec.rs:946:9– | 31736..31738 | if | value_inner | [value](codec-value-current.md): Depth pair observed; forwarded offset omission masked by later checks |
| B | weekend-db::weekend_db | src/storage/codec.rs:946:12– | 31739..31794 | short_circuit_or | value_inner | [value](codec-value-current.md): Depth pair observed; forwarded offset omission masked by later checks |
| C | weekend-db::weekend_db | src/storage/codec.rs:956:9– | 32234..32236 | if | value_inner | [value](codec-value-current.md): Coupled with cache pattern test |
| I | weekend-db::weekend_db | src/storage/codec.rs:957:13– | 32306..32308 | if | value_inner | [value](codec-value-current.md): Nonoverlapping aligned objects under selected traversal |
| E | weekend-db::weekend_db | src/storage/codec.rs:964:31– | 32581..32582 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:965:21– | 32604..32609 | match | value_inner | [value](codec-value-current.md): Scoped tag and scalar alternatives |
| E | weekend-db::weekend_db | src/storage/codec.rs:967:40– | 32678..32679 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:968:28– | 32708..32713 | match | value_inner | [value](codec-value-current.md): Scoped tag and scalar alternatives |
| E | weekend-db::weekend_db | src/storage/codec.rs:983:84– | 33279..33280 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:985:72– | 33374..33375 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:988:17– | 33491..33494 | for_iteration | value_inner | [value](codec-value-current.md): Scoped iteration over decoded entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:989:70– | 33580..33581 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:994:56– | 33818..33819 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:994:77– | 33839..33840 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1000:72– | 34070..34071 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1002:17– | 34133..34136 | for_iteration | value_inner | [value](codec-value-current.md): Scoped iteration over decoded entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:1003:56– | 34208..34209 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1003:77– | 34229..34230 | try_propagation | value_inner | [value](codec-value-current.md): Scoped propagation above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1037:9– | 35224..35226 | if | data_inner | [data](codec-data-current.md): Overlapping syntax under the documented parent pair |
| B | weekend-db::weekend_db | src/storage/codec.rs:1038:13– | 35315..35317 | if | data_inner | [data](codec-data-current.md): Cached end dominated by earlier read under the same descriptor bound |
| B | weekend-db::weekend_db | src/storage/codec.rs:1038:16– | 35318..35377 | short_circuit_or | data_inner | [data](codec-data-current.md): Cached end dominated by earlier read under the same descriptor bound |
| E | weekend-db::weekend_db | src/storage/codec.rs:1052:36– | 35852..35853 | try_propagation | data_inner | [data](codec-data-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1053:43– | 35897..35898 | try_propagation | data_inner | [data](codec-data-current.md): Scoped propagation above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1054:12– | 35911..35967 | short_circuit_or | data_inner | [data](codec-data-current.md): Overlapping syntax under the documented parent pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:1057:64– | 36107..36108 | try_propagation | data_inner | [data](codec-data-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1060:9– | 36193..36196 | for_iteration | data_inner | [data](codec-data-current.md): Scoped iteration over zero, one and multiple entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:1061:34– | 36246..36247 | try_propagation | data_inner | [data](codec-data-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1062:48– | 36296..36297 | try_propagation | data_inner | [data](codec-data-current.md): Scoped propagation above |
| C | weekend-db::weekend_db | src/storage/codec.rs:1063:16– | 36314..36372 | short_circuit_or | data_inner | [data](codec-data-current.md): Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:1063:16– | 36314..36413 | short_circuit_or | data_inner | [data](codec-data-current.md): Overlapping syntax under the documented parent pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:1071:76– | 36638..36639 | try_propagation | data_inner | [data](codec-data-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1074:13– | 36747..36750 | for_iteration | data_inner | [data](codec-data-current.md): Scoped iteration over zero, one and multiple entries |
| E | weekend-db::weekend_db | src/storage/codec.rs:1080:50– | 37056..37057 | try_propagation | data_inner | [data](codec-data-current.md): Scoped propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1084:52– | 37254..37255 | try_propagation | data_inner | [data](codec-data-current.md): Scoped propagation above |
| I | weekend-db::weekend_db | src/storage/codec.rs:538:33– | 18050..18051 | try_propagation | object_born | Valid slice length plus object header fits usize. |
| E | weekend-db::weekend_db | src/storage/codec.rs:750:17– | 25171..25176 | match | encode_with_append | Fresh and incremental VALUE cache selection. |
| E | weekend-db::weekend_db | src/storage/codec.rs:754:22– | 25319..25324 | match | encode_with_append | Fresh and incremental DATA cache selection. |
