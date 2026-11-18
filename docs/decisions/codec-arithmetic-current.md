# Current codec arithmetic and cursor decisions

This R1 batch starts at `edec275384c7cf1d13cf9e0a05d9933f646cc436`.
It reviews alignment and the cursor used by durable-image decoding. Production
codec behavior and serialized bytes are unchanged. The historical
[codec ledger](codec.md) retains its original source versions; its rows are not
silently repinned to the current file.

## Directed decisions and propagation scope

`storage::codec::tests::alignment_and_cursor_boundaries_preserve_failed_take_position`
supplies the existing controls. The new
`cursor_zero_limits_and_short_tags_preserve_the_expected_position` test adds
empty input, every 0–7-byte integer prefix through both count and key readers,
zero limits and values, consumed-prefix state on Limit, and the largest integer
on qualified 64-bit ABIs.

| Decision | Condition | False control | True control | Required result |
|---|---|---|---|---|
| CODEC-ALIGN-CAP | `value > MAX_IMAGE_BYTES` | align(cap) | align(cap+1) | Exact cap succeeds; rounded result above cap is Limit |
| CODEC-COUNT-MAX | `n > maximum` | count(3,3), count(0,0) | count(4,3), count(1,0) | Equality succeeds; excess returns Limit after consuming the valid tag |
| CODEC-CURSOR-DONE | `pos == bytes.len()` | Fresh nonempty cursor | Empty cursor or fully consumed cursor | Unconsumed bytes return Corrupt; exact completion succeeds |

Each predicate has one condition and both outcomes. The three tables are
directed source-level pairs, not compiler-measured MC/DC. Existing tests also
cover alignment overflow, zero/exact-region rounding, successful and failed
take, short integer reads and key limits/truncation. All failed `take` calls
preserve position. Count/key operations consume a valid length tag before a
later limit or payload failure; whole-cursor rollback is not their contract.

The five E rows below identify actual propagation sites and their scoped
success/error controls. They do not imply every transitive std or caller branch
was independently varied. Cursor's checked addition and `Option::filter`
predicate are delegated primitives; their overflow/bounds outcomes are tested
but are not additional compiler-verified predicates in this AST denominator.
The `usize::try_from(u64)` site is D: the largest value converts on the qualified
64-bit targets, while unsupported narrower representations retain a separate
portability obligation. No fabricated slices or invalid raw values are used.

The checksum-table G row is included deliberately as an unresolved example.
A historical G row must remain an individual gap; it cannot become evidence
merely by acquiring an exact source pin. A reconciler regression rejects the
previous promotion, and a guard mutation restores that bug to demonstrate
detection. Independently reviewed non-gap evidence can still qualify a record.

## Mutation controls

Reuse the existing bounded runner and nonempty baselines. The new controls
alter alignment overflow/cap handling, take equality and propagation, integer
and count propagation, count equality, key propagation, cursor completion,
and historical-G classification. Every detected mutant must compile and fail
an actual test with Cargo exit 101; timeouts, compiler errors and cleanup
failures cannot pass. Sources must be restored after every attempt.

```sh
cargo +1.99.1 test --offline --locked --all-features -p weekend-db --lib storage::codec::tests::alignment_and_cursor_boundaries_preserve_failed_take_position -- --exact
cargo +1.99.1 test --offline --locked --all-features -p weekend-db --lib storage::codec::tests::cursor_zero_limits_and_short_tags_preserve_the_expected_position -- --exact
cargo +1.99.1 xtask mutations --case r1-codec- --case r1-inventory-historical-gap --toolchain 1.99.1 --output target/codec-arithmetic-debug
cargo +1.99.1 xtask mutations --case r1-codec- --case r1-inventory-historical-gap --release --toolchain 1.99.1 --output target/codec-arithmetic-release
```

The remaining codec records stay in the individual G queue. The source table
below binds only this batch; it does not claim whole-codec qualification or
close compiler, generated-code, native or release gates. Completed platform
results retain the frozen source below.

## Current source rows

| Source | SHA-256 |
|---|---|
| `src/storage/codec.rs` | `15be88e7381e9ea806ee9c9219505526b1cc2d30c79b3bce877ea65ec28f71ea` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence / residual |
|---|---|---|---|---|---|---|
| G | weekend-db::weekend_db | src/storage/codec.rs:40:5– | 1217..1222 | while | crc_tables | Checksum table generation remains outside this batch |
| E | weekend-db::weekend_db | src/storage/codec.rs:95:29– | 3219..3220 | try_propagation | align | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| P | weekend-db::weekend_db | src/storage/codec.rs:96:5– | 3226..3228 | if | align | Named directed source pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:806:35– | 26797..26798 | try_propagation | take | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| E | weekend-db::weekend_db | src/storage/codec.rs:812:43– | 26973..26974 | try_propagation | int | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| E | weekend-db::weekend_db | src/storage/codec.rs:815:43– | 27104..27105 | try_propagation | count | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| D | weekend-db::weekend_db | src/storage/codec.rs:815:73– | 27134..27135 | try_propagation | count | Qualified 64-bit conversion; narrower ABI is delegated |
| P | weekend-db::weekend_db | src/storage/codec.rs:816:9– | 27145..27147 | if | count | Named directed source pair |
| E | weekend-db::weekend_db | src/storage/codec.rs:823:36– | 27338..27339 | try_propagation | key | Alignment/cursor boundaries plus zero-limit and short-tag controls |
| P | weekend-db::weekend_db | src/storage/codec.rs:827:9– | 27411..27413 | if | done | Named directed source pair |

