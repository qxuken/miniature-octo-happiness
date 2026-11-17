# Durability simulator source decision audit

`src/storage/simulator.rs` has **60 production AST records**: 34 `?`, 17 ifs,
four matches, two && records, two loops and one pattern test. Enclosing ifs and
pattern/logical records overlap. This is an executable model of persistence,
not a model of every filesystem/hardware failure or a compiler MC/DC percentage.

P = direct predicate controls; C = duplicate syntax; E = execution/propagation
controls; I = invariant/coupled domain; D = delegated allocation/target behavior
without direct call-site failure attribution. The complete source table follows.

## S1: leases and event dispatch

Existing `tests/bytestore.rs::lease_clones_crashes_and_stale_unlocks_preserve_exclusion`
checks unleased rejection, exclusive locking, clones without leases, crash
invalidation, stale unlock and reacquisition. For check A=disk locked and
B=lease equals current epoch, public states include (T,T), (T,F), (F,F).
(F,T) is unreachable through the public transitions: successful unlock clears its
lease; crashes advance epoch before unlocking; clones have no lease. Therefore
A has no independently toggled valid-state pair with B held true. This is a
coupled domain exclusion, not a falsely claimed full && independence result.

New `deferred_fault_fires_once_and_read_fault_variants_preserve_output_bounds`
arms step 1: step 0 succeeds, step 1 fails, step 2 succeeds. This distinguishes absent,
nonmatching and consumed matching faults. New private
`epoch_and_event_counters_accept_last_step_then_fail_without_wrap` accelerates
counters to MAX-1: the last step succeeds, wrap rejects Exhausted. A rejected
crash preserves the lease, volatile write and prior durable bytes; event overflow
leaves its counter at MAX and can be reset for test cleanup. These are private
counter-state controls, not billions of public operations.

New private `event_capacity_and_recording_controls_preserve_step_order` seeds a
consistent 262143-entry/step trace, accepts the 262144th event and rejects the next
without advancing steps. Disabling tracing admits I/O without growing the full
log; an actual MemoryStore instance verifies record=false. New
`trace_copy_and_first_event_failures_leave_io_state_unchanged` controls empty
nonallocating trace versus nonempty failed output copy and first-event allocation
failure before any I/O mutation. Existing backend allocation sweeps cover other
trace allocation paths.

## S2: ranges and extent

New `simulator_image_cap_rejects_before_copying_the_input` rejects a 64 MiB+1 input
and accepts empty input. Existing near-cap image/headroom tests exercise valid
large images. `byte_ranges_and_one_shot_faults_leave_a_usable_lease` controls
empty reads at the cap, nonempty reads above it, oversized offsets and writes.
New private `synthetic_range_addition_and_extent_guards_are_distinct` distinguishes
representable end==cap, representable end>cap and overflowing start+length.
The usize conversion cannot fail on 64-bit for a u64; a 32-bit-only assertion covers
an unrepresentable offset when that target is run. This host run does not claim
that 32-bit branch executed. Range failures in apply after a stored Pending entry
are coupled away: entries were range-checked before insertion and torn prefixes
cannot exceed the original length.

## S3: crash policy and masks

Existing `crash_policies_cover_writes_beyond_the_mask_word_and_prefix_bounds`
writes 66 bytes, checks DropAll/PersistAll/Mask/Torn, absent/present torn indices,
zero/oversized prefixes and reverse masked survival order for overlapping writes.
For `index<64 && selected_bit`, the selected-bit predicate has independent controls
at a valid index (mask 0 versus set bit). At index 64+, the shift is outside its
valid domain: do not pretend the skipped selected-bit expression can be held true
there. The bounds guard is a safe short-circuit/domain obligation with executed
in-range/out-of-range controls, not a full independent pair for both operands.

New `crash_and_sync_allocation_failures_keep_prior_images_and_lease` fails the
survivor copy and the later volatile copy independently, then permits both.
Failure keeps the prior images and live lease; success installs survivors and
invalidates the lease. Sync copy failure likewise preserves prior durable bytes
and can be retried. No partially prepared survivor image becomes authoritative.

### Empty torn prefixes at a sparse offset

