# Explicit migration of a two-writer CRDT history

[`crdt_policy_migration.rs`](../examples/crdt_policy_migration.rs) demonstrates an
application-controlled cutover, using the existing StudyProgress record. It is
an executable example, not an automatic migration service or actor replacement
API. It changes accumulated practice minutes to seconds and changes the weekly
goal's automatic conflict policy from maximum to minimum. All five visible
fields survive conversion. Old causal dots and counter components do not move
to the new actor roster.

```sh
cargo run --offline --features crdt --example crdt_policy_migration
cargo test --offline --all-features --test crdt_migration
cargo test --offline --all-features --test crdt_migration --release
cargo clippy --offline --all-features --test crdt_migration \
  --example crdt_policy_migration -- -D warnings
```

The example first reconciles concurrent old edits: 20 + 10 practice minutes,
completed topics 1 and 3, a planned topic, goal proposals 60 and 90, and session
limits 30 and 20. The old policies settle the goal at 90 and session limit at
20. Conversion preserves those reviewed values and imports 1,800 seconds once.
Subsequent offline goal assignments of 120 and 45 converge to one committed
value, 45, under the new minimum policy. Repeated sync adds no commits.

## Wire and identity decisions

| Item | Before | After |
|---|---|---|
| Record key | `course-progress` | Same key, in a fresh collection/history |
| Eight-byte format marker | `COURSES\x06` | `COURSES\x07` |
| Explicit payload order | completed, practice_minutes, planned, weekly_goal, session_limit | completed, practice_seconds, planned, weekly_goal, session_limit |
| Weekly-goal policy | MaxRegister | MinRegister |
| Session-limit policy | MinRegister | MinRegister |
| Demo logical collection | `[9; 16]` | `[10; 16]` |
| Demo generation | 7 | 8 |
| Demo actor roster | `[1; 16]`, `[2; 16]` | `[3; 16]`, `[4; 16]` |

These IDs are explicit demonstration constants, not production defaults. An
operator must allocate fresh nonzero logical collection and actor identities,
record their relationship to the retired history, and configure both endpoints
with the exact same policy. Local numeric collection IDs may both be 1 in fresh
stores; these are not the policy's 128-bit logical collection identity. Physical
store identity is also separate. A higher generation alone does not migrate data.

The derive explicitly lists field order. Both payloads are 2,140 bytes:
GrowSet 8, GrowCounter 16, AddWinsSet 2,048, and two 34-byte policy registers.
The core's 65-byte envelope retains its existing format: marker, logical
collection, generation, ordered actor roster, actor index. No new codec is added.
The changed marker prevents old bytes from silently being interpreted as seconds
or under a different goal policy, even though the byte lengths are equal.

`review` requires equal old payloads and checks multiplication and the new
single-actor counter bound before creating destination collections. The old
weekly goal is interpreted under the **old** maximum policy; conversion does not
retroactively choose a smaller value from retired proposals. The new minimum
policy applies to subsequent conflicts. Goals and session limits retain their
existing units. Completed and currently planned topic IDs are copied as values;
removed planned topics remain absent. Old tombstones and sequence numbers remain
available in the archived old files rather than being installed in the new roster.

## Quiescence, staging and cutover

1. Stop edits on both old actors, including background sync/policy resolution,
   then reconcile the old pair with controlled exchanges. The example owns its
   endpoints and has no concurrent application writers. Payload equality is a
   validation check, not a distributed quiescence lock.
2. Preserve both old stores, retained roots and recovery evidence. Record the old
   policy, physical store identities, revisions, retained-root identifiers,
   reviewed field values, conversion version and intended new policy in an
   operator-controlled migration record. The example prints demo identity values;
   it does not persist an operational migration journal or coordinate deployment.
3. Review and validate conversion. Build two fresh staging stores, enroll the new
   collection/generation/roster, and seed the reviewed aggregate on **new actor A
   only**. Seeding the full counter on both actors would double-count history.
   Exchange in both directions and verify equality, visible fields and totals.