## Frozen qualification at a255842

Source **`a2558421156844e528c49d88ed520e4e91236f50`**, Rust **1.99.1**,
passes both profiles on macOS arm64/APFS and Docker Linux arm64/ext4:

| Host | Inventory tests / profile | Core tests / profile | xtask tests / profile | R1 mutations / profile |
|---|---|---|---|---|
| macOS | 32 passed | 341 passed, 2 explicit ignores | 239 passed, 2 explicit ignores | 27/27 detected |
| Docker Linux | 32 passed | 348 passed, 2 explicit ignores | 268 passed, 1 explicit ignore | 27/27 detected |

Core execution uses `cargo +1.99.1 test --offline --locked --all-features -p
weekend-db`, with `--release` for the optimized profile. It includes all 39 codec
tests, exact serialization goldens, allocation/budget failures, crash/reopening
and retained-root suites. Inventory and xtask use their complete package tests. All three packages
pass strict Clippy for all targets; both touched workspaces pass formatting.
The mutation runs select the full `r1-` prefix, including seventeen earlier
controls and all ten additions. The first 73 registry entries are unchanged.

Core's two libtest ignores are the private `native_files::worker` entry (used
by bounded parent tests) and `native_manifest_growth_and_reseed_preserve_receipts`
(a separate maintenance lane). xtask's authenticated replica-progress fixture
is ignored on both hosts; macOS also ignores the native busy-detach control.
The table counts parent-test outcomes and does not give skipped entries implied
standalone execution credit.

Each of four campaigns starts from clean Git and has **778 copied files** and
**347 declared inputs** independently compared to frozen Git blobs. All copied
files match again after restoration. All **96 command reports per campaign**
confirm retirement, matching stream lengths and successful control handling.
Every baseline is nonempty; each mutant compiles and produces a genuine failed
test with exit 101. In particular, restoring historical-G promotion fails the
gap regression with `historical_scoped_disposition` instead of `G`; the corrected
baseline passes while its separate named control still qualifies the record.

The entire production codec prefix before its test module is byte-identical to
`edec275`; no serialized-byte change is inferred merely from a round trip.
Production inventory and reconciliation JSON are also byte-identical across
both profiles and hosts:

| Report | SHA-256 |
|---|---|
| inventory.json | `c49cd270c8a43c72f1e00987442582e4874ebba55bb557296b3220dec9b5f7cd` |
| review.json | `352c5360a7a752a3e9f75b040dcbe5dfef0a06a2bc504d36ccfec1445f6643aa` |

The reports retain 10,482 records, 215 files, 66 targets, 4,946 blind spots and
zero failures. They bind all 21 manifests, fourteen locks and 73 references;
all 22 zero-record fuzz targets remain visible. The nine new reviewed records
leave **9,740 individual G records** (2,411 runtime / 26 driver / 7,303 tooling).
All twenty-one Cargo manifests remain `publish = false`.

Linux has no network. Source, vendor dependencies, Cargo home, every build,
temporary/runtime files and evidence use the owned ext4 volume. The immutable
arm64 image is
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
The source bundle digest is
`6d288039d878f910e558fcb653d09902f40de409b03fcff407f30f2f778ef35b`;
the selected locked vendor archive retains
`7775e0b19c5b40db61946a31589855313dc6c91c12a46c96a1076a0cc80bddf6`.
The successful container exits 0; both owned containers and the volume are
removed after export and audit. An initial macOS launcher stopped before tests
because Bash 3.2 rejected an empty array under `set -u`. Its setup log is kept
without credit; the successful strict launcher uses portable positional flags.

The local proof is `target/archived-proof/r1-codec-a255842.tar.gz`, SHA-256
**`20ed2fa298ec901cb348c393a6da4a90cce01ed1acce5a54a1d36b60f69e1650`**.
Fresh extraction verifies **all 4,346 indexed payloads** and reruns both audits.
The proof contains source/vendor bundles, the launchers, raw logs/process reports,
restored source copies and the independent audit recipe. It excludes disposable
build caches. Restoration is local evidence; R7 custody/retention stays open.
Two restored-copy negative controls reject changed codec source and promotion
of the checksum G row. Restoring the original bytes passes again, and all
4,346 payloads still match their index afterward. The archived audit recipe
can be rerun from a checkout of `recipe/source.bundle`, passing the extracted
`macos` or `linux` directory and its `mutations-{profile}` path as arguments.
The source-level assurance scope above does not qualify physical power loss,
unsupported ABIs or complete compiler MC/DC. Native Windows and final-head CI
are follow-ups to this frozen local result.
