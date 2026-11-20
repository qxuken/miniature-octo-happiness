# Encoder bounds and append staging decisions

The encoder builds the object header, payload and padding before publication.
Append staging keeps the bytes contiguous for incremental commits and uses
region-aligned segments for fresh images. Both layouts write through the same
adapter helper. This batch adds tests and bindings to the existing format.

| Decision | False control | True control | Observed outcome |
|---|---|---|---|
| `Writer::add` length cap | Real slice at the image cap with allocation disabled | Slice one byte above cap under the same allocator | Exact cap reaches allocation and returns OutOfMemory. Excess returns Limit. Both leave the writer empty. |
| Bulk reserve | An incoming field equal in length to the written 17-byte prefix | Initial 17-byte field or an 18-byte field after 17 bytes | Equal append uses geometric capacity 64. Dominating fields reserve exact capacity. A rejected equal append preserves the prefix and live charge. |
| New segment | A partially filled segment can accept the next part | No segment exists, or the last segment is full | Segment reuse and rollover produce the same bytes and region writes as contiguous staging. The rollover control fails a variant that rejects only when replacing a full segment. |
| Encoder start | Aligned `APPEND_START` | Aligned start one region below, or unaligned start one byte above | Each invalid input changes one operand and returns Invalid. The aligned start builds a candidate. |

The key writer tests an empty key with the prefix allocation disabled. A
mutant that continues past the rejected prefix would succeed for the empty
body. For a 17-byte key, one successful allocation writes only the length
prefix before body allocation fails. A complete retry verifies the bytes.
The body error is a delegated return outside the syntax inventory.

The object at the last region checks an oversized payload, an ordinary
object under zero allocation, a successful object ending at the image cap,
and an object beyond the cap. Both early failures preserve the encoder end
and empty staging. The last rejection preserves the prior bytes and end.
The aligned-length and end checks retain propagation scope. The separate
header-length addition and overflow obligations retain their individual gaps.

For both append layouts, the simulator fails the first region write and the
first write after one full segment. The recorded requests stop at the chosen
write and durable bytes stay empty. The successful fixture compares all
bytes and the complete adapter trace. The fresh-image allocation sweep still
requires every rejected preparation to leave the durable destination empty
and restore the exact live charge.

The debug prototype detects all 19 changes in their intended tests and
verifies 73 command retirements. The start OR shares its two-condition table
with the enclosing guard. The recorded alternatives and propagations are
scoped to their fixtures. Delegated allocator and adapter internals and
compiler MC/DC keep separate scope. Storage maintainers own the coupled
start row and revisit it when the admission predicate changes.

The production region of `src/storage/codec.rs` matches the parent byte for
byte. The current codec ledgers bind the updated file hash after the test
additions. All 894 earlier exact ledger references retain their previous
decision bindings. Their archived builds keep their original source hashes.

