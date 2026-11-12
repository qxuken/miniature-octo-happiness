# Distributed application source decision audit

This audit classifies **222** explicit production AST records in the two
application entrypoints, native loops, protocols, shared TLS module (study target) and shared
StudyProgress support. These are demonstration applications, not a general
production coordination or replication service. Nine new tests exercise
admission, parsing, retries, authentication, deadlines and clean shutdown. That original tranche changed no runtime behavior; the shared TLS hardening
and coordinator credential admission extension below supersede that scope.

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
timeout elapsed. A stalled real TCP connection exercises the blocking handshake with a one-second
private deadline, bounded by a seven-second test assertion. Production uses the
fixed profile deadline (three seconds for study). rustls cryptography and certificate parsing remain
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
| `apps/study-peer/src/native.rs` | `82af7882059ea0a770aa19279f0564d48a65e07cd900db6abb98790b108122bf` |
| `apps/study-peer/src/native/protocol.rs` | `3a11026d333bf97ff105748dd38e98023720752bced98b2b12af2a251b55d73e` |
| `apps/study-peer/src/native/tls.rs` | `e16a32ddc4eea72c0497b2706a9640ea6959d02d8d60a374dcbb4f9c7be7bf05` |
| `apps/write-coordinator/src/main.rs` | `0baa2da45ecc666867adbf652fc95958edefe1ac0523c76d96934b69e7b5517b` |
| `apps/write-coordinator/src/native.rs` | `49d825369df34f61a1f316e5b156462d385478b1e5730fb59d3ba783166dda77` |
| `apps/write-coordinator/src/native/protocol.rs` | `544c5e161108c0c15d4c588ff2673b31d95ee12dcc318b615dccf864e573a858` |
| `examples/support/study.rs` | `d2a119e71a0863fa16ab2da04a8254e8fee9c6584448f2810edf80996522226d` |

Classification totals: C=22, D=131, E=34, I=3, P=32.

Spans are one-based line:column; byte offsets are end-exclusive.

