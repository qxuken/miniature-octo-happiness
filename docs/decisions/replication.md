# Snapshot replication source decision audit

This audit classifies all 140 explicit production AST records across
`src/replication/{mod,wire,follower}.rs`. It adds eight directed tests, reuses
primary progress controls, follower simulator barriers and native lifecycle
regressions. No runtime publication or acknowledgment behavior changes.

P means directed predicate controls, E execution/dispatch evidence (not all
residuals), C overlapping syntax, D delegated I/O/allocation/storage propagation,
and G a remaining directed case. Counts are syntax records, not measured MC/DC.

## R1: sender and wire admission

`enrollment_offer_and_wire_dispatch_boundaries` independently toggles zero stream
and zero file identity, checks exact image-length endpoints and neighbors,
wrong wire marker, each identity component, invalid command, truncated header,
truncated installed reply and all status/error mappings. An exhausted error
maps to Limit and uncertain publication maps to RecoveryRequired; they are not
serialized as successful durable receipts. `source_and_export_identity_revision_guards_are_independent`
isolates source identity vs source rollback and foreign export enrollment vs
export ahead of the verified head, asserting no bytes are sent on rejection.
The accepted control uses the exact export acknowledgment.

Existing `primary_poll_rejects_invalid_progress_without_losing_its_last_ack`
controls wrong stream, beyond-head revision, backward revision, equal revision
with different digest, exact acknowledgment, newer acknowledgment and Empty
before/after progress. The inner same-revision AND uses equal/different digest
and newer/different digest controls while older-revision rejection is held false.
`primary_ship_accepts_newer_progress_only_before_uploading` distinguishes immediate
newer/exact receipts from the post-upload exact-only receipt. These checks do
not equate a local database commit with follower durability.

## R2: selected image and recovery barriers

`installed_record_length_slot_and_offer_guards` controls wrong length vs invalid
slot independently, accepts slots 0/1 and maps invalid embedded length to Corrupt.
Existing `transport_rejects_bad_images_and_envelopes_without_advancing`,
`image_verification_failures_preserve_the_reported_revision` and
`lost_ack_is_recovered_and_a_delayed_offer_never_rolls_back_progress` cover newer,
older, exact and conflicting-same-revision offers, verification and lost replies.
Digest, identity, revision, committed end and witness failures leave the old
manifest authoritative even after the candidate image is fully durable.

`interrupted_image_and_manifest_barriers_recover_exactly_one_image` uses old
source revision 2/value1 and new revision 3/value2. Its exact reopened oracle is:

| Failed barrier | Drop pending writes | Persist pending writes |
|---|---|---|
| Candidate image sync | revision 2 | revision 2 |
| Manifest append sync | revision 2 | revision 2 |
| Manifest root sync | revision 2 | revision 3 |
| Manifest witness sync | revision 3 | revision 3 |

Candidate failure leaves old progress readable. Any manifest publication failure
requires recovery before reporting durable progress. The final old or new
manifest determines the recovered receipt; retry either installs revision3 or
returns the already selected revision3 without reinstalling. Invalid candidate
verification reopens revision2; successful install/lost terminal reply reopens
revision3. The unselected slot is never a fallback authority.

## R3: native lifecycle and delegated failure modes

Existing integration tests cover separate local/follower acknowledgment over
TCP, process interruption during upload/before reply, exact manifest reserved
headroom and one-over admission, compaction/reseed, inactive-file cleanup and
interrupted manifest checkpoint. `cleanup_only_removes_reserved_unselected_regular_files`
checks selected, inactive, missing and nonregular names and unrelated sentinels.
All filesystem error mapping alternatives (permissions, enumeration entry
errors, metadata/removal failures) are not injected here. New `manifest_enrollment_and_missing_selected_image_reject_without_fallback`
covers missing collection/row, short/nonbytes manifest value, each wrong
enrollment component and a missing selected image.
`nonempty_candidate_rejects_before_ready_and_preserves_old_revision` checks
the staging-store contract before Ready and reopens revision2.
`visit_image_length_and_callback_errors_stop_before_more_chunks` controls
length mismatch, empty input and first-chunk callback rejection. `manifest_and_image_open_preserve_non_not_found_errors` uses a manifest with
max_bytes=0 to produce Limit at lookup and a scripted Images adapter returning
Busy. These control non-NotFound propagation; the latter is not an OS lock test.
`native_admission_and_defensive_status_decisions` exercises real file/nonempty
directory admission with an unchanged sentinel. Its status check injects a
private selected-view fixture and replaces the manifest with a simulator-backed
database poisoned by an actual I/O failure. It checks suppression of durable
acknowledgment while retaining the read view. This is defensive status formatting
evidence, not a native image-installation or syscall-failure experiment.

