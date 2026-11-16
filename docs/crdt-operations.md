# Operating two enrolled CRDT writers

The core supports a fixed roster of exactly two actors for each replicated
record. A `Policy` binds a nonzero logical collection identity, a nonzero
policy generation, and two nonzero actor IDs in ascending order. Each
actor owns one writable database history. A database's physical store
identity and the policy's logical collection identity serve different
purposes. Copying a database does not allocate a new CRDT actor.

Enroll a record once through `Replica::enroll`. Reopen an existing actor
with `Replica::open` and the exact same policy and actor identity. A
record that already exists rejects enrollment. Opening a stored row as
the other actor rejects, and a different policy generation conflicts.
Use dedicated collections and reserve each `Record::KEY` for its replica.
The general raw database API can overwrite these rows, so the application
must enforce this ownership. Corrupt bytes and mismatched enrollment are
detected at decode/open, but the engine cannot distinguish a valid forged
same-actor state from an authorized raw rewrite. Raw writes bypass the
CRDT monotonicity and actor checks.

Local `update` validates that the actor advances only its owned components
and that the edited state covers the prior state. Before encoding an
outgoing or stored state, the engine now requires the application's
`Record::encode` output to decode back to the same payload. A lossy codec
returns an error before publication. Applications still supply lawful
merge functions, field codecs and deterministic resolution policies.
The required merge laws are associativity, commutativity, idempotence and
validity preservation. Runtime checks cannot prove these laws for an
arbitrary implementation.

Incoming state must have the same logical collection, generation and
actor roster, and claim the opposite actor. Actor-owned fields reject
incoming advances attributed to the local actor. The receiver merges,
optionally resolves conflicts as a local edit, and commits before replying.
Its reply covers the offered state. The sender independently validates
the policy, actor-owned progress and coverage before accepting that reply.
An authenticated channel must bind the remote endpoint to the configured
other actor. Merely putting an actor ID into the frame is not authentication.
The core reads one bounded frame from a fresh channel and the caller closes
it after exchange. It does not treat trailing stream bytes as another
message. The `study-peer` application supplies TLS identity checks and
transport deadlines.

A lost reply can follow a successful merge commit. Reopen and retry the
same state offer. The receiver's monotone join absorbs duplicates without
a new commit, and returns its current covering state. Local commit success
and a remote durable reply remain separate observations. A local
`OutcomeUnknown` requires recovery and application reconciliation before
replaying a non-idempotent edit such as an increment.

## Restart, backups and policy evolution

An ordinary restart reopens the same actor's current durable history.
A restored older backup cannot safely resume writes as that actor. If the
peer remembers newer progress for the restored actor, the restored
sender rejects the peer's reply because it advances its local actor
component. That protects acknowledgment integrity. It does not reconstruct
lost acknowledged events. If neither peer remembers the lost progress,
the core has no global authority that can detect the rollback. Reusing an
actor sequence for a different register value can also conflict.

Keep old backups read-only for inspection/export. Recovery into a writer
requires an explicit migration and fresh actor/collection enrollment,
with application reconciliation of old events and peer state. Such a
migration is currently an application design task. The library does not
silently re-enroll a stale writer or support actor replacement in place.
Physical backup and logical export preserve the old identities and do
not perform this migration.

The [policy/schema migration example](crdt-policy-migration.md) now supplies an
executable reference workflow: reconcile and freeze the old pair, review visible
state, seed a converted aggregate once into a fresh roster, reopen and verify
both destinations, then change application routing. A stale-backup scenario uses
the surviving peer's remembered events without resuming the retired writer.
Cross-store staging and operational cutover remain application responsibilities.

Changing a field's wire layout or resolution semantics requires an
explicit record format marker and policy-generation decision shared by
both peers. Mixed generations conflict before writing. A new generation
alone does not convert old data or establish a new roster. Stop and
reconcile the old pair, design and verify the conversion, then enroll
the new pair with an explicit relationship to the prior history.
Membership growth, gossip, discovery and automatic actor replacement
remain outside the two-actor contract.

## Staged roster cutover

`study-peer` can stage a fresh two-actor history for the same StudyProgress
schema. Keep both old databases and their retained roots as the archive for
the retired history. The [schema and policy migration example](crdt-policy-migration.md)
shows a separate conversion when the units, payload layout or resolution
policies change.