| Status | Target | Source span | Bytes | AST kind | Owner | Scope |
|---|---|---|---|---|---|---|
| E | study-peer::study-peer | apps/study-peer/src/main.rs:6:5–6:10 | 181..186 | match | main | A4 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:43:5–43:10 | 1294..1299 | match | main | A4 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:44:22–44:60 | 1339..1377 | match_guard | main | A4 |
| C | study-peer::study-peer | apps/study-peer/src/native.rs:44:22–44:60 | 1339..1377 | short_circuit_or | main | A4 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:48:27–48:46 | 1471..1490 | match_guard | main | A4 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:49:39–49:56 | 1569..1586 | match_guard | main | A4 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:50:69–50:70 | 1660..1661 | try_propagation | main | A4 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:62:14–62:82 | 1876..1944 | match_guard | main | A2 |
| C | study-peer::study-peer | apps/study-peer/src/native.rs:62:14–62:82 | 1876..1944 | short_circuit_and | main | A2 |
| C | study-peer::study-peer | apps/study-peer/src/native.rs:62:35–62:81 | 1897..1943 | short_circuit_or | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:63:58–63:59 | 2007..2008 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:65:59–65:60 | 2100..2101 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:66:52–66:53 | 2154..2155 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:67:14–67:15 | 2177..2178 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:68:66–68:67 | 2245..2246 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:70:57–70:58 | 2340..2341 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:70:80–70:81 | 2363..2364 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:70:82–70:83 | 2365..2366 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:71:33–71:34 | 2400..2401 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:71:42–71:43 | 2409..2410 | try_propagation | main | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:83:60–83:61 | 2739..2740 | try_propagation | initialize | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:84:44–84:45 | 2785..2786 | try_propagation | initialize | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:86:79–86:80 | 2919..2920 | try_propagation | initialize | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:108:6–108:7 | 3379..3380 | try_propagation | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:114:35–114:36 | 3496..3497 | try_propagation | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:119:64–119:65 | 3734..3735 | try_propagation | run | A2 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:126:13–126:18 | 3929..3934 | while | run | A2 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:127:17–127:22 | 3983..3988 | match | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:130:87–130:88 | 4215..4216 | try_propagation | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:132:25–132:27 | 4270..4272 | if | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:132:28–132:31 | 4273..4276 | pattern_test | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:136:35–136:76 | 4446..4487 | match_guard | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:139:35–139:77 | 4622..4664 | match_guard | run | A2 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:147:13–147:18 | 4873..4878 | while | run | A2 |
| P | study-peer::study-peer | apps/study-peer/src/native.rs:148:17–148:19 | 4917..4919 | if | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:153:65–153:66 | 5160..5161 | try_propagation | run | A2 |
| P | study-peer::study-peer | apps/study-peer/src/native.rs:158:21–158:23 | 5341..5343 | if | run | A2 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:171:13–171:16 | 5761..5764 | for_iteration | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:172:32–172:33 | 5833..5834 | try_propagation | run | A2 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:174:17–174:22 | 5923..5928 | match | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:180:64–180:65 | 6253..6254 | try_propagation | run | A2 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:186:29–186:34 | 6503..6508 | match | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:192:59–192:60 | 6878..6879 | try_propagation | run | A2 |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:194:26–194:31 | 6928..6933 | match | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:195:72–195:73 | 7037..7038 | try_propagation | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:196:79–196:80 | 7118..7119 | try_propagation | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:205:51–205:52 | 7367..7368 | try_propagation | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:205:52–205:53 | 7368..7369 | try_propagation | run | A2 |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:206:50–206:51 | 7420..7421 | try_propagation | run | A2 |
| E | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:22:21–22:26 | 638..643 | match | new | A1 |
| P | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:27:9–27:11 | 805..807 | if | new | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:34:26–34:27 | 1045..1046 | try_propagation | parse | A1 |
| P | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:35:9–35:11 | 1056..1058 | if | parse | A1 |
| C | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:35:12–35:72 | 1059..1119 | short_circuit_or | parse | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:38:30–38:31 | 1239..1240 | try_propagation | parse | A1 |
| I | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:38:61–38:62 | 1270..1271 | try_propagation | parse | A1 |
| E | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:53:20–53:25 | 1579..1584 | match | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:56:51–56:52 | 1707..1708 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:67:42–67:43 | 2164..2165 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:70:15–70:16 | 2289..2290 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:74:31–74:32 | 2434..2435 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:75:32–75:33 | 2468..2469 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:76:29–76:30 | 2499..2500 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:77:30–77:31 | 2531..2532 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:80:47–80:48 | 2638..2639 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:81:65–81:66 | 2705..2706 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:82:54–82:55 | 2761..2762 | try_propagation | command | A1 |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:84:15–84:16 | 2835..2836 | try_propagation | command | A1 |
| E | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:97:5–97:8 | 3275..3278 | for_iteration | edit | A1 |
| P | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:99:9–99:11 | 3343..3345 | if | edit | A1 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:25:73–25:74 | 767..768 | try_propagation | generate | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:34:37–34:38 | 1032..1033 | try_propagation | create | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:37:54–37:55 | 1198..1199 | try_propagation | create | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:41:39–41:40 | 1318..1319 | try_propagation | create | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:42:47–42:48 | 1366..1367 | try_propagation | create | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:43:61–43:62 | 1429..1430 | try_propagation | create | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:54:61–54:62 | 1813..1814 | try_propagation | load | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:55:85–55:86 | 1907..1908 | try_propagation | load | A6 |
| P | study-peer::study-peer | apps/support/pinned_tls.rs:70:9–70:11 | 2251..2253 | if | new | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:74:32–74:33 | 2437..2438 | try_propagation | new | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:77:84–77:85 | 2623..2624 | try_propagation | new | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:78:78–78:79 | 2703..2704 | try_propagation | new | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:81:79–81:80 | 2882..2883 | try_propagation | new | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:96:67–96:68 | 3375..3376 | try_propagation | connect | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:104:53–104:54 | 3644..3645 | try_propagation | client_session | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:105:71–105:72 | 3717..3718 | try_propagation | client_session | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:106:37–106:38 | 3756..3757 | try_propagation | client_session | A6 |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:109:9–109:11 | 3933..3935 | if | client_session | A6 |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:109:12–109:53 | 3936..3977 | short_circuit_or | client_session | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:112:48–112:49 | 4096..4097 | try_propagation | client_session | A6 |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:113:9–113:11 | 4107..4109 | if | client_session | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:120:38–120:39 | 4361..4362 | try_propagation | accept | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:125:65–125:66 | 4574..4575 | try_propagation | server_session | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:126:37–126:38 | 4613..4614 | try_propagation | server_session | A6 |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:129:9–129:11 | 4790..4792 | if | server_session | A6 |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:129:12–129:53 | 4793..4834 | short_circuit_or | server_session | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:132:48–132:49 | 4953..4954 | try_propagation | server_session | A6 |
| E | study-peer::study-peer | apps/support/pinned_tls.rs:133:9–133:11 | 4964..4966 | if | server_session | A6 |
| P | study-peer::study-peer | apps/support/pinned_tls.rs:140:9–140:11 | 5194..5196 | if | check_pin | A6 |
| P | study-peer::study-peer | apps/support/pinned_tls.rs:163:9–163:11 | 5774..5776 | if | remaining | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:173:59–173:60 | 6060..6061 | try_propagation | read | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:173:62–173:63 | 6063..6064 | try_propagation | read | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:180:60–180:61 | 6255..6256 | try_propagation | write | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:180:63–180:64 | 6258..6259 | try_propagation | write | A6 |
| P | study-peer::study-peer | apps/support/pinned_tls.rs:195:5–195:7 | 6725..6727 | if | bounded | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:195:27–195:28 | 6747..6748 | try_propagation | bounded | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:198:36–198:37 | 6866..6867 | try_propagation | bounded | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:199:35–199:36 | 6903..6904 | try_propagation | bounded | A6 |
| I | study-peer::study-peer | apps/support/pinned_tls.rs:200:5–200:7 | 6910..6912 | if | bounded | A6 |
| P | study-peer::study-peer | apps/support/pinned_tls.rs:206:9–206:11 | 7083..7085 | if | bounded | A6 |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:206:12–206:65 | 7086..7139 | short_circuit_and | bounded | A6 |
| D | study-peer::study-peer | apps/support/pinned_tls.rs:213:56–213:57 | 7367..7368 | try_propagation | bounded | A6 |
| P | study-peer::study-peer | apps/support/pinned_tls.rs:214:5–214:7 | 7374..7376 | if | bounded | A6 |
| C | study-peer::study-peer | apps/support/pinned_tls.rs:214:8–214:47 | 7377..7416 | short_circuit_or | bounded | A6 |
| P | study-peer::study-peer | examples/support/study.rs:34:9–34:11 | 1012..1014 | if | resolve_goal | A1 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/main.rs:7:5–7:10 | 262..267 | match | main | A4 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:29:5–29:10 | 946..951 | match | run | A4 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:30:22–30:60 | 991..1029 | match_guard | run | A4 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:30:22–30:60 | 991..1029 | short_circuit_or | run | A4 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:34:28–34:45 | 1124..1141 | match_guard | run | A4 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:34:79–34:80 | 1175..1176 | try_propagation | run | A4 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:35:34–35:52 | 1212..1230 | match_guard | run | A4 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:37:49–37:50 | 1345..1346 | try_propagation | run | A4 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:39:28–39:47 | 1391..1410 | match_guard | run | A4 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:39:71–39:72 | 1434..1435 | try_propagation | run | A4 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:40:48–40:93 | 1491..1536 | match_guard | run | A4 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:40:48–40:93 | 1491..1536 | short_circuit_and | run | A4 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:43:30–43:31 | 1623..1624 | try_propagation | run | A4 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:44:53–44:54 | 1678..1679 | try_propagation | run | A4 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:47:42–47:88 | 1747..1793 | match_guard | run | A4 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:47:42–47:88 | 1747..1793 | short_circuit_and | run | A4 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:48:33–48:34 | 1831..1832 | try_propagation | run | A4 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:48:72–48:73 | 1870..1871 | try_propagation | run | A4 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:59:9–59:11 | 2171..2173 | if | load | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:59:36–59:37 | 2198..2199 | try_propagation | load | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:62:45–62:46 | 2330..2331 | try_propagation | load | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:63:39–63:40 | 2371..2372 | try_propagation | load | A3 |
| I | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:64:9–64:11 | 2382..2384 | if | load | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:70:13–70:15 | 2575..2577 | if | load | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:75:46–75:47 | 2811..2812 | try_propagation | load | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:76:9–76:11 | 2822..2824 | if | load | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:79:9–79:11 | 2899..2901 | if | load | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:79:12–79:73 | 2902..2963 | short_circuit_or | load | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:92:41–92:42 | 3358..3359 | try_propagation | address | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:93:24–93:25 | 3383..3384 | try_propagation | address | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:98:60–98:61 | 3547..3548 | try_propagation | initialize | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:99:44–99:45 | 3593..3594 | try_propagation | initialize | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:102:44–102:45 | 3717..3718 | try_propagation | initialize | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:102:46–102:47 | 3719..3720 | try_propagation | initialize | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:103:16–103:17 | 3737..3738 | try_propagation | initialize | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:108:43–108:44 | 3849..3850 | try_propagation | configure | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:117:97–117:98 | 4231..4232 | try_propagation | read_line | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:123:62–123:63 | 4440..4441 | try_propagation | serve | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:124:15–124:16 | 4457..4458 | try_propagation | serve | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:125:46–125:47 | 4505..4506 | try_propagation | serve | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:126:70–126:71 | 4577..4578 | try_propagation | serve | A3 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:127:5–127:8 | 4584..4587 | for_iteration | serve | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:128:32–128:33 | 4651..4652 | try_propagation | serve | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:130:31–130:32 | 4729..4730 | try_propagation | serve | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:131:45–131:46 | 4776..4777 | try_propagation | serve | A3 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:132:27–132:32 | 4805..4810 | match | serve | A3 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:136:25–136:30 | 4968..4973 | match | serve | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:144:9–144:11 | 5265..5267 | if | serve | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:144:12–144:15 | 5268..5271 | pattern_test | serve | A3 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:152:5–152:8 | 5455..5458 | for_iteration | client | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:153:24–153:25 | 5519..5520 | try_propagation | client | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:155:9–155:11 | 5565..5567 | if | client | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:158:9–158:11 | 5625..5627 | if | client | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:161:71–161:72 | 5751..5752 | try_propagation | client | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:162:27–162:28 | 5780..5781 | try_propagation | client | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:163:9–163:11 | 5791..5793 | if | client | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:163:12–163:15 | 5794..5797 | pattern_test | client | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:164:83–164:84 | 5905..5906 | try_propagation | client | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:166:42–166:43 | 5966..5967 | try_propagation | client | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:168:39–168:40 | 6017..6018 | try_propagation | client | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:169:49–169:50 | 6068..6069 | try_propagation | client | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:9:61–9:62 | 383..384 | try_propagation | authorize | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:9:78–9:79 | 400..401 | try_propagation | authorize | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:10:5–10:7 | 407..409 | if | authorize | A3 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:25:5–25:10 | 833..838 | while | read_line_from | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:26:22–26:23 | 885..886 | try_propagation | read_line_from | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:28:9–28:11 | 924..926 | if | read_line_from | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:28:34–28:35 | 949..950 | try_propagation | read_line_from | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:31:9–31:11 | 995..997 | if | read_line_from | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:46:48–46:49 | 1470..1471 | try_propagation | state | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:47:32–47:33 | 1503..1504 | try_propagation | state | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:48:18–48:19 | 1522..1523 | try_propagation | state | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:53:5–53:7 | 1643..1645 | if | handle | A3 |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:57:5–57:10 | 1785..1790 | match | handle | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:59:46–59:47 | 1876..1877 | try_propagation | handle | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:63:79–63:80 | 2057..2058 | try_propagation | handle | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:64:73–64:74 | 2132..2133 | try_propagation | handle | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:67:48–67:49 | 2345..2346 | try_propagation | handle | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:68:13–68:15 | 2360..2362 | if | handle | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:72:74–72:75 | 2579..2580 | try_propagation | handle | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:72:76–72:77 | 2581..2582 | try_propagation | handle | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:76:79–76:80 | 2771..2772 | try_propagation | handle | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:77:73–77:74 | 2846..2847 | try_propagation | handle | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:85:5–85:7 | 3056..3058 | if | submit_identified | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:85:8–86:25 | 3059..3097 | short_circuit_or | submit_identified | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:85:8–89:74 | 3059..3207 | short_circuit_or | submit_identified | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:89:22–89:60 | 3155..3193 | short_circuit_or | submit_identified | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:89:22–89:73 | 3155..3206 | short_circuit_or | submit_identified | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:96:5–96:7 | 3432..3434 | if | submit_identified | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:96:8–96:11 | 3435..3438 | pattern_test | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:96:62–96:63 | 3489..3490 | try_propagation | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:97:67–97:68 | 3559..3560 | try_propagation | submit_identified | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:98:9–98:11 | 3570..3572 | if | submit_identified | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:98:12–98:46 | 3573..3607 | short_circuit_or | submit_identified | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:104:9–104:11 | 3918..3920 | if | submit_identified | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:104:12–105:55 | 3921..4030 | short_circuit_or | submit_identified | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:109:9–109:11 | 4099..4101 | if | submit_identified | A3 |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:109:12–109:68 | 4102..4158 | short_circuit_or | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:114:40–114:41 | 4312..4313 | try_propagation | submit_identified | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:115:5–115:7 | 4319..4321 | if | submit_identified | A3 |
| P | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:120:5–120:7 | 4540..4542 | if | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:120:28–120:29 | 4563..4564 | try_propagation | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:123:71–123:72 | 4692..4693 | try_propagation | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:129:48–129:49 | 4979..4980 | try_propagation | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:129:50–129:51 | 4981..4982 | try_propagation | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:130:59–130:60 | 5042..5043 | try_propagation | submit_identified | A3 |
| D | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:130:61–130:62 | 5044..5045 | try_propagation | submit_identified | A3 |

