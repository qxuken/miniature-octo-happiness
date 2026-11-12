# CRDT source decision audit

This audit classifies all 121 explicit production AST records across `src/crdt.rs`
and its three container modules. It reuses convergence, monotone-edit,
codec-round-trip and lost-reply tests and adds eight directed private tests. The register retirement follow-up also changes
production decoding/admission and clarifies compatible-history merge semantics.

P means directed predicate controls, E execution/dispatch evidence (not all
residuals), C overlapping AST syntax covered by the parent decision, D delegated
primitive/callback/I/O propagation, and G an explicit remaining directed gap.
Counts include overlapping syntax and are not a measured MC/DC percentage.

## C1: enrollment, size and frame validation

`policy_and_frame_independent_guards` toggles each policy OR term against the
valid baseline: zero collection, zero generation, zero first actor and an
unordered roster; equal actors additionally reject. Collection mismatch is
ForeignStore, whereas generation mismatch is Conflict. It controls short frame,
wrong marker, invalid decoded policy, actor index 2, valid actor 0 and both
header/body truncation. `replica_size_actor_enrollment_and_local_row_guards`
checks SIZE 4096/4097, unknown actor, missing row, duplicate enrollment, reopening
under the wrong actor, nonbytes row, same-actor peer and callback rejection
without a commit. Existing lossy-application-codec regression controls encode
round-trip equality. Full payload byte validity is delegated to Record::decode.

## C2: container bounds and causal register guards

`container_bounds_and_exhaustion_preserve_state` covers zero increment, exact
counter component maximum, one-over and checked-add overflow, invalid decoded
component, codec length mismatch, set IDs 63/64, repeated insertion, absent and
present removal, add-sequence overflow, removed>added and future local actor.
Exhausted mutations assert the entire previous state survives unchanged.

`register_decode_and_same_dot_predicate_controls` covers absent/zero payload,
absent/nonzero rejection, live/nonzero sequence, live/zero sequence rejection,
unknown tag, codec length and sequence exhaustion. For the same-dot conflict
AND, the rejecting baseline has equal sequence, two live values and unequal
payloads. Removing liveness no longer necessarily accepts: newly learned retirement must
also satisfy the independent causality guard below. Equal payloads accept; unequal sequences
on the remote actor accept while retaining two live unequal payloads. A future
local sequence rejects separately. The local actor ownership guard prevents
using a future local sequence as a valid independence control. Join exercises
lower/equal/higher sequence, retired and live states, and value deduplication.
Empty resolve returns None; two distinct choices call the policy.

### C2 follow-up: retirement requires an observed assignment

The old decoder accepted nonzero assignment sequences with no visible values.
The new regression failed before the fix (`[1,None;0,None]` decoded successfully).
There is no clear operation: assignment always creates a new visible dot, and a
finite union of compatible assignment histories always has a maximal event.
Decode therefore rejects an all-retired noninitial state. Default empty state,
live value zero, and a retired component alongside a live component remain valid.

Nonempty decoding alone was insufficient. Local `[A1=4,B1=5]` previously admitted
`[A1=None,B1=5]`, retiring a visible choice without any new assignment. Admission
now requires a newly learned retirement to have a higher opposite-actor sequence
than the local state. New retirement means an incoming absent value with either
a higher component sequence or the same sequence as a currently visible value.
If the witnessing opposite assignment had already been observed locally, its
retirement would already be present: merging cannot resurrect a retired dot.
The existing local-actor progress and equal-dot unequal-value guards remain.
This is a necessary observable causality check, not cryptographic proof of history
or a Byzantine-peer protocol. Authenticated enrollment/single writers remain
preconditions.