The source image length/digest pass delegates its read-all behavior and
allocation failures to storage tests; it does not make arbitrary external
ByteStore implementations correct. Offer length bounds guarantee positive
bounded streaming progress; fixed slices are converted only after exact reads.
Manifest decode checks length before indexing byte0. A successfully created
native image is empty; nonempty is reachable for a contract-violating custom
Images implementation and is not counted as infeasible for the generic code.
Lease failure/release semantics delegate to native/simulator locking suites.

## Inventory and limits

The scanner includes possible platform branches, excludes cfg(test) and does not
expand raw macro bodies or derives. Standard combinators, OS calls, digest
internals and delegated storage decisions are outside this explicit-syntax
count. Native Windows execution is separate CI evidence; this local audit runs
on arm64 macOS. No compiler-measured percentage or exhaustive transport/device
fault claim follows from this classification.

| Source | SHA-256 |
|---|---|
| `src/replication/follower.rs` | `7b1566228ed57f5b41d15be8a583416a14f8dcefae1521a5ba1764413e37e035` |
| `src/replication/mod.rs` | `64891dfc7a6413b042522fa58452d244a048f8037e888f03a3586c5230254cc7` |
| `src/replication/wire.rs` | `5cf5678754772352d27f094f0920607bf07db9e39dbf56a7f44535b7acf3c693` |

Columns identify exact one-based line:column spans and byte offsets (end exclusive),
so overlapping records remain individually accountable. D means the error
channel remains delegated, even where successful execution is covered.

Classification totals: C=15, D=77, E=15, P=33.

