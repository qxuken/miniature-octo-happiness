# Authenticated snapshot CLI source decision audit

The application added at `315068f` contains **115 explicit production AST
records**: main 1, native 70 and TLS 44. This audit keeps that production source
unchanged and adds four tests. Exact baseline/current hashes and every source
span are below. The Windows lease-safe fixture correction at `b3761af` is
included in final test evidence; manifest bytes are inspected only after closing
the follower. No core protocol, durability barrier or acknowledgment changes.

P denotes directed predicate controls, E execution/dispatch evidence with scoped
residuals, C overlapping syntax, I an invariant-infeasible residual, and D
delegated storage/OS/dependency errors. Counts are not measured MC/DC conditions.
In particular E does not claim every composite operand has an independence pair.

## S1: admission, process configuration and retry

`cli_identity_option_and_permanent_retry_controls` accepts 32 ASCII hex digits,
rejects a short string and an exactly 32-character nonhex string independently,
rejects zero identity and non-UTF8 Unix arguments. Once the exact hex-length
check passes, parsing into u128 cannot overflow or contain an invalid digit;
that residual is I. Option controls cover default, exact endpoints 1/3, zero,
one-over, malformed integer and wrong flag. Existing CLI tests assert the
exact attempts 4 rejection and absence of a fresh destination file, rather than
accepting unrelated connection or existing-export errors as the oracle.
`cli_help_and_wrong_dispatch_shapes_have_controls` executes both help aliases
and wrong command/arity shapes. Existing process tests execute describe,
follower-init, ship, offline status, finite serve and poll. Keygen's helper is
tested directly; the CLI dispatch table is E, not exhaustive guard MC/DC.

Retry controls distinguish permanent=true/last=false (authenticated
UnsupportedFormat on attempt 1 of 2), permanent=false/last=true (failed transport
with one allowed attempt), and permanent=false/last=false (lost terminal reply,
then reconnect). Permanent errors return without exhausting the remaining
attempts. The same Prepared Export remains alive through retries. Attempts=0
cannot reach ship via validated CLI admission; the defensive loop fallback is
not an exposed CLI success path. OS scheduling, stderr failures and arbitrary
error-kind dispatch from dependencies remain delegated.

`authorized_install_reopen_poll_and_retry_release_primary_lease` captures
revision 4/value 2, reopens the primary and commits revision 5/value 3 while the
Export lives, then installs revision 4 and retained root 1 at revision 2/value 1.
This proves both lease release and immutable capture. Lost-reply tests reopen
the exact durable revision/digest, and duplicate retry does not extend the
manifest. CLI output separates local captured/verified revisions from follower
durability. Core recovery-required admission and all publication faults remain
owned by the core/follower suites, not re-injected at every CLI wrapper.

## S2: trusted chain is insufficient without the exact leaf

New `signed_alternate_leaf_is_rejected_after_chain_trust_on_both_endpoints`
creates a configured certificate/key and a different valid leaf signed by that
key, with the correct SAN. The configured certificate serves as rustls's trust
anchor. This is intentionally distinct from the existing unknown-CA tests.
The unchanged exact-peer control completes a real TLS snapshot installation at
revision 2. Presenting the signed alternate as client or server fails with the
specific **peer certificate does not match the configured pin** diagnostic on
the rejecting endpoint. That diagnostic is reached only after complete_io and
completed-handshake checks, proving the cryptographic chain was accepted first.
It is not inferred merely from any TLS error.

On alternate-client rejection, serve_one returns Authentication before core
is called. On alternate-server rejection, the sender never passes a channel
to Primary; the correctly authenticated receiver may wait for a request and
observe EOF. In both orientations the follower stays empty, its closed manifest
is byte-identical, no staging file appears, and reopening still reports None.
The test does not pretend that the receiver can know which server identity the
client intended to trust. Core read admission and remote peer verification
belong to their respective endpoints.

