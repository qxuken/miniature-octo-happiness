# Distributed application source decision audit

This audit classifies **205** explicit production AST records in the two
application entrypoints, native loops, protocols, study TLS adapter and shared
StudyProgress support. These are demonstration applications, not a general
production coordination or replication service. Nine new tests exercise
admission, parsing, retries, authentication, deadlines and clean shutdown. No
runtime application or core CRDT code was changed in this tranche.

P means directed predicate controls, E execution/dispatch evidence (not every
arm or failure), C overlapping syntax, I an infeasible residual under the stated
invariant, and D delegated operating-system/library/core failure paths. Every
record is mapped below; the count is not a measured MC/DC denominator.

## A1: study enrollment, commands and reviewed resolution

`enrollment_commands_and_retry_budget` accepts actors a/b and mixed-case hex
IDs; rejects unknown actor, zero identity, short identity and exactly32
nonhex characters. Length and character OR operands have separate controls.
`non_utf8_arguments_are_rejected` covers native Unix argument encoding. Once
exactly32 ASCII hex bytes have passed validation, u128 parsing cannot fail;
that propagation is I, not an untested malformed-input case.

Commands reject unknown/empty shapes, malformed numeric fields, lesson64 and
zero minutes without changing revision. A successful practice update commits
once. The retry closure executes exactly three times for perpetual Conflict,
once for Limit/OutcomeUnknown/RecoveryRequired, and twice for Conflict followed
by success. The failed draft's increment is discarded, so the eventual state
adds only one minute. The synthetic returned errors test retry classification;
they are not claimed to be actual uncertain disk outcomes. Retry is allowed
only for definite Conflict. Closure side effects outside the draft are the
application author's responsibility.

Shared StudyProgress's reviewed-goal comparison uses existing
`peers_commit_the_same_policy_result_offline_and_converge_on_one_goal` controls
in tests/crdt.rs: stale review rejects, matching review assigns, and independently
resolved peers converge. Record/Resolution derives preserve explicit field order
and format markers, but their expansions are not AST records in this file.
The core CRDT ledger and register event-oracle model own merge/algebra checks.

## A2: study TLS, network loop and acknowledgment behavior

Existing `sessions_require_the_configured_cert_on_both_ends` runs actual mutual
TLS, rejecting an untrusted client and a wrong server certificate pin.
`pin_admission_and_expired_absolute_deadline` adds self-pin/malformed-pin
admission and missing/empty/wrong/correct presented chains. It uses a real socket
pair for successful reads, then directly expires the private deadline to check
TimedOut before read/write. That direct control is not a claim that the kernel
timeout elapsed. A separate stalled TCP connection exercises the actual blocking
TLS handshake and must fail within10 seconds under the3-second configured
whole-session deadline. rustls cryptography and certificate parsing remain
dependency assurance, not independently verified by these examples.

`two_writers_recover_a_lost_ack` exercises two actual processes, push on/off,
backoff after a lost reply, concurrent local edits, restart, and stable subsequent
sync. Peer B reopens at local revision5 with total40; A stays at revision3 until
it receives B's state, then advances once to revision4. Further successful
acknowledgments do not create extra commits. An outgoing acknowledgment reports
the peer state and does not install it locally. `paused_peer_reports_idle_errors_and_quits_cleanly`
checks idle/error output, blank command, push-off and bounded graceful shutdown
while the outgoing thread is parked. Listener WouldBlock is exercised by real
nonblocking idle runs. Interrupted/fatal accept errors, thread panics, stdout
failures and mutex poisoning are delegated; no fabricated OS-error claim is made.

## A3: coordinator framing, secrets and durable receipt guards

`authentication_and_line_bounds` exercises absent auth prefix/separator, wrong
length, wrong equal-length token and matching token; empty terminated lines,
255 bytes plus newline, oversized256-byte body, premature EOF and invalid UTF-8.
The callback timeout and scripted reader ConnectionReset test propagation only.
Existing process authentication tests send actual oversized/stalled sockets and
verify they close before service continues. Network framing is capped256 bytes;
the local interactive stdin reader is not covered by this network cap.

