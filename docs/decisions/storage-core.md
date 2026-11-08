# Storage core source decision audit

The [source inventory](../decision-inventory.md) identifies **57 production AST
records** in `src/storage/mod.rs`: 31 `?`, 11 ifs, 11 short-circuit OR records,
two while loops, one for loop and one match. Nested ORs/containing ifs overlap;
this is not a 57-condition compiler MC/DC denominator. Every record is classified
below. Tests added by this audit do not alter production storage behavior.

P = direct predicate controls; C = duplicate enclosing/intermediate syntax;
E = execution/propagation evidence (not every callee error variant); I = invariant
makes the error outcome infeasible. Native primitives remain a separate delegated
obligation even where their calling operation has E evidence.

## K1: physical length and health

New `tests/storage_decisions.rs::source_length_guards_distinguish_short_over_cap_and_valid_images`
uses a real valid database plus a backend reporting its actual length, zero, or
MAX_IMAGE_BYTES+1. For A=length below committed end and B=length above image cap,
(F,F) accepts, (T,F) rejects, (F,T) rejects. Both predicates are independently
controlled for space, persist and compact. Invalid sources are unchanged and no
later source I/O occurs. Space/persist poison publication; compact's existing
rejection does not mark the source poisoned. This distinction is preserved.
`tests/headroom.rs::impossible_lengths_block_publication` and
`headroom_counts_uncommitted_tail_and_uses_only_length` retain headroom, tail,
length-I/O retry and poison controls. After the length bound, align cannot fail:
length<=64MiB and rounding to 4096 neither overflows u64 nor exceeds the aligned cap.

## K2: verified image envelope

New private `storage::decision_tests::verification_envelope_predicates_have_independent_controls`
recovers a valid image and changes exactly one expected field or removes its
witness. Let A=identity mismatch, B=revision mismatch, C=end mismatch,
D=missing witness. Baseline (F,F,F,F) succeeds; (T,F,F,F), (F,T,F,F), (F,F,T,F),
(F,F,F,T) reject Corrupt. Each pair uses an otherwise valid, decodable image;
private expected-state changes are explicit defensive boundary tests. The witness
case is a valid unwitnessed image, not an invalid checksum. Both `Storage::verify`
and feature-gated `recover_replica` execute the same controls. Verification only
reads/queries length and leaves image bytes unchanged. This complements follower
image verification/fault tests without claiming their entire protocol here.

## K3: lease and creation

New `creation_identity_and_empty_destination_guards_release_failed_leases`
compares zero/nonzero identity and empty/nonempty destinations. Rejected creation
preserves source bytes and releases the lease; the valid control creates revision 0.
Existing `tests/compaction.rs::compaction_rejects_nonempty_or_leased_destinations_and_native_sources`
and `tests/bytestore.rs::lease_clones_crashes_and_stale_unlocks_preserve_exclusion`
exercise busy leases. `allocation_failures_during_copy_and_verification_never_change_the_source`
includes fallible backend boxing, candidate preparation and verifier allocations.
Default/native allocator and lock internals are outside this module's denominator.

## K4: expected revision and durable publication

New private `persist_revision_mismatch_and_exhaustion_reject_before_io` accepts
(current 0, proposed 1), rejects proposed 0/2, and rejects current MAX/proposed 0 when
checked increment has no value. Rejections leave bytes, health and trace unchanged.
The mismatch/overflow controls model a private precondition that valid engine
publication normally enforces earlier; they do not fabricate a wrapped commit.

Existing `tests/durability.rs::publication_failures_reopen_the_revision_that_survived`
and `torn_writes_short_writes_and_failed_syncs` sweep append, root and witness
writes/syncs. Append failure maps to Io; from the root-slot attempt onward failure
maps to OutcomeUnknown; either poisons publication. Successful completion exposes
the new head only after all barriers. `interrupted_initial_creation_has_no_prior_acknowledged_state`
sweeps initial header/zeroed metadata writes. `reopening_an_uncertain_volatile_root_stabilizes_it_before_exposure`
and `recovery_errors_and_short_io_do_not_damage_a_clean_image` cover recovery,
stabilization and witness propagation. These reuse PUB-V1/REC-V2 scoped evidence;
no new barrier proof or real power-loss guarantee is asserted.