Existing tests reject unknown client/server identities and absent/wrong ALPN,
with no staging/manifest mutation. `exact_leaf_pin_controls_are_distinct_from_chain_trust`
covers absent/empty/wrong/exact chain inputs directly. Self-pin admission now
has an explicit rejection control. ALPN's failure can occur inside rustls
negotiation or in the post-handshake check; E classifications do not attribute
all failures to one specific comparison without its exact diagnostic.

## S3: completed handshakes and absolute socket deadlines

The existing fragmented real TLS test sends Certificate and CertificateVerify
with negotiated ALPN but withholds Finished. On tested Unix, complete_io returns
after partial progress with identity/ALPN available yet is_handshaking true;
the server rejects with the exact unfinished-handshake diagnostic. Normal
sessions control the completed/no-pending-write alternative. The OR's
wants_write-only rejection has not been independently forced through the
blocking DeadlineSocket path, nor has a partial-server-flight client control.
Those remain explicit execution residuals; no fabricated OS WouldBlock or
claim of full handshake-guard MC/DC is made. rustls's state machine, certificate
validation and cryptographic internals remain dependency assurance.

`absolute_deadline_is_not_renewed_by_progress` verifies real socket progress
does not change the stored deadline, then directly expires it and checks read
and write TimedOut. That clock manipulation proves local admission, not native
elapsed timeout. Client deadline construction precedes TCP connect; connect
failure and set-timeout syscall errors are delegated. Local filesystem
verification/flush may outlast the 30-second socket budget. Serial service and
--max-exchanges1 are exercised by real child processes; infinite unattended
service, adversarial admission load, fatal accept errors and recovery supervision
are not qualified by those tests.

## S4: credential type, size and permissions

Existing tests verify pre-open FIFO rejection, exact 64 KiB/public certificate
read boundary, one-over size diagnostics, key 16 KiB cap, and Unix 0700/0600
key generation without overwriting an existing directory. New
`self_pin_empty_file_and_public_private_mode_controls` checks empty input,
nonregular paths and independently controls private=true and broad-mode=true:
public 0644 reads, private 0644 rejects, private 0600 reads. The mode check uses the
opened handle before key bytes are read. Credentials remain trusted local
configuration. Under the documented stable trusted namespace, a regular path
checked before open stays regular on the opened handle; the second type guard's
rejecting outcome requires a namespace race outside that premise. It is marked
I under that premise, not claimed exercised by the FIFO test (which stops before
open). Post-open metadata errors remain D.

Windows keygen deliberately fails closed; external ACL protection remains the
operator's obligation. Local macOS/Linux tests do not establish Windows ACL
semantics. Process credential fixtures are isolated and never logged. Directory
creation, disk-full key writing, permission-setting failure and hostile namespace
mutation are not silently included in successful provisioning coverage.

## Inventory and functional limits

The three production files have the same decision kinds, byte spans and condition
hashes as 315068f; new code is cfg(test) only. The scanner excludes test modules,
uses a union of platform branches and does not expand formatting/raw macros,
derives, iterator combinators or dependency code. The app adds 115 records to the
2,415-record audit checkpoint, not a compiler-expanded denominator. Statement
execution, dependency error propagation and generated formatting are not full
MC/DC. Local configuration trust, fixed peer/enrollment, finite retries, serial
admission, certificate lifecycle and explicit restart/reseed remain as documented
in the operations runbook. No new runtime defect was reproduced by this audit.

| Production source | SHA-256 at 315068f | Current full-file SHA-256 (test-only additions) |
|---|---|---|
| `apps/weekend-replicate/src/main.rs` | `50e2b99c9900f03ff4f0847485156542f9bf5dbb4a500a47612f2e38355d5c41` | `50e2b99c9900f03ff4f0847485156542f9bf5dbb4a500a47612f2e38355d5c41` |
| `apps/weekend-replicate/src/native.rs` | `acf91c9a1cd1a72c2822c1c0c52435158c08dc23209ae3f6912c20e24e4dc56e` | `9c4510f36f5c09a1f6b4775dd3ac71da2d4717c38bd3bd3fb1cce628cbcd9390` |
| `apps/weekend-replicate/src/tls.rs` | `97e38a825d7c1046cf64a83ffd3c3ef502d73204b22b9e63cb38927e7d116852` | `ad6e3bd1e822b9d8317004c528c8a83973153919fcad9eb16ddccfd30e871e7e` |