`secret_file_and_loopback_address_admission` checks regular-file admission,
64 hex bytes with/without newline, short/nonhex/extra-newline contents, actual
Unix600/644 permissions and loopback port bounds/encoding. Windows has no matching
ACL inspection in this application; Unix permission evidence must not be read
as a cross-platform access-control guarantee. Token comparison performs a
full fixed-length fold, but this audit is not a formal constant-time proof.

`request_identifier_and_recovery_admission` accepts alphanumeric, hyphen,
underscore and32-byte IDs; rejects empty,33-byte and punctuation IDs. Empty IDs
cannot originate from split_whitespace in the public command parser; that
private-helper defensive control is explicitly synthetic. A real simulator I/O
failure poisons the owner, and subsequent get/retry returns RecoveryRequired.
Existing malformed receipt tests isolate length/version, expected+1 mismatch,
ahead-of-head and overflow; mismatch of original expected or value rejects
independently. The receipt-capacity test fills1024 IDs, rejects new IDs without
eviction and still returns the original receipt. Valid retry returns its
original committed revision, even after later changes to the record.

Existing two-process tests check stale expected revision conflicts and durable
request receipt replay across lost response/restart/competing requests.
Authentication happens before either writes or receipt lookup. Local commit
and application reply remain different events: a lost reply must use the same
request ID to recover the recorded result. The unlabelled `set` command has no
durable idempotency token. Arithmetic exhaustion of a new receipt's revision
and every allocator/storage failure at each wrapper callsite remain delegated.
Fixed receipt integer slices are safe after exact length/version validation.

## A4: CLI dispatch and scope limits

`cli_dispatch_rejects_invalid_shapes_and_accepts_help` and
`command_line_dispatch_and_clean_client_exit` run actual binaries for help,
invalid command/flag/arity combinations and client blank/quit exit. Existing
process suites run coordinator init/serve/client, both secret modes, and the
study worker loop. Study key generation/initialization and worker setup are
also called directly in tests. The CLI match records are execution evidence,
not a claim every guard was independently toggled through a full process.
Non-native target fallback entrypoints are not executed on this macOS host.

Deployment gaps are explicit: coordinator is a single serialized loopback
owner with optional plaintext shared-secret authentication, no TLS/remote
identity/authorization roles, failover or durable request queue. It exposes one
record and uses whole-database revisions. Receipts are deliberately finite and
never expire; lifecycle/compaction does not grant an eviction protocol. Study
supports exactly two fixed actors and manually exchanged certificate pins,
without discovery/gossip, enrollment changes, certificate rotation/revocation
workflow, rate limits, admission queues or multi-tenant isolation. One stalled
connection can occupy the serial receiver until its deadline; timeouts bound
one exchange but do not guarantee availability against repeated connections.
Local stdin, credential files and CLI argument collection are not bounded by
the engine heap budget. Interrupted initialization/key creation can leave a
partial path for explicit operator cleanup. The examples do not implement an
automated recovery supervisor after an uncertain write. These limitations are
not silently promoted to production guarantees by passing tests.

The AST inventory excludes cfg(test), does not expand derives/raw macro bodies,
and does not inspect iterator combinator internals, dependencies, OS kernels or
hidden formatting/allocation decisions. Runtime closures are included only as
explicit syntax. No new mutation campaign or compiler-measured MC/DC run was
performed here. Dedicated coordinator_protocol and peer_transport fuzz targets
reuse these production parsers but do not fuzz a complete TLS handshake or OS
scheduler. No actionable application runtime defect was reproduced in this
tranche; the separate CRDT invariant finding is addressed by its owner.

