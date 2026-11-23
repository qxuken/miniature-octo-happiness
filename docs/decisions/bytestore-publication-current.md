# ByteStore transfer progress and local publication

The exact-transfer tests supply short reads and writes through ByteStore.
They verify accepted bytes and the next offset and suffix, including a
failure after partial progress. An empty buffer makes no callback. Reads
give the provider at most one region. Writes also end each request at or
before the next region boundary.

| Transfer decision | Cases | Result |
| --- | --- | --- |
| Buffer remains | Empty, one complete callback, multiple short callbacks | Skip I/O or transfer the exact buffer |
| Zero progress or overcount | `(false, false)`, `(true, false)`, `(false, true)` | Advance by the reported count, or return Io without another callback |
| Backend error | Success or Busy after an accepted prefix | Preserve the callback error and accepted prefix |
| Offset addition | Representable next offset or u64 overflow | Continue, or return Corrupt for reads and Limit for writes |
| Flush after write | Partial transfer error, completed transfer, sync error | Stop without flushing, flush once, or preserve the sync error |

Private persist rejects an already poisoned store and an invalid next
revision before I/O. Normal increment, mismatch and u64 exhaustion cover
the revision comparison. Backend-length failure poisons the store with Io.
A length in range passes the two lower and upper guards. A short image
or a length above the cap independently poisons the store with Corrupt.
The error exit of the subsequent alignment is infeasible here. Alignment
rounds up to a region boundary and the upper image cap is itself aligned,
so no admitted length can overflow or round above it.

## Reopen matrix

The fixture starts at revision 3 with revision 2 retained. It verifies the
complete successful trace, including region bounds for append writes and
parity-specific offsets for root 4 and its witness. It then fails each
callback in turn. Every failure stops at that callback, poisons the
writer and leaves revision 3 visible until reopen. The sweep checks the
exact current and retained rows against the successful and original
images. A successful commit and reopen both return revision 4 with the
same retained rows.

| Failed callback | Commit result | Reopened revision |
| --- | --- | --- |
| Length, append write, or append flush | Io | 3 |
| Root write | OutcomeUnknown | 3 |
| Root flush | OutcomeUnknown | 4 if its pending root persists in full, otherwise 3 |
| Witness write or witness flush | OutcomeUnknown | 4 |

The root-flush failure leaves one pending write. PersistAll and the mask
that selects it reopen revision 4. DropAll, the mask that omits it and
a torn 63-byte prefix reopen revision 3. These are the simulator's
persistence choices. Native device flush guarantees remain a deployment
gate. Follower durability acknowledgment remains a separate roadmap item.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/mod.rs` | `d0bd8f6272900611bfdf845205fbaa9c7d72fc36088dfdaef93d9c693ad5a1cd` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P | weekend-db::weekend_db | src/storage/mod.rs:286:9– | 10298..10300 | if | guard | Clear and poisoned store, before any backend callback. |
| E | weekend-db::weekend_db | src/storage/mod.rs:299:21– | 10602..10603 | try_propagation | persist | Poison refusal propagates without advancing to revision validation or length. |
| P | weekend-db::weekend_db | src/storage/mod.rs:300:9– | 10613..10615 | if | persist | Consecutive revision succeeds; mismatch and exhaustion stop before I/O. |
| E | weekend-db::weekend_db | src/storage/mod.rs:303:22– | 10741..10746 | match | persist | Length success continues; refusal poisons with Io before append. |
| P | weekend-db::weekend_db | src/storage/mod.rs:310:9– | 10929..10931 | if | persist | Each length boundary rejects independently; valid length prepares the append. |
| C | weekend-db::weekend_db | src/storage/mod.rs:310:12– | 10932..10984 | short_circuit_or | persist | The same two conditions are covered by the preceding if pair. |
| I | weekend-db::weekend_db | src/storage/mod.rs:314:41– | 11111..11112 | try_propagation | persist | An admitted length rounds up within the aligned cap and cannot overflow. |
| E | weekend-db::weekend_db | src/storage/mod.rs:324:10– | 11366..11367 | try_propagation | persist | ALLOC-COMMIT-PREPARATION rejects provider refusal before append and preserves both roots. |
| E | weekend-db::weekend_db | src/storage/mod.rs:341:51– | 12043..12044 | try_propagation | publish | Append and append flush must finish before the first root write. A refusal returns Io and poisons. |
| E | weekend-db::weekend_db | src/storage/mod.rs:352:47– | 12607..12608 | try_propagation | publish | The parity root must complete its flush before the witness; late refusal poisons with OutcomeUnknown. |
| E | weekend-db::weekend_db | src/storage/mod.rs:361:38– | 12924..12925 | try_propagation | write_synced | A short-write failure prevents sync; complete transfer permits sync and propagates its result. |
| P | weekend-db::weekend_db | src/storage/mod.rs:369:5– | 13072..13077 | while | read_all | An empty read skips I/O; a nonempty read visits, progresses and terminates. |
| E | weekend-db::weekend_db | src/storage/mod.rs:371:55– | 13192..13193 | try_propagation | read_all | Preserve Busy and the already read prefix when the next read fails. |
| P | weekend-db::weekend_db | src/storage/mod.rs:372:9– | 13203..13205 | if | read_all | Zero and oversized progress independently reject; positive valid progress continues. |
| C | weekend-db::weekend_db | src/storage/mod.rs:372:12– | 13206..13224 | short_circuit_or | read_all | The same two conditions are covered by the preceding read if pair. |
| E | weekend-db::weekend_db | src/storage/mod.rs:375:68– | 13339..13340 | try_propagation | read_all | An ordinary next read offset succeeds; overflow returns Corrupt. |
| P | weekend-db::weekend_db | src/storage/mod.rs:385:5– | 13513..13518 | while | write_all | An empty write skips I/O; nonempty input crosses a region edge correctly. |
| E | weekend-db::weekend_db | src/storage/mod.rs:387:54– | 13672..13673 | try_propagation | write_all | Preserve Busy and the already written prefix when the next write fails. |
| P | weekend-db::weekend_db | src/storage/mod.rs:388:9– | 13683..13685 | if | write_all | Zero and oversized progress independently reject; a positive partial write continues. |
| C | weekend-db::weekend_db | src/storage/mod.rs:388:12– | 13686..13704 | short_circuit_or | write_all | The same two conditions are covered by the preceding write if pair. |
| E | weekend-db::weekend_db | src/storage/mod.rs:391:66– | 13817..13818 | try_propagation | write_all | An ordinary next write offset succeeds; overflow returns Limit. |

## Verification at e8d76f1

- Rust 1.100.0 passed 236 tests per profile on macOS arm64 and 244 per
  profile on Docker Linux arm64 with ext4. Both golden histories passed
  in both profiles. The frozen candidate was
  `e8d76f14309343da92fac46cf4a4f4f69ef49fdc`.
- Miri passed all four exact-transfer tests. Strict Clippy passed on
  macOS and for the Windows MSVC target. Ten mutation-runner tests passed.
- On macOS, both profiles detected all 27 selected controls, including
  the existing commit-preparation refusal control. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-bytestore- --case r1-alloc-commit-preparation`
  and repeat with `--release`. Four disjoint shards per profile used
  the same frozen source.
- The inventory maps 20 additional source records. It reports 10,581
  total decisions and 9,628 open records, including 2,258 runtime.
