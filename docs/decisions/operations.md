# Operational storage source decision audit

This ledger classifies all **186** explicit production AST records in logical
export/import (152), inspection/backup/restore (18) and complete-root salvage
(16). It adds eight directed tests and reuses the operational integration,
corruption, allocation-budget, compatibility-fixture and fuzz oracles.
Runtime behavior and unsafe implementation code are unchanged.

P denotes directed predicate controls, E execution/dispatch evidence, C
syntax overlapping a parent predicate, I an infeasible residual with the stated
valid-input invariant, and D delegated allocation/storage/callback propagation.
A D does not mean every failure at the callsite was injected. There is no
compiler-measured MC/DC percentage or implicit coverage claim from round trips.

## O1: logical resource bounds and value parsing

`reader_writer_bounds_and_error_mapping` accepts exactly MAX_IMAGE_BYTES output,
then rejects one additional byte; empty append at the ceiling succeeds. Input
larger than the ceiling is rejected. Reader count is tested exactly at/above
its supplied bound. Independent depth/node controls are (depth=1,nodes=MAX-1)
accepted, (depth=2,nodes=MAX-1) rejected under max_depth=1, and
(depth=1,nodes=MAX) rejected. Writer nodes accept the last allowed node before
rejecting another. None/Boolean scalar bits accept valid representations and
map Invalid to Corrupt; a zero allocation budget produces Limit unchanged.
Unknown tags reject. Direct take truncation and the existing truncated-stream
matrix check parse failure without destination activity.

`collection_record_and_map_order_guards` controls each collection OR term with
all others valid: id<=previous, id>=next, zero content and content>data revision.
Zero next collection rejects separately. Two collections exercise duplicate
IDs. Record written=0, written=content and written>content control its OR.
Map and record keys use ascending, duplicate and descending binary sequences;
empty/single containers and all scalar/value tag dispatches are also exercised
by `logical_export_import_preserves_values_roots_revisions_and_future_ids`.

Writer byte-add overflow is infeasible for valid state: existing length is at
most64MiB, a Rust input slice has at most isize::MAX bytes, and their sum is below
usize::MAX on supported32/64-bit targets. Writer/Reader nodes increase by one
from at mostMAX_NODES before immediately checking the cap; they cannot wrap.
Depth is at most64 before recursive increment, so depth+1 cannot wrap. Fixed
width integer conversions after take(8) cannot fail. Writer scalar accessors
are selected by the same Value::kind and cannot return Type on valid Values;
collection map keys recovered by the codec are exactly eight bytes. These
specific residuals are I; surrounding fallible allocation/serialization is D.
On32-bit, u64→usize conversion may fail; this host does not execute that target.

## O2: logical roots, ordering and complete-history validation

`header_and_retention_predicates_have_independent_controls` changes magic and
version independently, rejects short input/bad CRC/zero identity and toggles
current data_revision>revision vs next_id=0 independently. It checks retained
count and data-count limits. For each retained descriptor it isolates
id<=previous, id>=next_id, captured>=current revision, data>captured and
data>current data_revision. The last control raises captured to9 while keeping
current revision10/data revision8, so the earlier data>captured test stays false.
Duplicate retained IDs also reject.

`data_revision_membership_order_and_next_collection_guards` controls too-new
data, ascending/descending/duplicate data heads, referenced/unreferenced old
data, current data with no retained reference, missing current data, missing
retained data, trailing bytes and retained next_collection greater/equal/less
than current. For unreferenced old data the AND operands have all three useful
controls: current+unreferenced accepts, old+referenced accepts, old+unreferenced
rejects. The data-order predicate contains index>0 AND last>=current. At index0,
the vector is empty and last is None; last>=current cannot be true. Therefore
independent control of index while holding that later condition true is
infeasible, rather than a missing test. The first, ascending later, duplicate
later and descending later iterations cover the feasible combinations.