| Source | SHA-256 |
|---|---|
| `apps/study-peer/src/main.rs` | `be4e4fefc32a3b20fb77f9063b2514f93e7bd2c2892b6449acf2fbc286f6c74d` |
| `apps/study-peer/src/native.rs` | `3f33be0776d157e808c800014365e4cf39e224b57a7869f6e553cca698fa1a12` |
| `apps/study-peer/src/native/protocol.rs` | `3a11026d333bf97ff105748dd38e98023720752bced98b2b12af2a251b55d73e` |
| `apps/study-peer/src/native/tls.rs` | `4e139b444cee4d18079c0078d0f927bdde4fea3b25fa680dd5aba47348f98ee0` |
| `apps/write-coordinator/src/main.rs` | `0baa2da45ecc666867adbf652fc95958edefe1ac0523c76d96934b69e7b5517b` |
| `apps/write-coordinator/src/native.rs` | `72618c112004ea71d68a219b21570c4a107d69694d1cb5306b1b78d6c929b553` |
| `apps/write-coordinator/src/native/protocol.rs` | `544c5e161108c0c15d4c588ff2673b31d95ee12dcc318b615dccf864e573a858` |
| `examples/support/study.rs` | `d2a119e71a0863fa16ab2da04a8254e8fee9c6584448f2810edf80996522226d` |

Classification totals: C=18, D=127, E=30, I=1, P=29.

Spans are one-based line:column; byte offsets are end-exclusive.

