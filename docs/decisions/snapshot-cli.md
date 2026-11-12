# Authenticated snapshot CLI source decision audit

The application initially audited at `315068f` had 115 explicit production AST
records. Offline maintenance adds 10 records: the current **127 records** comprise
main 1, native 80 and shared TLS 46 under the snapshot target. The original audit added four tests; the maintenance
tranche extends the native CLI process fixture. Exact baseline/current hashes and every source
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

## S5: offline maintenance commands

The native CLI fixture invokes both commands as independent processes with no
open follower handles. Wrong enrollment returns exact `ForeignStore` without
changing the manifest, selected image or abandoned staging file. Cleanup removes
only the inactive reserved image, preserves an unrelated entry and selected bytes,
and a repeat reports zero removals. Reported follower acknowledgment remains the
exact revision 4/digest captured by the transfer. Selected slot is explicitly 0.

On macOS/Linux the test extends the manifest to its admission ceiling, observes
maintenance-required, compacts and reopens below the ceiling with maintenance
cleared. Current revision 4/value 2 and retained root 1 at revision 2/value 1
remain readable. Windows instead rejects compact-manifest before opening any
path; the cfg-gated fixture checks its exact diagnostic and unchanged manifest.
That branch is source-reviewed but was not executed in this local tranche.

The two new dispatch guards are E: positive/negative command controls do not
constitute complete pattern/arity MC/DC. The eight `?` records are D: native
reopen, enrollment validation, verification, deletion and atomic replacement
failures remain delegated to existing core suites. No syscall failure or crash
is synthesized at the wrapper. No core wire/barrier changes or fallback were
introduced. Maintenance output and follower durability remain separate lines;
cleanup does not promise durable namespace deletion. Compaction consumes its
handle and may lose a competing lease race; after an error reopen before serving.

## Inventory and functional limits

The original 115 production decisions retain their dispositions; offline
maintenance adds two dispatch guards and eight delegated error propagations.
Current source spans/hashes below supersede the original positions. The scanner excludes test modules,
uses a union of platform branches and does not expand formatting/raw macros,
derives, iterator combinators or dependency code. The app adds 125 records to the
2,415-record audit checkpoint, not a compiler-expanded denominator. Statement
execution, dependency error propagation and generated formatting are not full
MC/DC. Local configuration trust, fixed peer/enrollment, finite retries, serial
admission, certificate lifecycle and explicit restart/reseed remain as documented
in the operations runbook. No new runtime defect was reproduced by this audit.

| Production source | SHA-256 at 315068f | Current full-file SHA-256 (wrappers delegate to shared module) |
|---|---|---|
| `apps/weekend-replicate/src/main.rs` | `50e2b99c9900f03ff4f0847485156542f9bf5dbb4a500a47612f2e38355d5c41` | `50e2b99c9900f03ff4f0847485156542f9bf5dbb4a500a47612f2e38355d5c41` |
| `apps/weekend-replicate/src/native.rs` | `acf91c9a1cd1a72c2822c1c0c52435158c08dc23209ae3f6912c20e24e4dc56e` | `21a20d764825b5eaa081838c9d12af67d63746b72a93e8beaebb999ab2ac666b` |
| `apps/weekend-replicate/src/tls.rs` | `97e38a825d7c1046cf64a83ffd3c3ef502d73204b22b9e63cb38927e7d116852` | `4235b303681b2111f4397fbe1d36de813009fd75bc3a078b03e4ffa0fbad6215` |

Disposition totals: C=9, D=81, E=20, I=2, P=15.

Spans are one-based line:column; byte offsets are end-exclusive.