`register_rejects_empty_history_and_unwitnessed_retirements` checks initial empty,
nonzero first/second/both sequences with no value, both same-sequence retirement
directions, a higher-sequence tombstone, an older absent component, unchanged
state and a legitimate higher opposite-actor assignment. `register_forged_retirement_frames_never_publish_or_acknowledge`
feeds exact protocol frames for empty erasure, nonempty same-dot erasure,
higher-sequence tombstone and incompatible cyclic histories. Every rejection
preserves the database revision and row and sends no acknowledgment; the valid
new-assignment control commits once and replies. Two individually valid states
that assert A1 followed B1 and B1 followed A1 are incompatible, and now reject
before merging. Raw `join` remains infallible with its existing compatible-enrolled-
history precondition; Record's documentation now makes that domain explicit.

`register_causal_event_oracle_accepts_bounded_honest_schedules` enumerates all
5,461 prefixes through six operations chosen from assignment by either actor or
directional sync to either actor. The independent model records full event sets
and the causal observations of each assignment; it does not calculate expected
choices by production join. Every prefix checks both peer admission directions,
local-edit admission, visible maximal events, and encode/decode validity. This
covers genuine offline choices and their retirement without permitting artificial
cyclic histories. It is bounded evidence, not an unbounded proof of the laws.

## C3: publication and resolution

Existing tests `generic_edits_preserve_existing_state_and_the_other_actors_component`,
`local_edit_racing_an_incoming_merge_conflicts_and_can_retry`,
`a_stale_actor_copy_cannot_claim_the_peers_acknowledgment`,
`sync_resolves_both_fields_in_one_commit_and_repeat_sync_is_read_only` and
`peers_commit_the_same_policy_result_offline_and_converge_on_one_goal` check
ownership, coverage of offered state, conflicting drafts, conditional merge
publication and automatic policy edits. `a_lost_merge_reply_is_recoverable_and_a_retry_needs_no_commit`
reopens the published merge before retry. The new register_causality fuzz model
uses independent causal-event sets, not production join, for manual/Max/Min
fields, exact revisions and acknowledgment bytes. It includes concurrent
equal-valued dots, delayed retired offers, lost replies and durable reopen.
Two live dots with the same value need no further policy assignment.

This audit does not count those model executions as independent-condition
coverage at every callback or error propagation site. A malicious application
Record may violate the required semilattice/codec contract; verifying arbitrary
application algebra is outside these built-in component tests. Generic codec
and resolution callback errors, allocator failures at every wrapper callsite,
and admission-error propagation at every CRDT entrypoint remain delegated.
`unhealthy_admission_and_exhausted_policy_resolution` controls healthy/unhealthy
admission directly and rejects a callback before execution after failed I/O.
It also checks that policy resolution at sequence exhaustion preserves state.

## Inventory and limits

The AST scanner excludes cfg(test), includes possible platform/feature variants,
and does not expand derives or macro token bodies. Derive-generated Record and
Resolution code, iterator adaptor internals (`any`, `filter`, `max`, `zip`),
checked arithmetic and Result/Option combinators are not independent records
unless their explicit syntax is visited. Their listed examples are execution
oracles, not a claim that the scanner sees all branches. Fixed-array loops
execute two or 128 entries; zero total iterations is infeasible for these
arrays, while loop termination still executes. Slice conversions after exact
length guards cannot fail. Actor indexes are private and created only from the
two-entry validated roster; no fabricated invalid Actor is used as coverage.

| Source | SHA-256 |
|---|---|
| `src/crdt.rs` | `314b94c63278801a91ee1cfd8f2f387a6c1452cc46c3ce5bbf380a3a6735bf45` |
| `src/crdt/add_wins_set.rs` | `f85d1760911aa28a43b1b6b09f365b4debcdda02d399c8023997277ae22d8085` |
| `src/crdt/multi_value_register.rs` | `e0df3ce6ac2d80038a8d7ba4b8ca31178f54a924cb11fc9829bdaaef06177380` |
| `src/crdt/policy_register.rs` | `ed9cc73268c0160fce60a7110eafed28a2baa740a6376fb06a1a4bf210090b30` |

