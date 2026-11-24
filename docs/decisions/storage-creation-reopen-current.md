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
| `src/storage/mod.rs` | `d0bd8f6272900611bfdf845205fbaa9c7d72fc36088dfdaef93d9c693ad5a1cd` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/mod.rs:210:89– | 7394..7395 | try_propagation | locked | Refuse the backend box and leave no charge, acquired lease or data I/O. |
| E | weekend-db::weekend_db | src/storage/mod.rs:211:23– | 7419..7420 | try_propagation | locked | Busy leaves the existing owner in control. A subsequent uncontended claim succeeds. |
| E | weekend-db::weekend_db | src/storage/mod.rs:230:53– | 7916..7917 | try_propagation | create | Contended creation propagates Busy before checking the destination. |
| P | weekend-db::weekend_db | src/storage/mod.rs:231:9– | 7927..7929 | if | create | Zero identity stops before querying length; nonzero identity continues. |
| P | weekend-db::weekend_db | src/storage/mod.rs:234:9– | 8010..8012 | if | create | Reject a nonempty destination and accept an empty one with a valid identity. |
| E | weekend-db::weekend_db | src/storage/mod.rs:234:36– | 8037..8038 | try_propagation | create | Length refusal returns Io without advancing to preparation or writes. |
| E | weekend-db::weekend_db | src/storage/mod.rs:239:93– | 8305..8306 | try_propagation | create | An allocation failure after length preserves the empty volatile and durable images. |
| E | weekend-db::weekend_db | src/storage/mod.rs:241:53– | 8406..8407 | try_propagation | create | Header failure returns Io before zeroing any metadata region. |
| E | weekend-db::weekend_db | src/storage/mod.rs:243:9– | 8449..8452 | for_iteration | create | On success write all four zero regions in order; each failure stops at its own callback. |
| E | weekend-db::weekend_db | src/storage/mod.rs:244:73– | 8537..8538 | try_propagation | create | Each zero-region failure returns Io and releases the lease. |
| E | weekend-db::weekend_db | src/storage/mod.rs:246:50– | 8599..8600 | try_propagation | create | Initial publication preserves its Io or OutcomeUnknown status. |
| E | weekend-db::weekend_db | src/storage/mod.rs:253:53– | 8826..8827 | try_propagation | open | Contended open propagates Busy with no reads, writes or flushes. |
| E | weekend-db::weekend_db | src/storage/mod.rs:254:67– | 8895..8896 | try_propagation | open | Fault each recovery I/O call; read or length refusal returns Io and preserves the clean image. |
| E | weekend-db::weekend_db | src/storage/mod.rs:257:78– | 9135..9136 | try_propagation | open | ALLOC-REOPEN-VALUE-INDEX refuses the recovered VALUE index before writes. |
| E | weekend-db::weekend_db | src/storage/mod.rs:257:80– | 9137..9138 | try_propagation | open | CODEC-CACHE-OPEN-VALUE-HEADER refuses the VALUE cache header after the index. |
| E | weekend-db::weekend_db | src/storage/mod.rs:258:87– | 9226..9227 | try_propagation | open | ALLOC-REOPEN-DATA-INDEX refuses the recovered DATA index before writes. |
| E | weekend-db::weekend_db | src/storage/mod.rs:258:89– | 9228..9229 | try_propagation | open | CODEC-CACHE-OPEN-DATA-HEADER refuses the DATA cache header after the index. |
| E | weekend-db::weekend_db | src/storage/mod.rs:262:54– | 9521..9522 | try_propagation | open | Stabilization error reports Io and gates witness write. |
| E | weekend-db::weekend_db | src/storage/mod.rs:269:32– | 9783..9784 | try_propagation | open | Return a handle only after flushing the selected generation witness. |

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
