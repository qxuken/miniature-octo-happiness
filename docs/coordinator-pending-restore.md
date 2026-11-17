# Restoring a coordinator and an unresolved journal

The R2 matrix in
`apps/write-coordinator/tests/writers/pending_restore.rs` exercises the real
coordinator and journal CLI processes against native files. It covers the
unresolved-intent cases absent from the earlier acknowledged-checkpoint drill.
Local coordinator commits and journal acknowledgments remain distinct from a
snapshot follower's durable acknowledgment.

## Capture and recovery

The fixture retains coordinator revision 3, then creates a journal for slot 0.
Two cases also acknowledge an initial write of 42 at revision 5. An owned
loopback relay passes the journal's real preflight request and accepts its
complete write request. It stops the client while withholding the reply.
Depending on the case, it leaves that request unforwarded or forwards it to the
authority after the client has stopped. This specifies the cut: the forwarded
branch commits after the requester is gone. The existing
`journal_restarts_after_lost_reply_and_rejects_incompatible_history` test
separately covers a server commit observed before terminating the client.

With both processes stopped, the fixture retains the pending journal root,
backs up both files through the public storage interface and restores them to
fresh paths. Each restored file must exactly match its backup. New observer
processes open both files and independently check the version-1 protocol marker,
counter, slot receipt, journal identity/flags/fields, and both retained roots.
They use the public core interface and the fixed row layouts, rather than the
coordinator's private encoders or decoders.

| Prior journal acknowledgment | Captured request forwarded | Captured coordinator revision/value | Captured journal revision | Final coordinator revision/value | Final receipt | Final journal revision |
|---|---|---|---:|---|---|---:|
| None | No | 4 / 0 | 4 | 5 / 42 | `1 4 42 5` | 5 |
| None | Yes | 5 / 42 | 4 | 5 / 42 | `1 4 42 5` | 5 |
| `1 4 42 5` | No | 5 / 42 | 6 | 6 / 99 | `2 5 99 6` | 7 |
| `1 4 42 5` | Yes | 6 / 99 | 6 | 6 / 99 | `2 5 99 6` | 7 |

Recovery loses its first reply too: the resumed client stops before the relay
forwards the exact captured request. The coordinator returns the original
durable receipt, while the journal remains byte-identically pending. After
another coordinator restart, a normal resume records that receipt locally.
Repeating resume with the endpoint stopped returns the saved local
acknowledgment and leaves both files unchanged. It provides no new remote
observation. An already committed request must not change the coordinator's
file bytes during replay.

Every final coordinator retains revision 3 with value 0 and no slot receipt.
Every final journal retains its original pending root: revision 3 without a
prior acknowledgment, or revision 5 with the prior acknowledgment. Those old
views stay unchanged when the current journal records the new acknowledgment.
All original files and all backup files remain unchanged.

## Authority and stale restores

The matrix also starts an isolated older same-identity backup at revision 4,
value 0. A journal with no previous acknowledgment reports
`pending-eligible 4 0`, even when the real captured authority had already
committed revision 5. Eligibility alone cannot establish that a backup is the
current authority or that no effect occurred. The fixture never submits a
replay to that endpoint. With a prior acknowledgment, the stale check rejects
with `behind 4 0`; after final acknowledgment it rejects in every case.

An incident operator must establish the authoritative checkpoint and fence the
old endpoint before permitting replay. A journal cannot reconstruct newer
server history that was discarded with a stale backup. External side effects
still need their own idempotency/outbox policy.

The fresh-reader negative control supplies this older image where the final
checkpoint is required. It must execute the named observer test and reject
specifically at the coordinator revision check, preserving both files.
Positive reader invocations require one actual passing test; a misspelled test
filter or empty execution cannot qualify. Interrupted clients must produce no
result before their stdout closes.

## Reproduction and scope

```sh
cargo test --offline --locked -p write-coordinator --test writers \
  pending_restore::restored_pending_pairs_replay_once_and_preserve_retained_roots \
  -- --exact --nocapture
cargo test --offline --locked --release -p write-coordinator --test writers \
  pending_restore::restored_pending_pairs_replay_once_and_preserve_retained_roots \
  -- --exact --nocapture
```

The normal coordinator test suite and existing native CI execute this case;
it adds no ignored test. Observer environment variables are internal fixture
controls, not application configuration. Temporary databases are removed by
the existing owned-directory fixture. Runtime qualification must identify the
source, test/CLI executables, platform and toolchain of its actual run.

This is a source-built CLI and native storage recovery matrix, not the final
delivered-artifact qualification. It does not simulate torn writes, device
power loss, arbitrary slots or side effects, live overlapping authorities,
automatic fencing, deployment-account permissions or operator sign-off. Those
retain their roadmap gates. Production protocol and storage code are unchanged.

## Initial local results

The macOS arm64/APFS run uses Rust 1.99.1, merged runtime `b5493a3` and the
new process-test overlay. All four schedules and their stale-reader rejection
controls pass. The complete coordinator package passes 32 tests with two
explicitly ignored extended workloads in each of debug and release; strict
all-target Clippy passes. The frozen follow-up qualification below records its
exact source and binaries.

## Frozen source qualification

Source `4cc267821c5d8a6384acdaa8bbe48bbbac70edd0` passes the four schedules
in debug and release on macOS arm64/APFS and Docker Linux arm64/ext4 with
Rust 1.99.1. Each focused invocation executes one passing, nonignored test,
including the fresh-reader negative control. Both source checkouts remain
clean. Strict all-target coordinator Clippy passes on both platforms. The
Windows MSVC test cross-check compiles; native Windows execution remains open.

Linux uses immutable image
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
Source, registry copy, builds, temporary databases and evidence are on an owned
ext4 volume. Networking is disabled except for the owned loopback processes.
The source bundle is verified before checkout; its SHA-256 is
`619c116e2dcf147eb7108cbaa4c717eab62619e4954b66f9249d0474f211f868`.
Cargo build metadata identifies the test and CLI executables. Their retained
copies match these SHA-256 values:

| Platform/profile | Coordinator CLI | Writers test executable |
|---|---|---|
| macOS/debug | `de3431b91d71151bde2b0de04f19d12de7d4ce3b8b9ff7888e67827b2e986227` | `a9afe637646675a9e9b4e01d9d3d50ac34b85e5984ad18498ceafda3e225b349` |
| macOS/release | `349a6da94480997c447d7033ee9c05443cc361e3d8ee4ad6d858f043836e1b5c` | `4d5c72c6117c55fc76619ba2d7e7014b71a83af3c9c94b2b49be424f1dd76a2c` |
| Linux/debug | `e7b343ad81c270c148207823a233b496ae37a009bdf84641b9580b390cb79873` | `5f62cfb471026d792d31dede8bf0e2624b708010a6c67e8af11befb40f9c053b` |
| Linux/release | `62244f9a31199cd6ceaed598c5fc3ce8a48912f904ccba77be5d15f9607ff18d` | `1dd9af9a566f8b334ba4136cae386319b4c79487bb737a472669e71ffbabbd75` |

Local logs, build metadata, retained executables and audits are under
`target/r2/pending-mac-4cc2678`, `target/r2/pending-linux-4cc2678` and
`target/r2/pending-{mac,linux}-audit.json`. These ignored local files do not
provide durable proof custody. R2 still requires native Windows execution,
delivered-artifact qualification and operator/deployment handoff; R7 retains
the custody requirement. Subsequent documentation-only changes do not imply
that runtime qualification was rerun on a different source.