`shared_dag_expansion_checks_nodes_and_physical_restore_size_without_source_writes`
constructs tiny physical DAGs with two references to each child. Depth13 expands
to16,383 values and passes Writer's node bound, but physical reconstruction
would exceed the64MiB image budget; public export rejects Limit. Depth14 exceeds
the node budget itself. Both preserve source bytes and perform no source write
or sync. Thus a small shared physical graph is not assumed to produce a
restorable logical export. The existing version-one fixture and CLI tests
check successful current/retained values, revision and future-ID preservation,
NaN bit identity, unknown bytes, create-new destination and unsupported-version
rejection before destination creation. Fuzz `logical_import_export` adds bounded
malformed streams and round-trip/state oracles, not all residual coverage.

## O3: inspection, backup and restore

`empty_inspection_reports_no_retained_roots_and_no_source_writes` covers the
empty retained loop and exact revision0 metadata. Existing
`inspection_verifies_all_roots_without_stabilizing_or_writing` checks retained
metadata and an unwitnessed selected root without stabilization.
`backup_preserves_history_and_rejects_busy_or_nonempty_stores`,
`backup_faults_preserve_source_and_never_report_partial_success` and
`inspection_and_backup_source_read_faults_release_lease_without_writes` check
leases, all roots, destination admission and source/destination fault paths.
`native_backup_restore_and_check_run_in_independent_processes` runs real file
backup/restore/verification; invalid source does not create a destination and
existing paths are not overwritten. Fuzz backup_restore and typed cross-process
compatibility tests supply additional logical oracles. Information/copy wrappers
mostly propagate codec/storage/memory errors, so those records remain D rather
than claiming independent injection at every wrapper layer.

## O4: explicit salvage and read-only candidate views

`candidate_view_masks_only_unselected_root_and_witness_intersections` checks both
selected slots, empty reads, full metadata reads, partial boundary overlaps and
nonoverlaps. Expected bytes are derived from absolute region membership. A
scripted backend overreporting count returns Io; offset+count overflow returns
Corrupt. These are defensive ByteStore-contract tests, not native device faults.
Candidate lock/write/sync reject Invalid and never forward source mutation.

Existing `salvage_selects_highest_complete_graph_without_changing_strict_recovery`
checks highest revision4, explicit fallback to complete revision3 when newest
is damaged/missing, and ignored witnesses. Its retained revision2/value10 must
survive either result; ordinary inspection still rejects damaged newest roots.
`salvage_later_candidate_and_resource_rejection_preserve_source` exercises both
verified candidates [4,5] and selects later slot1/revision5, preserving its
retained value. A resource error aborts without touching destination, rather
than treating the candidate as damaged. Existing tests cover no valid candidate,
unsupported format and I/O error, nonempty destination and create-new native
salvage. Selection has first-candidate, higher and lower subsequent candidate
controls. Equal revision shares the strict-greater false branch; it is not a
separate condition. Only Corrupt permits trying the other complete root; all
other errors propagate. Salvage remains explicit and reports possible loss of
acknowledged data. No implicit recovery fallback was introduced.

## Inventory scope

The scanner excludes cfg(test) while retaining possible feature/platform
variants. Raw macro token bodies, derives, iterator/combinator implementations
and dependencies are not expanded. Explicit short-circuit nodes overlap their
parent if; closures in is_some_and/is_none_or may introduce semantics outside
this syntactic denominator. No mutation campaign or compiler-measured MC/DC was
run for this tranche. The source-span table is an audit map, not a percentage.

| Source | SHA-256 |
|---|---|
| `src/storage/logical.rs` | `8dc6d69d3f2bfb86dcc5920777c44c900021b4cbe33bcb7eee868b4e15948ac8` |
| `src/storage/inspection.rs` | `3465472e0bb67fe2b3bdc81840a433c662091c287441dd4f54a102442413e055` |
| `src/storage/salvage.rs` | `aa5aa505afcde9d987f843582c75af4d29069d06bedd7c3f7a73b57d3050a69d` |

Classification totals: C=14, D=123, E=19, I=6, P=24.

Spans are one-based line:column; byte offsets are end-exclusive.