Columns identify exact one-based line:column spans and byte offsets (end exclusive),
so overlapping records remain individually accountable. D means the error
channel remains delegated, even where successful execution is covered.

Classification totals: C=11, D=63, E=14, P=33.

| Status | Source span | Bytes | AST kind | Owner | Evidence/scope |
|---|---|---|---|---|---|
| D | src/crdt.rs:317:67–317:68 | 10037..10038 | try_propagation | read | C1 |
| P | src/crdt.rs:318:9–318:11 | 10048..10050 | if | read | C1 |
| D | src/crdt.rs:321:67–321:68 | 10204..10205 | try_propagation | read | C1 |
| D | src/crdt.rs:327:56–327:57 | 10374..10375 | try_propagation | write | C1 |
| D | src/crdt.rs:349:70–349:71 | 11014..11015 | try_propagation | enroll | C1 |
| P | src/crdt.rs:351:9–351:11 | 11058..11060 | if | enroll | C1 |
| D | src/crdt.rs:351:38–351:39 | 11087..11088 | try_propagation | enroll | C1 |
| D | src/crdt.rs:359:57–359:58 | 11341..11342 | try_propagation | enroll | C1 |
| D | src/crdt.rs:359:59–359:60 | 11343..11344 | try_propagation | enroll | C1 |
| D | src/crdt.rs:360:20–360:21 | 11365..11366 | try_propagation | enroll | C1 |
| D | src/crdt.rs:370:70–370:71 | 11615..11616 | try_propagation | open | C1 |
| D | src/crdt.rs:371:23–371:24 | 11640..11641 | try_propagation | open | C1 |
| D | src/crdt.rs:381:26–381:27 | 11848..11849 | try_propagation | configured | C1 |
| P | src/crdt.rs:382:9–382:11 | 11859..11861 | if | configured | C1 |
| D | src/crdt.rs:389:40–389:41 | 12081..12082 | try_propagation | configured | C1 |
| D | src/crdt.rs:404:35–404:36 | 12434..12435 | try_propagation | read_at | C1 |
| D | src/crdt.rs:408:63–408:64 | 12584..12585 | try_propagation | state_at | C1 |
| D | src/crdt.rs:412:49–412:50 | 12712..12713 | try_propagation | local_state | C1 |
| D | src/crdt.rs:413:84–413:85 | 12798..12799 | try_propagation | local_state | C1 |
| D | src/crdt.rs:413:86–413:87 | 12800..12801 | try_propagation | local_state | C1 |
| D | src/crdt.rs:414:42–414:43 | 12844..12845 | try_propagation | local_state | C1 |
| P | src/crdt.rs:415:9–415:11 | 12855..12857 | if | local_state | C1 |
| D | src/crdt.rs:422:56–422:57 | 13077..13078 | try_propagation | value | C1 |
| D | src/crdt.rs:426:41–426:42 | 13201..13202 | try_propagation | check_peer | C1 |
| P | src/crdt.rs:427:9–427:11 | 13212..13214 | if | check_peer | C1 |
| P | src/crdt.rs:434:9–434:11 | 13417..13419 | if | require_healthy | C3 |
| D | src/crdt.rs:443:31–443:32 | 13799..13800 | try_propagation | update | C3 |
| D | src/crdt.rs:445:73–445:74 | 13912..13913 | try_propagation | update | C3 |
| D | src/crdt.rs:445:75–445:76 | 13914..13915 | try_propagation | update | C3 |
| D | src/crdt.rs:447:45–447:46 | 13999..14000 | try_propagation | update | C3 |
| D | src/crdt.rs:448:55–448:56 | 14056..14057 | try_propagation | update | C3 |
| D | src/crdt.rs:449:59–449:60 | 14117..14118 | try_propagation | update | C3 |
| D | src/crdt.rs:449:61–449:62 | 14119..14120 | try_propagation | update | C3 |
| D | src/crdt.rs:454:56–454:57 | 14275..14276 | try_propagation | check_local_edit | C3 |
| P | src/crdt.rs:461:9–461:11 | 14463..14465 | if | check_covers | C3 |
| P | src/crdt.rs:53:9–53:11 | 1461..1463 | if | validate | C1 |
| C | src/crdt.rs:53:12–54:36 | 1464..1526 | short_circuit_or | validate | C1 |
| C | src/crdt.rs:53:12–55:41 | 1464..1567 | short_circuit_or | validate | C1 |
| C | src/crdt.rs:53:12–56:48 | 1464..1615 | short_circuit_or | validate | C1 |
| D | src/crdt.rs:470:31–470:32 | 14831..14832 | try_propagation | push_authenticated | C3 |
| D | src/crdt.rs:471:57–471:58 | 14890..14891 | try_propagation | push_authenticated | C3 |
| D | src/crdt.rs:472:36–472:37 | 14928..14929 | try_propagation | push_authenticated | C3 |
| D | src/crdt.rs:473:45–473:46 | 14975..14976 | try_propagation | push_authenticated | C3 |
| D | src/crdt.rs:474:65–474:66 | 15042..15043 | try_propagation | push_authenticated | C3 |
| D | src/crdt.rs:474:67–474:68 | 15044..15045 | try_propagation | push_authenticated | C3 |
| D | src/crdt.rs:475:58–475:59 | 15104..15105 | try_propagation | push_authenticated | C3 |
| D | src/crdt.rs:485:45–485:46 | 15478..15479 | try_propagation | serve_authenticated | C3 |
| D | src/crdt.rs:486:31–486:32 | 15511..15512 | try_propagation | serve_authenticated | C3 |
| D | src/crdt.rs:488:73–488:74 | 15624..15625 | try_propagation | serve_authenticated | C3 |
| D | src/crdt.rs:488:75–488:76 | 15626..15627 | try_propagation | serve_authenticated | C3 |
| D | src/crdt.rs:489:37–489:38 | 15665..15666 | try_propagation | serve_authenticated | C3 |
| P | src/crdt.rs:492:9–492:11 | 15757..15759 | if | serve_authenticated | C3 |
| D | src/crdt.rs:494:56–494:57 | 15883..15884 | try_propagation | serve_authenticated | C3 |
| P | src/crdt.rs:495:13–495:15 | 15898..15900 | if | serve_authenticated | C3 |
| D | src/crdt.rs:496:61–496:62 | 15987..15988 | try_propagation | serve_authenticated | C3 |
| D | src/crdt.rs:498:63–498:64 | 16066..16067 | try_propagation | serve_authenticated | C3 |
| D | src/crdt.rs:498:65–498:66 | 16068..16069 | try_propagation | serve_authenticated | C3 |
| D | src/crdt.rs:499:24–499:25 | 16094..16095 | try_propagation | serve_authenticated | C3 |
| D | src/crdt.rs:535:36–535:37 | 17162..17163 | try_propagation | count_at | C3 |
| P | src/crdt.rs:64:9–64:11 | 1756..1758 | if | matches | C1 |
| D | src/crdt.rs:548:54–548:55 | 17784..17785 | try_propagation | push_authenticated | C3 |
| P | src/crdt.rs:67:9–67:11 | 1860..1862 | if | matches | C1 |
| E | src/crdt.rs:86:9–86:12 | 2268..2271 | for_iteration | join | C1 |
| P | src/crdt.rs:92:9–92:11 | 2471..2473 | if | increment | C1 |
| D | src/crdt.rs:98:33–98:34 | 2690..2691 | try_propagation | increment | C1 |
| P | src/crdt.rs:114:9–114:11 | 3194..3196 | if | from_le_bytes | C1 |
| P | src/crdt.rs:122:9–122:11 | 3501..3503 | if | check_peer | C1 |
| D | src/crdt.rs:137:71–137:72 | 3965..3966 | try_propagation | insert | C1 |
| D | src/crdt.rs:211:87–211:88 | 6759..6760 | try_propagation | encode | C1 |
| D | src/crdt.rs:218:57–218:58 | 6955..6956 | try_propagation | decode | C1 |
| D | src/crdt.rs:237:87–237:88 | 7460..7461 | try_propagation | encode | C1 |
| D | src/crdt.rs:243:73–243:74 | 7640..7641 | try_propagation | decode | C1 |
| D | src/crdt.rs:279:50–279:51 | 8776..8777 | try_propagation | encode | C1 |
| P | src/crdt.rs:280:9–280:11 | 8787..8789 | if | encode | C1 |
| D | src/crdt.rs:280:39–280:40 | 8817..8818 | try_propagation | encode | C1 |
| P | src/crdt.rs:287:9–287:11 | 8966..8968 | if | decode | C1 |
| P | src/crdt.rs:290:9–290:11 | 9061..9063 | if | decode | C1 |
| D | src/crdt.rs:301:54–301:55 | 9521..9522 | try_propagation | decode | C1 |
| P | src/crdt.rs:303:9–303:11 | 9576..9578 | if | decode | C1 |
| D | src/crdt.rs:306:50–306:51 | 9709..9710 | try_propagation | decode | C1 |
| D | src/crdt/add_wins_set.rs:34:71–34:72 | 1080..1081 | try_propagation | insert | C2 |
| D | src/crdt/add_wins_set.rs:40:73–40:74 | 1316..1317 | try_propagation | remove | C2 |
| E | src/crdt/add_wins_set.rs:42:9–42:12 | 1371..1374 | for_iteration | remove | C2 |
| E | src/crdt/add_wins_set.rs:61:9–61:12 | 1862..1865 | for_iteration | join | C2 |
| P | src/crdt/add_wins_set.rs:79:9–79:11 | 2390..2392 | if | encode | C2 |
| E | src/crdt/add_wins_set.rs:83:9–83:12 | 2540..2543 | for_iteration | encode | C2 |
| P | src/crdt/add_wins_set.rs:91:9–91:11 | 2820..2822 | if | decode | C2 |
| E | src/crdt/add_wins_set.rs:96:9–96:12 | 3006..3009 | for_iteration | decode | C2 |
| P | src/crdt/add_wins_set.rs:99:13–99:15 | 3233..3235 | if | decode | C2 |
| P | src/crdt/add_wins_set.rs:113:9–113:11 | 3587..3589 | if | check_peer | C2 |
| D | src/crdt/add_wins_set.rs:32:73–32:74 | 961..962 | try_propagation | insert | C2 |
| D | src/crdt/multi_value_register.rs:50:34–50:35 | 1689..1690 | try_propagation | resolve_with | C2 |
| E | src/crdt/multi_value_register.rs:51:14–51:19 | 1705..1710 | match | resolve_with | C2 |
| E | src/crdt/multi_value_register.rs:60:9–60:12 | 2015..2018 | for_iteration | join | C2 |
| E | src/crdt/multi_value_register.rs:61:13–61:18 | 2085..2090 | match | join | C2 |
| P | src/crdt/multi_value_register.rs:81:9–81:11 | 2875..2877 | if | encode | C2 |
| E | src/crdt/multi_value_register.rs:85:9–85:12 | 3028..3031 | for_iteration | encode | C2 |
| P | src/crdt/multi_value_register.rs:94:9–94:11 | 3385..3387 | if | decode | C2 |
| E | src/crdt/multi_value_register.rs:99:9–99:12 | 3576..3579 | for_iteration | decode | C2 |
| E | src/crdt/multi_value_register.rs:102:25–102:30 | 3811..3816 | match | decode | C2 |
| P | src/crdt/multi_value_register.rs:103:22–103:32 | 3849..3859 | match_guard | decode | C2 |
| P | src/crdt/multi_value_register.rs:104:22–104:35 | 3890..3903 | match_guard | decode | C2 |
| P | src/crdt/multi_value_register.rs:112:9–112:11 | 4287..4289 | if | decode | C2 |
| C | src/crdt/multi_value_register.rs:112:12–113:63 | 4290..4402 | short_circuit_and | decode | C2 |
| P | src/crdt/multi_value_register.rs:126:9–126:11 | 4685..4687 | if | check_peer | C2 |
| E | src/crdt/multi_value_register.rs:129:9–129:12 | 4801..4804 | for_iteration | check_peer | C2 |
| P | src/crdt/multi_value_register.rs:133:34–135:85 | 5152..5312 | short_circuit_and | check_peer | C2 |
| C | src/crdt/multi_value_register.rs:134:21–135:84 | 5195..5311 | short_circuit_or | check_peer | C2 |
| C | src/crdt/multi_value_register.rs:135:25–135:83 | 5252..5310 | short_circuit_and | check_peer | C2 |
| P | src/crdt/multi_value_register.rs:136:13–136:15 | 5326..5328 | if | check_peer | C2 |
| C | src/crdt/multi_value_register.rs:136:16–136:94 | 5329..5407 | short_circuit_and | check_peer | C2 |
| P | src/crdt/multi_value_register.rs:139:13–139:15 | 5481..5483 | if | check_peer | C2 |
| C | src/crdt/multi_value_register.rs:139:16–140:72 | 5484..5589 | short_circuit_and | check_peer | C2 |
| C | src/crdt/multi_value_register.rs:139:16–141:26 | 5484..5615 | short_circuit_and | check_peer | C2 |
| C | src/crdt/multi_value_register.rs:140:20–140:23 | 5537..5540 | pattern_test | check_peer | C2 |
| D | src/crdt/multi_value_register.rs:26:37–26:38 | 822..823 | try_propagation | assign | C2 |
| E | src/crdt/multi_value_register.rs:27:9–27:12 | 833..836 | for_iteration | assign | C2 |
| P | src/crdt/policy_register.rs:74:9–74:11 | 2387..2389 | if | resolve | C2 |
| C | src/crdt/policy_register.rs:74:12–74:15 | 2390..2393 | pattern_test | resolve | C2 |
| E | src/crdt/policy_register.rs:75:57–75:58 | 2515..2516 | try_propagation | resolve | C2 |
| D | src/crdt/policy_register.rs:96:56–96:57 | 3060..3061 | try_propagation | decode | C2 |

## Validation

On arm64 macOS, `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/register-fuzz`
with `cargo test --offline --locked -p weekend-db --all-features --lib
--test crdt --test replication` passed 105 library, 11 CRDT integration and
seven replication integration tests in both debug and release before the final
two directed replication additions. After those additions, all 14 scoped
replication library tests passed in debug/release and all-target warnings-denied
Clippy passed. This batch adds five CRDT and eight replication tests. That earlier batch did not change runtime/unsafe code. The retirement follow-up
changes the safe register decoder and admission guard as described above.
No mutation campaign or compiler-measured MC/DC run was performed for this
tranche. Source hashes above bind the explicit AST classification. Logs are
`/tmp/crdt-repl-final-{debug,release}.log` and the final scoped
`/tmp/replication-gap-{debug,release,clippy}.log`.

### Focused Miri follow-up

The four tests `register_causal_event_oracle_accepts_bounded_honest_schedules`,
`register_decode_and_same_dot_predicate_controls`,
`register_forged_retirement_frames_never_publish_or_acknowledge`, and
`register_rejects_empty_history_and_unwitnessed_retirements` passed Miri together
(212.52 seconds). This covers those bounded guard/model/frame paths, not all
application Record implementations or all CRDT schedules.