Disposition totals: C=9, D=71, E=22, I=2, P=11.

Spans are one-based line:column; byte offsets are end-exclusive.

| Status | Source span | Bytes | AST kind | Owner | Scope |
|---|---|---|---|---|---|
| E | apps/weekend-replicate/src/main.rs:5:5–5:10 | 180..185 | match | main | S1 |
| D | apps/weekend-replicate/src/native.rs:31:20–31:21 | 1427..1428 | try_propagation | identity | S1 |
| P | apps/weekend-replicate/src/native.rs:32:5–32:7 | 1434..1436 | if | identity | S1 |
| C | apps/weekend-replicate/src/native.rs:32:8–32:66 | 1437..1495 | short_circuit_or | identity | S1 |
| I | apps/weekend-replicate/src/native.rs:35:41–35:42 | 1613..1614 | try_propagation | identity | S1 |
| P | apps/weekend-replicate/src/native.rs:36:5–36:7 | 1634..1636 | if | identity | S1 |
| D | apps/weekend-replicate/src/native.rs:43:36–43:37 | 1853..1854 | try_propagation | enrollment | S1 |
| D | apps/weekend-replicate/src/native.rs:44:38–44:39 | 1893..1894 | try_propagation | enrollment | S1 |
| E | apps/weekend-replicate/src/native.rs:51:5–51:10 | 2048..2053 | match | ack_line | S1 |
| D | apps/weekend-replicate/src/native.rs:63:43–63:44 | 2409..2410 | try_propagation | transport | S1 |
| D | apps/weekend-replicate/src/native.rs:64:44–64:45 | 2455..2456 | try_propagation | transport | S1 |
| E | apps/weekend-replicate/src/native.rs:73:5–73:10 | 2601..2606 | match | option | S1 |
| P | apps/weekend-replicate/src/native.rs:75:26–75:38 | 2666..2678 | match_guard | option | S1 |
| D | apps/weekend-replicate/src/native.rs:76:32–76:33 | 2715..2716 | try_propagation | option | S1 |
| D | apps/weekend-replicate/src/native.rs:76:50–76:51 | 2733..2734 | try_propagation | option | S1 |
| P | apps/weekend-replicate/src/native.rs:77:13–77:15 | 2748..2750 | if | option | S1 |
| C | apps/weekend-replicate/src/native.rs:77:16–77:37 | 2751..2772 | short_circuit_or | option | S1 |
| E | apps/weekend-replicate/src/native.rs:88:5–88:10 | 3040..3045 | match | main | S1 |
| E | apps/weekend-replicate/src/native.rs:89:16–89:42 | 3079..3105 | match_guard | main | S1 |
| C | apps/weekend-replicate/src/native.rs:89:16–89:42 | 3079..3105 | short_circuit_or | main | S1 |
| E | apps/weekend-replicate/src/native.rs:93:21–93:34 | 3193..3206 | match_guard | main | S1 |
| E | apps/weekend-replicate/src/native.rs:94:32–94:47 | 3278..3293 | match_guard | main | S1 |
| D | apps/weekend-replicate/src/native.rs:95:42–95:43 | 3340..3341 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:96:84–96:85 | 3426..3427 | try_propagation | main | S1 |
| E | apps/weekend-replicate/src/native.rs:110:35–110:55 | 3869..3889 | match_guard | main | S1 |
| D | apps/weekend-replicate/src/native.rs:111:45–111:46 | 3939..3940 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:112:64–112:65 | 4005..4006 | try_propagation | main | S1 |
| E | apps/weekend-replicate/src/native.rs:116:35–116:48 | 4124..4137 | match_guard | main | S1 |
| D | apps/weekend-replicate/src/native.rs:117:65–117:66 | 4207..4208 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:117:86–117:87 | 4228..4229 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:118:31–118:32 | 4261..4262 | try_propagation | main | S1 |
| E | apps/weekend-replicate/src/native.rs:135:14–135:26 | 4768..4780 | match_guard | main | S1 |
| D | apps/weekend-replicate/src/native.rs:136:45–136:46 | 4830..4831 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:137:40–137:41 | 4872..4873 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:137:49–137:50 | 4881..4882 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:138:81–138:82 | 4964..4965 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:139:50–139:51 | 5016..5017 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:140:69–140:70 | 5087..5088 | try_propagation | main | S1 |
| E | apps/weekend-replicate/src/native.rs:152:14–152:25 | 5335..5346 | match_guard | main | S1 |
| D | apps/weekend-replicate/src/native.rs:153:42–153:43 | 5393..5394 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:154:40–154:41 | 5435..5436 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:154:49–154:50 | 5444..5445 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:155:63–155:64 | 5509..5510 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:156:50–156:51 | 5561..5562 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:157:81–157:82 | 5644..5645 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:164:33–164:34 | 5920..5921 | try_propagation | main | S1 |
| E | apps/weekend-replicate/src/native.rs:165:13–165:18 | 5935..5940 | match | main | S1 |
| E | apps/weekend-replicate/src/native.rs:170:59–170:70 | 6388..6399 | match_guard | main | S1 |
| D | apps/weekend-replicate/src/native.rs:171:42–171:43 | 6446..6447 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:172:40–172:41 | 6488..6489 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:172:49–172:50 | 6497..6498 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:173:50–173:51 | 6549..6550 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:174:68–174:69 | 6619..6620 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:176:56–176:57 | 6730..6731 | try_propagation | main | S1 |
| D | apps/weekend-replicate/src/native.rs:191:64–191:65 | 7193..7194 | try_propagation | prepare | S1 |
| D | apps/weekend-replicate/src/native.rs:192:48–192:49 | 7243..7244 | try_propagation | prepare | S1 |
| D | apps/weekend-replicate/src/native.rs:193:59–193:60 | 7304..7305 | try_propagation | prepare | S1 |
| E | apps/weekend-replicate/src/native.rs:208:5–208:8 | 7569..7572 | for_iteration | ship | S1 |
| D | apps/weekend-replicate/src/native.rs:211:88–211:89 | 7740..7741 | try_propagation | ship | S1 |
| E | apps/weekend-replicate/src/native.rs:212:9–212:14 | 7753..7758 | match | ship | S1 |
| P | apps/weekend-replicate/src/native.rs:227:17–227:19 | 8362..8364 | if | ship | S1 |
| C | apps/weekend-replicate/src/native.rs:227:20–227:56 | 8365..8401 | short_circuit_or | ship | S1 |
| E | apps/weekend-replicate/src/native.rs:247:9–247:14 | 9019..9024 | match | fmt | S1 |
| D | apps/weekend-replicate/src/native.rs:255:75–255:76 | 9415..9416 | try_propagation | serve_one | S1 |
| D | apps/weekend-replicate/src/native.rs:261:46–261:47 | 9660..9661 | try_propagation | serve | S1 |
| D | apps/weekend-replicate/src/native.rs:263:25–263:26 | 9737..9738 | try_propagation | serve | S1 |
| E | apps/weekend-replicate/src/native.rs:264:5–264:8 | 9744..9747 | for_iteration | serve | S1 |
| D | apps/weekend-replicate/src/native.rs:265:30–265:31 | 9825..9826 | try_propagation | serve | S1 |
| E | apps/weekend-replicate/src/native.rs:266:9–266:11 | 9836..9838 | if | serve | S1 |
| C | apps/weekend-replicate/src/native.rs:266:12–266:15 | 9839..9842 | pattern_test | serve | S1 |
| D | apps/weekend-replicate/src/native.rs:270:28–270:29 | 10054..10055 | try_propagation | serve | S1 |
| D | apps/weekend-replicate/src/tls.rs:29:73–29:74 | 878..879 | try_propagation | generate | S4 |
| D | apps/weekend-replicate/src/tls.rs:38:37–38:38 | 1143..1144 | try_propagation | create | S4 |
| D | apps/weekend-replicate/src/tls.rs:41:54–41:55 | 1309..1310 | try_propagation | create | S4 |
| D | apps/weekend-replicate/src/tls.rs:45:39–45:40 | 1429..1430 | try_propagation | create | S4 |
| D | apps/weekend-replicate/src/tls.rs:46:47–46:48 | 1477..1478 | try_propagation | create | S4 |
| D | apps/weekend-replicate/src/tls.rs:47:61–47:62 | 1540..1541 | try_propagation | create | S4 |
| D | apps/weekend-replicate/src/tls.rs:58:61–58:62 | 1924..1925 | try_propagation | load | S4 |
| D | apps/weekend-replicate/src/tls.rs:59:85–59:86 | 2018..2019 | try_propagation | load | S4 |
| P | apps/weekend-replicate/src/tls.rs:74:9–74:11 | 2362..2364 | if | new | S2 |
| D | apps/weekend-replicate/src/tls.rs:78:32–78:33 | 2548..2549 | try_propagation | new | S2 |
| D | apps/weekend-replicate/src/tls.rs:81:84–81:85 | 2734..2735 | try_propagation | new | S2 |
| D | apps/weekend-replicate/src/tls.rs:82:78–82:79 | 2814..2815 | try_propagation | new | S2 |
| D | apps/weekend-replicate/src/tls.rs:85:79–85:80 | 2993..2994 | try_propagation | new | S2 |
| D | apps/weekend-replicate/src/tls.rs:100:67–100:68 | 3486..3487 | try_propagation | connect | S3 |
| D | apps/weekend-replicate/src/tls.rs:108:53–108:54 | 3755..3756 | try_propagation | client_session | S3 |
| D | apps/weekend-replicate/src/tls.rs:109:71–109:72 | 3828..3829 | try_propagation | client_session | S3 |
| D | apps/weekend-replicate/src/tls.rs:110:37–110:38 | 3867..3868 | try_propagation | client_session | S3 |
| E | apps/weekend-replicate/src/tls.rs:113:9–113:11 | 4044..4046 | if | client_session | S3 |
| C | apps/weekend-replicate/src/tls.rs:113:12–113:53 | 4047..4088 | short_circuit_or | client_session | S3 |
| D | apps/weekend-replicate/src/tls.rs:116:48–116:49 | 4207..4208 | try_propagation | client_session | S3 |
| E | apps/weekend-replicate/src/tls.rs:117:9–117:11 | 4218..4220 | if | client_session | S3 |
| D | apps/weekend-replicate/src/tls.rs:124:38–124:39 | 4498..4499 | try_propagation | accept | S3 |
| D | apps/weekend-replicate/src/tls.rs:129:65–129:66 | 4711..4712 | try_propagation | server_session | S3 |
| D | apps/weekend-replicate/src/tls.rs:130:37–130:38 | 4750..4751 | try_propagation | server_session | S3 |
| E | apps/weekend-replicate/src/tls.rs:133:9–133:11 | 4927..4929 | if | server_session | S3 |
| C | apps/weekend-replicate/src/tls.rs:133:12–133:53 | 4930..4971 | short_circuit_or | server_session | S3 |
| D | apps/weekend-replicate/src/tls.rs:136:48–136:49 | 5090..5091 | try_propagation | server_session | S3 |
| E | apps/weekend-replicate/src/tls.rs:137:9–137:11 | 5101..5103 | if | server_session | S3 |
| P | apps/weekend-replicate/src/tls.rs:144:9–144:11 | 5357..5359 | if | check_pin | S2 |
| P | apps/weekend-replicate/src/tls.rs:167:9–167:11 | 5937..5939 | if | remaining | S3 |
| D | apps/weekend-replicate/src/tls.rs:177:59–177:60 | 6223..6224 | try_propagation | read | S3 |
| D | apps/weekend-replicate/src/tls.rs:177:62–177:63 | 6226..6227 | try_propagation | read | S3 |
| D | apps/weekend-replicate/src/tls.rs:184:60–184:61 | 6418..6419 | try_propagation | write | S3 |
| D | apps/weekend-replicate/src/tls.rs:184:63–184:64 | 6421..6422 | try_propagation | write | S3 |
| P | apps/weekend-replicate/src/tls.rs:199:5–199:7 | 6888..6890 | if | bounded | S4 |
| D | apps/weekend-replicate/src/tls.rs:199:27–199:28 | 6910..6911 | try_propagation | bounded | S4 |
| D | apps/weekend-replicate/src/tls.rs:202:36–202:37 | 7029..7030 | try_propagation | bounded | S4 |
| D | apps/weekend-replicate/src/tls.rs:203:35–203:36 | 7066..7067 | try_propagation | bounded | S4 |
| I | apps/weekend-replicate/src/tls.rs:204:5–204:7 | 7073..7075 | if | bounded | S4 |
| P | apps/weekend-replicate/src/tls.rs:210:9–210:11 | 7246..7248 | if | bounded | S4 |
| C | apps/weekend-replicate/src/tls.rs:210:12–210:65 | 7249..7302 | short_circuit_and | bounded | S4 |
| D | apps/weekend-replicate/src/tls.rs:217:56–217:57 | 7530..7531 | try_propagation | bounded | S4 |
| P | apps/weekend-replicate/src/tls.rs:218:5–218:7 | 7537..7539 | if | bounded | S4 |
| C | apps/weekend-replicate/src/tls.rs:218:8–218:47 | 7540..7579 | short_circuit_or | bounded | S4 |