| Status | Source span | Bytes | AST kind | Owner | Evidence/scope |
|---|---|---|---|---|---|
| D | src/storage/inspection.rs:33:75–33:76 | 1060..1061 | try_propagation | information | O3 |
| E | src/storage/inspection.rs:34:5–34:8 | 1067..1070 | for_iteration | information | O3 |
| D | src/storage/inspection.rs:46:48–46:49 | 1544..1545 | try_propagation | information | O3 |
| D | src/storage/inspection.rs:56:34–56:35 | 1940..1941 | try_propagation | inspect_on | O3 |
| D | src/storage/inspection.rs:57:51–57:52 | 1993..1994 | try_propagation | inspect_on | O3 |
| D | src/storage/inspection.rs:58:63–58:64 | 2058..2059 | try_propagation | inspect_on | O3 |
| D | src/storage/inspection.rs:68:47–68:48 | 2327..2328 | try_propagation | copy_verified | O3 |
| D | src/storage/inspection.rs:76:6–76:7 | 2530..2531 | try_propagation | copy_verified | O3 |
| D | src/storage/inspection.rs:77:28–77:29 | 2560..2561 | try_propagation | copy_verified | O3 |
| D | src/storage/inspection.rs:93:34–93:35 | 3159..3160 | try_propagation | backup_on | O3 |
| D | src/storage/inspection.rs:94:51–94:52 | 3212..3213 | try_propagation | backup_on | O3 |
| D | src/storage/inspection.rs:95:63–95:64 | 3277..3278 | try_propagation | backup_on | O3 |
| D | src/storage/inspection.rs:101:54–101:55 | 3554..3555 | try_propagation | inspect_file | O3 |
| D | src/storage/inspection.rs:113:34–113:35 | 4063..4064 | try_propagation | backup_file | O3 |
| D | src/storage/inspection.rs:114:78–114:79 | 4143..4144 | try_propagation | backup_file | O3 |
| D | src/storage/inspection.rs:114:86–114:87 | 4151..4152 | try_propagation | backup_file | O3 |
| D | src/storage/inspection.rs:115:63–115:64 | 4216..4217 | try_propagation | backup_file | O3 |
| D | src/storage/inspection.rs:116:60–116:61 | 4278..4279 | try_propagation | backup_file | O3 |
| P | src/storage/logical.rs:21:9–21:11 | 654..656 | if | bytes | O1 |
| D | src/storage/logical.rs:29:55–29:56 | 899..900 | try_propagation | bytes | O1 |
| D | src/storage/logical.rs:37:37–37:38 | 1146..1147 | try_propagation | blob | O1 |
| P | src/storage/logical.rs:42:9–42:11 | 1269..1271 | if | value | O1 |
| D | src/storage/logical.rs:45:42–45:43 | 1386..1387 | try_propagation | value | O1 |
| E | src/storage/logical.rs:46:9–46:14 | 1397..1402 | match | value | O1 |
| I | src/storage/logical.rs:48:64–48:65 | 1520..1521 | try_propagation | value | O1 |
| I | src/storage/logical.rs:49:53–49:54 | 1577..1578 | try_propagation | value | O1 |
| I | src/storage/logical.rs:50:54–50:55 | 1641..1642 | try_propagation | value | O1 |
| I | src/storage/logical.rs:51:51–51:52 | 1695..1696 | try_propagation | value | O1 |
| I | src/storage/logical.rs:52:54–52:55 | 1762..1763 | try_propagation | value | O1 |
| D | src/storage/logical.rs:54:50–54:51 | 1842..1843 | try_propagation | value | O1 |
| D | src/storage/logical.rs:55:47–55:48 | 1891..1892 | try_propagation | value | O1 |
| E | src/storage/logical.rs:56:17–56:20 | 1910..1913 | for_iteration | value | O1 |
| D | src/storage/logical.rs:57:36–57:37 | 1975..1976 | try_propagation | value | O1 |
| D | src/storage/logical.rs:58:39–58:40 | 2016..2017 | try_propagation | value | O1 |
| D | src/storage/logical.rs:63:37–63:38 | 2138..2139 | try_propagation | value | O1 |
| D | src/storage/logical.rs:63:46–63:47 | 2147..2148 | try_propagation | value | O1 |
| E | src/storage/logical.rs:64:17–64:20 | 2166..2169 | for_iteration | value | O1 |
| D | src/storage/logical.rs:64:44–64:45 | 2193..2194 | try_propagation | value | O1 |
| D | src/storage/logical.rs:65:54–65:55 | 2250..2251 | try_propagation | value | O1 |
| D | src/storage/logical.rs:65:56–65:57 | 2252..2253 | try_propagation | value | O1 |
| D | src/storage/logical.rs:72:37–72:38 | 2425..2426 | try_propagation | data | O1 |
| D | src/storage/logical.rs:73:23–73:24 | 2450..2451 | try_propagation | data | O1 |
| D | src/storage/logical.rs:75:67–75:68 | 2568..2569 | try_propagation | data | O1 |
| I | src/storage/logical.rs:78:78–78:79 | 2735..2736 | try_propagation | data | O1 |
| D | src/storage/logical.rs:82:11–82:12 | 2812..2813 | try_propagation | data | O1 |
| D | src/storage/logical.rs:84:43–84:44 | 2914..2915 | try_propagation | data | O1 |
| E | src/storage/logical.rs:85:9–85:12 | 2925..2928 | for_iteration | data | O1 |
| D | src/storage/logical.rs:86:25–86:26 | 2987..2988 | try_propagation | data | O1 |
| D | src/storage/logical.rs:87:42–87:43 | 3031..3032 | try_propagation | data | O1 |
| D | src/storage/logical.rs:89:69–89:70 | 3151..3152 | try_propagation | data | O1 |
| D | src/storage/logical.rs:93:15–93:16 | 3294..3295 | try_propagation | data | O1 |
| D | src/storage/logical.rs:95:43–95:44 | 3398..3399 | try_propagation | data | O1 |
| E | src/storage/logical.rs:96:13–96:16 | 3413..3416 | for_iteration | data | O1 |
| D | src/storage/logical.rs:97:31–97:32 | 3474..3475 | try_propagation | data | O1 |
| D | src/storage/logical.rs:98:42–98:43 | 3518..3519 | try_propagation | data | O1 |
| D | src/storage/logical.rs:99:42–99:43 | 3562..3563 | try_propagation | data | O1 |
| D | src/storage/logical.rs:109:34–109:35 | 3900..3901 | try_propagation | export_logical_on | O1 |
| D | src/storage/logical.rs:110:51–110:52 | 3953..3954 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:111:63–111:64 | 4018..4019 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:117:24–117:25 | 4152..4153 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:118:24–118:25 | 4178..4179 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:119:38–119:39 | 4218..4219 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:120:40–120:41 | 4260..4261 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:121:45–121:46 | 4307..4308 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:122:44–122:45 | 4353..4354 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:123:57–123:58 | 4412..4413 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:125:66–125:67 | 4518..4519 | try_propagation | export_logical_on | O2 |
| E | src/storage/logical.rs:127:5–127:8 | 4587..4590 | for_iteration | export_logical_on | O2 |
| D | src/storage/logical.rs:128:28–128:29 | 4657..4658 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:129:39–129:40 | 4698..4699 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:130:44–130:45 | 4744..4745 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:135:34–135:35 | 4960..4961 | try_propagation | export_logical_on | O2 |
| E | src/storage/logical.rs:136:5–136:8 | 4967..4970 | for_iteration | export_logical_on | O2 |
| D | src/storage/logical.rs:137:32–137:33 | 5025..5026 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:139:42–139:43 | 5075..5076 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:142:32–142:33 | 5265..5266 | try_propagation | export_logical_on | O2 |
| D | src/storage/logical.rs:153:82–153:83 | 5541..5542 | try_propagation | take | O1 |
| D | src/storage/logical.rs:158:43–158:44 | 5677..5678 | try_propagation | int | O1 |
| D | src/storage/logical.rs:161:43–161:44 | 5808..5809 | try_propagation | count | O1 |
| D | src/storage/logical.rs:161:71–161:72 | 5836..5837 | try_propagation | count | O1 |
| P | src/storage/logical.rs:162:9–162:11 | 5847..5849 | if | count | O1 |
| D | src/storage/logical.rs:169:56–169:57 | 6051..6052 | try_propagation | blob | O1 |
| D | src/storage/logical.rs:171:30–171:31 | 6111..6112 | try_propagation | blob | O1 |
| P | src/storage/logical.rs:175:9–175:11 | 6210..6212 | if | value | O1 |
| C | src/storage/logical.rs:175:12–175:73 | 6213..6274 | short_circuit_or | value | O1 |
| D | src/storage/logical.rs:178:31–178:32 | 6355..6356 | try_propagation | value | O1 |
| E | src/storage/logical.rs:179:9–179:14 | 6369..6374 | match | value | O1 |
| D | src/storage/logical.rs:188:38–188:39 | 6674..6675 | try_propagation | value | O1 |
| P | src/storage/logical.rs:190:21–190:23 | 6763..6765 | if | value | O1 |
| D | src/storage/logical.rs:198:40–198:41 | 6995..6996 | try_propagation | value | O1 |
| D | src/storage/logical.rs:202:66–202:67 | 7147..7148 | try_propagation | value | O1 |
| E | src/storage/logical.rs:205:17–205:20 | 7273..7276 | for_iteration | value | O1 |
| D | src/storage/logical.rs:206:42–206:43 | 7330..7331 | try_propagation | value | O1 |
| P | src/storage/logical.rs:207:21–207:23 | 7353..7355 | if | value | O1 |
| D | src/storage/logical.rs:213:54–213:55 | 7625..7626 | try_propagation | value | O1 |
| D | src/storage/logical.rs:214:57–214:58 | 7684..7685 | try_propagation | value | O1 |
| D | src/storage/logical.rs:220:66–220:67 | 7892..7893 | try_propagation | value | O1 |
| E | src/storage/logical.rs:222:17–222:20 | 7955..7958 | for_iteration | value | O1 |
| D | src/storage/logical.rs:223:54–223:55 | 8024..8025 | try_propagation | value | O1 |
| D | src/storage/logical.rs:224:54–224:55 | 8080..8081 | try_propagation | value | O1 |
| D | src/storage/logical.rs:232:34–232:35 | 8307..8308 | try_propagation | data | O1 |
| D | src/storage/logical.rs:233:30–233:31 | 8339..8340 | try_propagation | data | O1 |
| P | src/storage/logical.rs:234:9–234:11 | 8350..8352 | if | data | O1 |
| D | src/storage/logical.rs:237:58–237:59 | 8472..8473 | try_propagation | data | O1 |
| E | src/storage/logical.rs:240:9–240:12 | 8558..8561 | for_iteration | data | O1 |
| D | src/storage/logical.rs:241:32–241:33 | 8605..8606 | try_propagation | data | O1 |
| D | src/storage/logical.rs:242:37–242:38 | 8644..8645 | try_propagation | data | O1 |
| P | src/storage/logical.rs:243:13–243:15 | 8659..8661 | if | data | O1 |
| C | src/storage/logical.rs:243:16–243:47 | 8662..8693 | short_circuit_or | data | O1 |
| C | src/storage/logical.rs:243:16–243:63 | 8662..8709 | short_circuit_or | data | O1 |
| C | src/storage/logical.rs:243:16–243:85 | 8662..8731 | short_circuit_or | data | O1 |
| D | src/storage/logical.rs:247:66–247:67 | 8887..8888 | try_propagation | data | O1 |
| E | src/storage/logical.rs:250:13–250:16 | 9005..9008 | for_iteration | data | O1 |
| D | src/storage/logical.rs:251:38–251:39 | 9062..9063 | try_propagation | data | O1 |
| P | src/storage/logical.rs:252:17–252:19 | 9081..9083 | if | data | O1 |
| D | src/storage/logical.rs:258:41–258:42 | 9320..9321 | try_propagation | data | O1 |
| P | src/storage/logical.rs:259:17–259:19 | 9339..9341 | if | data | O1 |
| C | src/storage/logical.rs:259:20–259:53 | 9342..9375 | short_circuit_or | data | O1 |
| D | src/storage/logical.rs:262:42–262:43 | 9485..9486 | try_propagation | data | O1 |
| D | src/storage/logical.rs:270:18–270:19 | 9726..9727 | try_propagation | data | O1 |
| D | src/storage/logical.rs:280:14–280:15 | 10014..10015 | try_propagation | data | O1 |
| D | src/storage/logical.rs:287:15–287:16 | 10185..10186 | try_propagation | data | O1 |
| P | src/storage/logical.rs:299:5–299:7 | 10392..10394 | if | decode | O2 |
| P | src/storage/logical.rs:302:5–302:7 | 10471..10473 | if | decode | O2 |
| P | src/storage/logical.rs:306:5–306:7 | 10597..10599 | if | decode | O2 |
| C | src/storage/logical.rs:306:8–306:99 | 10600..10691 | short_circuit_or | decode | O2 |
| P | src/storage/logical.rs:309:5–309:7 | 10750..10752 | if | decode | O2 |
| D | src/storage/logical.rs:317:35–317:36 | 10997..10998 | try_propagation | decode | O2 |
| P | src/storage/logical.rs:318:5–318:7 | 11024..11026 | if | decode | O2 |
| D | src/storage/logical.rs:321:32–321:33 | 11122..11123 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:322:37–322:38 | 11161..11162 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:323:31–323:32 | 11194..11195 | try_propagation | decode | O2 |
| P | src/storage/logical.rs:324:5–324:7 | 11201..11203 | if | decode | O2 |
| C | src/storage/logical.rs:324:8–324:48 | 11204..11244 | short_circuit_or | decode | O2 |
| D | src/storage/logical.rs:327:53–327:54 | 11341..11342 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:329:35–329:36 | 11417..11418 | try_propagation | decode | O2 |
| E | src/storage/logical.rs:331:5–331:8 | 11450..11453 | for_iteration | decode | O2 |
| D | src/storage/logical.rs:332:30–332:31 | 11499..11500 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:333:36–333:37 | 11537..11538 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:334:32–334:33 | 11571..11572 | try_propagation | decode | O2 |
| P | src/storage/logical.rs:335:9–335:11 | 11582..11584 | if | decode | O2 |
| C | src/storage/logical.rs:335:12–336:29 | 11585..11628 | short_circuit_or | decode | O2 |
| C | src/storage/logical.rs:335:12–337:36 | 11585..11664 | short_circuit_or | decode | O2 |
| C | src/storage/logical.rs:335:12–338:31 | 11585..11695 | short_circuit_or | decode | O2 |
| C | src/storage/logical.rs:335:12–339:36 | 11585..11731 | short_circuit_or | decode | O2 |
| D | src/storage/logical.rs:346:57–346:58 | 11919..11920 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:348:34–348:35 | 11993..11994 | try_propagation | decode | O2 |
| E | src/storage/logical.rs:349:5–349:8 | 12000..12003 | for_iteration | decode | O2 |
| D | src/storage/logical.rs:350:41–350:42 | 12064..12065 | try_propagation | decode | O2 |
| P | src/storage/logical.rs:351:9–351:11 | 12075..12077 | if | decode | O2 |
| C | src/storage/logical.rs:351:12–355:20 | 12078..12296 | short_circuit_or | decode | O2 |
| C | src/storage/logical.rs:352:17–355:19 | 12129..12295 | short_circuit_and | decode | O2 |
| P | src/storage/logical.rs:359:9–359:11 | 12365..12367 | if | decode | O2 |
| C | src/storage/logical.rs:359:12–360:71 | 12368..12474 | short_circuit_and | decode | O2 |
| P | src/storage/logical.rs:366:5–366:7 | 12578..12580 | if | decode | O2 |
| D | src/storage/logical.rs:372:31–372:32 | 12790..12791 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:377:7–377:8 | 12923..12924 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:379:43–379:44 | 13009..13010 | try_propagation | decode | O2 |
| E | src/storage/logical.rs:380:5–380:8 | 13016..13019 | for_iteration | decode | O2 |
| D | src/storage/logical.rs:384:35–384:36 | 13207..13208 | try_propagation | decode | O2 |
| P | src/storage/logical.rs:385:9–385:11 | 13218..13220 | if | decode | O2 |
| D | src/storage/logical.rs:395:15–395:16 | 13558..13559 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:414:6–414:7 | 13984..13985 | try_propagation | decode | O2 |
| D | src/storage/logical.rs:425:6–425:7 | 14267..14268 | try_propagation | install | O2 |
| D | src/storage/logical.rs:434:34–434:35 | 14536..14537 | try_propagation | import_logical_on | O2 |
| D | src/storage/logical.rs:435:36–435:37 | 14574..14575 | try_propagation | import_logical_on | O2 |
| D | src/storage/logical.rs:440:61–440:62 | 14839..14840 | try_propagation | export_logical_file | O2 |
| D | src/storage/logical.rs:449:34–449:35 | 15151..15152 | try_propagation | import_logical_file | O2 |
| D | src/storage/logical.rs:450:36–450:37 | 15189..15190 | try_propagation | import_logical_file | O2 |
| D | src/storage/logical.rs:451:50–451:51 | 15241..15242 | try_propagation | import_logical_file | O2 |
| D | src/storage/salvage.rs:31:50–31:51 | 1016..1017 | try_propagation | read | O4 |
| P | src/storage/salvage.rs:32:9–32:11 | 1027..1029 | if | read | O4 |
| D | src/storage/salvage.rs:35:73–35:74 | 1167..1168 | try_propagation | read | O4 |
| E | src/storage/salvage.rs:36:9–36:12 | 1178..1181 | for_iteration | read | O4 |
| P | src/storage/salvage.rs:40:13–40:15 | 1374..1376 | if | read | O4 |
| D | src/storage/salvage.rs:59:34–59:35 | 1891..1892 | try_propagation | salvage | O4 |
| D | src/storage/salvage.rs:60:51–60:52 | 1944..1945 | try_propagation | salvage | O4 |
| D | src/storage/salvage.rs:61:47–61:48 | 1993..1994 | try_propagation | salvage | O4 |
| E | src/storage/salvage.rs:64:5–64:8 | 2101..2104 | for_iteration | salvage | O4 |
| E | src/storage/salvage.rs:69:9–69:14 | 2277..2282 | match | salvage | O4 |
| P | src/storage/salvage.rs:72:17–72:19 | 2423..2425 | if | salvage | O4 |
| D | src/storage/salvage.rs:83:59–83:60 | 2859..2860 | try_propagation | salvage | O4 |
| D | src/storage/salvage.rs:85:22–85:23 | 2919..2920 | try_propagation | salvage | O4 |
| D | src/storage/salvage.rs:91:6–91:7 | 3059..3060 | try_propagation | salvage | O4 |
| D | src/storage/salvage.rs:92:22–92:23 | 3083..3084 | try_propagation | salvage | O4 |
| D | src/storage/salvage.rs:127:49–127:50 | 4494..4495 | try_propagation | salvage_file | O4 |

## Validation

On arm64 macOS, `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/register-fuzz`
with `cargo test --offline --locked -p weekend-db --all-features --lib storage::`
passed all 57 selected storage unit tests in debug and release. Separate runs
without a name filter of `--test logical --test inspection --test salvage`
passed all 12 integration tests in both profiles. All-target warnings-denied
Clippy and diff checks passed. Logs are `/tmp/operations-{debug,release}.log`,
`/tmp/operations-integration-{debug,release}.log` and
`/tmp/operations-clippy.log`. No unsafe implementation changed; Miri and a new
mutation campaign were not run for this tranche. Source hashes above bind the
classification on the development tree, rather than asserting a new release.
