# Coordinated writer handoff

This runbook applies to the authenticated localhost sequenced coordinator and
its durable client journals. It moves a paused, acknowledged checkpoint to
fresh files. The operator chooses the authority and keeps the previous
endpoint stopped. The software cannot establish that another host or process
has been fenced. Rotating the replacement's secret does not disable an old
server that still holds the old secret.

## Before moving files

Record the qualified CLI source, executable hashes, platform/filesystem, and
the complete operator-owned slot assignment list. One logical writer owns
each assigned slot and journal. Server receipt rows cannot enumerate assigned
but unused slots. Pause every writer and wait for its journal commands to exit.

For **each** assigned journal, run these commands against the chosen authority:

```sh
write-coordinator journal status writer.db
write-coordinator journal check writer.db PORT --secret-file current.secret
```

Reconcile pending requests against that authority before taking an acknowledged
checkpoint. `pending-eligible` is insufficient to select an authority: an older
same-identity backup can report it after the original server committed the
request. Use the [pending recovery procedure](coordinator-pending-restore.md)
and reconcile external effects before replay. Stop if the authority or history
is uncertain.

Require every check to report the **same** `aligned REV VALUE`. Record that
checkpoint and the exact lineage, slot and receipt from every status. Stop the
old server, verify its process/lease retirement, and keep submissions paused.
An inaccessible endpoint alone is not proof of retirement.

## Restore and verify before routing writers

Use [offline verified backup and restore](operations.md) on the coordinator
and every journal. Give every backup and destination a fresh path; retain the
originals and the complete backup set with member names and hashes.

```sh
weekend-admin backup coordinator.db coordinator.backup
weekend-admin backup writer.db writer.backup
weekend-admin restore coordinator.backup restored-coordinator.db
weekend-admin restore writer.backup restored-writer.db
weekend-admin check restored-coordinator.db
weekend-admin check restored-writer.db
```

Repeat the journal operations for the complete slot list. Verify the selected
revision, identity, retained roots and application data; a generic file check
does not validate an application's reserved rows. A stale server or a journal
from a different checkpoint is not a coherent recovery set merely because
each file is structurally valid.

Provision the replacement credential under the intended account's private
directory/ACL policy. Keep the old endpoint stopped and start the restored
coordinator on an isolated loopback port using that new credential. With
submissions still paused, require every restored status to match its recorded
checkpoint and every live check to return the common `aligned REV VALUE`.
Verify old credentials are rejected if rotation is part of the cutover.

`journal resume` with no pending intent returns the locally saved receipt.
That result alone neither contacts the server nor proves authority freshness.
The live `journal check` is a separate observation. Neither result is a
snapshot follower's durable acknowledgment.

Route the assigned writers to the verified server and their restored journal
paths. Continue with conditional writes and normal conflict handling. Keep
the originals, backup hashes and authority decision until the continued state
has been checked. A failure before routing leaves writers paused; preserve
the failed outputs and select a recovery action explicitly. Never start both
copies to see which one appears newer.

## Executable rehearsal

The existing private `cargo xtask incident-recovery` command now runs this
procedure for synthetic slots **0 and 1023**, in addition to its physical,
logical and one-journal recovery cases. It uses the selected real CLI binaries,
new owned files and the existing bounded process supervisors. Evidence is
private at directory creation. All synthetic credential files are removed
before the final inventory, including when the drill fails.

| Step | Required observation |
|---|---|
| Original writers commit 42 then 77 | Server revision 5; both checks `aligned 5 77`; slot receipts `1 3 42 4` and `1 4 77 5` |
| Stop the owned original, back up all three files, restore fresh paths | Every restored file equals its backup; originals/backups remain unchanged |
| Inspect an isolated revision-3 server | Both restored journals reject `behind 3 0`; no replay is submitted |
| Pair the revision-5 server with a pre-write journal | `receipt-mismatch 5 77`; the fresh raw reader also rejects that mixed set at its journal-revision check |
| Start the chosen revision-5 set with the replacement credential | Old credentials reject `Unauthorized`; both statuses and aligned checks match the checkpoint |
| Report saved receipts without writing | Local acknowledgments 4 and 5; server and journals retain their exact bytes |
| Continue both writers | Slot 0 commits revision 6/value 99; slot 1023's stale expectation conflicts before staging and preserves its journal bytes; its revised write commits revision 7/value 111 |
| Stop and read in a fresh core process | Exact protocol marker, counter, both binary receipt rows, lineage, slots, journal revision/flags and empty pending fields match the independent fixed oracle |
| Repeat local resume with no listening endpoint | Saved acknowledgments 6 and 7; all three files remain unchanged |