`copy_faults_at_every_io_leave_source_unchanged_and_only_whole_copies_recover`
reuses compaction creation/verification error paths. `Storage::guard` healthy versus
poisoned controls are also supplied by the new length tests and existing engine
publication-gate tests.

## K5: exact transfer progress

Existing `storage::transfer_tests::exact_transfers_reject_zero_oversized_and_failed_progress`
controls A=(n==0), B=(n>requested): n=1 on a two-byte request completes in two calls;
n=0 and n=3 reject Io independently. Scripted Err stops immediately.
`transfers_skip_empty_buffers_and_bound_offsets_and_regions` covers empty/nonempty
loops, u64::MAX offset overflow (read Corrupt/write Limit), unaligned write splits
and multi-region reads. `write_synced` preserves the order write-all then sync;
the publication fault suites cover both error boundaries. The metadata-zeroing
loop always has four iterations when reached; fault sweeps cover early exit,
not a fabricated empty range.

## Scope and residual obligations

Every listed record has a disposition, but E is not automatic MC/DC credit for
transitive branches. Standard-library iterator/Option/Result internals, allocator
implementation, native write/sync/lock semantics and actual hardware durability
remain delegated. The expected envelope checks are explicitly internal boundary
controls, not proof that every malicious custom backend is modeled. No production
refactor, new fault API, or compiler-measured coverage percentage was added.

## Source records

