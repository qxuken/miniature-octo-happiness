# Service diagnostics and bounded canary drills

## Opt-in structured diagnostics

Both TLS CLIs accept a **leading** `--diagnostics=json` flag. Without it,
their existing text output is unchanged. In structured mode, fatal CLI/startup
errors and Study local-command errors produce a single JSON object instead of
formatting the original error. Successful readiness, state, capture, progress
and acknowledgment output retains its existing text format. The fixed snapshot
reconnection notice also retains its existing text; this is not an all-JSON
stdout/stderr protocol.

```sh
weekend-replicate --diagnostics=json describe SOURCE_DB STREAM_HEX
study-peer --diagnostics=json run FILE a COLLECTION_HEX LISTEN PEER CREDENTIALS PIN --no-push
```

The first command can return this record for a truncated image:

```json
{"schema":"weekend-diagnostic/1","app":"weekend-replicate","command":"describe","phase":"storage_verify","category":"engine","code":"Corrupt","native":null,"verification":{"stage":"image_length","offset":null}}
```

In a structured Study session, the existing local `incoming` command retrieves
a bounded deferred snapshot:

```json
{"schema":"weekend-diagnostic/1","app":"study-peer","command":"run","phase":"incoming","served":0,"failed":1,"busy":0,"last":{"phase":"tls_handshake","category":"tls","code":"invalid_message","native":null,"verification":null}}
```

The counters saturate at `u64::MAX`. `last` means the last observed incoming
failure, retained across later successes, matching the existing text status's
last-error policy. It is null before an observed failure. In structured mode,
Study `peer` also renders its failed outgoing attempt safely; successful and
idle reports remain text. Workers only update memory. Operator code copies a
safe diagnostic snapshot and releases status locks before writing output.
No unsolicited diagnostic write is added to either service's network workers.

The shared implementation is
[`apps/support/service_diagnostics.rs`](../apps/support/service_diagnostics.rs).
Its JSON object and newline together fit within 1 KiB. Fields are closed enum
strings or numeric values; they never contain argument text, messages, paths,
payloads, certificates or keys. Categories are `engine`, `io`, `tls`, `input`
and `internal`. Engine codes preserve the authoritative engine error names.
Known I/O kinds and selected rustls variants have explicit codes; other I/O/TLS
variants map to `other`. Unknown error types map to `internal/unclassified`,
without invoking their `Display`, `Debug` or arbitrary `source()` chains.
Opaque string-backed pin/ALPN rejection errors consequently remain
`unclassified` with the known operation phase; no text matching invents a cause.

Phases identify the observed operation: argument handling, credential or
configuration loading, TLS configuration, storage create/open/verify/maintenance,
export, listen/connect, TLS handshake, replication, local command, incoming or
shutdown. They are not a general causal trace. Each instrumented synchronous
storage boundary captures the original engine error plus optional native
operation/category/OS code and verification stage/structure-start offset.
Unavailable details are null. A plain OS error can expose its numeric code with
null native operation/category; the implementation does not invent a storage
operation for it. OS codes are platform-specific, and a verification offset
is not necessarily the damaged byte or even inside the file.

The capture is operation-local and thread-local. New operations and retry
attempts clear prior context; nested captures and unwinding restore their
caller, and worker threads receive their settings explicitly. Original error
values remain unwrapped, preserving permanent-failure downcasts and retry
decisions. Invalid saved descriptors report `storage_verify`; a descriptor
that cannot be opened keeps its deeper `storage_open` context.

### A diagnostic is not a follower acknowledgment

Every failed `ship`/`ship-captured` diagnostic includes the fixed fields
`"follower_outcome":"unconfirmed"` and
`"guidance":"poll_or_reconcile_before_retry"`. These fields do not assert that
a shipment was attempted or that the follower did or did not commit. For example,
a final `connect/io/connection_refused` error can follow an earlier attempt
that installed the image durably but lost its reply. The record describes the
observed failing operation; it is never a `DurableAck`.

Keep the existing immutable export and descriptor when available. Recover an
uncertain local primary first, then poll/reconcile through a fresh authenticated
connection before deciding whether to retry that exact export. A new diagnostic
format does not make replay of a different export or non-idempotent local edit
safe. The [replication operations contract](replication-operations.md) remains
authoritative.

### Implemented scope and regression evidence