The checkpoint is retained as `handoff-checkpoint.json`, including both journal
statuses and the three backup hashes. `evidence.json` records actual CLI and
consumer hashes, command outcomes, cleanup and the final inventory. Successful
CLI invocations require empty stderr. The observer uses documented raw version-1
rows through the public core API and does not import coordinator codecs.

```sh
cargo +1.99.1 xtask incident-recovery \
  --admin /absolute/qualified/weekend-admin \
  --coordinator /absolute/qualified/write-coordinator \
  --artifact-source FULL_SELECTED_CLI_SOURCE_ID \
  --output target/new-operator-handoff
```

This finite rehearsal does not qualify arbitrary slot sets, application
schemas, remote/HA authorization, external effects, deployed account policies,
device power loss, or the operator's actual authority decision. CLI source
provenance remains separately established by delivery/build evidence; the
command's `--artifact-source` is declared metadata. Native Windows retains its
own execution gate; cross-checking its build is a separate observation.

## Frozen macOS and Docker Linux execution

Controller `f5287e6f503dfe0147836f61fd6de0b597909474` passes the complete
rehearsal on native macOS arm64/APFS and Docker Linux arm64/ext4 using Rust
1.99.1. The administrative and coordinator binaries are built from clean
selected source `2bc8e795934af30d108dae7bc6c7b0cbd5c4715b`, separately in debug
and release. The fresh independent consumer uses the controller's source and
debug profile in both runs. Each of the four runs passes **85 command/service
outcomes**, including eight verified service retirements. Both source trees
remain clean throughout their builds/runs.

The independent audit verifies all 286 retained payloads per run, actual
selected/retained executable hashes, all 336 scoped inputs against the frozen
Git blobs, exact checkpoint/conflict/rejection/observer/local-replay outputs,
and every command's stream sizes and cleanup reports. All three synthetic
credential files are absent. Backup member hashes match the checkpoint file.
Source inputs match across both operating systems; executable hashes retain
their platform/profile scope.

| Platform / selected CLI profile | Evidence SHA-256 |
|---|---|
| macOS / debug | `91fb416e559432bf2500fb9cce522e3dfdc926d5634f60fa1640657140243c80` |
| macOS / release | `a7c97e78a3164cf90109835aab414a117f6a10292dd171977409bd5319014a8d` |
| Linux / debug | `5223b6ea051d8f3260849200b8604ab9e4d6d9160ad4f2a38de2485d7e398bcc` |
| Linux / release | `c258da593ef5300f2844bab96861d0eb8b33d5eb5d0ece1edf423f309cba237a` |

The complete macOS xtask suite passes **237 tests, zero failures and two
explicit ignores per profile**. Both CLI transcript adapter controls pass
in each profile on Linux; strict all-target Clippy passes for xtask and the
consumer on both hosts. Windows MSVC test/consumer cross-checks compile.
Standards and specification reviews are both clear. The source reconciliation
records 8,360 decisions in 134 files with 3,596 blind spots and no failures;
new tooling records and the separate consumer's coverage obligations remain
open rather than becoming MC/DC claims.

The Linux image is
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
Source, locked vendored dependencies, Cargo home, every build target, temporary
files, native databases and evidence use the owned ext4 volume. External
networking is disabled. The source bundle SHA-256 is
`506db5298a6789b9ec576bc272046f5994b284659005d53909f29a0174f34472`;
the locked-dependency archive SHA-256 is
`fcb7eba9136ea9989ebc65d000cf39cf97487036c8367115ffc2966f8f14b5cc`.
Both archives are checked before extraction/use. The worker runs as UID 0,
which does not establish permission denial against root. The final container
exits 0; its container and volume are removed and their absence is verified
after copying and auditing the evidence.

