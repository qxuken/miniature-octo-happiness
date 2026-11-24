# VALUE encoder decisions

VALUE encoding reuses cached node offsets and otherwise emits a typed
payload. Children are encoded before their parent. The acknowledged cache
stays borrowed until a successful object append needs a private fork.

| Decision | Controls and observed result |
|---|---|
| Cached node | The real encoder seeds four values. With allocation disabled, the shared node returns its original offset. It leaves the append empty and keeps the borrowed cache. Uncached inputs take the encoding path and successfully add an indexed entry once allocation succeeds. |
| Value kind | The scalar and golden-history tests exercise each of the eight kinds. Separate variants change the payload for None, Boolean, Integer, Unsigned, Float and Bytes, or the item count for Map and List. Their intended tests reject each change. |
| Map and list traversal | Empty and populated maps and lists exercise both loops. The populated shape contains shared children and a new descendant. The golden lifecycle checks exact image fingerprints and retained roots across reopen and compaction. |
| Allocation propagation | The sweep fails before each allocation in scalar, byte, empty-container and nested-container encoding. Nineteen per-site controls change the returned allocation error to Invalid. Each is required to fail the exact test expecting OutOfMemory. |
| Cache fork and reserve | A borrowed half-full index must grow when the first new node is emitted. Errors during the fork, index growth and cache-entry reserve propagate. Dropping each attempt restores the exact live charge and preserves the original cache entries and offset. |

The cache pattern has a directed source pair. The enclosing `if` overlaps
that pattern. The public encoding path has five reserved regions, and each
distinct emitted VALUE occupies at least one additional region. The physical
64 MiB limit therefore binds before the cache count reaches `MAX_OBJECTS`.
The defensive count check retains I for that public invariant. Existing
file-boundary controls qualify the physical cap.

`Value::kind` and the scalar/bytes accessors inspect the same immutable node.
The selected list arm uses that same list for both length calls and the
index range passed to `list_get`. The type and bounds results at these
accessors are invariant in the encoder. Map encoding sorts borrowed entries
in the encoder's memory domain. It keeps the originating value's
transaction-byte preflight and has a separate propagation control for the
fallible scratch allocation. Storage maintainers own
the invariant and coupled dispositions and revisit them if the value
representation, traversal range or image geometry changes.

The wire alternatives and loops retain scoped evidence. Compiler MC/DC and
delegated allocator and tree internals remain separate obligations.

The map and list offset writes both have the expression `payload.int(offset)`.
Their manifest selectors add exact byte positions 20369 and 20632. The new
reconciliation test checks two repeated source decisions, selects each by
its position, and rejects missing, malformed and unmatched positions where
the selector would otherwise be ambiguous. The two added tooling decisions
retain individual gaps.

The debug prototype caught all 28 VALUE controls in their intended tests
and verified 104 command retirements. The production codec bytes match
the [append batch](codec-append-current.md) parent. The current codec ledgers
bind the new file hash after test additions, and archived runs retain their
original source hashes.