The current working-tree implementation passes native macOS arm64 Rust 1.99.1
app tests and strict Clippy for both apps' complete targets. Tests cover both
default and JSON canary scenarios, complete input-byte preservation, actual
snapshot shipment/reopen receipt equality, corrupt/missing saved descriptors,
and structured Study incoming retrieval. Shared unit tests use unknown errors
whose formatting and source callbacks panic if called; they also check original
error downcasts, retry/nested/thread/unwind isolation, leading-only opt-in and
maximal numeric rendering. The existing full-stderr Study handshake/convergence
test now runs with both default and structured receivers.

The Study command protocol remains a pure reusable module: the fuzz library
imports it without a TLS or CLI diagnostic dependency. Observation lives in a
CLI-local adapter around a complete synchronous command. The adapter observes a
copy of a returned engine error to capture native/verification details, then
returns the identical original boxed result; parser errors remain parser errors.
It does not change the protocol's three-attempt conflict budget. A regression
checks actual native context, parser isolation, unchanged revisions and retry
count. Locked/offline fuzz library checking and all 29 stable replay tests pass
without a fuzzer shim or manifest/lock change.

The final Mac `peer_transport` ASan smoke also passes 100 completed executions
with the existing `cargo-fuzz` 0.13.2 and pinned `nightly-2026-11-12`
(rustc 1.102.0-nightly, `3ef0c22cd`). Its requested limits were 100 runs/two
seconds, with unchanged source, seed and lock inventories, empty artifact
directory and verified child cleanup. Evidence is retained at
`target/diagnostics-peer-transport-seam-fix-final-20261116/`; its `evidence.json`
SHA-256 is
`c89f33f2bec41d1ce3bef2f79543ed028cf6a16d3c917db471110cca655bf374`.
The report identifies working-tree source `b566f51` plus the uncommitted
diagnostic changes; this smoke is not a sustained fuzz or release-candidate
qualification.

Parallel default/JSON process tests also exposed that two temporary roots could
share a coarse clock tick. The shared fixture now adds a per-process atomic
serial alongside its timestamp and PID; production runtime behavior is unaffected.

A real TLS regression drops the terminal reply after durable installation,
then closes admission so the next attempt receives connection refusal. Its JSON
reports that last connect error and the unconfirmed-outcome guidance. Independent
reopen and authenticated poll recover revision 4 and its exact export digest;
retrying the retained export returns the same receipt without extending the
manifest. The retained revision-2 state also remains intact.

This implementation does **not** add a snapshot stdin/control thread or a live
snapshot peer-error query. Snapshot admission still suppresses peer-controlled
errors; a terminal failure propagated from its follower worker currently has
the caller's replication phase and engine category, without the original worker's
native/verification context. That is a remaining observability gap. This first
API step does not close all F06 deployment obligations. The frozen Linux
structured result follows below. Windows execution of this implementation and
final-candidate reruns remain separate qualifications; the historical Linux
canary proof near the end of this document predates the structured API.

### Final Linux structured-results and pure-protocol seam

On 2026-11-16, the finalized working-tree implementation passed the complete
`study-peer` and `weekend-replicate` app suites on Docker Linux arm64/ext4 with
Rust 1.99.0 (`b940084d7`, LLVM 23.1.1) and Cargo 1.99.0 (`5f94df478`). Both
debug and release passed **69 tests**, with one registered skip and nine test
summaries per profile. The extra pass relative to the final 68-test macOS scope is
the Linux snapshot blocked-stderr regression. The skip is the existing
`full_storage::authenticated_follower_reopens_at_the_old_receipt_when_tmpfs_is_full`,
which requires its own dedicated empty tmpfs of at most 8 MiB; this ext4 run
does not claim that exhaustion case. Strict Clippy passed for all targets of
both apps.

This final profile includes the correction for a build integration gap:
`fuzz/src/peer.rs` imports the reusable Study protocol directly, so the protocol
must not depend on CLI diagnostics. `apps/study-peer/src/native/protocol.rs`
is byte-identical to the clean base; observation now lives at the CLI command
boundary. The final run also includes the shared test-root atomic serial fix.
The initial thirteen-file Linux run qualified its app-only checks before these
corrections. Its unchanged proof at
`/private/tmp/weekend-structured-diagnostics-linux-final` and qualification
SHA-256 `112556ac0714ed111f418cd198b3eb0a52834cc1b24d5c44fc143f81954e0cb0`
are **pre-fix evidence**, not final-source fuzz compatibility evidence.