| Status | Target | Source span | Bytes | AST kind | Owner | Scope |
|---|---|---|---|---|---|---|
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:25:73–25:74 | 767..768 | try_propagation | generate | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:34:37–34:38 | 1032..1033 | try_propagation | create | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:37:54–37:55 | 1198..1199 | try_propagation | create | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:41:39–41:40 | 1318..1319 | try_propagation | create | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:42:47–42:48 | 1366..1367 | try_propagation | create | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:43:61–43:62 | 1429..1430 | try_propagation | create | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:54:61–54:62 | 1813..1814 | try_propagation | load | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:55:85–55:86 | 1907..1908 | try_propagation | load | S6 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:70:9–70:11 | 2251..2253 | if | new | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:74:32–74:33 | 2437..2438 | try_propagation | new | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:77:84–77:85 | 2623..2624 | try_propagation | new | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:78:78–78:79 | 2703..2704 | try_propagation | new | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:81:79–81:80 | 2882..2883 | try_propagation | new | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:96:67–96:68 | 3375..3376 | try_propagation | connect | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:104:53–104:54 | 3644..3645 | try_propagation | client_session | S7 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:105:71–105:72 | 3717..3718 | try_propagation | client_session | S7 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:106:53–106:54 | 3772..3773 | try_propagation | client_session | S7 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:115:32–115:33 | 3997..3998 | try_propagation | client_handshake | S7 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:118:9–118:11 | 4174..4176 | if | client_handshake | S7 |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:118:12–118:53 | 4177..4218 | short_circuit_or | client_handshake | S7 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:121:48–121:49 | 4337..4338 | try_propagation | client_handshake | S7 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:122:9–122:11 | 4348..4350 | if | client_handshake | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:129:38–129:39 | 4575..4576 | try_propagation | accept | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:134:65–134:66 | 4788..4789 | try_propagation | server_session | S7 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:135:53–135:54 | 4843..4844 | try_propagation | server_session | S7 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:144:32–144:33 | 5068..5069 | try_propagation | server_handshake | S7 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:147:9–147:11 | 5245..5247 | if | server_handshake | S7 |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:147:12–147:53 | 5248..5289 | short_circuit_or | server_handshake | S7 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:150:48–150:49 | 5408..5409 | try_propagation | server_handshake | S7 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:151:9–151:11 | 5419..5421 | if | server_handshake | S6 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:158:9–158:11 | 5622..5624 | if | check_pin | S6 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:181:9–181:11 | 6202..6204 | if | remaining | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:191:59–191:60 | 6488..6489 | try_propagation | read | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:191:62–191:63 | 6491..6492 | try_propagation | read | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:198:60–198:61 | 6683..6684 | try_propagation | write | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:198:63–198:64 | 6686..6687 | try_propagation | write | S6 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:213:5–213:7 | 7153..7155 | if | bounded | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:213:27–213:28 | 7175..7176 | try_propagation | bounded | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:216:36–216:37 | 7294..7295 | try_propagation | bounded | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:217:35–217:36 | 7331..7332 | try_propagation | bounded | S6 |
| I | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:218:5–218:7 | 7338..7340 | if | bounded | S6 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:224:9–224:11 | 7511..7513 | if | bounded | S6 |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:224:12–224:65 | 7514..7567 | short_circuit_and | bounded | S6 |
| D | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:231:56–231:57 | 7795..7796 | try_propagation | bounded | S6 |
| P | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:232:5–232:7 | 7802..7804 | if | bounded | S6 |
| C | weekend-replicate::weekend-replicate | apps/support/pinned_tls.rs:232:8–232:47 | 7805..7844 | short_circuit_or | bounded | S6 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/main.rs:5:5–5:10 | 180..185 | match | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:33:20–33:21 | 1593..1594 | try_propagation | identity | S1 |
| P | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:34:5–34:7 | 1600..1602 | if | identity | S1 |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:34:8–34:66 | 1603..1661 | short_circuit_or | identity | S1 |
| I | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:37:41–37:42 | 1779..1780 | try_propagation | identity | S1 |
| P | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:38:5–38:7 | 1800..1802 | if | identity | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:45:36–45:37 | 2019..2020 | try_propagation | enrollment | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:46:38–46:39 | 2059..2060 | try_propagation | enrollment | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:53:5–53:10 | 2214..2219 | match | ack_line | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:65:43–65:44 | 2575..2576 | try_propagation | transport | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:66:44–66:45 | 2621..2622 | try_propagation | transport | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:75:5–75:10 | 2767..2772 | match | option | S1 |
| P | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:77:26–77:38 | 2832..2844 | match_guard | option | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:78:32–78:33 | 2881..2882 | try_propagation | option | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:78:50–78:51 | 2899..2900 | try_propagation | option | S1 |
| P | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:79:13–79:15 | 2914..2916 | if | option | S1 |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:79:16–79:37 | 2917..2938 | short_circuit_or | option | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:90:5–90:10 | 3206..3211 | match | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:91:16–91:42 | 3245..3271 | match_guard | main | S1 |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:91:16–91:42 | 3245..3271 | short_circuit_or | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:95:21–95:34 | 3359..3372 | match_guard | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:96:32–96:47 | 3444..3459 | match_guard | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:97:42–97:43 | 3506..3507 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:98:84–98:85 | 3592..3593 | try_propagation | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:112:35–112:55 | 4035..4055 | match_guard | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:113:45–113:46 | 4105..4106 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:114:64–114:65 | 4171..4172 | try_propagation | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:118:35–118:48 | 4290..4303 | match_guard | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:119:65–119:66 | 4373..4374 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:119:86–119:87 | 4394..4395 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:120:31–120:32 | 4427..4428 | try_propagation | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:128:35–128:58 | 4786..4809 | match_guard | main | S5 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:129:69–129:70 | 4883..4884 | try_propagation | main | S5 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:129:90–129:91 | 4904..4905 | try_propagation | main | S5 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:130:47–130:48 | 4953..4954 | try_propagation | main | S5 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:135:35–135:58 | 5136..5159 | match_guard | main | S5 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:138:69–138:70 | 5313..5314 | try_propagation | main | S5 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:138:90–138:91 | 5334..5335 | try_propagation | main | S5 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:139:40–139:41 | 5376..5377 | try_propagation | main | S5 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:140:45–140:46 | 5438..5439 | try_propagation | main | S5 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:141:39–141:40 | 5479..5480 | try_propagation | main | S5 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:167:14–167:26 | 6258..6270 | match_guard | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:168:45–168:46 | 6320..6321 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:169:40–169:41 | 6362..6363 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:169:49–169:50 | 6371..6372 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:170:81–170:82 | 6454..6455 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:171:50–171:51 | 6506..6507 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:172:69–172:70 | 6577..6578 | try_propagation | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:184:14–184:25 | 6825..6836 | match_guard | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:185:42–185:43 | 6883..6884 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:186:40–186:41 | 6925..6926 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:186:49–186:50 | 6934..6935 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:187:63–187:64 | 6999..7000 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:188:50–188:51 | 7051..7052 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:189:81–189:82 | 7134..7135 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:196:33–196:34 | 7410..7411 | try_propagation | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:197:13–197:18 | 7425..7430 | match | main | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:202:59–202:70 | 7878..7889 | match_guard | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:203:42–203:43 | 7936..7937 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:204:40–204:41 | 7978..7979 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:204:49–204:50 | 7987..7988 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:205:50–205:51 | 8039..8040 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:206:68–206:69 | 8109..8110 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:208:56–208:57 | 8220..8221 | try_propagation | main | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:223:64–223:65 | 8683..8684 | try_propagation | prepare | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:224:48–224:49 | 8733..8734 | try_propagation | prepare | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:225:59–225:60 | 8794..8795 | try_propagation | prepare | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:240:5–240:8 | 9059..9062 | for_iteration | ship | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:243:88–243:89 | 9230..9231 | try_propagation | ship | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:244:9–244:14 | 9243..9248 | match | ship | S1 |
| P | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:259:17–259:19 | 9852..9854 | if | ship | S1 |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:259:20–259:56 | 9855..9891 | short_circuit_or | ship | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:279:9–279:14 | 10509..10514 | match | fmt | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:287:75–287:76 | 10905..10906 | try_propagation | serve_one | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:293:46–293:47 | 11150..11151 | try_propagation | serve | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:295:25–295:26 | 11227..11228 | try_propagation | serve | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:296:5–296:8 | 11234..11237 | for_iteration | serve | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:297:30–297:31 | 11315..11316 | try_propagation | serve | S1 |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:298:9–298:11 | 11326..11328 | if | serve | S1 |
| C | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:298:12–298:15 | 11329..11332 | pattern_test | serve | S1 |
| D | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:302:28–302:29 | 11544..11545 | try_propagation | serve | S1 |