## Historical validation (before shared TLS hardening)

On arm64 macOS, the original audit development tree passed
`CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/register-fuzz cargo test --offline
--locked -p study-peer -p write-coordinator` in debug and release: six study
unit/process tests, one study CLI test, five coordinator unit tests and four
coordinator process/CLI tests (16 total per profile). Nine are new in this
tranche. `cargo clippy` with the same package selection, `--all-targets -- -D
warnings`, passed. Logs: `/tmp/apps-decision-{debug,release,clippy}.log`.
The local source hashes above bind the ledger; this is not a frozen-release or
Windows-execution claim. No unsafe or runtime application changes were made.

Current shared source SHA-256: `apps/support/pinned_tls.rs` = `ebb0d4e90c1cfbf3e7bac61e9e3177f84b5ae778ab0807558861821d6ccb94b8`.

## A6: shared TLS binding and current qualification

The fixed profile compiles `apps/support/pinned_tls.rs` in this binary. The
44 shared records are inventoried separately for each target, not collapsed into
one coverage denominator. See [shared TLS evidence](../shared-app-tls.md) for
real partial-Finished, exact signed-leaf pin, required ALPN, file caps/FIFO/modes,
and absolute deadline controls. Pending-write-only and client partial-flight
independence remain open; cryptographic/OS errors remain delegated. Existing
manual dispositions are rebound to current spans; this is not measured MC/DC.