The retained full-test transcripts confirm the following assertions in each
profile:

| Case | Verified behavior |
|---|---|
| Both apps' default and JSON canary process tests | Neither complete stream exposes the synthetic input/path/key canaries; input bytes are preserved; structured records retain their required categories and fields |
| Snapshot invalid and missing saved descriptors | Corrupt descriptor: `storage_verify/engine/Corrupt`; missing descriptor: `storage_open/engine/NotFound`; their JSON records report an unconfirmed follower outcome, and descriptor bytes remain unchanged |
| Shared classification, capture and rendering unit tests in both binaries | Original errors remain downcastable; retry, nested, thread and unwind context stays isolated; unknown errors never invoke arbitrary formatting/source callbacks; leading opt-in and the bounded maximal-numeric renderer pass |
| CLI command-observation adapter regression | Actual native storage context is retained, parser errors remain isolated, the identical boxed result returns, revisions stay unchanged and the existing retry budget is preserved |
| Lost terminal reply followed by connection refusal | The last JSON operation is `connect/io/connection_refused` with unconfirmed-outcome guidance; independent reopen and authenticated poll recover exact revision 4 and its export digest with retained revision 2; retry returns the same receipt without growing the manifest |
| Existing receipt, credential-cutover and repeat-sync tests | Receipt/state equality and unchanged repeat-sync revisions remain intact |
| Full-stderr regressions | Study completes authenticated convergence with both default and JSON receivers; Linux snapshot admission remains live with its diagnostic pipe blocked |
| Locked offline fuzz library check and replay | The pure protocol compiles without a diagnostics shim; all 29 replay tests pass without a manifest or lock change |
| Bounded `peer_transport` ASan smoke | 100 completed executions, empty artifact directory, unchanged source/seed/lock inventories and verified nested process cleanup |

These are the frozen tests' checked assertions, rather than a new collection
of production diagnostics or real credentials. The source, complete Cargo
stdout/stderr, supervisor requests/process reports and four app binaries are
retained. The command profile was:

```sh
cargo test --offline --locked -p study-peer -p weekend-replicate
cargo test --offline --locked --release -p study-peer -p weekend-replicate
cargo clippy --offline --locked -p study-peer -p weekend-replicate --all-targets -- -D warnings
cargo check --offline --locked --manifest-path fuzz/Cargo.toml --lib
cargo test --offline --locked --manifest-path fuzz/Cargo.toml --lib
cargo xtask fuzz --target peer_transport --toolchain nightly-2026-11-01 \
  --runs 100 --seconds 2 --rss-mb 512 --max-len 256 \
  --build-directory /work/fuzz-build --output /work/evidence/peer-smoke
```

Each test profile was compiled first with its matching `--no-run` command.
The stable fuzz library used a separate `/work/fuzz-lib-build` target directory.
The image already contained `nightly-2026-11-01`: rustc 1.101.0-nightly,
commit `f9ceb4a4d45a471eed7bd8c40dddb7c38e749e21`, LLVM 23.1.4. Its cargo-fuzz
0.13.2 tool was built `--offline --locked` from the existing cached source and
lockfile into the owned volume. No tool or dependency was fetched and no
diagnostics module, dependency or shim was added to the fuzz crate. The smoke's
compiler differs from the separately recorded macOS smoke; neither result is
a sustained campaign or an all-target fuzz qualification.

Default app features and the two packages' normal dependency feature union
were used; no manifest or lockfile was changed. The source was frozen before
execution from clean commit `bbcf68dcfbb1246e24ed2a2cb80b1d12da12c21c`, tree
`9b09b32a306389f0ceffe221ed6c81b7637c28b3`, plus exactly the twelve changed app
files, new `apps/support/service_diagnostics.rs` and the shared
`tests/support/service_diagnostics.rs` fixture fix. The author's readiness
manifest hashes all fourteen included files. The observed host head was
`b566f51c08afb87a02a2f94f7a72bdfb9062c96c`; its typed test/decision-ledger changes
and all working documentation were excluded from this frozen runtime profile.
This is a **dirty source-plus-overlay profile**, not an unmodified `bbcf68d`
run or a fabricated clean revision. All 744 frozen source files, fourteen
lockfiles, dirty Git status and fourteen host overlay files matched their
before/after inventories.

