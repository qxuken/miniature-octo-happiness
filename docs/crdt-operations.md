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