The first Linux rehearsal passes its operations but inherits `/build` for
helper tests/Clippy, outside the owned volume; it receives no complete
all-builds-on-ext4 qualification. A subsequent full-cache copy exhausts setup
space before any database operation. Replacing that copy with only the locked
dependencies uses about 144 MiB of vendored source (a 154 MiB archive).
The checksum guard then rejects a
premature archive hash before extraction; hashing after archive completion
corrects preparation. These setup reports remain separate from the passing
final run. All of their owned containers and volumes are also retired.

The initial macOS development run also catches an incorrect new-oracle
expectation: a stale journal preflight never stages its rejected intent. The
corrected oracle requires final local journal revision 6 in both slots, and
the fixture checks that the preflight conflict preserves the exact journal
bytes. Production journal behavior is unchanged.

Local proofs are under `target/r2/handoff-macos-f5287e6-{debug,release}` and
`target/r2/handoff-linux-final-f5287e6`, with `handoff-logs/frozen-audit.json`
and retained controller binaries. These ignored local files are not durable
off-machine custody. The three-platform CI job selects the same CLI source,
executes both selected CLI profiles, and records its own controller identity.
Its attempt-specific upload excludes
all synthetic credential files. Native Windows execution, deployment-account
acceptance, external authority/fencing, and the final release remain separate
gates. Subsequent documentation does not relabel this frozen execution.

## Hosted native follow-up at PR #6

Head `3de695f70284ce562dcc6e03343a3f029f6b0abb` passes all 22 checks.
The tested merge `75c20a9ea174741cc6bcc6215d84792851c072db` has the same Git
tree. The [Engine run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44208666922)
executes both selected CLI profiles on native Linux x64, macOS arm64 and
Windows x64. Every one of the six rehearsals passes 85 outcomes and eight
service retirements with selected CLI source `2bc8e79`.

The downloaded original ZIPs match these digests:

| Platform | Artifact ID | ZIP SHA-256 |
|---|---|---|
| Linux | `14385271355` | `af0c61333b21d8d0abde93bb2e4a26265c7545ccce6f23b1a67bd2a6f24bc87e` |
| macOS | `14384259616` | `dca598ce3c776bd354063dfe9683bf2d40285c7a307f149477e77ffd96738911` |
| Windows | `14384163898` | `468435fcc81430cb25b6302709fe73a44be07fda061ecaff548f480e7ea23ed7` |

The independent extraction audit checks complete inventories, actual retained
binaries, exact checkpoint/rejection/continuation/observer outputs, backup
hashes and every command/cleanup report. Unix proofs contain 286 payloads and
336 scoped inputs each; Windows proofs contain 201 payloads and its existing
33 selected inputs. Every declared input matches the tested Git merge.
Windows process reports are inline in its evidence; it does not export the
Unix per-command process files. All synthetic credential files are absent.
The Windows owner SID is the hosted administrative account, retaining the
ordinary/service-account deployment gate.

Workspace ASan and TSan each pass 766 tests with seven explicit ignores; both
archives and all 336 recorded input hashes are independently checked. TSan's
errno-reuse and both expected race controls pass. Original sanitizer archive
digests are `24a53042be80b08172f849780749f518c54b0ef1f17bcbfb70a6b0487e6ee0f5`
(ASan) and `19c52cf9a415a24c68b20e7614f65b2bbd9f3238f7d79b2777700f1e503553a5`
(TSan). Native platforms, minimum compilers, quota, R1, coverage and both bounded
fuzz jobs also pass. No production code or durability barrier changes.

Hosted reports and the independent audit are retained under
`target/r2/handoff-pr6-ci/`. This closes the named native Windows execution
gap of the frozen local result above. External authority/fencing, actual
deployment/account acceptance, further promised profiles and final release
custody remain open.