## Test bindings and results

| Test source | SHA-256 |
|---|---|
| `apps/weekend-replicate/src/native/pin_tests.rs` | `713487422c3e0343a25730eb8681b75d37fda6d2031e9d17921791b732ed35bc` |
| `apps/weekend-replicate/src/native/tests.rs` | `23ea109bba8ee38c6550b71c39494b7b95929bbf5fd9f564ad8570b05079fb86` |
| `apps/weekend-replicate/tests/audit_cli.rs` | `48e2b3cb7a742a7dfc4b24a7e51499cb3a4ae6e74ee050127a99a076089ee3bd` |
| `apps/weekend-replicate/tests/cli.rs` | `4a975a9d859ff6beafbbdeb7c59eea396fcbc6319a0ac503dbd0acfa08d178d9` |

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

Historical pre-sharing inventory source-set SHA-256: `133bdb4a94e46015da5aac537daaf358a28fb9b862d3a1bcea5319fb6da8b8d7`
(2,540 records, 50 files, 314 blind spots, zero parse failures).

Maintenance validation: the same 14 focused tests passed native macOS debug and
release plus strict all-target Clippy, and isolated aarch64 Linux debug. Windows
MSVC cross-check was attempted but blocked in ring's C compilation by missing
Windows SDK headers; it is not a Rust compilation or runtime pass. Logs:
`/tmp/maintenance-{debug,release,clippy,linux,windows}.log`. These results cover
new wrapper behavior, not a new crash/power-loss campaign.

