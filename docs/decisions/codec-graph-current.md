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
| `src/storage/codec.rs` | `367ce41fbb62e2efe1acc7bd0c2c38e57511a3060d2fffb92f1c6eadfe884903` |

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

## Frozen qualification at 4d2e3eb

Source `4d2e3ebb266f65d65365a29db4827b303935d220` passed on Rust 1.99.1
in debug and release on macOS arm64/APFS and Docker Linux arm64/ext4.

| Host | Inventory tests per profile | Core tests per profile | xtask tests per profile | Controls per profile |
|---|---|---|---|---|
| macOS | 32 passed | 342 passed, 2 explicit ignores | 242 passed, 2 explicit ignores | 95/95 detected |
| Docker Linux | 32 passed | 349 passed, 2 explicit ignores | 271 passed, 1 explicit ignore | 95/95 detected |

The all-features core suite includes allocation, budget, exact serialization,
crash and retained-root tests. Both hosts passed strict all-target Clippy for
all three packages and formatting for both workspaces. The earlier
`633ffa2` candidate failed reconciliation on a stale reference pin.
At `813c95e`, the tests and controls passed but strict Clippy rejected an
identity mask in the test. The final candidate computes the same aligned
offset by subtracting the remainder, and all checks ran again from that source.

Each host ran three disjoint shards per profile, with 32, 32 and 31 cases.
The audits verify all **1,384 command retirements** across twelve campaigns,
the intended named test failures for the 33 graph additions and 35 metadata
controls, and **781 copied/restored files and 347 input hashes per campaign**.
All 46 prior codec bindings remain exact. The three B rows have no direct
or overlap-control promotion. All four reports agree.

| Report | SHA-256 |
|---|---|
| inventory.json | `08197eabb77a97ba2a539beb594025501d48b090d9fe2c8c1fe88192043f6213` |
| review.json | `cba925380145fc693f4da3c6e288778865d1fc6311bb71288c11eadb523be6cf` |

Docker uses image
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`
without networking. Source, dependencies, Cargo home, builds, temporary
files and runtime evidence are on the owned ext4 volume. Export and audit
passed before removal of the two owned containers and volume.

The local proof `target/archived-proof/r1-graph-4d2e3eb.tar.gz` has SHA-256
`7c115f87ec6e1c0c47067fa80ae0681fc639bdc8d1c93f49d51be06fa65942fc`.
Fresh extraction verifies **13,621 indexed payloads** and repeats the audits
against Git objects from the bundled source. The archive contains the source
bundle, locked vendor inputs, scripts, reports and restored source copies.
Hosted checks, including native Windows, retain their separate source scope.

## Hosted results at a1a95a1

PR #12 head `a1a95a108593b9ef8b491570b831c90b4ed903f9` passed all
[26 engine jobs](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44277108233)
and [bounded fuzzing](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44277108308).
The tested merge `0c2896457eabc8176c4f39f257a1d75aad88e3f5` has the
same tree as the head. The audit verifies all seven artifact digests and
exact reconciliation reports, including Windows. The Linux and macOS unions
contain all 95 controls per profile with no duplicated cases. All 1,384
process retirements verify. These artifacts contain copied-source hashes
and command reports. The local proof above includes the restored files.