## Test bindings and results

| Test source | SHA-256 |
|---|---|
| `apps/weekend-replicate/src/native/pin_tests.rs` | `713487422c3e0343a25730eb8681b75d37fda6d2031e9d17921791b732ed35bc` |
| `apps/weekend-replicate/src/native/tests.rs` | `23ea109bba8ee38c6550b71c39494b7b95929bbf5fd9f564ad8570b05079fb86` |
| `apps/weekend-replicate/tests/audit_cli.rs` | `48e2b3cb7a742a7dfc4b24a7e51499cb3a4ae6e74ee050127a99a076089ee3bd` |
| `apps/weekend-replicate/tests/cli.rs` | `b3933aa7ba266d7d28b2fe9001eb6f94f757828fbc72a29a4dec956055d3f95c` |

macOS arm64 debug and release passed 12 unit/native/TLS tests plus two CLI
integration tests; strict all-target Clippy passed. The targeted aarch64 Linux
Docker debug run also passed all 14 tests using rustc 1.99.1 and an isolated source
copy/build directory. Linux release/Clippy and Windows were not run in this
tranche. The new real TLS leaf-pin case passed on both tested platforms. Tests
include b3761af's Windows-sharing-safe fixture shape, without claiming a new
Windows execution result.

Logs: `/tmp/snapshot-cli-audit-{debug,release,clippy}.log`,
`/tmp/snapshot-cli-audit-linux-final.log`; Linux app-source bindings are in
`/tmp/snapshot-cli-linux-hashes.json`. Production/test code is frozen after
these checks. No mutation campaign, sanitizer campaign, physical power-loss or
whole-project measured MC/DC result is claimed.

Fresh inventory source-set SHA-256: `695f67c71b81665b725555ac682d84eaae2994272683d2a73672187d7ef098cf`
(2,530 records, 50 files, 310 blind spots,zero parse failures).