Current shared source SHA-256: `apps/support/pinned_tls.rs` = `dcf734724390f67032e97525b222e8be983b67f58beeddf6fe9de05765afa942`.

## S6: shared TLS binding and current qualification

The fixed profile compiles `apps/support/pinned_tls.rs` in this binary. The
44 shared records are inventoried separately for each target, not collapsed into
one coverage denominator. See [shared TLS evidence](../shared-app-tls.md) for
real partial-Finished, exact signed-leaf pin, required ALPN, file caps/FIFO/modes,
and absolute deadline controls. This historical tranche left pending-write-only and client partial-flight
independence open; the new decision pairs below close them; cryptographic/OS errors remain delegated. Existing
manual dispositions are rebound to current spans; this is not measured MC/DC.

Mac debug/release and strict Clippy, and isolated aarch64 Docker Linux
debug/release and Clippy passed: study 14 and snapshot 19 tests each profile.
Mise Rust 1.99.1 / cargo-xwin 0.23.1 cross-linked five Windows test executables;
no Windows runtime claim. Logs `/tmp/shared-tls-{debug,release,clippy,linux,windows}.log`.
The earlier 2490631 qualification remains historical. Current source-set SHA-256:
`e656b7667508b2ab60acc92dc5c1c35ffb9328ef45c254cdfd9dcb0168cf7595`.

Current test bindings (shared module tests are bound by its full-file hash):

| Test source | SHA-256 |
|---|---|
| `apps/weekend-replicate/src/native/tests.rs` | `23ea109bba8ee38c6550b71c39494b7b95929bbf5fd9f564ad8570b05079fb86` |
| `apps/weekend-replicate/src/native/pin_tests.rs` | `713487422c3e0343a25730eb8681b75d37fda6d2031e9d17921791b732ed35bc` |
| `apps/weekend-replicate/tests/cli.rs` | `4a975a9d859ff6beafbbdeb7c59eea396fcbc6319a0ac503dbd0acfa08d178d9` |
| `apps/weekend-replicate/tests/audit_cli.rs` | `48e2b3cb7a742a7dfc4b24a7e51499cb3a4ae6e74ee050127a99a076089ee3bd` |

## S7: completed and drained handshake admission

[Directed H/W pairs](../tls-handshake-decisions.md) independently vary the real
rustls handshaking and pending-output predicates at both endpoints. Exact pin
and ALPN remain valid; all failing cases reach the production guard after
complete_io returns Ok. The overlapping OR syntax is C; each guard is P.
Private session-to-handshake propagation is D: the tests do not inject every
OS/dependency failure at each caller. Two added propagation records per target
bring shared TLS to 46 records, snapshot to 127 and distributed apps to 224.
Mac and isolated Docker Linux debug/release each passed 35 focused tests, plus
strict Clippy. No native Windows or whole-application MC/DC claim is added.
Current source-set SHA-256: `4c204b338de0482b6264325666ac7d987b39cdf277ac0106e4b7cc9fcaae0e5a`.