Each endpoint uses the same versioned policy file with one marker, a
32-hex-digit collection ID, a nonzero decimal generation and two ascending
nonzero 32-hex-digit actor IDs. Files are limited to 256 bytes. Slots `a`
and `b` select the first and second actor IDs. Use a fresh logical
collection, a higher generation and two unused actor IDs for the cutover.
For an existing default deployment with collection `0909…0909`, example
old and fresh policy files are shown below. Allocate unique IDs for a real
deployment.

```text
study-policy-v1 09090909090909090909090909090909 7 01010101010101010101010101010101 02020202020202020202020202020202
study-policy-v1 0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a 8 03030303030303030303030303030303 04040404040404040404040404040404
```

1. Freeze edits on both old actors and reconcile their state with the old
   authenticated routes. Use `--sync-only` to allow synchronization and
   disable `edit` and `practice` on a restarted endpoint. Stop both actors
   and preserve the old files. Local exclusive leases protect the files
   opened by the command. Operators also stop any remote or copied writers.
2. Capture a checkpoint from the reviewed old pair while both files are
   quiescent. Keep the checkpoint in the trusted cutover record before
   substituting any archived or restored copies. Then stage against it.

   ```sh
   study-peer checkpoint-roster old-a.db old-b.db old.policy > old.checkpoint
   study-peer stage-roster old-a.db old-b.db old.policy new-a.db new-b.db new.policy \
     --expect-old old.checkpoint > stage.report
   grep '^study-roster-stage-v1 ' stage.report > stage.receipt
   ```

   Check both commands exited successfully before using the staging receipt.
   Each old-pair command holds both old leases and requires exact equality
   of the causal payloads. Guarded staging checks the checkpoint before
   creating either destination and checks that the practice total fits in
   one actor component. It seeds the total, completed and currently planned
   topics and effective register values on actor A only. Old causal dots
   and tombstones stay in the old files. Save the report with the checkpoint
   and receipt. An error may leave one or both partial destinations. Keep old
   routes frozen and use fresh paths for a retry after inspecting the failed
   pair.
3. Provision and pin fresh TLS identities for the new actors. Start both
   new stores with `run-policy` and `--sync-only` on staging routes, using
   their respective policy slot and peer address.

   ```sh
   study-peer run-policy new-a.db a new.policy 127.0.0.1:8301 127.0.0.1:8302 \
     a-new b-new/cert.der --sync-only
   study-peer run-policy new-b.db b new.policy 127.0.0.1:8302 127.0.0.1:8301 \
     b-new a-new/cert.der --sync-only
   ```

   Wait for both sides to report covering peer acknowledgments. The TLS
   pin identifies the configured peer. The core also compares the
   collection, generation and roster in every incoming frame.
4. Stop the staging processes and verify the quiescent files together.
   For a split-host setup, verify quiescent native copies of both files
   and keep the deployed staging files frozen during verification.

   ```sh
   study-peer verify-roster new-a.db new-b.db new.policy --expect-staged stage.receipt
   ```

   Verification requires exact causal payload equality and checks that the
   pair matches the staging receipt. Once verified, route the new actors
   to each other and restart them without `--sync-only`. Keep the old
   routes and actor identities retired for writes.

The filesystem writes and route change are separate operations. Before new
writes, the operator can discard the staging pair and keep the frozen old
history. Once new writes are acknowledged, reconciliation must preserve
them before any further migration. The operator must choose the old pair
that covers all acknowledged edits before generating the checkpoint.

The checkpoint and staging receipt are each one ASCII line with a version
marker, a space, a lowercase SHA-256 digest and a line feed. Preserve that
line ending when extracting the receipt on Windows. Both bind the
ordered storage identities, policy and exact encoded StudyProgress record.
The checkpoint also binds both old revisions. The staging receipt compares
the agreed seed payload after exchange, so its digest can survive B's local
revision advancing during staging. These are trusted operator records. Store
them independently of old database restores and protect them alongside the
policy files. They cover the selected StudyProgress record. Archive
verification and quiescence of remote copies remain operator duties.
The shorter forms of `stage-roster` and `verify-roster` still perform their
pair-equality checks for existing callers.

The digest hashes, in order, the marker bytes with no delimiter, the
16-byte collection ID, the generation as an unsigned 64-bit little-endian
integer, both 16-byte actor IDs, and the storage identity for each slot
A then B. In the checkpoint, each storage identity is followed by its
unsigned 64-bit little-endian revision. The final inputs are the 8-byte
StudyProgress format marker and the canonical `Record::encode` payload.

### Guarded cutover results

The native process test at `0608bf5807545e6b1a976da87220f26cf175fb13`
checks both guards with the actual `study-peer` executable. It keeps the
old files and their retained roots byte-identical.