| Status | Source span | Bytes | AST kind | Owner | Evidence/scope |
|---|---|---|---|---|---|
| E | apps/study-peer/src/main.rs:6:5–6:10 | 181..186 | match | main | A4 |
| E | apps/study-peer/src/native.rs:44:5–44:10 | 1288..1293 | match | main | A4 |
| E | apps/study-peer/src/native.rs:45:22–45:60 | 1333..1371 | match_guard | main | A4 |
| C | apps/study-peer/src/native.rs:45:22–45:60 | 1333..1371 | short_circuit_or | main | A4 |
| E | apps/study-peer/src/native.rs:49:27–49:46 | 1465..1484 | match_guard | main | A4 |
| E | apps/study-peer/src/native.rs:50:39–50:56 | 1563..1580 | match_guard | main | A4 |
| D | apps/study-peer/src/native.rs:51:69–51:70 | 1654..1655 | try_propagation | main | A4 |
| E | apps/study-peer/src/native.rs:63:14–63:82 | 1870..1938 | match_guard | main | A2 |
| C | apps/study-peer/src/native.rs:63:14–63:82 | 1870..1938 | short_circuit_and | main | A2 |
| C | apps/study-peer/src/native.rs:63:35–63:81 | 1891..1937 | short_circuit_or | main | A2 |
| D | apps/study-peer/src/native.rs:64:58–64:59 | 2001..2002 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:65:66–65:67 | 2069..2070 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:67:57–67:58 | 2164..2165 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:67:80–67:81 | 2187..2188 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:67:82–67:83 | 2189..2190 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:68:33–68:34 | 2224..2225 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:68:42–68:43 | 2233..2234 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:70:63–70:64 | 2329..2330 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:71:34–71:35 | 2365..2366 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:72:18–72:19 | 2392..2393 | try_propagation | main | A2 |
| D | apps/study-peer/src/native.rs:83:60–83:61 | 2701..2702 | try_propagation | initialize | A2 |
| D | apps/study-peer/src/native.rs:84:44–84:45 | 2747..2748 | try_propagation | initialize | A2 |
| D | apps/study-peer/src/native.rs:86:79–86:80 | 2881..2882 | try_propagation | initialize | A2 |
| D | apps/study-peer/src/native.rs:108:6–108:7 | 3341..3342 | try_propagation | run | A2 |
| D | apps/study-peer/src/native.rs:114:35–114:36 | 3458..3459 | try_propagation | run | A2 |
| D | apps/study-peer/src/native.rs:119:64–119:65 | 3696..3697 | try_propagation | run | A2 |
| E | apps/study-peer/src/native.rs:126:13–126:18 | 3891..3896 | while | run | A2 |
| E | apps/study-peer/src/native.rs:127:17–127:22 | 3945..3950 | match | run | A2 |
| D | apps/study-peer/src/native.rs:130:87–130:88 | 4177..4178 | try_propagation | run | A2 |
| D | apps/study-peer/src/native.rs:132:25–132:27 | 4232..4234 | if | run | A2 |
| D | apps/study-peer/src/native.rs:132:28–132:31 | 4235..4238 | pattern_test | run | A2 |
| D | apps/study-peer/src/native.rs:136:35–136:76 | 4408..4449 | match_guard | run | A2 |
| D | apps/study-peer/src/native.rs:139:35–139:77 | 4584..4626 | match_guard | run | A2 |
| E | apps/study-peer/src/native.rs:147:13–147:18 | 4835..4840 | while | run | A2 |
| P | apps/study-peer/src/native.rs:148:17–148:19 | 4879..4881 | if | run | A2 |
| D | apps/study-peer/src/native.rs:153:65–153:66 | 5122..5123 | try_propagation | run | A2 |
| P | apps/study-peer/src/native.rs:158:21–158:23 | 5303..5305 | if | run | A2 |
| E | apps/study-peer/src/native.rs:171:13–171:16 | 5723..5726 | for_iteration | run | A2 |
| D | apps/study-peer/src/native.rs:172:32–172:33 | 5795..5796 | try_propagation | run | A2 |
| E | apps/study-peer/src/native.rs:174:17–174:22 | 5885..5890 | match | run | A2 |
| D | apps/study-peer/src/native.rs:180:64–180:65 | 6215..6216 | try_propagation | run | A2 |
| E | apps/study-peer/src/native.rs:186:29–186:34 | 6465..6470 | match | run | A2 |
| D | apps/study-peer/src/native.rs:192:59–192:60 | 6840..6841 | try_propagation | run | A2 |
| E | apps/study-peer/src/native.rs:194:26–194:31 | 6890..6895 | match | run | A2 |
| D | apps/study-peer/src/native.rs:195:72–195:73 | 6999..7000 | try_propagation | run | A2 |
| D | apps/study-peer/src/native.rs:196:79–196:80 | 7080..7081 | try_propagation | run | A2 |
| D | apps/study-peer/src/native.rs:205:51–205:52 | 7329..7330 | try_propagation | run | A2 |
| D | apps/study-peer/src/native.rs:205:52–205:53 | 7330..7331 | try_propagation | run | A2 |
| D | apps/study-peer/src/native.rs:206:50–206:51 | 7382..7383 | try_propagation | run | A2 |
| E | apps/study-peer/src/native/protocol.rs:22:21–22:26 | 638..643 | match | new | A1 |
| P | apps/study-peer/src/native/protocol.rs:27:9–27:11 | 805..807 | if | new | A1 |
| D | apps/study-peer/src/native/protocol.rs:34:26–34:27 | 1045..1046 | try_propagation | parse | A1 |
| P | apps/study-peer/src/native/protocol.rs:35:9–35:11 | 1056..1058 | if | parse | A1 |
| C | apps/study-peer/src/native/protocol.rs:35:12–35:72 | 1059..1119 | short_circuit_or | parse | A1 |
| D | apps/study-peer/src/native/protocol.rs:38:30–38:31 | 1239..1240 | try_propagation | parse | A1 |
| I | apps/study-peer/src/native/protocol.rs:38:61–38:62 | 1270..1271 | try_propagation | parse | A1 |
| E | apps/study-peer/src/native/protocol.rs:53:20–53:25 | 1579..1584 | match | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:56:51–56:52 | 1707..1708 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:67:42–67:43 | 2164..2165 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:70:15–70:16 | 2289..2290 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:74:31–74:32 | 2434..2435 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:75:32–75:33 | 2468..2469 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:76:29–76:30 | 2499..2500 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:77:30–77:31 | 2531..2532 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:80:47–80:48 | 2638..2639 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:81:65–81:66 | 2705..2706 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:82:54–82:55 | 2761..2762 | try_propagation | command | A1 |
| D | apps/study-peer/src/native/protocol.rs:84:15–84:16 | 2835..2836 | try_propagation | command | A1 |
| E | apps/study-peer/src/native/protocol.rs:97:5–97:8 | 3275..3278 | for_iteration | edit | A1 |
| P | apps/study-peer/src/native/protocol.rs:99:9–99:11 | 3343..3345 | if | edit | A1 |
| D | apps/study-peer/src/native/tls.rs:27:73–27:74 | 777..778 | try_propagation | generate | A2 |
| D | apps/study-peer/src/native/tls.rs:35:37–35:38 | 1025..1026 | try_propagation | create | A2 |
| D | apps/study-peer/src/native/tls.rs:40:58–40:59 | 1230..1231 | try_propagation | create | A2 |
| D | apps/study-peer/src/native/tls.rs:44:28–44:29 | 1325..1326 | try_propagation | create | A2 |
| D | apps/study-peer/src/native/tls.rs:47:39–47:40 | 1420..1421 | try_propagation | create | A2 |
| D | apps/study-peer/src/native/tls.rs:48:47–48:48 | 1468..1469 | try_propagation | create | A2 |
| D | apps/study-peer/src/native/tls.rs:49:61–49:62 | 1531..1532 | try_propagation | create | A2 |
| D | apps/study-peer/src/native/tls.rs:55:49–55:50 | 1678..1679 | try_propagation | load | A2 |
| D | apps/study-peer/src/native/tls.rs:56:72–56:73 | 1759..1760 | try_propagation | load | A2 |
| P | apps/study-peer/src/native/tls.rs:71:9–71:11 | 2103..2105 | if | new | A2 |
| D | apps/study-peer/src/native/tls.rs:75:32–75:33 | 2290..2291 | try_propagation | new | A2 |
| D | apps/study-peer/src/native/tls.rs:78:84–78:85 | 2472..2473 | try_propagation | new | A2 |
| D | apps/study-peer/src/native/tls.rs:79:78–79:79 | 2552..2553 | try_propagation | new | A2 |
| D | apps/study-peer/src/native/tls.rs:82:79–82:80 | 2727..2728 | try_propagation | new | A2 |
| D | apps/study-peer/src/native/tls.rs:94:91–94:92 | 3089..3090 | try_propagation | connect | A2 |
| D | apps/study-peer/src/native/tls.rs:95:53–95:54 | 3145..3146 | try_propagation | connect | A2 |
| D | apps/study-peer/src/native/tls.rs:96:71–96:72 | 3218..3219 | try_propagation | connect | A2 |
| D | apps/study-peer/src/native/tls.rs:97:37–97:38 | 3257..3258 | try_propagation | connect | A2 |
| D | apps/study-peer/src/native/tls.rs:98:48–98:49 | 3307..3308 | try_propagation | connect | A2 |
| D | apps/study-peer/src/native/tls.rs:103:38–103:39 | 3472..3473 | try_propagation | accept | A2 |
| D | apps/study-peer/src/native/tls.rs:105:65–105:66 | 3593..3594 | try_propagation | accept | A2 |
| D | apps/study-peer/src/native/tls.rs:106:37–106:38 | 3632..3633 | try_propagation | accept | A2 |
| D | apps/study-peer/src/native/tls.rs:107:48–107:49 | 3682..3683 | try_propagation | accept | A2 |
| P | apps/study-peer/src/native/tls.rs:112:9–112:11 | 3823..3825 | if | check_pin | A2 |
| P | apps/study-peer/src/native/tls.rs:135:9–135:11 | 4403..4405 | if | remaining | A2 |
| D | apps/study-peer/src/native/tls.rs:145:59–145:60 | 4689..4690 | try_propagation | read | A2 |
| D | apps/study-peer/src/native/tls.rs:145:62–145:63 | 4692..4693 | try_propagation | read | A2 |
| D | apps/study-peer/src/native/tls.rs:152:60–152:61 | 4884..4885 | try_propagation | write | A2 |
| D | apps/study-peer/src/native/tls.rs:152:63–152:64 | 4887..4888 | try_propagation | write | A2 |
| E | apps/write-coordinator/src/main.rs:7:5–7:10 | 262..267 | match | main | A4 |
| E | apps/write-coordinator/src/native.rs:29:5–29:10 | 946..951 | match | run | A4 |
| E | apps/write-coordinator/src/native.rs:30:22–30:60 | 991..1029 | match_guard | run | A4 |
| C | apps/write-coordinator/src/native.rs:30:22–30:60 | 991..1029 | short_circuit_or | run | A4 |
| E | apps/write-coordinator/src/native.rs:34:28–34:45 | 1124..1141 | match_guard | run | A4 |
| D | apps/write-coordinator/src/native.rs:34:79–34:80 | 1175..1176 | try_propagation | run | A4 |
| E | apps/write-coordinator/src/native.rs:35:34–35:52 | 1212..1230 | match_guard | run | A4 |
| D | apps/write-coordinator/src/native.rs:37:49–37:50 | 1345..1346 | try_propagation | run | A4 |
| E | apps/write-coordinator/src/native.rs:39:28–39:47 | 1391..1410 | match_guard | run | A4 |
| D | apps/write-coordinator/src/native.rs:39:71–39:72 | 1434..1435 | try_propagation | run | A4 |
| E | apps/write-coordinator/src/native.rs:40:48–40:93 | 1491..1536 | match_guard | run | A4 |
| C | apps/write-coordinator/src/native.rs:40:48–40:93 | 1491..1536 | short_circuit_and | run | A4 |
| D | apps/write-coordinator/src/native.rs:43:30–43:31 | 1623..1624 | try_propagation | run | A4 |
| D | apps/write-coordinator/src/native.rs:44:53–44:54 | 1678..1679 | try_propagation | run | A4 |
| E | apps/write-coordinator/src/native.rs:47:42–47:88 | 1747..1793 | match_guard | run | A4 |
| C | apps/write-coordinator/src/native.rs:47:42–47:88 | 1747..1793 | short_circuit_and | run | A4 |
| D | apps/write-coordinator/src/native.rs:48:33–48:34 | 1831..1832 | try_propagation | run | A4 |
| D | apps/write-coordinator/src/native.rs:48:72–48:73 | 1870..1871 | try_propagation | run | A4 |
| D | apps/write-coordinator/src/native.rs:57:45–57:46 | 2054..2055 | try_propagation | load | A3 |
| D | apps/write-coordinator/src/native.rs:58:39–58:40 | 2095..2096 | try_propagation | load | A3 |
| P | apps/write-coordinator/src/native.rs:59:9–59:11 | 2106..2108 | if | load | A3 |
| P | apps/write-coordinator/src/native.rs:65:13–65:15 | 2299..2301 | if | load | A3 |
| D | apps/write-coordinator/src/native.rs:70:46–70:47 | 2535..2536 | try_propagation | load | A3 |
| P | apps/write-coordinator/src/native.rs:71:9–71:11 | 2546..2548 | if | load | A3 |
| P | apps/write-coordinator/src/native.rs:74:9–74:11 | 2623..2625 | if | load | A3 |
| C | apps/write-coordinator/src/native.rs:74:12–74:73 | 2626..2687 | short_circuit_or | load | A3 |
| D | apps/write-coordinator/src/native.rs:87:41–87:42 | 3082..3083 | try_propagation | address | A3 |
| D | apps/write-coordinator/src/native.rs:88:24–88:25 | 3107..3108 | try_propagation | address | A3 |
| D | apps/write-coordinator/src/native.rs:93:60–93:61 | 3271..3272 | try_propagation | initialize | A3 |
| D | apps/write-coordinator/src/native.rs:94:44–94:45 | 3317..3318 | try_propagation | initialize | A3 |
| D | apps/write-coordinator/src/native.rs:97:44–97:45 | 3441..3442 | try_propagation | initialize | A3 |
| D | apps/write-coordinator/src/native.rs:97:46–97:47 | 3443..3444 | try_propagation | initialize | A3 |
| D | apps/write-coordinator/src/native.rs:98:16–98:17 | 3461..3462 | try_propagation | initialize | A3 |
| D | apps/write-coordinator/src/native.rs:103:43–103:44 | 3573..3574 | try_propagation | configure | A3 |
| D | apps/write-coordinator/src/native.rs:112:97–112:98 | 3955..3956 | try_propagation | read_line | A3 |
| D | apps/write-coordinator/src/native.rs:118:62–118:63 | 4164..4165 | try_propagation | serve | A3 |
| D | apps/write-coordinator/src/native.rs:119:15–119:16 | 4181..4182 | try_propagation | serve | A3 |
| D | apps/write-coordinator/src/native.rs:120:46–120:47 | 4229..4230 | try_propagation | serve | A3 |
| D | apps/write-coordinator/src/native.rs:121:70–121:71 | 4301..4302 | try_propagation | serve | A3 |
| E | apps/write-coordinator/src/native.rs:122:5–122:8 | 4308..4311 | for_iteration | serve | A3 |
| D | apps/write-coordinator/src/native.rs:123:32–123:33 | 4375..4376 | try_propagation | serve | A3 |
| D | apps/write-coordinator/src/native.rs:125:31–125:32 | 4453..4454 | try_propagation | serve | A3 |
| D | apps/write-coordinator/src/native.rs:126:45–126:46 | 4500..4501 | try_propagation | serve | A3 |
| E | apps/write-coordinator/src/native.rs:127:27–127:32 | 4529..4534 | match | serve | A3 |
| E | apps/write-coordinator/src/native.rs:131:25–131:30 | 4692..4697 | match | serve | A3 |
| P | apps/write-coordinator/src/native.rs:139:9–139:11 | 4989..4991 | if | serve | A3 |
| C | apps/write-coordinator/src/native.rs:139:12–139:15 | 4992..4995 | pattern_test | serve | A3 |
| E | apps/write-coordinator/src/native.rs:147:5–147:8 | 5179..5182 | for_iteration | client | A3 |
| D | apps/write-coordinator/src/native.rs:148:24–148:25 | 5243..5244 | try_propagation | client | A3 |
| P | apps/write-coordinator/src/native.rs:150:9–150:11 | 5289..5291 | if | client | A3 |
| P | apps/write-coordinator/src/native.rs:153:9–153:11 | 5349..5351 | if | client | A3 |
| D | apps/write-coordinator/src/native.rs:156:71–156:72 | 5475..5476 | try_propagation | client | A3 |
| D | apps/write-coordinator/src/native.rs:157:27–157:28 | 5504..5505 | try_propagation | client | A3 |
| P | apps/write-coordinator/src/native.rs:158:9–158:11 | 5515..5517 | if | client | A3 |
| C | apps/write-coordinator/src/native.rs:158:12–158:15 | 5518..5521 | pattern_test | client | A3 |
| D | apps/write-coordinator/src/native.rs:159:83–159:84 | 5629..5630 | try_propagation | client | A3 |
| D | apps/write-coordinator/src/native.rs:161:42–161:43 | 5690..5691 | try_propagation | client | A3 |
| D | apps/write-coordinator/src/native.rs:163:39–163:40 | 5741..5742 | try_propagation | client | A3 |
| D | apps/write-coordinator/src/native.rs:164:49–164:50 | 5792..5793 | try_propagation | client | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:9:61–9:62 | 383..384 | try_propagation | authorize | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:9:78–9:79 | 400..401 | try_propagation | authorize | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:10:5–10:7 | 407..409 | if | authorize | A3 |
| E | apps/write-coordinator/src/native/protocol.rs:25:5–25:10 | 833..838 | while | read_line_from | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:26:22–26:23 | 885..886 | try_propagation | read_line_from | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:28:9–28:11 | 924..926 | if | read_line_from | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:28:34–28:35 | 949..950 | try_propagation | read_line_from | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:31:9–31:11 | 995..997 | if | read_line_from | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:46:48–46:49 | 1470..1471 | try_propagation | state | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:47:32–47:33 | 1503..1504 | try_propagation | state | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:48:18–48:19 | 1522..1523 | try_propagation | state | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:53:5–53:7 | 1643..1645 | if | handle | A3 |
| E | apps/write-coordinator/src/native/protocol.rs:57:5–57:10 | 1785..1790 | match | handle | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:59:46–59:47 | 1876..1877 | try_propagation | handle | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:63:79–63:80 | 2057..2058 | try_propagation | handle | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:64:73–64:74 | 2132..2133 | try_propagation | handle | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:67:48–67:49 | 2345..2346 | try_propagation | handle | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:68:13–68:15 | 2360..2362 | if | handle | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:72:74–72:75 | 2579..2580 | try_propagation | handle | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:72:76–72:77 | 2581..2582 | try_propagation | handle | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:76:79–76:80 | 2771..2772 | try_propagation | handle | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:77:73–77:74 | 2846..2847 | try_propagation | handle | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:85:5–85:7 | 3056..3058 | if | submit_identified | A3 |
| C | apps/write-coordinator/src/native/protocol.rs:85:8–86:25 | 3059..3097 | short_circuit_or | submit_identified | A3 |
| C | apps/write-coordinator/src/native/protocol.rs:85:8–89:74 | 3059..3207 | short_circuit_or | submit_identified | A3 |
| C | apps/write-coordinator/src/native/protocol.rs:89:22–89:60 | 3155..3193 | short_circuit_or | submit_identified | A3 |
| C | apps/write-coordinator/src/native/protocol.rs:89:22–89:73 | 3155..3206 | short_circuit_or | submit_identified | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:96:5–96:7 | 3432..3434 | if | submit_identified | A3 |
| C | apps/write-coordinator/src/native/protocol.rs:96:8–96:11 | 3435..3438 | pattern_test | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:96:62–96:63 | 3489..3490 | try_propagation | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:97:67–97:68 | 3559..3560 | try_propagation | submit_identified | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:98:9–98:11 | 3570..3572 | if | submit_identified | A3 |
| C | apps/write-coordinator/src/native/protocol.rs:98:12–98:46 | 3573..3607 | short_circuit_or | submit_identified | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:104:9–104:11 | 3918..3920 | if | submit_identified | A3 |
| C | apps/write-coordinator/src/native/protocol.rs:104:12–105:55 | 3921..4030 | short_circuit_or | submit_identified | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:109:9–109:11 | 4099..4101 | if | submit_identified | A3 |
| C | apps/write-coordinator/src/native/protocol.rs:109:12–109:68 | 4102..4158 | short_circuit_or | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:114:40–114:41 | 4312..4313 | try_propagation | submit_identified | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:115:5–115:7 | 4319..4321 | if | submit_identified | A3 |
| P | apps/write-coordinator/src/native/protocol.rs:120:5–120:7 | 4540..4542 | if | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:120:28–120:29 | 4563..4564 | try_propagation | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:123:71–123:72 | 4692..4693 | try_propagation | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:129:48–129:49 | 4979..4980 | try_propagation | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:129:50–129:51 | 4981..4982 | try_propagation | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:130:59–130:60 | 5042..5043 | try_propagation | submit_identified | A3 |
| D | apps/write-coordinator/src/native/protocol.rs:130:61–130:62 | 5044..5045 | try_propagation | submit_identified | A3 |
| P | examples/support/study.rs:34:9–34:11 | 1012..1014 | if | resolve_goal | A1 |

## Validation

On arm64 macOS, the current development tree passed
`CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/register-fuzz cargo test --offline
--locked -p study-peer -p write-coordinator` in debug and release: six study
unit/process tests, one study CLI test, five coordinator unit tests and four
coordinator process/CLI tests (16 total per profile). Nine are new in this
tranche. `cargo clippy` with the same package selection, `--all-targets -- -D
warnings`, passed. Logs: `/tmp/apps-decision-{debug,release,clippy}.log`.
The local source hashes above bind the ledger; this is not a frozen-release or
Windows-execution claim. No unsafe or runtime application changes were made.