| Status | Source span | Bytes | AST kind | Owner | Evidence/scope |
|---|---|---|---|---|---|
| D | src/replication/follower.rs:39:21–39:22 | 1273..1274 | try_propagation | new | R2 |
| P | src/replication/follower.rs:58:9–58:11 | 1597..1599 | if | decode | R2 |
| C | src/replication/follower.rs:58:12–58:45 | 1600..1633 | short_circuit_or | decode | R2 |
| D | src/replication/follower.rs:66:53–66:54 | 1979..1980 | try_propagation | decode | R2 |
| D | src/replication/follower.rs:88:37–88:38 | 2446..2447 | try_propagation | create | R2 |
| D | src/replication/follower.rs:96:30–96:31 | 2642..2643 | try_propagation | create | R2 |
| P | src/replication/follower.rs:110:21–110:23 | 3025..3027 | if | open | R2 |
| D | src/replication/follower.rs:115:19–115:20 | 3185..3186 | try_propagation | open | R2 |
| D | src/replication/follower.rs:116:39–116:40 | 3225..3226 | try_propagation | open | R2 |
| D | src/replication/follower.rs:117:70–117:71 | 3297..3298 | try_propagation | open | R2 |
| D | src/replication/follower.rs:118:82–118:83 | 3381..3382 | try_propagation | open | R2 |
| P | src/replication/follower.rs:119:13–119:15 | 3396..3398 | if | open | R2 |
| C | src/replication/follower.rs:119:16–119:92 | 3399..3475 | short_circuit_or | open | R2 |
| P | src/replication/follower.rs:122:13–122:15 | 3553..3555 | if | open | R2 |
| D | src/replication/follower.rs:125:57–125:58 | 3674..3675 | try_propagation | open | R2 |
| P | src/replication/follower.rs:127:21–127:23 | 3781..3783 | if | open | R2 |
| D | src/replication/follower.rs:132:19–132:20 | 3941..3942 | try_propagation | open | R2 |
| D | src/replication/follower.rs:132:21–132:22 | 3943..3944 | try_propagation | open | R2 |
| D | src/replication/follower.rs:134:91–134:92 | 4066..4067 | try_propagation | open | R2 |
| P | src/replication/follower.rs:150:19–150:21 | 4541..4543 | if | select | R2 |
| C | src/replication/follower.rs:150:22–150:25 | 4544..4547 | pattern_test | select | R2 |
| D | src/replication/follower.rs:159:55–159:56 | 4946..4947 | try_propagation | select | R2 |
| D | src/replication/follower.rs:161:69–161:70 | 5061..5062 | try_propagation | select | R2 |
| D | src/replication/follower.rs:162:20–162:21 | 5083..5084 | try_propagation | select | R2 |
| P | src/replication/follower.rs:171:9–171:11 | 5270..5272 | if | verify | R2 |
| D | src/replication/follower.rs:171:45–171:46 | 5306..5307 | try_propagation | verify | R2 |
| P | src/replication/follower.rs:184:9–184:11 | 5642..5644 | if | progress | R2 |
| D | src/replication/follower.rs:193:39–193:40 | 5987..5988 | try_propagation | handle | R2 |
| E | src/replication/follower.rs:194:9–194:12 | 5998..6001 | let_else | handle | R2 |
| D | src/replication/follower.rs:194:71–194:72 | 6060..6061 | try_propagation | handle | R2 |
| P | src/replication/follower.rs:197:9–197:11 | 6172..6174 | if | handle | R2 |
| C | src/replication/follower.rs:197:12–197:15 | 6175..6178 | pattern_test | handle | R2 |
| P | src/replication/follower.rs:198:13–198:15 | 6228..6230 | if | handle | R2 |
| C | src/replication/follower.rs:198:16–198:77 | 6231..6292 | short_circuit_or | handle | R2 |
| P | src/replication/follower.rs:201:13–201:15 | 6406..6408 | if | handle | R2 |
| D | src/replication/follower.rs:209:46–209:47 | 6686..6687 | try_propagation | handle | R2 |
| D | src/replication/follower.rs:211:60–211:61 | 6798..6799 | try_propagation | handle | R2 |
| D | src/replication/follower.rs:211:62–211:63 | 6800..6801 | try_propagation | handle | R2 |
| P | src/replication/follower.rs:212:9–212:11 | 6811..6813 | if | handle | R2 |
| D | src/replication/follower.rs:212:28–212:29 | 6830..6831 | try_propagation | handle | R2 |
| D | src/replication/follower.rs:215:53–215:54 | 6941..6942 | try_propagation | handle | R2 |
| E | src/replication/follower.rs:218:9–218:14 | 7018..7023 | while | handle | R2 |
| D | src/replication/follower.rs:220:61–220:62 | 7200..7201 | try_propagation | handle | R2 |
| D | src/replication/follower.rs:221:51–221:52 | 7253..7254 | try_propagation | handle | R2 |
| D | src/replication/follower.rs:224:23–224:24 | 7330..7331 | try_propagation | handle | R2 |
| D | src/replication/follower.rs:225:94–225:95 | 7426..7427 | try_propagation | handle | R2 |
| D | src/replication/follower.rs:227:36–227:37 | 7485..7486 | try_propagation | handle | R2 |
| D | src/replication/follower.rs:234:15–234:16 | 7797..7798 | try_propagation | serve | R2 |
| D | src/replication/follower.rs:254:61–254:62 | 8309..8310 | try_propagation | new | R3 |
| P | src/replication/follower.rs:255:13–255:15 | 8324..8326 | if | new | R3 |
| P | src/replication/follower.rs:264:25–264:27 | 8607..8609 | if | slot | R3 |
| E | src/replication/follower.rs:272:9–272:14 | 8791..8796 | match | io_error | R3 |
| E | src/replication/follower.rs:281:13–281:18 | 9092..9097 | match | create | R3 |
| E | src/replication/follower.rs:283:27–283:62 | 9178..9213 | match_guard | create | R3 |
| D | src/replication/follower.rs:292:73–292:74 | 9578..9579 | try_propagation | check_manifest_headroom | R3 |
| P | src/replication/follower.rs:293:13–293:15 | 9599..9601 | if | check_manifest_headroom | R3 |
| D | src/replication/follower.rs:328:34–328:35 | 10987..10988 | try_propagation | create | R3 |
| D | src/replication/follower.rs:329:30–329:31 | 11019..11020 | try_propagation | create | R3 |
| D | src/replication/follower.rs:330:60–330:61 | 11081..11082 | try_propagation | create | R3 |
| P | src/replication/follower.rs:331:13–331:15 | 11096..11098 | if | create | R3 |
| D | src/replication/follower.rs:332:35–332:36 | 11157..11158 | try_propagation | create | R3 |
| D | src/replication/follower.rs:335:35–335:36 | 11246..11247 | try_propagation | create | R3 |
| D | src/replication/follower.rs:341:53–341:54 | 11453..11454 | try_propagation | create | R3 |
| D | src/replication/follower.rs:344:14–344:15 | 11542..11543 | try_propagation | create | R3 |
| D | src/replication/follower.rs:355:34–355:35 | 12029..12030 | try_propagation | open | R3 |
| D | src/replication/follower.rs:356:30–356:31 | 12061..12062 | try_propagation | open | R3 |
| D | src/replication/follower.rs:357:60–357:61 | 12123..12124 | try_propagation | open | R3 |
| D | src/replication/follower.rs:358:85–358:86 | 12210..12211 | try_propagation | open | R3 |
| D | src/replication/follower.rs:367:35–367:36 | 12637..12638 | try_propagation | status | R3 |
| P | src/replication/follower.rs:371:30–371:32 | 12771..12773 | if | status | R3 |
| D | src/replication/follower.rs:374:36–374:37 | 12879..12880 | try_propagation | status | R3 |
| D | src/replication/follower.rs:387:28–387:29 | 13565..13566 | try_propagation | cleanup_inactive | R3 |
| P | src/replication/follower.rs:389:13–389:15 | 13665..13667 | if | cleanup_inactive | R3 |
| C | src/replication/follower.rs:389:16–389:19 | 13668..13671 | pattern_test | cleanup_inactive | R3 |
| D | src/replication/follower.rs:390:77–390:78 | 13776..13777 | try_propagation | cleanup_inactive | R3 |
| D | src/replication/follower.rs:390:79–390:80 | 13778..13779 | try_propagation | cleanup_inactive | R3 |
| D | src/replication/follower.rs:396:18–396:19 | 13997..13998 | try_propagation | cleanup_inactive | R3 |
| E | src/replication/follower.rs:399:13–399:16 | 14059..14062 | for_iteration | cleanup_inactive | R3 |
| P | src/replication/follower.rs:400:17–400:19 | 14094..14096 | if | cleanup_inactive | R3 |
| E | src/replication/follower.rs:404:17–404:22 | 14271..14276 | match | cleanup_inactive | R3 |
| E | src/replication/follower.rs:405:35–405:74 | 14341..14380 | match_guard | cleanup_inactive | R3 |
| P | src/replication/follower.rs:407:37–407:68 | 14486..14517 | match_guard | cleanup_inactive | R3 |
| D | src/replication/follower.rs:409:64–409:65 | 14643..14644 | try_propagation | cleanup_inactive | R3 |
| D | src/replication/follower.rs:422:28–422:29 | 15231..15232 | try_propagation | compact_manifest | R3 |
| D | src/replication/follower.rs:428:64–428:65 | 15523..15524 | try_propagation | compact_manifest | R3 |
| P | src/replication/mod.rs:32:9–32:11 | 1180..1182 | if | validate | R1 |
| C | src/replication/mod.rs:32:12–32:70 | 1183..1241 | short_circuit_or | validate | R1 |
| P | src/replication/mod.rs:56:9–56:11 | 1787..1789 | if | validate | R1 |
| D | src/replication/mod.rs:92:75–92:76 | 2871..2872 | try_propagation | new | R1 |
| D | src/replication/mod.rs:97:30–97:31 | 3002..3003 | try_propagation | new | R1 |
| D | src/replication/mod.rs:114:49–114:50 | 3540..3541 | try_propagation | export_to | R1 |
| D | src/replication/mod.rs:125:49–125:50 | 4014..4015 | try_propagation | export_file | R1 |
| D | src/replication/mod.rs:126:69–126:70 | 4085..4086 | try_propagation | export_file | R1 |
| D | src/replication/mod.rs:129:61–129:62 | 4227..4228 | try_propagation | check_source | R1 |
| P | src/replication/mod.rs:130:9–130:11 | 4238..4240 | if | check_source | R1 |
| C | src/replication/mod.rs:130:12–130:90 | 4241..4319 | short_circuit_or | check_source | R1 |
| D | src/replication/mod.rs:141:68–141:69 | 4633..4634 | try_propagation | capture | R1 |
| D | src/replication/mod.rs:143:65–143:66 | 4740..4741 | try_propagation | capture | R1 |
| D | src/replication/mod.rs:149:25–149:26 | 4868..4869 | try_propagation | capture | R1 |
| D | src/replication/mod.rs:161:59–161:60 | 5410..5411 | try_propagation | poll | R1 |
| E | src/replication/mod.rs:162:9–162:14 | 5421..5426 | match | poll | R1 |
| D | src/replication/mod.rs:162:45–162:46 | 5457..5458 | try_propagation | poll | R1 |
| P | src/replication/mod.rs:164:35–164:58 | 5567..5590 | match_guard | poll | R1 |
| P | src/replication/mod.rs:176:9–176:11 | 5997..5999 | if | ship | R1 |
| C | src/replication/mod.rs:176:12–176:91 | 6000..6079 | short_circuit_or | ship | R1 |
| D | src/replication/mod.rs:180:66–180:67 | 6253..6254 | try_propagation | ship | R1 |
| E | src/replication/mod.rs:181:9–181:14 | 6264..6269 | match | ship | R1 |
| D | src/replication/mod.rs:181:45–181:46 | 6300..6301 | try_propagation | ship | R1 |
| P | src/replication/mod.rs:182:44–182:95 | 6347..6398 | match_guard | ship | R1 |
| C | src/replication/mod.rs:182:44–182:95 | 6347..6398 | short_circuit_or | ship | R1 |
| D | src/replication/mod.rs:188:19–188:20 | 6656..6657 | try_propagation | ship | R1 |
| D | src/replication/mod.rs:189:55–189:56 | 6713..6714 | try_propagation | ship | R1 |
| E | src/replication/mod.rs:190:17–190:22 | 6732..6737 | match | ship | R1 |
| D | src/replication/mod.rs:190:53–190:54 | 6768..6769 | try_propagation | ship | R1 |
| P | src/replication/mod.rs:191:52–191:67 | 6823..6838 | match_guard | ship | R1 |
| P | src/replication/mod.rs:199:9–199:11 | 7070..7072 | if | observe | R1 |
| C | src/replication/mod.rs:199:12–199:95 | 7073..7156 | short_circuit_or | observe | R1 |
| P | src/replication/mod.rs:202:9–202:11 | 7222..7224 | if | observe | R1 |
| C | src/replication/mod.rs:203:13–203:94 | 7273..7354 | short_circuit_or | observe | R1 |
| C | src/replication/mod.rs:203:47–203:93 | 7307..7353 | short_circuit_and | observe | R1 |
| P | src/replication/mod.rs:217:5–217:7 | 7615..7617 | if | visit_image | R1 |
| D | src/replication/mod.rs:217:24–217:25 | 7634..7635 | try_propagation | visit_image | R1 |
| E | src/replication/mod.rs:222:5–222:10 | 7752..7757 | while | visit_image | R1 |
| D | src/replication/mod.rs:224:41–224:42 | 7898..7899 | try_propagation | visit_image | R1 |
| D | src/replication/mod.rs:225:21–225:22 | 7921..7922 | try_propagation | visit_image | R1 |
| D | src/replication/mod.rs:236:7–236:8 | 8188..8189 | try_propagation | image_digest | R1 |
| P | src/replication/wire.rs:17:16–17:18 | 515..517 | if | request | R1 |
| C | src/replication/wire.rs:17:19–17:22 | 518..521 | pattern_test | request | R1 |
| D | src/replication/wire.rs:30:37–30:38 | 1029..1030 | try_propagation | read_request | R1 |
| P | src/replication/wire.rs:31:5–31:7 | 1036..1038 | if | read_request | R1 |
| P | src/replication/wire.rs:34:5–34:7 | 1121..1123 | if | read_request | R1 |
| C | src/replication/wire.rs:34:8–34:85 | 1124..1201 | short_circuit_or | read_request | R1 |
| E | src/replication/wire.rs:37:5–37:10 | 1255..1260 | match | read_request | R1 |
| D | src/replication/wire.rs:40:44–40:45 | 1354..1355 | try_propagation | read_request | R1 |
| D | src/replication/wire.rs:46:29–46:30 | 1645..1646 | try_propagation | read_request | R1 |
| E | src/replication/wire.rs:63:16–63:21 | 2025..2030 | match | reply | R1 |
| E | src/replication/wire.rs:86:5–86:10 | 2844..2849 | match | read_reply | R1 |
| D | src/replication/wire.rs:86:29–86:30 | 2868..2869 | try_propagation | read_reply | R1 |
| D | src/replication/wire.rs:90:44–90:45 | 2995..2996 | try_propagation | read_reply | R1 |
| D | src/replication/wire.rs:111:58–111:59 | 3766..3767 | try_propagation | read | R1 |

## Validation

On arm64 macOS, `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/register-fuzz`
with `cargo test --offline --locked -p weekend-db --all-features --lib
--test crdt --test replication` passed 105 library, 11 CRDT integration and
seven replication integration tests in both debug and release before the final
two directed replication additions. After those additions, all 14 scoped
replication library tests passed in debug/release and all-target warnings-denied
Clippy passed. This batch adds five CRDT and eight replication tests. Runtime
and unsafe implementation code is unchanged.
No mutation campaign or compiler-measured MC/DC run was performed for this
tranche. Source hashes above bind the explicit AST classification. Logs are
`/tmp/crdt-repl-final-{debug,release}.log` and the final scoped
`/tmp/replication-gap-{debug,release,clippy}.log`.