| Input to guarded cutover | Result |
|---|---|
| Two agreeing restored copies from before the acknowledged old edits | Staging returns `Conflict` before creating either destination |
| Both archived lineages brought to the selected revision numbers using retained-root commits, with their older causal state intact | Staging returns `Conflict` |
| The selected causal state in both files, with one extra retain commit | Staging returns `Conflict` |
| Two exact copies of the reviewed files | Checkpoint is identical |
| The selected new pair before authenticated sync | Verification returns `Conflict` |
| The selected pair after authenticated sync, using a receipt from a separately staged pair seeded with the same record | Verification returns `Conflict` |
| The selected receipt after sync and a staging peer restart | Both files verify at revision 3 with 12 minutes, completed topics 1 and 2, planned topic 4, goal 40 and limit 50 |
| The original staging receipt after both peers accept new edits and converge | Guarded verification returns `Conflict`. The resumed pair reaches 19 minutes on both actors, and repeat sync adds no commits |

Three separate test mutations omit the old revision, omit the exact record
bytes, or omit the staged storage identities from the hash. The native
case fails each time at the corresponding acceptance check. The existing
test still checks authenticated old-generation rejection, fresh TLS pins,
and disabled local edits during staging.

macOS arm64/APFS with Rust 1.99.1 and Docker Linux arm64/ext4 with Rust
1.99.0 passed all 20 app tests in debug and release, plus strict Clippy.
The former pre-ALPN CLI replay on macOS also passed with this new binary.
The three Windows MSVC x64 test executables cross-linked using
`cargo-xwin 0.23.1`. Native Windows execution of the guarded profile
requires its own result.

```sh
cargo test --locked --offline -p study-peer --all-targets
cargo test --locked --offline --release -p study-peer --all-targets
cargo clippy --locked --offline -p study-peer --all-targets -- -D warnings
cargo xtask study-cli-upgrade --new 0608bf5807545e6b1a976da87220f26cf175fb13 \
  --output target/guarded-roster-upgrade
```

The local archive `target/archived-proof/study-roster-cutover-0608bf5.tar.zst`
contains the frozen source, native logs, mutation outputs and upgrade
evidence. Its SHA-256 is
`abc0ddce7d1ab9de75c03adf0b02b3910053557eb2fcb97d0467c3f33dd854ce`.

## Bounded Study peer admission

At most four incoming connection workers handle TLS sessions, each with the
three-second deadline set when accepted. Only one authenticated incoming
exchange can hold the merge and reply path at a time. Admission closes an
excess socket at either boundary. The sender retries with its existing
backoff. `quit` closes the listener and cancels pending TLS handshakes
between bounded waits. An admitted merge may finish its disk operation and
reply during shutdown.

Incoming errors update bounded in-memory status. On its local input,
the operator can request `incoming` to see served exchanges, failures,
busy refusals and the last incoming error. A served exchange can be a
replay, so the count is separate from the number of commits. `local`
reports acknowledged edits to the local store. `peer` reports covering
replies to that process's outgoing offers.

On the preceding build, one rejected handshake against a full diagnostics
stream left the receiving worker blocked in its stderr write. The
authenticated sender reported zero acknowledgments after six seconds.
Disabling just the write recovered the exchange in the control run.
The regression at `fdc04a5babae4bef070bfb0147c3e55e620eacd8` fills a
Unix socket used as stderr and keeps it full through the authenticated
exchange and clean shutdown. The receiver durably merges the edit. The
sender observes the covering reply, both stopped files reopen with the
same causal payload, and the diagnostics stream contains its original bytes.

A separate test holds four raw connections, observes refusal of a fifth,
and frees capacity for an authenticated exchange while raw connections
remain stalled. A second authenticated connection reaches the busy gate
without entering the merge. The first exchange replies, and stopping joins
the stalled handshakes within two seconds. Three separate negative
controls remove the session cap, remove merge serialization or disable
handshake cancellation. Each triggers the corresponding test failure.

All 22 app tests pass in debug and release on macOS arm64/APFS with Rust
1.99.1 and Docker Linux arm64/ext4 with Rust 1.99.0, with strict Clippy and
12 additional repetitions of both new tests per platform. The pre-ALPN
upgrade replay passes on macOS. Windows MSVC x64 cross-links all three test
executables. Native Windows execution is a separate result.