The immutable image was
`sha256:bda5ef43469a7d63b240adcbc5a5b0599be442bce204d12936f6083224f56b18`.
It used cached offline dependencies only, with read-only registry and seed
mounts, no external networking, four CPUs, 5 GiB memory and 512 PIDs. Source,
builds, databases, evidence and `TMPDIR` were on the independently owned ext4
volume. Existing xtask supervision verified all 28 host actions against their
expected statuses, all nine Linux build/test/tool/smoke commands and all five
nested fuzz commands, including descendant retirement and stream byte counts.
Cargo/test wrapper streams were capped at 4 MiB; source-archive setup capture
used a 16 MiB ceiling, and the nested fuzz runner used its existing 64 MiB
ceiling. Compile/test/tool-build deadlines were 300 seconds,
Clippy 180 seconds, the smoke wrapper 660 seconds, and the container
had a 25-minute outer limit with a 15-second kill grace. Cleanup checked the
exact ownership labels before removing only that container and volume, then
authoritative inventories proved both names absent.

Retained evidence is `/private/tmp/weekend-structured-diagnostics-linux-seam-final`.
`freeze.json` records all fourteen overlay and fourteen lockfile hashes;
`independent-audit.json` verifies source/binary hashes and the terminal reports;
`qualification.json` records the exact profile and case coverage. The nested
Campaign retains the raw libFuzzer log and its parsed completed count under
`linux-arm64/peer-smoke/`.

| Evidence | SHA-256 |
|---|---|
| Clean `bbcf68d` source archive | `8b0b56e7debd5aeffda24c3f18b45a7ddfea6079d7fb82342c13a05f6c6a0e75` |
| Exact fourteen-file overlay patch | `5a3a0eff1530afaa7bc7e364be2f5acd6f63de80642a8f2a50444097e1a0cfde` |
| Unchanged pure Study protocol | `3a11026d333bf97ff105748dd38e98023720752bced98b2b12af2a251b55d73e` |
| Workspace lockfile | `69a917a2f7fcbe4e81b3320a6c3729c184f7100255a5c822ce038bb051c00ca3` |
| Shared structured-diagnostic implementation | `02a7c1dcba60626104be3f58da640459b5f185546b64a532c189d3f62ef07208` |
| Final qualification record | `ef3e586bae3eb4de5acd21c86cedb0664772fe5f19f36660c6b1249fe247e99a` |
| Independent audit | `d69c771b1a4a7b7357c3b912c9170183970c7f90f2585425e195eacf7b72f16b` |
| Peer ASan smoke report | `7743df21b69a6a1fb3a75fcea816f69e88dcec0bfffa3dba711428d7670af11c` |

This qualifies the named Docker arm64/ext4 debug/release profile. It does not
extend the earlier `bbcf68d` hosted CI or sanitizer results to the uncommitted
overlay. Native Windows, a clean final candidate, additional OS/I/O/TLS causes,
snapshot worker-context propagation and deployment log sinks remain separate
work. Test timings are observations, not service or recovery-time budgets.

## Original bounded canary drill (historical)

The final structured implementation and the explicitly pre-fix profile are
retained in `target/archived-proof/structured-diagnostics-e195e06.tar.zst`.
Its SHA-256 is
`1b026cb1db389771b457e205149e3df27bf75afff196e5d9b6bb216c034b58fe`.
Root restoration verified all 2,074 indexed files. Separate clean-`e195e06`
macOS debug/release logs each record 68 passes and no skips, with strict Clippy.
The archive retains exact Mac debug/release binaries; older smoke and Linux
overlay reports keep their actual source identities.

