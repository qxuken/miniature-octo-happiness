# Task application, admin CLI, and support binaries

This audit classifies 246 source records across the task library and CLI (93),
admin CLI (26), benchmark (93), process fault worker (33), and runtime build
identity (1). The inventory counts overlapping syntax. P identifies directed
predicate controls, E identifies executed paths, C identifies syntax accounted
for by its parent decision, I identifies an invariant, and D identifies a
delegated error path. The table records source review, with measured coverage
tracked separately.

## T1: task storage and edit boundaries

`attach_recovers_next_id_and_failed_actions_preserve_tasks` imports a legacy
task with ID 41, checks that repeated attachment leaves the revision alone, and
requires the next new task to have ID 42. It varies empty slug and title
independently, exercises absent lookups and a rejected edit callback, and
requires the old title and revision to survive. An invalid undo target creates
no checkpoint. `counter_initialization_and_increment_report_exhaustion` uses
legacy IDs one below and at `u64::MAX` to exercise each exhaustion path. Table
and schema registration during attachment can precede a counter error. The
inner counter absence check repeats an observation under exclusive access to
the store. Its present branch requires another writer to have inserted the row
between those observations, which the safe `&mut Store` call does not permit.

`task_ids_survive_failed_inserts_deletion_and_undo` checks that rejected unique
inserts leave the counter available and that undo preserves the allocator's
monotone progress. Undo retains a redo checkpoint before restoring the table.
An error during the second operation can leave the redo checkpoint. The
portable demo exercises typed migration, old-model updates and undo/redo.
Task counter-row corruption and arbitrary typed/core errors are delegated to
the catalog, transaction and codec evidence.

## T2: task command admission and native execution

`cli_admission_rejects_arguments_before_opening_a_file` varies wrong counts,
the list flag, unknown commands, excess demo/compact arguments, a missing
database path and Unix non-UTF-8 arguments. It checks help and demo without a
database. A sentinel verifies that argument rejection leaves the supplied file
untouched. For the list condition, `list`, `list --snapshot ID`, and
`list --wrong ID` exercise the coupled argument shape. An empty argument vector
cannot evaluate `args[0]` safely, so the short circuit is part of that guard.

`cli_edit_branches_preserve_failed_actions_and_deduplicate_tags` checks blank
text, invalid priority values, missing records and bad checkpoint IDs against
the revision of an initialized file. It also checks completion/reopening,
rename, title, note clearing, duplicate tags, maximum priority, and removal.
`cli_requires_explicit_upgrade_before_editing_old_records` checks the migration
error hint and revision preservation across a rejected edit in a fresh process.
It then runs the explicit upgrade and verifies that the edit succeeds.
`cli_reopens_history_and_restores_real_files` covers retained reads, undo,
forget, compact, regular list without a new revision, and a second create
against an existing path. Command dispatch's final unreachable arm follows
from the earlier exhaustive validation and immutable `command` binding.

## A1: administrative dispatch

`invalid_admin_commands_do_not_touch_paths_and_errors_do_not_print_records`
checks that excess arguments and malformed image verification leave source and
destination bytes alone. Verification errors omit the source path and record
bytes. Existing native process tests cover `check`, `backup`, `restore`,
`salvage`, `export`, and `import`, including retained values after restore.
The CLI's backup/restore guard has separate success cases and rejects other
verbs for that arm. The export loop implements short-write progress and zero/overreported-progress
rejection; direct executable predicate pairs are delegated. Its error channels are delegated to the storage adapter
and exact-I/O tests. This audit has no injected native short-write case in the
admin executable. Import reads at most one byte over the image cap, then delegates
format, size and destination validation to the logical importer.

## B1: benchmark and runtime identity

The benchmark is a private measurement driver. Ordinary and `--compact` runs
execute memory, full-sync, retention and reopen cases. The compact run also
checks compaction with and without retention. Retention at iteration 0 and 64
exercises both successful operands of the history condition, and the other
iterations leave it false. Measurements of successful runs depend on verified
inputs and successful lower-layer operations. Their propagated failures are
delegated to the engine and storage suites. The driver intentionally stops if
its seeded oracle fails.