At `7f43297`, the present-write path applies a prefix only when the
requested length is nonzero. For the length pair, the
[sparse append recovery](../near-cap-interruption-recovery.md#sparse-append-recovery)
test holds the pending write index fixed and chooses zero or one byte.
Only the one-byte prefix extends the durable image to the sparse offset.
For the index pair, the ByteStore bounds test holds the nonzero guard true.
An in-range write contributes bytes and an absent write index preserves
the durable image. The separate extent regression verifies a zero prefix
under both a direct crash and a failed sync. The source table below
retains its historical snapshot identity.

## S4: read/write/sync fault arms

The new deferred-read table covers Short0/1/oversized, generic Error, Tear and
SyncError used on a read, and EOF. Rejected reads preserve output bytes.
`write_prefix_failure_and_immediate_durability_have_separate_controls` compares
normal, short/zero writes, zero/nonzero tears, durable false/true and incompatible
fault variants. At equal nonzero prefix, Short succeeds while Tear fails; Tear
with durable false/true changes immediate durable bytes independently of volatile
prefix mutation. Zero prefix bypasses queue growth but still returns the tear's
Io. Existing `tests/durability.rs::pending_write_limit_rejects_only_writes_that_would_grow_the_queue`
controls full pending queue with zero/nonzero effective writes and pre-admission
failure. The durable branches have the same boolean at two distinct points;
both execute in the matching tear fixtures.

`durable_tear_growth_allocation_failure_leaves_both_images_and_pending_queue_unchanged`
adds a sparse one-byte tear on an empty image with trace capacity prepared before
the instance-local allocation hook. Three allocations admit the prefix copy,
pending capacity and volatile capacity: a non-durable tear then returns Io with
the volatile prefix installed. The same budget with immediate durability enabled
returns OutOfMemory at durable-image capacity reservation, preserving both empty
images and the live lease. A subsequent PersistAll crash proves no pending write
was admitted. Four allocations admit the durable tear, which returns Io with both
images extended to 33 bytes. These paired controls cover this reservation's error
outcome; ordinary publication allocation sweeps do not enable immediate tears.

`failed_sync_preserves_only_its_selected_pending_writes` covers ordinary failed
sync variants and SyncError with DropAll, PersistAll, Mask and Torn. Following
successful sync persists the complete volatile image. Crash policies and fault
variants are enum-arm evidence, not compound Boolean conditions.

## Remaining model boundary

`from_image` creates its own Memory before an instance-level failure budget can be
installed. Initial control-block/copy/shared-box allocation failure propagation
is D: allocator-level tests exist, but this tranche adds no global allocation hook
or false constructor call-site attribution. The same limitation applies to
MemoryStore's constructor wrapper. Standard-library Mutex poison panics and
allocator internals are outside this AST inventory. Native durability is not
proved by simulator coverage. Source records are classified; these delegated and
coupled obligations prevent a blanket full-MC/DC claim.

## Source records

Snapshot SHA-256: `16a8bbad3a1724247cfbae2734b8ab20f1443b14552a0c0d5246b9936e4511e0`.
Production record identities below are line:column plus the first 12 hex digits
of the inventory source-token hash; test-only additions are excluded by the tool.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 82:9 | `if` / `from_image` | P | S2 | `935f68319d4f` |
| 85:49 | `try_propagation` / `from_image` | D | Constructor allocation boundary | `8a8de823d5ed` |
| 87:47 | `try_propagation` / `from_image` | D | Constructor allocation boundary | `8a8de823d5ed` |
| 88:46 | `try_propagation` / `from_image` | D | Constructor allocation boundary | `8a8de823d5ed` |
| 96:12 | `try_propagation` / `from_image` | D | Constructor allocation boundary | `8a8de823d5ed` |
| 116:63 | `try_propagation` / `trace` | E | S1 | `8a8de823d5ed` |
| 125:70 | `try_propagation` / `crash` | E | S1 | `8a8de823d5ed` |
| 126:56 | `try_propagation` / `crash` | E | S3 | `8a8de823d5ed` |
| 127:56 | `try_propagation` / `crash` | E | S3 | `8a8de823d5ed` |
| 143:9 | `if` / `check` | C | S1 | `935f68319d4f` |
| 143:12 | `short_circuit_and` / `check` | I | S1 | `4b7b33842a91` |
| 150:25 | `try_propagation` / `event` | E | S1 | `8a8de823d5ed` |
| 151:9 | `if` / `event` | P | S1 | `935f68319d4f` |
| 152:13 | `if` / `event` | P | S1 | `935f68319d4f` |
| 155:59 | `try_propagation` / `event` | E | S1 | `8a8de823d5ed` |
| 159:71 | `try_propagation` / `event` | E | S1 | `8a8de823d5ed` |
| 160:9 | `if` / `event` | P | S1 | `935f68319d4f` |
| 168:66 | `try_propagation` / `range` | D | S2: 64-bit invariant; 32-bit conditional control | `8a8de823d5ed` |
| 172:29 | `try_propagation` / `range` | E | S2 | `8a8de823d5ed` |
| 176:53 | `try_propagation` / `apply` | I | S2: validated pending range / pre-reserved write capacity | `8a8de823d5ed` |
| 177:5 | `if` / `apply` | P | S2 | `935f68319d4f` |
| 178:55 | `try_propagation` / `apply` | E | S2 | `8a8de823d5ed` |
| 185:53 | `try_propagation` / `survive` | E | S3 | `8a8de823d5ed` |
| 186:5 | `match` / `survive` | E | S3 | `4945a70fa7f9` |
| 189:13 | `for_iteration` / `survive` | E | S3 | `10c22bcf4c76` |
| 190:71 | `try_propagation` / `survive` | E | S3 | `8a8de823d5ed` |
| 194:13 | `for_iteration` / `survive` | E | S3 | `10c22bcf4c76` |
| 195:17 | `if` / `survive` | C | S3 | `935f68319d4f` |
| 195:20 | `short_circuit_and` / `survive` | I | S3 | `7717068a26d5` |
| 196:75 | `try_propagation` / `survive` | E | S3 | `8a8de823d5ed` |
| 201:13 | `if` / `survive` | P | S3 | `935f68319d4f` |
| 201:16 | `pattern_test` / `survive` | C | S3 | `5654d4106d70` |
| 207:18 | `try_propagation` / `survive` | E | S3 | `8a8de823d5ed` |
| 216:9 | `if` / `lock` | P | S1 | `935f68319d4f` |
| 225:9 | `if` / `unlock` | P | S1 | `935f68319d4f` |
| 232:9 | `if` / `length` | P | S1 | `935f68319d4f` |
| 240:14 | `try_propagation` / `length` | E | S1 | `8a8de823d5ed` |
| 256:10 | `try_propagation` / `read` | E | S4 | `8a8de823d5ed` |
| 257:25 | `match` / `read` | E | S4 | `4945a70fa7f9` |
| 262:50 | `try_propagation` / `read` | E | S2 | `8a8de823d5ed` |
| 264:9 | `if` / `read` | P | S4 | `935f68319d4f` |
| 278:10 | `try_propagation` / `write` | E | S4 | `8a8de823d5ed` |
| 279:34 | `match` / `write` | E | S4 | `4945a70fa7f9` |
| 285:44 | `try_propagation` / `write` | E | S2 | `8a8de823d5ed` |
| 286:9 | `if` / `write` | P | S4 | `935f68319d4f` |
| 287:13 | `if` / `write` | P | S4 | `935f68319d4f` |
| 290:59 | `try_propagation` / `write` | E | S4 | `8a8de823d5ed` |
| 291:60 | `try_propagation` / `write` | E | S4 | `8a8de823d5ed` |
| 293:65 | `try_propagation` / `write` | E | S4 | `8a8de823d5ed` |
| 295:13 | `if` / `write` | P | S4 | `935f68319d4f` |
| 297:68 | `try_propagation` / `write` | E | S4 | `8a8de823d5ed` |
| 299:63 | `try_propagation` / `write` | I | S2: validated pending range / pre-reserved write capacity | `8a8de823d5ed` |
| 300:13 | `if` / `write` | P | S4 | `935f68319d4f` |
| 301:66 | `try_propagation` / `write` | I | S2: validated pending range / pre-reserved write capacity | `8a8de823d5ed` |
| 308:9 | `if` / `write` | P | S4 | `935f68319d4f` |
| 319:10 | `try_propagation` / `sync` | E | S4 | `8a8de823d5ed` |
| 320:9 | `match` / `sync` | E | S4 | `4945a70fa7f9` |
| 322:69 | `try_propagation` / `sync` | E | S4 | `8a8de823d5ed` |
| 328:65 | `try_propagation` / `sync` | E | S3 | `8a8de823d5ed` |
| 342:40 | `try_propagation` / `new` | D | Constructor allocation boundary | `8a8de823d5ed` |

Disposition totals: C=3, D=6, E=31, I=5, P=15; **60 records**.

## Executed validation

On the local macOS host, all-feature debug and release runs passed: 38 storage
unit tests, 8 `storage_decisions`, 3 `bytestore`, 14 `durability`, 3 `headroom`,
and 10 `compaction` tests (76 tests per profile). Strict Clippy for the library
and new integration target passed. The source inventory completed with zero
parse failures. This run does not add Windows, Linux, 32-bit, or physical
power-loss evidence; it does not claim mutation or compiler MC/DC measurement.

For the sparse durable-tear reservation regression, local macOS all-feature debug
and release checks passed the targeted test, 70 storage unit tests and 45 related
integration tests per profile: 10 `storage_decisions`, 4 `bytestore`, 17
`durability`, 3 `headroom` and 11 `compaction`. Strict Clippy passed for the
`storage_decisions` target. This adds failure evidence for the instance-local
durable-image reservation, without changing the constructor or platform boundary.

The empty-prefix correction also required updating the follower staging
oracle in `6e03837`. Its prior expectation allocated zero-filled bytes
through the selected write's offset even when the persisted prefix was
empty. Hosted library checks exposed that mismatch. The follower test
now expects an empty new image for this case, while retaining every
receipt, selected-image, retry and retained-root check.

At `525ad6c`, Docker Linux arm64 Rust 1.99.0 passes the full all-feature
workspace debug suite with 733 passes and eight registered ignores.
Its release run passes 148 core unit tests and ten storage-decision
tests, followed by all 29 fuzz replays. The Mac workspace run with the
same verified source bytes passes 697 tests and six registered ignores.
The first Linux run used a bare source archive and four tool tests
failed because they require Git identity. The successful rerun used a
Git bundle of the exact source and checked its clean detached commit.
The local archive
`target/archived-proof/follower-oracle-allocation-525ad6c.tar.zst`
retains both Linux logs, the source bundle and tar, and the Mac source
hashes and logs. All 24 indexed files match after extraction. Archive
SHA-256
`dd89e0fad5f78d7909a2c90dbcb94c1a75858a18f08abc099e275b50f6cc143e`.