Retained payloads are also preserved in the
[operational qualification archive](roadmap-status.md#administrative-recovery-and-service-output-rehearsal),
whose complete file index passed a fresh restoration check.

The original F06 follow-up checked the actual `weekend-replicate` and `study-peer`
process entry points. It covers operational stdout/stderr, including startup
failure and a live service rejecting input. It added regression tests without
changing either service's runtime code. No runtime deficiency was reproduced
in those cases; their evidence is separate from the later opt-in API above.

## Observed behavior

Both entry points print the returned error's `Display` representation to stderr
and exit unsuccessfully on startup failure. The tested storage categories and
credential errors omit the path and supplied bytes. This is observed behavior
of these dependencies and entry points, not a universal property of boxed
errors: introducing a context string containing a path, credential or input
could break the contract.

| Actual path exercised | Required output / state |
|---|---|
| Snapshot `describe` on a canary-filled corrupt image | Fails with `Corrupt`; never prints the image contents or path; input bytes stay unchanged. |
| Study `init` under a nonexistent canary-bearing parent path | Fails with `NotFound`; never prints the path. |
| Both service startups with missing credentials | Fail before readiness; retain an OS error code (`os error`), without exposing the credential directory. |
| Both service startups with a canary-filled malformed private key | Fail before readiness; preserve a key-related diagnostic, without exposing key bytes or paths. The valid key is restored only inside the test fixture. |
| Snapshot finite server receives a plaintext frame containing all canaries | Rejects the session, retires normally after its one accepted connection, prints readiness only and no stderr. Offline status still reports an empty follower and no durable acknowledgment. |
| Live Study service receives `practice` with a canary instead of a number | Prints the integer-parse error without echoing the command or operand. |
| Live Study service receives a plaintext frame containing all canaries | A later local `incoming` command reports `served=0 failed=1 busy=0 error ...`; no canary or unsolicited stderr appears. The service exits through `quit`, and the database remains byte-identical. |

The Study `incoming` report is deferred: network workers update in-memory
counters and the last error; only the operator command writes its output.
Snapshot admission deliberately suppresses peer-controlled handshake/session
errors instead of writing to a potentially blocked stderr. Its one-connection
test consequently establishes rejection and continued authoritative state, not
a peer failure category in the server log. Existing admission/backpressure tests
cover the separate blocked-diagnostic-stream behavior.

The useful categories above remained available at their existing command/phase
boundaries. At that checkpoint the CLIs did not provide a uniform structured error
record with command, phase and native/verification fields. The command name in
the operator invocation and the Study `incoming` prefix identify the tested
phase; a generic `Corrupt` message alone does not identify its structure offset.
Do not infer more detailed phase attribution from these results.

## Reproduction and bounds

Run the two integration targets with the same compiler as their cache:

```sh
cargo +1.99.1 test --offline --locked --target-dir target/xtask \
  -p weekend-replicate -p study-peer --test service_diagnostics
cargo +1.99.1 clippy --offline --locked --target-dir target/xtask \
  -p weekend-replicate -p study-peer --test service_diagnostics -- -D warnings
```

The tests are
[`weekend-replicate/tests/service_diagnostics.rs`](../apps/weekend-replicate/tests/service_diagnostics.rs)
and [`study-peer/tests/service_diagnostics.rs`](../apps/study-peer/tests/service_diagnostics.rs).
Both import the small shared
[`tests/support/service_diagnostics.rs`](../tests/support/service_diagnostics.rs)
fixture by path. It owns the process guards, stream bounds, deadlines and canary
checks; only the application prefix/certificate DNS name varies. No new crate
or dependency is introduced. Each creates an owned temporary directory with a
unique nonce. On Unix the
directory is mode 0700 and the synthetic private keys are mode 0600. Both streams
are captured to owned files; each observation checks a 16 KiB per-stream bound.
Readiness, deferred observation and process retirement use ten-second deadlines;
the injected loopback connection/write use two-second socket bounds. Each owned
child is killed and reaped if its guard drops, including on an assertion failure.
Temporary credentials and outputs are removed afterward. These are bounded
user-space checks under normal OS process/filesystem behavior, not kernel
liveness or hard disk-usage guarantees.

The checks examine both complete streams after process retirement. They reject
any appearance of the synthetic path, credential or record canary, require the
specific category/counter output above, and independently compare the affected
database/image bytes. A zero exit status or an empty log alone cannot satisfy
the service-state or category checks.

## Qualification scope and remaining work

The initial run on 2026-11-16 used native macOS arm64 and Rust 1.99.1 over
`6eb5f1a884bd75cfd71cdd75e3fa1cf0be597e63` plus the new test files and shared fixture.
Both process tests pass; strict Clippy passes for the two integration targets.
This is working-tree regression evidence, separate from the four-platform
`7e0e44f` package checkpoint. The frozen-source Linux follow-up is recorded
below. Windows execution and final-candidate reruns remain separate
qualifications.

### Frozen-source Linux arm64/ext4 result

On 2026-11-16, both tests passed independently in Docker with Rust
1.99.0 (`b940084d7`, LLVM 23.1.1) and Cargo 1.99.0 (`5f94df478`).
The immutable local image was
`sha256:bda5ef43469a7d63b240adcbc5a5b0599be442bce204d12936f6083224f56b18`.
The runtime reported Linux 7.0.14-linuxkit, `aarch64-unknown-linux-gnu`.
The image was used without pulling or installing anything. Both Cargo
invocations used `--offline --locked` and the existing read-only registry cache.

The source was a clean Git archive of
`6eb5f1a884bd75cfd71cdd75e3fa1cf0be597e63`, tree
`adcf057f8c9254376fb93b4fec0a484fb7aa6c80`. A checked patch added only the two
diagnostic integration tests and their shared fixture. No engine, service,
manifest or lockfile was changed. The archive's 732 regular tracked files plus
the three overlay files produced identical complete source inventories before
and after both builds/tests. The host overlay files also retained their hashes.
This establishes the named source-plus-test-overlay profile, not a test of an
unmodified `6eb5f1a` tree or a new four-platform package checkpoint.

The UUID-owned Docker volume reported ext4 for the copied source, build output,
test databases and `TMPDIR`. The container had networking disabled, two CPUs,
2 GiB memory and 256 PIDs; the tests used only its loopback sockets. A 600-second
container timeout and the existing host xtask supervisor bounded execution.
Each service target passed its one process test, with no skips, in 0.09 seconds
and 0.12 seconds respectively. The container exited zero. All 17 recorded host
commands have successful process reports and verified cleanup. Removal checked
the ownership label before deleting only that container and volume; successful
authoritative Docker inventories then confirmed both UUID names absent.

Retained local evidence is
`target/service-diagnostics-linux-be56fabe-4ae7-4684-a918-71cb46ab337c/`.
It includes the exact archive/patch/overlay, shell runner, compiler/filesystem
metadata, both Cargo stdout/stderr logs, before/after source inventories,
supervisor requests and terminal reports, container state and final Docker
inventories. Build caches were removed with the owned volume.

| Input / evidence | SHA-256 |
|---|---|
| Clean source archive | `0b740ab2b537b1702faf529f31a6f5246bf18d4a4dbdd8b165dd1c9d6a728008` |
| Exact three-file overlay patch | `0c5f638ffb5259ef12479b2648dc6da1f45f14fff15087f4bf43f7b10dd019a2` |
| Snapshot service test | `94d34f9be0b62701ecae3674ad3f89c941dc80ed8bfba99fa2a5e0fc5fa034b1` |
| Study service test | `d3a683014f20d0a0508c93595dcd3648a0414ed7d1031b94c1bc1719eee859e8` |
| Shared fixture | `90f2a83d4b004221eddf51b574cf144d7212bfd77dbcf6e179d2b63465120416` |
| Container shell runner | `fe800b58858bead526248f9aabbec2db29673a78dd5ea34eb2e658ede208c14e` |
| Complete source inventory, both observations | `95e0ccad5d4e2701e331789dedb3db39a3047127fb1f69a6f80d00f0f608f0b0` |
| Final `metadata.json` | `dcaa07e26dc19e933596dfaa7fb8b8e3e4a9b27c6e89386f00cf6b0ee038099f` |

### Remaining scope

The tests cover startup, parser and malformed unauthenticated TLS input. They
do not cover every certificate/OS diagnostic, all authenticated corrupt record
frames, panic/debug output, crash dumps, external service managers, third-party
log sinks or arbitrary user `Record` implementations. Study `show` and normal
acknowledgment/progress commands intentionally expose their documented state;
this drill is not a confidentiality promise for those successful commands.
Process command lines and test fixture files are also outside stdout/stderr
redaction. The test harness never receives real secrets.

The [native diagnostic contract](verification-diagnostics.md) offers a separate
path-free, record-free `StorageFailure` with authoritative error and optional
native operation/category/code and verification stage/offset. The original
canary drill did not capture those fields. The structured API above now captures
them at its instrumented boundaries. Additional boundaries, snapshot worker
context, deployment sinks and platforms need separate review and execution.
Arbitrary application logging remains outside this evidence.