Snapshot SHA-256: `d6e716b2e02dd1f52d32d0d7fb760011fcc2fb0d7d0a657d895e642f4c4674cc`.
Production record identities below are line:column plus the first 12 hex digits
of the inventory source-token hash; test-only additions are excluded by the tool.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 122:21 | `try_propagation` / `space` | E | K1 | `8a8de823d5ed` |
| 123:51 | `try_propagation` / `space` | E | K1 | `8a8de823d5ed` |
| 124:9 | `if` / `space` | C | K1 | `935f68319d4f` |
| 124:12 | `short_circuit_or` / `space` | P | K1 | `9fddf171fb65` |
| 128:62 | `try_propagation` / `space` | I | K1 | `8a8de823d5ed` |
| 154:21 | `try_propagation` / `compact` | E | K1 | `8a8de823d5ed` |
| 155:49 | `try_propagation` / `compact` | E | K1 | `8a8de823d5ed` |
| 156:9 | `if` / `compact` | C | K1 | `935f68319d4f` |
| 156:12 | `short_circuit_or` / `compact` | P | K1 | `278473a23bca` |
| 168:10 | `try_propagation` / `compact` | E | K4 | `8a8de823d5ed` |
| 169:25 | `try_propagation` / `compact` | E | K4 | `8a8de823d5ed` |
| 177:63 | `try_propagation` / `verify` | E | K2 | `8a8de823d5ed` |
| 178:9 | `if` / `verify` | C | K2 | `935f68319d4f` |
| 178:12 | `short_circuit_or` / `verify` | C | K2 | `f39b51a480c0` |
| 178:12 | `short_circuit_or` / `verify` | C | K2 | `9e2c7c20249b` |
| 178:12 | `short_circuit_or` / `verify` | P | K2 | `c6fe81c9cb2b` |
| 188:65 | `try_propagation` / `locked` | E | K3 | `8a8de823d5ed` |
| 189:23 | `try_propagation` / `locked` | E | K3 | `8a8de823d5ed` |
| 208:53 | `try_propagation` / `create` | E | K3 | `8a8de823d5ed` |
| 209:9 | `if` / `create` | P | K3 | `935f68319d4f` |
| 212:9 | `if` / `create` | P | K3 | `935f68319d4f` |
| 212:36 | `try_propagation` / `create` | E | K3 | `8a8de823d5ed` |
| 226:10 | `try_propagation` / `create` | E | K4 | `8a8de823d5ed` |
| 228:53 | `try_propagation` / `create` | E | K4 | `8a8de823d5ed` |
| 230:9 | `for_iteration` / `create` | E | K4 | `10c22bcf4c76` |
| 231:73 | `try_propagation` / `create` | E | K4 | `8a8de823d5ed` |
| 233:50 | `try_propagation` / `create` | E | K4 | `8a8de823d5ed` |
| 240:53 | `try_propagation` / `open` | E | K4 | `8a8de823d5ed` |
| 241:67 | `try_propagation` / `open` | E | K4 | `8a8de823d5ed` |
| 245:54 | `try_propagation` / `open` | E | K4 | `8a8de823d5ed` |
| 252:32 | `try_propagation` / `open` | E | K4 | `8a8de823d5ed` |
| 269:9 | `if` / `guard` | P | K1 | `935f68319d4f` |
| 282:21 | `try_propagation` / `persist` | E | K1 | `8a8de823d5ed` |
| 283:9 | `if` / `persist` | P | K4 | `935f68319d4f` |
| 286:22 | `match` / `persist` | E | K1 | `4945a70fa7f9` |
| 293:9 | `if` / `persist` | C | K1 | `935f68319d4f` |
| 293:12 | `short_circuit_or` / `persist` | P | K1 | `3840c9281a7d` |
| 297:41 | `try_propagation` / `persist` | I | K1 | `8a8de823d5ed` |
| 307:10 | `try_propagation` / `persist` | E | K4 | `8a8de823d5ed` |
| 322:51 | `try_propagation` / `publish` | E | K4 | `8a8de823d5ed` |
| 333:47 | `try_propagation` / `publish` | E | K4 | `8a8de823d5ed` |
| 342:38 | `try_propagation` / `write_synced` | E | K5 | `8a8de823d5ed` |
| 350:5 | `while` / `read_all` | E | K5 | `07a875073882` |
| 352:55 | `try_propagation` / `read_all` | E | K5 | `8a8de823d5ed` |
| 353:9 | `if` / `read_all` | C | K5 | `935f68319d4f` |
| 353:12 | `short_circuit_or` / `read_all` | P | K5 | `536dce618ae9` |
| 356:68 | `try_propagation` / `read_all` | E | K5 | `8a8de823d5ed` |
| 366:5 | `while` / `write_all` | E | K5 | `07a875073882` |
| 368:54 | `try_propagation` / `write_all` | E | K5 | `8a8de823d5ed` |
| 369:9 | `if` / `write_all` | C | K5 | `935f68319d4f` |
| 369:12 | `short_circuit_or` / `write_all` | P | K5 | `536dce618ae9` |
| 372:66 | `try_propagation` / `write_all` | E | K5 | `8a8de823d5ed` |
| 389:49 | `try_propagation` / `recover_replica` | E | K2 | `8a8de823d5ed` |
| 392:5 | `if` / `recover_replica` | C | K2 | `935f68319d4f` |
| 392:8 | `short_circuit_or` / `recover_replica` | C | K2 | `0f002c6cf44a` |
| 392:8 | `short_circuit_or` / `recover_replica` | C | K2 | `40c0d12e0185` |
| 392:8 | `short_circuit_or` / `recover_replica` | P | K2 | `618a07bbe38b` |

Disposition totals: C=11, E=33, I=2, P=11; **57 records**.

## Executed validation

On the local macOS host, all-feature debug and release runs passed: 38 storage
unit tests, 8 `storage_decisions`, 3 `bytestore`, 14 `durability`, 3 `headroom`,
and 10 `compaction` tests (76 tests per profile). Strict Clippy for the library
and new integration target passed. The source inventory completed with zero
parse failures. This run does not add Windows, Linux, 32-bit, or physical
power-loss evidence; it does not claim mutation or compiler MC/DC measurement.
