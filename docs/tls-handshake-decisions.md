# TLS handshake admission decision pairs

Source base: `ed8f9f922bc46bd6e52b7d0776f69dad93f3184a` plus this private seam/test change. Full shared source
`apps/support/pinned_tls.rs` SHA-256: `dcf734724390f67032e97525b222e8be983b67f58beeddf6fe9de05765afa942`.
The module compiles separately for study-peer and weekend-replicate; the same
pairs run under both actual SAN/ALPN/deadline profiles.

## Production decisions and private transport seam

Client admission at line 118 and server admission at line 147 reject when
`H || W`, where H is rustls `is_handshaking()` and W is `wants_write()`.
Native session construction now calls private `client_handshake` and
`server_handshake` methods with its real connection and DeadlineSocket. Those
methods run the existing `complete_io`, guard, exact-pin and ALPN checks. They
accept a private generic Read+Write transport to enable directed tests. No
boolean helper, fabricated TLS state or public transport API was introduced.
Client connect still starts its absolute deadline before TCP connect, and native
socket reads/writes retain the same budget. No application wire, credential,
local/follower acknowledgment or durability behavior changed.

## Independence pairs

`handshake_decision_tests::both_endpoints_reject_independent_unfinished_and_pending_write_states`
executes these three cases for each endpoint in each binary:

| Case | H | W | Guard | Outcome |
|---|---|---|---|---|
| Complete and drained | false | false | false | Admitted with exact pin and ALPN |
| Withheld peer Finished | true | false | true | Exact `TLS handshake incomplete` error |
| Blocked final local write | false | true | true | Exact `TLS handshake incomplete` error |

The first/second rows establish H independence while W stays false. The
first/third establish W independence while H stays false. H=true masks W in
Rust's short-circuit evaluation; the test inspects both real connection state
values afterward but does not claim W was evaluated in that case. Each failing
case returns the exact application guard diagnostic, which occurs only after
`complete_io` returned Ok. Every case independently verifies the configured leaf
pin and negotiated ALPN, so a missing certificate or protocol cannot serve as
a substitute rejection oracle.

The duplex fixture uses a genuine opposite rustls ClientConnection or
ServerConnection to process bytes. Both endpoints fragment records to 64 bytes
so withholding the last complete TLS record leaves authenticated certificate
material available before Finished. The client partial-server-flight case now
asserts H=true/W=false with exact server pin and ALPN, closing the previously
recorded direction gap. The symmetric server partial-client-flight case does
the same and complements the existing real TCP partial-Finished regression.

For pending output, the fixture returns WouldBlock when the client tries to
write after reading the server flight, or when the server writes after the peer
client has finished handshaking. Earlier traffic has made progress, allowing
pinned rustls to return Ok with H=false/W=true. The test asserts that its write
fault actually fired. Successful controls use the same transport without faults
and require H=false/W=false. This is deterministic transport-fault evidence,
not a claim that OS backpressure or kernel scheduling was reproduced.

## Validation and limits

Native arm64 macOS and isolated aarch64 Docker Linux debug and release each
passed 15 study and 20 snapshot tests per profile (35 total), including the
preexisting process/recovery/acknowledgment oracles. Strict all-target Clippy
passed on both platforms. Linux used immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`
with Rust 1.99.1 and the archived base plus this exact shared source.
`/tmp/tls-handshake-linux-binding.json` binds source/lock/image and results;
`/tmp/tls-handshake-linux.log` records both profiles and Clippy. Reproduce from the repository root:

```sh
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/tls-handshake-decisions cargo test --offline --locked -p study-peer -p weekend-replicate
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/tls-handshake-decisions cargo test --offline --locked -p study-peer -p weekend-replicate --release
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/tls-handshake-decisions cargo clippy --offline --locked -p study-peer -p weekend-replicate --all-targets -- -D warnings
```

Logs: `/tmp/tls-handshake-{debug,release,clippy}.log`. Four isolated mutations
removed H or W separately at each production guard; the directed study test
failed for every mutation. The working source was never mutated. The repository
runner now reproduces those cases with a positive, nonzero-test baseline:

```sh
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$PWD/target/tls-guard-mutations" mise exec rust@1.99.1 python@3.14.8 -- python tools/mutations.py --case tls- --output target/tls-guard-mutations-replay
```

Choose a fresh output directory when replaying. Evidence and baseline/mutation
logs are in `target/tls-guard-mutations-replay`; `evidence.json` binds copied
source hashes and actual commands/statuses. All four cases were caught (Rust
test exit 101 and `test result: FAILED`); the shared baseline passed one test.
Only the two production guard occurrences before the first test module are
eligible (`matches=2`, occurrence 1 or 2). The 46 prior mutation cases retain
their package/target/filter command semantics; `package` defaults to weekend-db,
while these four use study-peer `--bins handshake_decision_tests`.
This validates detection of these four omissions, not a broader mutation score.

These directed pairs close the previously recorded H/W guard independence and
client partial-flight residuals for pinned rustls 0.23.46 under the two profiles.
They do not establish complete compiler-measured application MC/DC, cryptographic
correctness, OS liveness, every TLS failure, or any new Windows result.
Exact-pin, ALPN, credential and deadline evidence remains in the existing app
ledgers. The integrated inventory has 2,561 records across 51 files and 316 macro blind
spots: shared TLS contributes 46 records separately per target, with 127 snapshot
and 224 distributed-app records. Source-set SHA-256:
`4c204b338de0482b6264325666ac7d987b39cdf277ac0106e4b7cc9fcaae0e5a`.
The central ledgers retain the broader delegated and generated-code scopes.

Repository mutation runner SHA-256: `35fcb18b1edd8bc40e88fd518209e16b8f83cafb30085a3f3c21b71446260d2e`.

Root review regenerated the inventory and checked all 127 snapshot and 224
distributed-app records by exact target/span/byte range/kind/owner. Five app
Windows test executables cross-linked with the previously qualified pinned
MSVC toolchain; none executed. Eleven proof records/logs, including binary
hashes, are preserved in `target/local-qualification-handshake-pairs/index.json`.