The nested-map regression measured five `Global` requests before the change
and zero during commit and compaction after it. The budget regression checks
the original row limit and allocation refusal under a separate encoder budget.
All 17 golden files match the bytes from `815fc3b` in debug and release on
macOS arm64 and Docker Linux arm64/ext4.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `593b1dc249ceca770b436133c894b41aad0596c5f20be79abf96cd69644205ee` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| C | weekend-db::weekend_db | src/storage/codec.rs:574:9– | 19342..19344 | if | value | Coupled to the cache pattern |
| P | weekend-db::weekend_db | src/storage/codec.rs:574:12– | 19345..19348 | pattern_test | value | Directed cache-hit pair |
| I | weekend-db::weekend_db | src/storage/codec.rs:577:9– | 19447..19449 | if | value | Physical region bound above |
| E | weekend-db::weekend_db | src/storage/codec.rs:581:41– | 19629..19630 | try_propagation | value | Kind tag allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:582:9– | 19640..19645 | match | value | Eight wire alternatives |
| E | weekend-db::weekend_db | src/storage/codec.rs:583:41– | 19701..19702 | try_propagation | value | None payload allocation |
| I | weekend-db::weekend_db | src/storage/codec.rs:584:67– | 19770..19771 | try_propagation | value | Selected immutable Boolean |
| E | weekend-db::weekend_db | src/storage/codec.rs:584:70– | 19773..19774 | try_propagation | value | Boolean payload allocation |
| I | weekend-db::weekend_db | src/storage/codec.rs:585:56– | 19831..19832 | try_propagation | value | Selected immutable Integer |
| E | weekend-db::weekend_db | src/storage/codec.rs:585:65– | 19840..19841 | try_propagation | value | Integer payload allocation |
| I | weekend-db::weekend_db | src/storage/codec.rs:586:57– | 19899..19900 | try_propagation | value | Selected immutable Unsigned |
| E | weekend-db::weekend_db | src/storage/codec.rs:586:59– | 19901..19902 | try_propagation | value | Unsigned payload allocation |
| I | weekend-db::weekend_db | src/storage/codec.rs:587:54– | 19957..19958 | try_propagation | value | Selected immutable Float |
| E | weekend-db::weekend_db | src/storage/codec.rs:587:66– | 19969..19970 | try_propagation | value | Float payload allocation |
| I | weekend-db::weekend_db | src/storage/codec.rs:588:56– | 20027..20028 | try_propagation | value | Selected immutable Bytes |
| E | weekend-db::weekend_db | src/storage/codec.rs:588:58– | 20029..20030 | try_propagation | value | Bytes payload allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:590:71– | 20129..20130 | try_propagation | value | Borrowed map-entry scratch reservation |
| E | weekend-db::weekend_db | src/storage/codec.rs:591:50– | 20181..20182 | try_propagation | value | Map count allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:592:17– | 20200..20203 | for_iteration | value | Empty and populated map |
| E | weekend-db::weekend_db | src/storage/codec.rs:593:51– | 20284..20285 | try_propagation | value | Recursive map child error |
| E | weekend-db::weekend_db | src/storage/codec.rs:594:41– | 20327..20328 | try_propagation | value | Map key allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:595:40– | 20369..20370 | try_propagation | value | Map offset allocation |
| I | weekend-db::weekend_db | src/storage/codec.rs:599:40– | 20471..20472 | try_propagation | value | Selected immutable List |
| E | weekend-db::weekend_db | src/storage/codec.rs:599:49– | 20480..20481 | try_propagation | value | List count allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:600:17– | 20499..20502 | for_iteration | value | Empty and populated list |
| I | weekend-db::weekend_db | src/storage/codec.rs:600:40– | 20522..20523 | try_propagation | value | Selected immutable List |
| I | weekend-db::weekend_db | src/storage/codec.rs:601:63– | 20588..20589 | try_propagation | value | Index from the same list length |
| E | weekend-db::weekend_db | src/storage/codec.rs:601:65– | 20590..20591 | try_propagation | value | Recursive list child error |
| E | weekend-db::weekend_db | src/storage/codec.rs:602:40– | 20632..20633 | try_propagation | value | List offset allocation |
| E | weekend-db::weekend_db | src/storage/codec.rs:606:63– | 20739..20740 | try_propagation | value | Object staging error |
| E | weekend-db::weekend_db | src/storage/codec.rs:607:66– | 20807..20808 | try_propagation | value | Borrowed cache fork error |
| E | weekend-db::weekend_db | src/storage/codec.rs:610:72– | 20914..20915 | try_propagation | value | Index growth error |
| E | weekend-db::weekend_db | src/storage/codec.rs:612:67– | 21000..21001 | try_propagation | value | Entry growth error |

## Frozen qualification at cf3a5a2

Source `cf3a5a215273c4b448cdfc0e0c2888cb4abe090c` passed in both profiles
using Rust 1.99.1.

| Host | Inventory tests per profile | Core tests per profile | xtask tests per profile | New controls per profile |
|---|---|---|---|---|
| macOS arm64/APFS | 33 passed | 348 passed, 2 explicit ignores | 243 passed, 3 explicit ignores | 29/29 detected |
| Docker Linux arm64/ext4 | 33 passed | 355 passed, 2 explicit ignores | 272 passed, 1 explicit ignore | 29/29 detected |

Formatting and strict all-target Clippy pass. The core suites include
exact serialization, allocation failures, crash and retained-root tests.
The pinned Docker build and runtime used an owned ext4 volume, retired
after auditing the export. The core, xtask and inventory test targets
cross-compile for Windows MSVC.

Three disjoint local shards select 10, 10 and 9 controls. The audit verifies
each intended failed test, including the selector-bypass control. It checks
432 command retirements, 786 copied and restored source files and 347
declared inputs per campaign. Both report hashes agree across platforms
and profiles. All 915 prior exact references retain their bindings.

| Report | SHA-256 |
|---|---|
| inventory.json | `ee69107989ef6dd93fb50bed6a761d1c1b327a4f93c9bacc1e33cc3731786ff4` |
| review.json | `9756a7bfa1ce69dfd4495c66de8951af9b79e7e116d14e4ee057f71bd3fcd5df` |

Local proof `target/archived-proof/r1-value-encoding-cf3a5a2.tar.gz` has
SHA-256 `92a99727cf10f066ecaf676d721ab96d36f91a97a8b867acd97861755a1c549f`.
Fresh extraction verifies 10,826 indexed payloads and replays both audits
using the archived Git bundle. Actionlint 1.7.12 accepts the eight-shard
matrix. The actual CLI lists all 234 hosted controls exactly once per
profile. Hosted execution keeps its own scope.