The package tool supplies the source ID and checks it through the loaded C ABI
and Rust source consumer. Ordinary builds use the `option_env!` fallback.
ABI signatures, symbols and layout evidence remains in the private release
contract and packaged-consumer checks.

## F1: process fault worker

`file-worker` is a fault-injection binary used by the native process tests.
Those tests exercise held and released leases, trace and cut modes, and killing
the child around actual create/reopen/write/retain/release or replacement
operations. They check durable revisions and full allowed state after reopening.
`before` and `after` control which traced boundary parks the worker, with
distinct before/after cuts and an uninterrupted baseline. Recovery cuts cover
verification entry and stabilization writes/syncs, not every recovery read.
Compaction traces only the selected replacement hooks; it does not interrupt
every internal read or the middle of a kernel call. The tests use valid
private driver invocations. Argument indexing, malformed driver inputs and
stdout/mutex failures have no supported user-facing contract. The native tests
classify process interruption. Simulator and device assumptions remain separate
in the storage ledgers.

## Inventory

| Source | SHA-256 |
|---|---|
| `apps/weekend-tasks/src/lib.rs` | `887d9ce8c80ae6b4d92166e5c6947dce64dbf0611fd393559b8510d23d1980c6` |
| `apps/weekend-tasks/src/main.rs` | `c30b765061790065812d268281f5d6f7c54113d91977f48692635f8045003f46` |
| `tools/admin.rs` | `24da379179f9a92511526d9cde24fa8fc692709958df495d96169c690eafd864` |
| `tools/benchmark.rs` | `a522795d6ad4346dad50b9e9d8b1e790838895649bda034e2d25cdcc44d107ac` |
| `src/version.rs` | `5006fbd7ced8252c579cb6695cd0ecbf1c6ab15491a7fc43f8a999539cb2fcf3` |
| `tools/file_worker.rs` | `991190d7e55147765a10d89ec30ca2fd71ad373532cf50d3febceab1006cbf5b` |

Classification totals: C=11, D=168, E=20, I=1, P=46.

