# Current recovery object and input decisions

This R1 batch starts at `e5ca577d2c4273d4df14732f798866c86e020e09`.
It maps the object reader and the metadata I/O propagation sites to
[REC-V4 and REC-V7](../../DESIGN.md#rec-v4-verify-the-selected-graph-before-opening)
and adds a public open regression for the object read boundaries. The codec
file keeps the exact [arithmetic](codec-arithmetic-current.md) and
[metadata and root-selection](codec-metadata-current.md) pins.

| Decision | Cases and observed outcomes |
|---|---|
| Object start and header extent | An ordinary aligned object opens. An aligned offset below the append region, an unaligned offset within the graph, and an aligned DATA reference at or beyond its parent each reject before another object read. The all-false case and each single-true case form three source pairs. |
| Object header shape | Valid magic, kind, version, identity and reserved bytes allow the full read. Change each field separately and repair the checksum. Each case rejects the selected graph. All false versus each single true supplies five pairs. |
| Object length and end | A normal extent is accepted. Header plus payload overflow, alignment error and an aligned extent past its bound return Corrupt. A checksummed COMMIT that fits the physical file but extends beyond the selected slot end must reject after its 64-byte header. |
| Object birth revision | The valid graph opens. Moving a value's birth revision beyond the selected revision with a repaired checksum rejects after the first read. |
| Full object verification | Stable matching header, checksum and padding allow open. Change the header only during the full read with valid checksum and padding, corrupt the checksum with a stable header, or set nonzero padding with a repaired checksum. Each case rejects. All false versus each single true supplies three pairs. |
| Region index allocation | The first valid object starts with an empty region index. Once it is populated, later objects reach the same decision with an index present. The allocation sweep opens a fresh copy per attempt. Injected buffer or index reservation failure returns OutOfMemory and leaves each source unchanged. A later attempt with sufficient budget opens and preserves shared value identity. |
| Recovery input and diagnostics | Fault every operation in the clean open trace. Length and metadata reads, the initial object header and full object reads each propagate Io on failure. Zero bytes on a read return Io and positive short reads finish recovery. Failure leaves the image unchanged and a retry recovers the original state. The four outer metadata-stage propagation sites are caught independently. |

The new `object_extents_and_directory_references_reject_before_outside_reads`
test opens revision **4** with retained snapshot **1** at revision **2**, with
distinct rows in the two snapshots. Its
damaged newest graphs return Corrupt with the newest slot still selected and
the newest witness cleared. It requires the complete operation trace to match
the healthy prefix through the last permitted read, forbids writes and syncs,
and compares the whole image. Removing the newest slot separately opens
revision **3** with the expected rows and no retained IDs. A healthy image
with an unselected tail still opens revision 4 with the same retained state.
The test computes CRC independently and checks its recurrence against a known
vector and the actual original descriptor.

The existing graph tests cover both overlap traversal orders and occupancy
markers for both an ordinary occupied region and a cached value start.
The complete overlap guard and its OR/AND sites retain B. The object count
cannot reach MAX_OBJECTS in public graph recovery because objects occupy
disjoint aligned regions within the capped image and five regions are
reserved for metadata. A true occupancy predicate also requires an existing
index, so the absent-index case has no independent rejection pair with
occupancy held true. The four overlap controls establish the tested behavior.

The two region range propagation sites retain I. Aligned offsets are ordered
and bounded by the selected end, and the index covers every region up to that
end. The work limit propagation site also retains I. Disjoint accepted objects
consume at most the selected image length plus one 64-byte header read per
region, below twice the image cap. The u64 to usize conversion retains D on
the qualified 64-bit ABIs. Narrower targets and delegated arithmetic remain
outside this evidence. C rows record overlapping syntax under their parent
pairs. E rows record scoped success and failure propagation.

The registry adds **33** controls to the prior 118 and preserves their order.
The complete R1 selection is now **95**, divided into **32 / 32 / 31** by the
existing hosted shard rule. The old codec unit target passed despite removal
of the early header bound. The first graph prototype also survived removal
of the parent end filter because a later check still rejected the chosen
fixture. The new public read trace regression detects both omissions. The
rebased debug prototype detected all 33 graph controls. Frozen qualification
results are tracked separately from this working-source feedback.

```sh
cargo +1.99.1 xtask mutations --case r1-graph- --toolchain 1.99.1 --output target/graph-debug-new
cargo +1.99.1 xtask mutations --case r1-graph- --release --toolchain 1.99.1 --output target/graph-release-new
```

VALUE and DATA decoding, cached-reference decisions, descriptor payloads and
retained catalog decisions retain their own source records. Compiler MC/DC,
delegated code and the remaining R1 through R7 gates stay open.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `34baf455a1e1ad7e8d76c932ed417fe60c324bbeaad64efd0ca398205012472d` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| P | weekend-db::weekend_db | src/storage/codec.rs:860:9– | 28256..28258 | if | object | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:860:12– | 28259..28340 | short_circuit_or | object | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:860:12– | 28259..28460 | short_circuit_or | object | Overlapping syntax under the documented parent pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:869:52– | 28617..28618 | try_propagation | object | Scoped success/error propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:870:9– | 28628..28630 | if | object | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:870:12– | 28631..28700 | short_circuit_or | object | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:870:12– | 28631..28746 | short_circuit_or | object | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:870:12– | 28631..28793 | short_circuit_or | object | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:870:12– | 28631..28845 | short_circuit_or | object | Overlapping syntax under the documented parent pair |
| D | weekend-db::weekend_db | src/storage/codec.rs:878:91– | 28996..28997 | try_propagation | object | Delegated conversion is infallible on qualified 64-bit ABIs |
| E | weekend-db::weekend_db | src/storage/codec.rs:881:35– | 29122..29123 | try_propagation | object | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:882:61– | 29185..29186 | try_propagation | object | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:886:35– | 29319..29320 | try_propagation | object | Scoped success/error propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:888:9– | 29370..29372 | if | object | Directed source pair above |
| B | weekend-db::weekend_db | src/storage/codec.rs:893:9– | 29573..29575 | if | object | Partial overlap guard with dominated or coupled atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:893:12– | 29576..29867 | short_circuit_or | object | Partial overlap guard with dominated or coupled atoms |
| B | weekend-db::weekend_db | src/storage/codec.rs:894:17– | 29620..29866 | short_circuit_and | object | Partial overlap guard with dominated or coupled atoms |
| I | weekend-db::weekend_db | src/storage/codec.rs:898:43– | 29791..29792 | try_propagation | object | Range covered by selected image index |
| I | weekend-db::weekend_db | src/storage/codec.rs:908:33– | 30110..30111 | try_propagation | object | Disjoint region and image-cap work bound |
| E | weekend-db::weekend_db | src/storage/codec.rs:910:52– | 30207..30208 | try_propagation | object | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:912:51– | 30300..30301 | try_propagation | object | Scoped success/error propagation above |
| P | weekend-db::weekend_db | src/storage/codec.rs:913:9– | 30311..30313 | if | object | Directed source pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:913:12– | 30314..30404 | short_circuit_or | object | Overlapping syntax under the documented parent pair |
| C | weekend-db::weekend_db | src/storage/codec.rs:913:12– | 30314..30480 | short_circuit_or | object | Overlapping syntax under the documented parent pair |
| P | weekend-db::weekend_db | src/storage/codec.rs:922:9– | 30784..30786 | if | object | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:923:67– | 30879..30880 | try_propagation | object | Scoped success/error propagation above |
| I | weekend-db::weekend_db | src/storage/codec.rs:928:35– | 31049..31050 | try_propagation | object | Range covered by selected image index |
| E | weekend-db::weekend_db | src/storage/codec.rs:1131:38– | 38688..38689 | try_propagation | recover | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1136:7– | 38843..38844 | try_propagation | recover | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1139:42– | 38993..38994 | try_propagation | recover | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1158:7– | 39745..39746 | try_propagation | recover | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1164:69– | 40037..40038 | try_propagation | recover | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1166:11– | 40113..40114 | try_propagation | recover | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1170:73– | 40342..40343 | try_propagation | recover | Scoped success/error propagation above |
| E | weekend-db::weekend_db | src/storage/codec.rs:1172:15– | 40425..40426 | try_propagation | recover | Scoped success/error propagation above |