4. Persist and reopen both destinations before changing routing. The integration
   test exercises this with simulated durable stores and their real image codec.
   The runnable example uses in-memory stores; it is not a native-file migration
   command. Provision fresh transport identities and bind them to the new roster.
   Replace old routes and keep the old generation disabled for writes.
5. Resume new-generation edits only after that application-owned cutover. Check
   that repeat sync leaves both revisions unchanged.

`stage_pair` is deliberately not atomic across two stores. If enrollment or
seeding fails, the first destination may already contain an enrolled row. Do not
route traffic to partial targets: discard both staging stores, investigate, and
retry with a fresh plan. A directed allocation failure in the second destination
proves that old revisions/values remain intact while the first staging store is
partial. Before cutover, rollback means abandoning the staging pair and retaining
the frozen old pair. After new edits are acknowledged, switching back to the old
pair would lose them; another explicit reconciliation/conversion is required.

The demo loopback exchange helper does not authenticate sockets. Production must
use authenticated channels bound to the new actors. This workflow adds no gossip,
coordinator, distributed transaction, automatic identity rotation or discovery.

## Recovering from a stale backup

The recovery test saves A's durable image, then A commits five minutes and a
completed topic and transfers them to B. A is retired; B adds three minutes.
Opening a restored older A copy succeeds: the core has no global rollback oracle.
An attempted state exchange with B rejects the reply with `Conflict`, because B
remembers newer progress for A. Neither store commits during this rejected
exchange, and the restored copy still reports its stale total. It cannot claim
B's durable acknowledgment.

This does **not** mean the core prohibits every offline edit on a restored copy.
It cannot detect a lost event absent from both peers, and the raw database API
can also bypass actor rules. Operationally, never resume the old writer identity:
keep original backups immutable and inspect a disposable restored copy without
new edits. Restore/open may perform normal recovery I/O on that disposable copy.
The test preserves the original backup image and does not edit the stale actor.

In this bounded scenario, the surviving peer covers every event in the backup.
A pure join of their compatible, inspected payloads verifies that fact; no merge
is published into the stale history. The reviewed total is eight minutes, imported
as 480 seconds into fresh stores with fresh identities. Both destinations reopen,
repeat sync makes no commits, and a new one-minute edit converges to 540 seconds.
If backups and surviving peers contain incomparable histories, reconciliation
requires application review of all available evidence before conversion. Do not
merge conflicting reused dots as if they were compatible histories. No procedure
can reconstruct events absent from every available source.

## Directed evidence and limits

The five integration cases cover:

- Quiescent reconciliation, all visible fields, old retained-root preservation,
  seeding once, changed-policy convergence and repeat-sync zero commits.
- Old format, generation, logical collection and actor roster rejected with exact
  expected errors, no row/revision changes and no reply bytes. Opening a new
  history with an old actor identity also rejects without writes.
- Durable stale-image rejection followed by explicit fresh-history recovery,
  destination reopening, convergence and subsequent edits.
- Identity reuse and unrepresentable conversion rejected before staging, with
  accepted/rejected counter boundaries.
- Partial staging under injected allocation failure and a fresh-target retry.

The final case requires `fault-injection` (included by `--all-features`). These
are application examples and directed tests, not compiler MC/DC or native
power-loss qualification. Fixed roster size remains two; this does not qualify
arbitrary schema conversion or arbitrary user-defined CRDT laws.

## Initial in-memory/simulator results and source binding

Five directed integration tests passed in debug and release on macOS 27.0.1
arm64, rustc 1.99.1 (`58cf8f9c3`, LLVM 23.1.1). The example completed in both
profiles, and strict targeted Clippy passed. Commands above used
`CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/root-qualification`. No core
implementation changes were required. There was no new native Windows, sanitizer, fuzz campaign or hardware durability
qualification in this batch.