## Exact current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `d0ec5a37605a1221aebfa6cf5cc53f1a7314139b19d477d4e104807a32cbf373` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/storage/codec.rs:414:24– | 13864..13869 | match | write_append | Both append layouts |
| E | weekend-db::weekend_db | src/storage/codec.rs:419:9– | 14084..14087 | for_iteration | write_append | One and multiple segments |
| E | weekend-db::weekend_db | src/storage/codec.rs:420:48– | 14157..14158 | try_propagation | write_append | Append write propagation |
| E | weekend-db::weekend_db | src/storage/codec.rs:444:9– | 14770..14775 | match | object | Both staging layouts |
| E | weekend-db::weekend_db | src/storage/codec.rs:446:60– | 14883..14884 | try_propagation | object | Contiguous allocation propagation |
| E | weekend-db::weekend_db | src/storage/codec.rs:447:17– | 14902..14905 | for_iteration | object | Header, payload and padding copies |
| E | weekend-db::weekend_db | src/storage/codec.rs:453:17– | 15106..15109 | for_iteration | object | Header, payload and padding copies |
| E | weekend-db::weekend_db | src/storage/codec.rs:454:21– | 15150..15155 | while | object | Complete and split parts |
| P | weekend-db::weekend_db | src/storage/codec.rs:455:25– | 15199..15201 | if | object | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:463:30– | 15599..15600 | try_propagation | object | Segment table allocation propagation |
| E | weekend-db::weekend_db | src/storage/codec.rs:468:88– | 15956..15957 | try_propagation | object | Segment data allocation propagation |
| P | weekend-db::weekend_db | src/storage/codec.rs:493:9– | 16536..16538 | if | add | Directed source pair above |
| P | weekend-db::weekend_db | src/storage/codec.rs:501:9– | 16735..16737 | if | add | Directed source pair above |
| E | weekend-db::weekend_db | src/storage/codec.rs:505:59– | 17061..17062 | try_propagation | add | Exact reservation propagation |
| E | weekend-db::weekend_db | src/storage/codec.rs:508:82– | 17183..17184 | try_propagation | add | Geometric reservation propagation |
| E | weekend-db::weekend_db | src/storage/codec.rs:517:37– | 17437..17438 | try_propagation | key | Key prefix allocation propagation |
| E | weekend-db::weekend_db | src/storage/codec.rs:539:40– | 18092..18093 | try_propagation | object_born | Alignment success and oversized rejection |
| E | weekend-db::weekend_db | src/storage/codec.rs:544:33– | 18284..18285 | try_propagation | object_born | Last accepted and first rejected ends |
| E | weekend-db::weekend_db | src/storage/codec.rs:560:10– | 18940..18941 | try_propagation | object_born | Staging allocation propagation |
| P | weekend-db::weekend_db | src/storage/codec.rs:747:5– | 25013..25015 | if | encode_with_append | Directed start pair above |
| C | weekend-db::weekend_db | src/storage/codec.rs:747:8– | 25016..25083 | short_circuit_or | encode_with_append | Coupled to the enclosing OR guard |

## Frozen qualification at 906b8ed

Source `906b8eddfc408eea0b08998ca49b9325c64cae91` passed in both profiles using
Rust 1.99.1.

| Host | Inventory tests per profile | Core tests per profile | xtask tests per profile | New controls per profile |
|---|---|---|---|---|
| macOS arm64/APFS | 32 passed | 347 passed, 2 explicit ignores | 243 passed, 3 explicit ignores | 19/19 detected |
| Docker Linux arm64/ext4 | 32 passed | 354 passed, 2 explicit ignores | 272 passed, 1 explicit ignore | 19/19 detected |

Both platforms pass formatting and strict all-target Clippy. The core suites
include exact serialization, allocation failures, crash and retained-root
tests. The Linux build and runtime used an owned ext4 volume with the same
pinned image and vendor inputs as the [VALUE batch](codec-value-current.md#frozen-qualification-at-1450e64).
The Windows MSVC core and xtask test targets cross-compile.

The three local shards selected 7, 6 and 6 new controls. Audits checked every
named failure, 320 command retirements, 785 copied and restored source files
and 347 declared inputs per campaign. The reports agree across platforms and
profiles, with all 894 prior references and 21 new exact rows.

| Report | SHA-256 |
|---|---|
| inventory.json | `44bef7ea6d6d10953161506073ed2bb01c64f521b8f6bc10084033a5ab0719e9` |
| review.json | `685dbfd5ff4cafac53b7455066a97dc82bf26c5b6b8655b055129c0308ee895f` |

Local proof `target/archived-proof/r1-encoder-906b8ed.tar.gz` has SHA-256
`39d65ade3a584301e2af6455e05b40c58e049bceee5f1a62d925a57896501aaa`.
Fresh extraction checks 10,476 indexed payloads and replays both audits with
the included Git bundle. Actionlint 1.7.12 accepts the seven-shard workflow.
The actual CLI lists all 205 controls exactly once per profile in that
layout. Hosted execution keeps its own source scope.