The release macOS-to-Linux [peer soak](study-peer-soak.md) passed ten
offline-edit and convergence rounds with forced restarts of both peers
after rounds five and ten. Every round verified the two covering reply
counts, displayed state and stable revisions during repeat sync. Both
peers reopened at revision 22 with 165 minutes after the second restart.
The audit verified all 149 controller payload hashes, 719 source hashes,
the macOS binary, both final database hashes, and removal of the owned
Docker container and volume. The soak evidence SHA-256 is
`7743dcc421945f0dfedbd10233461ed368b0c82add9e51a51e8582d1d23a643f`.
The finite run qualifies these limits and schedules. Service under
continuous saturation and deployment latency budgets remain separate.

```sh
cargo test --locked --offline -p study-peer --all-targets
cargo xtask study-peer-soak "$PWD/target/study-admission-soak" \
  --source fdc04a5babae4bef070bfb0147c3e55e620eacd8 --rounds 10 --restart-every 5 \
  --image sha256:d276cf4b0967c0337be30eb9c302bf3b77794a7ade6b667968ebe5cbc037317c
```

The local archive `target/archived-proof/study-admission-fdc04a5.tar.zst`
contains frozen source, tests and negative controls, soak logs, public
test certificates and both final databases. Its SHA-256 is
`b03573991e5f14c51425599efd2e0556860b5c6161c9d20e4112473c45f3fa5a`.

## Evidence and fuzzing

The existing CRDT tests exercise stale backup rejection, generation
mismatch, lost merge replies, local edits racing incoming commits,
add-wins behavior, multi-value conflict resolution, automatic policies,
reopening and repeat sync without commits. The new lossy-codec regression
checks that an invalid application codec cannot publish an unreadable
edit or advance its local revision.

`crdt_frames` feeds arbitrary or repaired valid counter frames through
both acknowledgment validation and receiver merge. Rejected inputs must
leave state and revision unchanged. Accepted receiver frames must preserve
the local component, produce the independently calculated sum, and replay
without a commit. `crdt_schedule` composes a grow-only counter and set in
one record over two simulated durable stores. A separate pair of integer
components and membership bitsets predicts every peer state through
bounded offline edits, delayed/duplicate offers, lost replies and reopen.
Final exchange must converge, and another exchange must leave revisions
alone. This model covers those containers. Register causality and custom
policies retain their deterministic tests and need additional fuzz models.

Generate permanent seed inputs and run replay without the fuzz runtime.

```sh
cargo run --offline --locked --manifest-path fuzz/Cargo.toml --example crdt_seeds
cargo test --offline --locked --manifest-path fuzz/Cargo.toml --lib crdt::
```

Run each target under the same nightly AddressSanitizer workflow as the
other targets, using a temporary mutable corpus and `fuzz/seeds/TARGET`
as the seed directory. Schedule inputs contain up to 24 command/argument
pairs. The command low bit selects an actor, and the next bits select
increment, insert, send, save offer, replay saved offer or restart.
Frame inputs select raw bytes with an even first byte or a valid counter
frame plus up to 16 offset/XOR patches with an odd first byte.

On arm64 macOS, all 11 CRDT integration tests passed in debug and release,
and deterministic fuzz replay and warnings-denied Clippy passed. Both
CRDT fuzz targets completed 5,000 AddressSanitizer executions with seed 1
and maximum input length 128 without findings. These are smoke results
for the initial corpora. All-feature test cross-compilation passed for
x86_64 Windows MSVC and x86_64 Linux GNU.

## Check an application's record laws

Copy [the record law helper](../examples/support/record_laws.rs) into an
integration test and call `assert_merge_laws(&states)` with snapshots from
one compatible history. Include stale states, independent offline edits and
their joins. Keep divergent actor histories and admission tests separate.

The [StudyProgress test](../tests/crdt.rs) supplies seven reachable states.
They include a concurrent observed removal and addition, conflicts in both
registers, the committed maximum and minimum resolutions, and a subsequent
assignment. The helper checks idempotence, deterministic and commutative
pairwise merges, associativity over every triple, and codec round trips
for the inputs and merge results. A deliberately left-biased merge fails
the same helper. The sample covers this history and its tested joins.

Run these tests with `cargo test --offline --locked --features crdt --test crdt`.
All 13 tests passed in debug and release on macOS and Docker Linux. The log
SHA-256 values are `6e4b2b673b230e77301b313f7d82c7fff612724a2d573f8c41004528f1f01e47`
and `52ae5b5e113b1d5e180e3c8560bfe3d7d556fb9640308a42f99cc83b68d73c46`.