The measured local source was based on commit
`ed8f9f922bc46bd6e52b7d0776f69dad93f3184a` plus the example, test and manifest
changes below. Hashes bind the implementation, old wire model and tests:

| File | SHA-256 |
|---|---|
| `examples/crdt_policy_migration.rs` | `22b9e35114d1f922a01bb04bb252f255c74f63239e16354bc3f8e71172f48d2a` |
| `tests/crdt_migration.rs` | `dc72f99b4114ce931ffe22a29a5ccc9037d683cbe615d0ee0d9e692e9d242427` |
| `examples/support/study.rs` | `d2a119e71a0863fa16ab2da04a8254e8fee9c6584448f2810edf80996522226d` |
| `Cargo.toml` | `90108bda9fe3dc5842aa5b2b162164b89ac27498db7b95a9b613005f4e21396e` |

Root review checked these source bindings and preserved nine local proof
records/logs in `target/local-qualification-crdt-migration/index.json`. The new
test executable also cross-linked with the pinned Windows MSVC toolchain;
zero Windows tests executed. Simulated durable-image coverage is distinct from
native Windows or device power-loss evidence.

The same five tests and example also passed in both debug/release profiles on
Linux aarch64 in an isolated Docker container, with strict targeted Clippy.
The immutable tool image was
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
using Rust 1.99.1. The runner extracted Git archive
`ed8f9f922bc46bd6e52b7d0776f69dad93f3184a` and overlaid only the manifest, example and
integration test whose hashes appear above. Archive SHA-256:
`9274405db24894e4e2fe2d3ad1043337600c7d242185c140a924504764372648`.
Commands used offline/locked Cargo, `CARGO_INCREMENTAL=0`, and a private `/build`
target directory. Local evidence: `/tmp/crdt-migration-linux.json` and
`/tmp/crdt-migration-linux.log`; these temporary paths are not a release artifact
retention promise. This is Linux runtime execution, not cross-compilation.

## Native files and independent processes

The follow-up native fixture lives entirely in `tests/crdt_migration.rs`; it
reuses the example's conversion and staging helpers. Its ignored `native_files::worker`
test is a child-process entrypoint, invoked explicitly by the two ordinary
parent tests. It is not an operational migration CLI. The in-memory example and
five earlier tests remain unchanged in scope.

A child creates two real old-generation files and retains both the default
record and the fully reconciled record in each. The migration child opens both
old files and holds their exclusive native writer leases throughout review and
staging. Holding either old file open in the parent makes migration return
`Busy` before creating the target directory. While the migration child is
parked at every interruption point, both old files reject another writer open.
Once it exits or is killed, the leases are released.

Those leases establish local file quiescence, not a distributed stop lock. They
cannot fence an old writer using a different backup, host or copied identity.
Stopping all old endpoints and controlling routing remains the operator's task;
matching payloads alone never establishes that condition.

Successful staging closes both new files. Each destination is then opened and
verified by a separate fresh child process, with the new enrollment, all five
converted fields, and its retained imported root checked. Separate children also
verify both old files: the entire default retained record and current/reconciled
retained record are checked. The parent compares every encoded record and
revision in the old current and retained roots against its pre-migration oracle.
A final independent child opens both new stores and proves repeat sync makes no
commit. Old history remains in the archived old files; conversion does not relabel
old retained roots as new-generation events.

Six deterministic process-kill points exercise native staging. The parent waits
for an explicit flushed boundary marker, kills and reaps that child, and uses
fresh child processes to inspect the durable destinations before discarding them:

| Kill boundary | Expected destination state after reopening |
|---|---|
| Both old leases acquired | No staging directory |
| First destination created | A empty at revision 0; B absent |
| Both destinations created | A and B empty at revision 0 |
| Migration staged and converged | A and B at revision 3, complete converted records, no retained roots |
| First imported root retained | A at revision 4 with retained root 1 at revision 3; B at revision 3 without a retained root |
| Both imported roots retained | Both at revision 4 with retained root 1 at revision 3 |