| Status | Source span | Bytes | AST kind | Owner | Evidence/scope |
|---|---|---|---|---|---|
| D | tools/benchmark.rs:33:7–33:8 | 1013..1014 | try_propagation | main | B1 |
| D | tools/benchmark.rs:35:15–35:16 | 1054..1055 | try_propagation | main | B1 |
| D | tools/benchmark.rs:36:37–36:38 | 1092..1093 | try_propagation | main | B1 |
| D | tools/benchmark.rs:36:68–36:69 | 1123..1124 | try_propagation | main | B1 |
| D | tools/benchmark.rs:36:70–36:71 | 1125..1126 | try_propagation | main | B1 |
| D | tools/benchmark.rs:36:72–36:73 | 1127..1128 | try_propagation | main | B1 |
| D | tools/benchmark.rs:39:31–39:32 | 1233..1234 | try_propagation | main | B1 |
| D | tools/benchmark.rs:41:50–41:51 | 1306..1307 | try_propagation | main | B1 |
| D | tools/benchmark.rs:41:52–41:53 | 1308..1309 | try_propagation | main | B1 |
| D | tools/benchmark.rs:42:60–42:61 | 1370..1371 | try_propagation | main | B1 |
| D | tools/benchmark.rs:44:7–44:8 | 1395..1396 | try_propagation | main | B1 |
| D | tools/benchmark.rs:45:29–45:30 | 1426..1427 | try_propagation | main | B1 |
| E | tools/benchmark.rs:46:5–46:8 | 1433..1436 | for_iteration | main | B1 |
| D | tools/benchmark.rs:47:46–47:47 | 1496..1497 | try_propagation | main | B1 |
| D | tools/benchmark.rs:47:48–47:49 | 1498..1499 | try_propagation | main | B1 |
| D | tools/benchmark.rs:49:30–49:31 | 1536..1537 | try_propagation | main | B1 |
| D | tools/benchmark.rs:51:54–51:55 | 1641..1642 | try_propagation | main | B1 |
| D | tools/benchmark.rs:51:72–51:73 | 1659..1660 | try_propagation | main | B1 |
| D | tools/benchmark.rs:53:7–53:8 | 1684..1685 | try_propagation | main | B1 |
| D | tools/benchmark.rs:56:42–56:43 | 1807..1808 | try_propagation | main | B1 |
| E | tools/benchmark.rs:57:9–57:12 | 1818..1821 | for_iteration | main | B1 |
| D | tools/benchmark.rs:58:48–58:49 | 1885..1886 | try_propagation | main | B1 |
| D | tools/benchmark.rs:60:30–60:31 | 1927..1928 | try_propagation | main | B1 |
| D | tools/benchmark.rs:62:7–62:8 | 1952..1953 | try_propagation | main | B1 |
| E | tools/benchmark.rs:66:9–66:12 | 2073..2076 | for_iteration | main | B1 |
| D | tools/benchmark.rs:67:66–67:67 | 2167..2168 | try_propagation | main | B1 |
| D | tools/benchmark.rs:67:68–67:69 | 2169..2170 | try_propagation | main | B1 |
| D | tools/benchmark.rs:69:19–69:20 | 2200..2201 | try_propagation | main | B1 |
| D | tools/benchmark.rs:72:7–72:8 | 2278..2279 | try_propagation | main | B1 |
| D | tools/benchmark.rs:73:35–73:36 | 2315..2316 | try_propagation | main | B1 |
| D | tools/benchmark.rs:74:83–74:84 | 2400..2401 | try_propagation | main | B1 |
| D | tools/benchmark.rs:75:22–75:23 | 2424..2425 | try_propagation | main | B1 |
| D | tools/benchmark.rs:78:62–78:63 | 2564..2565 | try_propagation | main | B1 |
| D | tools/benchmark.rs:78:64–78:65 | 2566..2567 | try_propagation | main | B1 |
| D | tools/benchmark.rs:79:30–79:31 | 2598..2599 | try_propagation | main | B1 |
| D | tools/benchmark.rs:81:7–81:8 | 2623..2624 | try_propagation | main | B1 |
| D | tools/benchmark.rs:84:17–84:18 | 2683..2684 | try_propagation | main | B1 |
| D | tools/benchmark.rs:88:12–88:13 | 2835..2836 | try_propagation | main | B1 |
| D | tools/benchmark.rs:100:54–100:55 | 3203..3204 | try_propagation | files | B1 |
| D | tools/benchmark.rs:102:61–102:62 | 3311..3312 | try_propagation | files | B1 |
| D | tools/benchmark.rs:103:22–103:23 | 3335..3336 | try_propagation | files | B1 |
| E | tools/benchmark.rs:10:5–10:8 | 334..337 | for_iteration | timed | B1 |
| D | tools/benchmark.rs:106:62–106:63 | 3476..3477 | try_propagation | files | B1 |
| D | tools/benchmark.rs:106:64–106:65 | 3478..3479 | try_propagation | files | B1 |
| D | tools/benchmark.rs:107:30–107:31 | 3510..3511 | try_propagation | files | B1 |
| D | tools/benchmark.rs:109:7–109:8 | 3535..3536 | try_propagation | files | B1 |
| D | tools/benchmark.rs:111:82–111:83 | 3633..3634 | try_propagation | files | B1 |
| D | tools/benchmark.rs:112:22–112:23 | 3657..3658 | try_propagation | files | B1 |
| D | tools/benchmark.rs:11:15–11:16 | 368..369 | try_propagation | timed | B1 |
| D | tools/benchmark.rs:115:42–115:43 | 3786..3787 | try_propagation | files | B1 |
| E | tools/benchmark.rs:116:9–116:12 | 3797..3800 | for_iteration | files | B1 |
| D | tools/benchmark.rs:117:48–117:49 | 3864..3865 | try_propagation | files | B1 |
| D | tools/benchmark.rs:119:30–119:31 | 3906..3907 | try_propagation | files | B1 |
| D | tools/benchmark.rs:121:7–121:8 | 3931..3932 | try_propagation | files | B1 |
| D | tools/benchmark.rs:124:61–124:62 | 4053..4054 | try_propagation | files | B1 |
| D | tools/benchmark.rs:125:35–125:36 | 4090..4091 | try_propagation | files | B1 |
| D | tools/benchmark.rs:129:29–129:30 | 4274..4275 | try_propagation | files | B1 |
| D | tools/benchmark.rs:129:67–129:68 | 4312..4313 | try_propagation | files | B1 |
| D | tools/benchmark.rs:129:69–129:70 | 4314..4315 | try_propagation | files | B1 |
| D | tools/benchmark.rs:131:37–131:38 | 4386..4387 | try_propagation | files | B1 |
| D | tools/benchmark.rs:132:20–132:21 | 4408..4409 | try_propagation | files | B1 |
| P | tools/benchmark.rs:133:9–133:11 | 4419..4421 | if | files | B1 |
| C | tools/benchmark.rs:133:12–133:29 | 4422..4439 | short_circuit_or | files | B1 |
| D | tools/benchmark.rs:134:47–134:48 | 4488..4489 | try_propagation | files | B1 |
| D | tools/benchmark.rs:137:7–137:8 | 4522..4523 | try_propagation | files | B1 |
| D | tools/benchmark.rs:139:50–139:51 | 4616..4617 | try_propagation | files | B1 |
| D | tools/benchmark.rs:139:66–139:67 | 4632..4633 | try_propagation | files | B1 |
| D | tools/benchmark.rs:141:7–141:8 | 4657..4658 | try_propagation | files | B1 |
| D | tools/benchmark.rs:143:58–143:59 | 4731..4732 | try_propagation | files | B1 |
| D | tools/benchmark.rs:146:63–146:64 | 4881..4882 | try_propagation | files | B1 |
| E | tools/benchmark.rs:148:9–148:12 | 4944..4947 | for_iteration | files | B1 |
| D | tools/benchmark.rs:149:47–149:48 | 5009..5010 | try_propagation | files | B1 |
| D | tools/benchmark.rs:149:74–149:75 | 5036..5037 | try_propagation | files | B1 |
| D | tools/benchmark.rs:154:44–154:45 | 5149..5150 | try_propagation | files | B1 |
| P | tools/benchmark.rs:155:5–155:7 | 5156..5158 | if | files | B1 |
| D | tools/benchmark.rs:157:69–157:70 | 5321..5322 | try_propagation | files | B1 |
| D | tools/benchmark.rs:17:35–17:36 | 550..551 | try_propagation | seed | B1 |
| D | tools/benchmark.rs:163:50–163:51 | 5511..5512 | try_propagation | files | B1 |
| D | tools/benchmark.rs:164:63–164:64 | 5576..5577 | try_propagation | files | B1 |
| D | tools/benchmark.rs:167:54–167:55 | 5725..5726 | try_propagation | files | B1 |
| D | tools/benchmark.rs:167:70–167:71 | 5741..5742 | try_propagation | files | B1 |
| D | tools/benchmark.rs:169:11–169:12 | 5774..5775 | try_propagation | files | B1 |
| D | tools/benchmark.rs:171:63–171:64 | 5857..5858 | try_propagation | files | B1 |
| E | tools/benchmark.rs:172:9–172:12 | 5868..5871 | for_iteration | files | B1 |
| E | tools/benchmark.rs:19:5–19:8 | 586..589 | for_iteration | seed | B1 |
| D | tools/benchmark.rs:173:36–173:37 | 5922..5923 | try_propagation | files | B1 |
| D | tools/benchmark.rs:177:69–177:70 | 6057..6058 | try_propagation | files | B1 |
| D | tools/benchmark.rs:20:52–20:53 | 657..658 | try_propagation | seed | B1 |
| D | tools/benchmark.rs:20:54–20:55 | 659..660 | try_propagation | seed | B1 |
| D | tools/benchmark.rs:22:16–22:17 | 683..684 | try_propagation | seed | B1 |
| D | tools/benchmark.rs:27:29–27:30 | 807..808 | try_propagation | main | B1 |
| D | tools/benchmark.rs:28:22–28:23 | 831..832 | try_propagation | main | B1 |
| D | tools/benchmark.rs:31:66–31:67 | 979..980 | try_propagation | main | B1 |
| P | tools/file_worker.rs:37:13–37:15 | 1054..1056 | if | before | F1 |
| P | tools/file_worker.rs:43:13–43:15 | 1301..1303 | if | after | F1 |
| C | tools/file_worker.rs:43:16–43:19 | 1304..1307 | pattern_test | after | F1 |
| C | tools/file_worker.rs:43:16–44:73 | 1304..1400 | short_circuit_and | after | F1 |
| P | tools/file_worker.rs:85:9–85:11 | 2757..2759 | if | run | F1 |
| P | tools/file_worker.rs:89:23–89:25 | 2988..2990 | if | run | F1 |
| E | tools/file_worker.rs:95:28–95:33 | 3245..3250 | match | run | F1 |
| P | tools/file_worker.rs:112:17–112:19 | 4103..4105 | if | run | F1 |
| P | tools/file_worker.rs:116:17–116:19 | 4240..4242 | if | run | F1 |
| D | tools/file_worker.rs:123:15–123:16 | 4515..4516 | try_propagation | run | F1 |
| D | tools/file_worker.rs:124:66–124:67 | 4583..4584 | try_propagation | run | F1 |
| P | tools/file_worker.rs:126:13–126:15 | 4647..4649 | if | run | F1 |
| P | tools/file_worker.rs:142:9–142:11 | 5114..5116 | if | run | F1 |
| E | tools/file_worker.rs:143:13–143:18 | 5150..5155 | match | run | F1 |
| P | tools/file_worker.rs:150:9–150:11 | 5400..5402 | if | run | F1 |
| D | tools/file_worker.rs:151:67–151:68 | 5491..5492 | try_propagation | run | F1 |
| C | tools/file_worker.rs:155:20–155:67 | 5612..5659 | short_circuit_or | run | F1 |
| P | tools/file_worker.rs:157:18–157:20 | 5710..5712 | if | run | F1 |
| P | tools/file_worker.rs:163:20–163:22 | 5888..5890 | if | run | F1 |
| D | tools/file_worker.rs:164:36–164:37 | 5950..5951 | try_propagation | run | F1 |
| D | tools/file_worker.rs:166:34–166:35 | 6002..6003 | try_propagation | run | F1 |
| P | tools/file_worker.rs:172:18–172:20 | 6126..6128 | if | run | F1 |
| D | tools/file_worker.rs:173:80–173:81 | 6232..6233 | try_propagation | run | F1 |
| D | tools/file_worker.rs:175:56–175:57 | 6306..6307 | try_propagation | run | F1 |
| E | tools/file_worker.rs:178:9–178:14 | 6371..6376 | match | run | F1 |
| D | tools/file_worker.rs:181:38–181:39 | 6500..6501 | try_propagation | run | F1 |
| D | tools/file_worker.rs:182:54–182:55 | 6556..6557 | try_propagation | run | F1 |
| D | tools/file_worker.rs:183:54–183:55 | 6612..6613 | try_propagation | run | F1 |
| D | tools/file_worker.rs:184:28–184:29 | 6642..6643 | try_propagation | run | F1 |
| D | tools/file_worker.rs:187:51–187:52 | 6735..6736 | try_propagation | run | F1 |
| D | tools/file_worker.rs:189:48–189:49 | 6799..6800 | try_propagation | run | F1 |
| P | tools/file_worker.rs:195:9–195:11 | 6996..6998 | if | run | F1 |
| P | tools/file_worker.rs:32:13–32:15 | 900..902 | if | before | F1 |
| E | tools/admin.rs:28:13–28:16 | 1311..1314 | for_iteration | run | A1 |
| P | tools/admin.rs:36:43–36:86 | 1587..1630 | match_guard | run | A1 |
| C | tools/admin.rs:36:43–36:86 | 1587..1630 | short_circuit_or | run | A1 |
| P | tools/admin.rs:37:25–37:27 | 1660..1662 | if | run | A1 |
| D | tools/admin.rs:38:77–38:78 | 1761..1762 | try_propagation | run | A1 |
| D | tools/admin.rs:40:78–40:79 | 1861..1862 | try_propagation | run | A1 |
| P | tools/admin.rs:48:43–48:62 | 2096..2115 | match_guard | run | A1 |
| D | tools/admin.rs:49:80–49:81 | 2200..2201 | try_propagation | run | A1 |
| E | tools/admin.rs:6:5–6:10 | 221..226 | match | run | A1 |
| D | tools/admin.rs:50:67–50:68 | 2269..2270 | try_propagation | run | A1 |
| D | tools/admin.rs:51:24–51:25 | 2295..2296 | try_propagation | run | A1 |
| E | tools/admin.rs:53:13–53:18 | 2342..2347 | while | run | A1 |
| D | tools/admin.rs:54:74–54:75 | 2444..2445 | try_propagation | run | A1 |
| D | tools/admin.rs:55:17–55:19 | 2463..2465 | if | run | A1 |
| C | tools/admin.rs:55:20–55:66 | 2466..2512 | short_circuit_or | run | A1 |
| D | tools/admin.rs:60:24–60:25 | 2660..2661 | try_propagation | run | A1 |
| P | tools/admin.rs:7:43–7:63 | 276..296 | match_guard | run | A1 |
| P | tools/admin.rs:65:43–65:62 | 2812..2831 | match_guard | run | A1 |
| D | tools/admin.rs:66:86–66:87 | 2922..2923 | try_propagation | run | A1 |
| D | tools/admin.rs:70:52–70:53 | 3116..3117 | try_propagation | run | A1 |
| D | tools/admin.rs:71:81–71:82 | 3199..3200 | try_propagation | run | A1 |
| P | tools/admin.rs:87:5–87:7 | 3769..3771 | if | main | A1 |
| C | tools/admin.rs:87:8–87:11 | 3772..3775 | pattern_test | main | A1 |
| D | tools/admin.rs:8:87–8:88 | 388..389 | try_propagation | run | A1 |
| P | tools/admin.rs:17:28–17:46 | 809..827 | match_guard | run | A1 |
| D | tools/admin.rs:18:70–18:71 | 902..903 | try_propagation | run | A1 |
| E | src/version.rs:6:29–6:34 | 311..316 | match |  | B1 |
| P | apps/weekend-tasks/src/main.rs:34:8–34:10 | 1618..1620 | if | open | T2 |
| D | apps/weekend-tasks/src/main.rs:35:33–35:34 | 1662..1663 | try_propagation | open | T2 |
| D | apps/weekend-tasks/src/main.rs:37:31–37:32 | 1707..1708 | try_propagation | open | T2 |
| P | apps/weekend-tasks/src/main.rs:55:5–55:7 | 2182..2184 | if | print_task | T2 |
| P | apps/weekend-tasks/src/main.rs:58:5–58:7 | 2273..2275 | if | print_task | T2 |
| C | apps/weekend-tasks/src/main.rs:58:8–58:11 | 2276..2279 | pattern_test | print_task | T2 |
| P | apps/weekend-tasks/src/main.rs:66:5–66:7 | 2507..2509 | if | run | T2 |
| D | apps/weekend-tasks/src/main.rs:67:85–67:86 | 2651..2652 | try_propagation | run | T2 |
| P | apps/weekend-tasks/src/main.rs:70:5–70:8 | 2694..2697 | let_else | run | T2 |
| D | apps/weekend-tasks/src/main.rs:76:54–76:55 | 2878..2879 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:82:32–82:33 | 3070..3071 | try_propagation | run | T2 |
| P | apps/weekend-tasks/src/main.rs:83:5–83:7 | 3077..3079 | if | run | T2 |
| P | apps/weekend-tasks/src/main.rs:87:5–87:7 | 3187..3189 | if | run | T2 |
| P | apps/weekend-tasks/src/main.rs:88:9–88:11 | 3218..3220 | if | run | T2 |
| D | apps/weekend-tasks/src/main.rs:91:30–91:31 | 3336..3337 | try_propagation | run | T2 |
| P | apps/weekend-tasks/src/main.rs:94:5–94:7 | 3372..3374 | if | run | T2 |
| P | apps/weekend-tasks/src/main.rs:95:9–95:11 | 3406..3408 | if | run | T2 |
| D | apps/weekend-tasks/src/main.rs:100:51–100:52 | 3620..3621 | try_propagation | run | T2 |
| E | apps/weekend-tasks/src/main.rs:112:20–112:25 | 3977..3982 | match | run | T2 |
| P | apps/weekend-tasks/src/main.rs:117:13–117:15 | 4228..4230 | if | run | T2 |
| P | apps/weekend-tasks/src/main.rs:125:5–125:7 | 4418..4420 | if | run | T2 |
| P | apps/weekend-tasks/src/main.rs:128:5–128:7 | 4538..4540 | if | run | T2 |
| C | apps/weekend-tasks/src/main.rs:128:8–128:45 | 4541..4578 | short_circuit_and | run | T2 |
| C | apps/weekend-tasks/src/main.rs:128:8–128:72 | 4541..4605 | short_circuit_and | run | T2 |
| D | apps/weekend-tasks/src/main.rs:131:51–131:52 | 4725..4726 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:132:42–132:43 | 4769..4770 | try_propagation | run | T2 |
| E | apps/weekend-tasks/src/main.rs:133:5–133:10 | 4776..4781 | match | run | T2 |
| P | apps/weekend-tasks/src/main.rs:140:26–140:28 | 5023..5025 | if | run | T2 |
| D | apps/weekend-tasks/src/main.rs:141:53–141:54 | 5096..5097 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:143:52–143:53 | 5170..5171 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:143:80–143:81 | 5198..5199 | try_propagation | run | T2 |
| E | apps/weekend-tasks/src/main.rs:145:13–145:16 | 5227..5230 | for_iteration | run | T2 |
| D | apps/weekend-tasks/src/main.rs:151:70–151:71 | 5425..5426 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:152:55–152:56 | 5481..5482 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:158:11–158:12 | 5660..5661 | try_propagation | run | T2 |
| P | apps/weekend-tasks/src/main.rs:160:13–160:15 | 5707..5709 | if | run | T2 |
| P | apps/weekend-tasks/src/main.rs:164:17–164:19 | 5881..5883 | if | run | T2 |
| D | apps/weekend-tasks/src/main.rs:170:15–170:16 | 6078..6079 | try_propagation | run | T2 |
| P | apps/weekend-tasks/src/main.rs:173:13–173:15 | 6122..6124 | if | run | T2 |
| P | apps/weekend-tasks/src/main.rs:177:17–177:19 | 6289..6291 | if | run | T2 |
| D | apps/weekend-tasks/src/main.rs:181:15–181:16 | 6426..6427 | try_propagation | run | T2 |
| P | apps/weekend-tasks/src/main.rs:184:22–184:24 | 6515..6517 | if | run | T2 |
| D | apps/weekend-tasks/src/main.rs:190:11–190:12 | 6665..6666 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:192:40–192:41 | 6731..6732 | try_propagation | run | T2 |
| P | apps/weekend-tasks/src/main.rs:193:13–193:15 | 6746..6748 | if | run | T2 |
| D | apps/weekend-tasks/src/main.rs:199:15–199:16 | 6948..6949 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:201:51–201:52 | 7011..7012 | try_propagation | run | T2 |
| E | apps/weekend-tasks/src/main.rs:204:13–204:16 | 7110..7113 | for_iteration | run | T2 |
| D | apps/weekend-tasks/src/main.rs:204:38–204:39 | 7135..7136 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:205:80–205:81 | 7218..7219 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:205:89–205:90 | 7227..7228 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:214:61–214:62 | 7532..7533 | try_propagation | run | T2 |
| D | apps/weekend-tasks/src/main.rs:214:63–214:64 | 7534..7535 | try_propagation | run | T2 |
| E | apps/weekend-tasks/src/main.rs:224:5–224:10 | 7757..7762 | match | main | T2 |
| P | apps/weekend-tasks/src/main.rs:228:13–228:15 | 7870..7872 | if | main | T2 |
| D | apps/weekend-tasks/src/lib.rs:43:43–43:44 | 1068..1069 | try_propagation | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:44:49–44:50 | 1119..1120 | try_propagation | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:45:57–45:58 | 1178..1179 | try_propagation | attach | T1 |
| P | apps/weekend-tasks/src/lib.rs:46:9–46:11 | 1189..1191 | if | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:46:48–46:49 | 1228..1229 | try_propagation | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:46:61–46:62 | 1241..1242 | try_propagation | attach | T1 |
| I | apps/weekend-tasks/src/lib.rs:48:17–48:19 | 1291..1293 | if | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:48:38–48:39 | 1312..1313 | try_propagation | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:50:38–50:39 | 1396..1397 | try_propagation | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:56:72–56:73 | 1653..1654 | try_propagation | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:57:61–57:62 | 1716..1717 | try_propagation | attach | T1 |
| D | apps/weekend-tasks/src/lib.rs:60:15–60:16 | 1774..1775 | try_propagation | attach | T1 |
| P | apps/weekend-tasks/src/lib.rs:65:9–65:11 | 1920..1922 | if | add | T1 |
| C | apps/weekend-tasks/src/lib.rs:65:12–65:61 | 1923..1972 | short_circuit_or | add | T1 |
| D | apps/weekend-tasks/src/lib.rs:69:45–69:46 | 2131..2132 | try_propagation | add | T1 |
| D | apps/weekend-tasks/src/lib.rs:69:75–69:76 | 2161..2162 | try_propagation | add | T1 |
| D | apps/weekend-tasks/src/lib.rs:72:64–72:65 | 2290..2291 | try_propagation | add | T1 |
| D | apps/weekend-tasks/src/lib.rs:76:15–76:16 | 2418..2419 | try_propagation | add | T1 |
| D | apps/weekend-tasks/src/lib.rs:88:14–88:15 | 2737..2738 | try_propagation | add | T1 |
| D | apps/weekend-tasks/src/lib.rs:100:53–100:54 | 3030..3031 | try_propagation | update | T1 |
| D | apps/weekend-tasks/src/lib.rs:101:63–101:64 | 3094..3095 | try_propagation | update | T1 |
| D | apps/weekend-tasks/src/lib.rs:109:53–109:54 | 3351..3352 | try_propagation | remove | T1 |
| D | apps/weekend-tasks/src/lib.rs:110:63–110:64 | 3415..3416 | try_propagation | remove | T1 |
| D | apps/weekend-tasks/src/lib.rs:112:38–112:39 | 3477..3478 | try_propagation | remove | T1 |
| D | apps/weekend-tasks/src/lib.rs:118:57–118:58 | 3715..3716 | try_propagation | undo | T1 |
| D | apps/weekend-tasks/src/lib.rs:119:40–119:41 | 3757..3758 | try_propagation | undo | T1 |
| D | apps/weekend-tasks/src/lib.rs:120:45–120:46 | 3804..3805 | try_propagation | undo | T1 |
| D | apps/weekend-tasks/src/lib.rs:125:36–125:37 | 3899..3900 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:126:49–126:50 | 3950..3951 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:138:7–138:8 | 4254..4255 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:139:38–139:39 | 4294..4295 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:140:42–140:43 | 4338..4339 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:152:7–152:8 | 4660..4661 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:158:7–158:8 | 4806..4807 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:160:42–160:43 | 4871..4872 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:165:7–165:8 | 5008..5009 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:166:44–166:45 | 5054..5055 | try_propagation | demo | T1 |
| D | apps/weekend-tasks/src/lib.rs:168:29–168:30 | 5159..5160 | try_propagation | demo | T1 |