Mac debug/release and strict Clippy, and isolated aarch64 Docker Linux
debug/release and Clippy passed: study 14 and snapshot 19 tests each profile.
Mise Rust 1.99.1 / cargo-xwin 0.23.1 cross-linked five Windows test executables;
no Windows runtime claim. Logs `/tmp/shared-tls-{debug,release,clippy,linux,windows}.log`.
The earlier 2490631 qualification remains historical. Current source-set SHA-256:
`e656b7667508b2ab60acc92dc5c1c35ffb9328ef45c254cdfd9dcb0168cf7595`.

Coordinator pre-open credential admission adds two records. The new FIFO test
exercises both serve/client rejection before database open or input waiting,
preserving database bytes; existing regular-secret controls establish acceptance.
The new path metadata guard rejects FIFO/directory inputs. Under the stable
trusted configuration namespace, opened-handle nonregular rejection now requires
a namespace race outside that premise; its row is I, not credited to the directory
test. Metadata/open/read errors remain delegated. Coordinator qualification is
recorded separately by its owner; the study/snapshot counts above exclude it.

Current test bindings (shared module tests are bound by its full-file hash):

| Test source | SHA-256 |
|---|---|
| `apps/study-peer/src/native/tests.rs` | `8a70da75bf3208f74a025aa097d5b3912f238809e947c83f5c377aa9248b83d6` |
| `apps/study-peer/tests/cli.rs` | `6e3ad714e6102cb4545fcaa16e30e57734a2eb1bcc0e38c6c1388a3dca6d9f8d` |
| `apps/write-coordinator/tests/writers.rs` | `7a94250820b7b200e33dba685f44b7a94fd2bd7d3e7ee698266ce245197f5c7b` |

Coordinator FIFO extension qualification passed 5 unit and 5 integration tests
in both debug/release on macOS and Linux, plus strict macOS Clippy. This is
separate from the 33 study/snapshot tests above. The opened-handle type guard
remains a scoped invariant residual, not an exercised native namespace race.
