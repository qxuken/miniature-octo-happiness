# Storage creation, lease ownership and reopen stabilization

Creation and reopen allocate the backend box and acquire the exclusive
lease before media I/O. A contested lease returns Busy and preserves the
current owner. Backend boxing can fail before lock. The engine drops the
backend after a failed lock call. Once lock succeeds, the Storage owner
unlocks on normal close and when a later step fails.

| Operation | Success prerequisite | Failure evidence |
| --- | --- | --- |
| Allocate backend box | Allocator grants the request | OutOfMemory without media I/O |
| Acquire lease | Exclusive claim is available | Busy with an empty I/O trace and the previous owner still able to read |
| Admit a new image | Nonzero identity and empty destination | Zero identity skips the length call. An occupied destination stops after length. Both preserve bytes. |
| Prepare a new image | All allocations through candidate encoding succeed | Refuse one request at a time, check an empty durable and volatile image, zero live allocator bytes and immediate lease reacquisition |
| Write the initial regions | Header, four zero regions and append in that order | Fail each callback and check the exact trace prefix and immediate lease release |
| Read and verify on open | Complete graph and cache preparation | A failed read or refused allocation unwinds before stabilization. Existing cache controls distinguish the VALUE and DATA indexes and headers. |

## Initial creation

| First failed operation | Create result | Result of reopen after interruption |
| --- | --- | --- |
| Length, header, zero-region write, append write or append flush | Io | Corrupt |
| First root write attempt | OutcomeUnknown | Corrupt |
| Root flush | OutcomeUnknown | Revision 0 if PersistAll preserves the pending root, otherwise Corrupt |
| Witness write or witness flush | OutcomeUnknown | Revision 0 |

The recovered revision 0 has the supplied identity, an empty retained catalog
and the same durable bytes as an uninterrupted creation. The failure sweep
reclaims the lease before invoking crash, which would also reset it.

## Stabilization of a volatile root

The second-failure sweep begins with revision 3 and retains revision 2.
An uncertain commit leaves the root for revision 4 visible in the running
process. A separate successful commit supplies the expected rows and final
durable image. Open must verify the graph, sync the pending root, write
the selected parity witness and sync the witness before returning a handle.

| Open outcome | First failed stage | Reopened revision |
| --- | --- | --- |
| Success | None | 4 |
| Io and lease released | Root stabilization flush | 4 if the failed flush or subsequent crash preserves the root, otherwise 3 |
| Io and lease released | Witness write or witness flush | 4 |

The test verifies the exact last three callbacks and the parity and region
size of the witness. A torn witness cannot discard the stabilized root.
Every retry matches complete current and retained rows against the saved
images. When revision 4 wins, including after a torn witness, reopening
recreates the exact successful durable image.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/mod.rs` | `68916e5c5a0e7003a5e77a047e40f6af8de5c8a569c384e254e26f2851d0b457` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/mod.rs:202:89– | 7212..7213 | try_propagation | locked | Refuse the backend box and leave no charge, acquired lease or data I/O. |
| E | weekend-db::weekend_db | src/storage/mod.rs:203:23– | 7237..7238 | try_propagation | locked | Busy leaves the existing owner in control. A subsequent uncontended claim succeeds. |
| E | weekend-db::weekend_db | src/storage/mod.rs:222:53– | 7734..7735 | try_propagation | create | Contended creation propagates Busy before checking the destination. |
| P | weekend-db::weekend_db | src/storage/mod.rs:223:9– | 7745..7747 | if | create | Zero identity stops before querying length; nonzero identity continues. |
| P | weekend-db::weekend_db | src/storage/mod.rs:226:9– | 7828..7830 | if | create | Reject a nonempty destination and accept an empty one with a valid identity. |
| E | weekend-db::weekend_db | src/storage/mod.rs:226:36– | 7855..7856 | try_propagation | create | Length refusal returns Io without advancing to preparation or writes. |
| E | weekend-db::weekend_db | src/storage/mod.rs:231:93– | 8123..8124 | try_propagation | create | An allocation failure after length preserves the empty volatile and durable images. |
| E | weekend-db::weekend_db | src/storage/mod.rs:233:53– | 8224..8225 | try_propagation | create | Header failure returns Io before zeroing any metadata region. |
| E | weekend-db::weekend_db | src/storage/mod.rs:235:9– | 8267..8270 | for_iteration | create | On success write all four zero regions in order; each failure stops at its own callback. |
| E | weekend-db::weekend_db | src/storage/mod.rs:236:73– | 8355..8356 | try_propagation | create | Each zero-region failure returns Io and releases the lease. |
| E | weekend-db::weekend_db | src/storage/mod.rs:238:50– | 8417..8418 | try_propagation | create | Initial publication preserves its Io or OutcomeUnknown status. |
| E | weekend-db::weekend_db | src/storage/mod.rs:245:53– | 8644..8645 | try_propagation | open | Contended open propagates Busy with no reads, writes or flushes. |
| E | weekend-db::weekend_db | src/storage/mod.rs:246:67– | 8713..8714 | try_propagation | open | Fault each recovery I/O call; read or length refusal returns Io and preserves the clean image. |
| E | weekend-db::weekend_db | src/storage/mod.rs:249:78– | 8953..8954 | try_propagation | open | ALLOC-REOPEN-VALUE-INDEX refuses the recovered VALUE index before writes. |
| E | weekend-db::weekend_db | src/storage/mod.rs:249:80– | 8955..8956 | try_propagation | open | CODEC-CACHE-OPEN-VALUE-HEADER refuses the VALUE cache header after the index. |
| E | weekend-db::weekend_db | src/storage/mod.rs:250:87– | 9044..9045 | try_propagation | open | ALLOC-REOPEN-DATA-INDEX refuses the recovered DATA index before writes. |
| E | weekend-db::weekend_db | src/storage/mod.rs:250:89– | 9046..9047 | try_propagation | open | CODEC-CACHE-OPEN-DATA-HEADER refuses the DATA cache header after the index. |
| E | weekend-db::weekend_db | src/storage/mod.rs:254:54– | 9339..9340 | try_propagation | open | Stabilization error reports Io and gates witness write. |
| E | weekend-db::weekend_db | src/storage/mod.rs:261:32– | 9601..9602 | try_propagation | open | Return a handle only after flushing the selected generation witness. |

## Verification at de38aac

- Rust 1.100.0 debug and release passed 238 tests per profile on macOS
  arm64 and 246 on Docker Linux arm64 with ext4. Both golden histories
  passed in all four runs. The frozen source was
  `de38aac343a989299ff8ca5e52e9f092b33b0eb1`.
- Both macOS profiles detected 24 controls. The selection includes 20
  new controls and the four existing cache allocation controls. The
  unlock control detects a leaked lease on drop. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-storage-admission- --case r1-alloc-reopen- --case r1-cache-open-`
  and repeat with `--release`. The eight clean-source shard reports
  verify 218 supervised commands.
- Miri passed the cold-open cache allocation and lease cleanup sweep.
  Strict Clippy passed on macOS and for Windows MSVC. Ten mutation-runner
  tests passed.
- The inventory maps 15 additional source records. It reports 10,581
  decisions and 9,613 open records, including 2,243 runtime.