Every cut preserves the complete old logical oracle. Each partial staging
directory is removed only after the child exits and releases its leases. A retry
uses a new target directory, logical collection, generation and actor roster;
both destinations reopen independently, all fields/history are verified, and
repeat sync makes no commit. Even a fully staged pair at the last cut is discarded
when there is no operational cutover record. This intentionally avoids guessing
whether an interrupted application switched its routing.

The worker's parent has bounded marker/exit waits and kills/reaps children on
assertion failure. Test-owned directories clean up on exit. These cuts occur
between completed application steps, not inside every system call or durability
barrier. They qualify process interruption on the tested native adapters; they
do not simulate sudden hardware power loss. There is still no durable deployment
journal, distributed routing coordinator or automatic actor replacement.

### Native process qualification results

On macOS 27.0.1 arm64 and Docker Linux 7.0.14-linuxkit aarch64, the complete
migration integration suite passed in debug and release: **7 passed, 1 ignored**
(the ignored entrypoint is exercised explicitly by the parent process tests).
Strict targeted Clippy also passed for the integration test and example. Both
platforms used rustc 1.99.1 (`58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM
23.1.1). Commands, with `CARGO_INCREMENTAL=0` and separate target directories:

```sh
cargo test --offline --all-features --test crdt_migration
cargo test --offline --all-features --test crdt_migration --release
cargo clippy --offline --all-features --test crdt_migration --example crdt_policy_migration -- -D warnings
```

The native test source SHA-256 is
`c91b172b2eca7d5fbfeb6a3525b22f47bd48fc81922a555ed76de75f9f272ddf`;
the example and Cargo manifest retain the hashes in the initial evidence table.
Linux executed an isolated archive of `96e105f80c189a2f9de12530518c4d834f2796ea`
plus those exact three overlays. The base archive SHA-256 was
`64b04352c995246b1f3f1a528f1d7a16539f67f53aa04a13031794a601892528`.
The immutable container image was
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
Temporary evidence is `/tmp/crdt-native-migration-linux.json`,
`/tmp/crdt-native-migration-linux.log`, and the three macOS
`/tmp/crdt-native-migration-{debug,release,clippy}.log` files; these are local
records, not promised permanent release artifacts. No native Windows tests,
sanitizer campaign, or device power-loss tests were run for this follow-up.

### Serial test-harness regression

Root review reproduced a fixture-only failure with `RUST_TEST_THREADS=1`:
libtest prefixed the child test name on the same output line as the flushed
migration marker, so the parent failed to recognize it and reported a disconnected
marker channel. The child invocation now adds `--quiet` while retaining
`--nocapture`, keeping marker lines independent of libtest scheduling. No engine
or migration semantics changed.

After this correction, the full integration suite passed on both macOS and
isolated Docker Linux with default scheduling **and** `RUST_TEST_THREADS=1`,
in debug and release: **7 passed, 1 ignored** in each of the eight runs.
Strict targeted Clippy passed on both platforms. The toolchains, immutable Linux
image, archived base and unchanged example/manifest hashes are the same as the
native qualification above. Final test SHA-256: `a16a3ba5f29d1f83a36fe288311224f5c1d9baf0691b6abcd14c440482eaced7`.
Linux source bindings and terminal exit 0 are recorded in
`/tmp/crdt-native-serial-linux.json`; its log is
`/tmp/crdt-native-serial-linux.log`. macOS logs are
`/tmp/crdt-native-serial-{debug,release,clippy}.log` and
`/tmp/crdt-native-default-{debug,release}.log`. The previous native hash records
the initial parallel-only qualification; this corrected snapshot supersedes it.

Root review independently reproduced the serial failure and reran the positive
case after the fix. The final integration-test executable also cross-linked
for Windows MSVC; no native Windows execution is implied. Current bindings,
serial/default logs, red/fixed controls and cross-link evidence are retained in
`target/local-qualification-crdt-native-migration/index.json`.
