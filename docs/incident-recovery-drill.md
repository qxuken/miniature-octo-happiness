# Administrative incident and recovery drill

The current [R2 refresh](r2-qualification.md) adds native Windows support and
uses four owned service lifetimes: the read-only local-acknowledgment phase
retires before raw byte comparison, then a fresh service performs the continued
write. Windows' exclusive writer lease deliberately excludes raw live reads.
The updated Mac/Linux drill has 39 expected outcomes; the original 38-outcome,
three-service results below retain their original source and scope.

The executed temporary paths below are historical. After restoration checks,
their indexed payloads were retained in the
[operational qualification archive](roadmap-status.md#administrative-recovery-and-service-output-rehearsal)
and the temporary trees were retired.

On 2026-11-16, a fresh Rust consumer passed a finite rehearsal on native
macOS arm64/APFS using the retained debug `weekend-admin` and
`write-coordinator` executables from tested merge
`7e0e44f98809b5a82a223604f05425dae3c9ab2b`. The commands ran in new processes
against synthetic fixtures in a newly created private temporary directory.
No user database, credentials or endpoint participated.

The maintained command is `cargo xtask incident-recovery`, implemented in
[`xtask/src/incident_recovery.rs`](../xtask/src/incident_recovery.rs). Its tiny
separate [core consumer](../tools/incident-recovery/src/main.rs) only reads the
restored fixture and asserts the two roots. The runner accepts explicit CLI
paths, an operator-declared artifact revision and a **new** output directory.
All fixture/database writes occur inside that directory; normal Cargo build
outputs use `target/incident-recovery` and the configured Cargo cache.

Every subprocess uses the existing `Campaign` supervisor, bounded logs and
verified descendant retirement. Ordinary CLI commands and readiness waits
have 15-second deadlines; build and service lifetimes have 180-second limits.
After the Campaign's standard identity setup, child output is capped at 1 MiB.
The three owned loopback services run one at a time; each is retired before
the next offline copy or service. The test-only credential is removed before
retaining the result.

## Operator decisions exercised

Follow [operations.md](operations.md), [logical-format.md](logical-format.md)
and the coordinator's [coordinated backup/restore runbook](../apps/write-coordinator/README.md#back-up-and-restore-a-coordinator-with-active-journals).
Pause writers and stop the original endpoint before offline maintenance.
Preserve the original files, client journals, failed outputs and their hashes.
Use read-only inspection before selecting a recovery source. A failed newest
graph is an error; an older readable graph does not authorize automatic
fallback or salvage. Restore a verified source only to a fresh destination,
then validate its application state before routing writers to it.

| Rehearsal step | Observed result |
|---|---|
| Import frozen `tests/fixtures/logical-v1.bin`, remove source write permission, inspect and back up | Format 2, witnessed revision/data revision 4, retained ID 1 at revision/data revision 2; original bytes remained identical |
| Flip the physical magic byte in a separate owned copy | `check` and `restore` rejected with `UnsupportedFormat`; damaged copy and original stayed unchanged; rejected destination was absent |
| Restore the verified backup to a fresh path | Identity and checkpoint matched the original; exact logical export reproduced every fixture byte |
| Read the restored file in the separate Rust consumer | Current key 0 was 21 at revision 4; retained ID 1 held key 0 equal to 11 at revision 2 |
| Retry restore to that existing destination | `AlreadyExists`; destination bytes remained identical |
| Flip a checksum byte in a separate logical fixture copy | `Corrupt`; no destination was created and the input stayed unchanged |
| Commit one sequenced journal write, record the common checkpoint, stop the coordinator, back up and restore coordinator plus journal | Receipt `1 3 42 4` and `aligned 4 42` survived the coordinated restore |
| Compare the acknowledged journal with an isolated same-lineage pre-write restore | Unsuccessful `behind 3 0`; both images stayed unchanged; the stale endpoint was stopped without submitting a replay |
| Read the restored journal's saved acknowledgment with `journal resume` after the live aligned check | Returned its local `committed 4` acknowledgment without changing either image; no pending request was sent |
| Submit one new conditional write on the restored pair | `committed 5`, then `aligned 5 99`; original coordinator, original journal and all three coordinator/journal backups stayed unchanged |

The pre-write image is structurally valid but is **ineligible for this
acknowledged journal checkpoint**. Restoring it rolls back receipt history.
The drill selects the coordinated revision-4 set as the authority and keeps
the original endpoint stopped. It does not retry the stale copy or infer that
a missing receipt means no external effect occurred. In an actual incident,
reconcile every assigned journal and any external effects before cutover.
An older binary additionally needs its own supported format, schema and
protocol compatibility evidence; this drill does not establish binary
downgrade eligibility.

For suspected data loss or a security defect, follow [SECURITY.md](../SECURITY.md):
contact the release owner privately with revision, build identity, platform,
feature profile and a minimal reproduction. Preserve the evidence locally;
omit database contents, credentials, private keys and personal records from
the initial report. A suitable sensitive-evidence transfer method remains an
owner decision.

## Reproduction and retained evidence

The selected administrative and coordinator executables must already exist.
The command builds the small core consumer offline with its committed lockfile
through the same supervisor. Use a fresh output path for every attempt; the
runner refuses an existing one. The recorded run used Rust/Cargo 1.99.1 for
xtask and the fresh consumer; the selected CLI artifacts used Rust 1.99.0.

```sh
cargo +1.99.1 xtask incident-recovery \
  --admin /path/to/qualified/weekend-admin \
  --coordinator /path/to/qualified/write-coordinator \
  --artifact-source 7e0e44f98809b5a82a223604f05425dae3c9ab2b \
  --output /private/tmp/weekend-incident-recovery-new
```

The final shared-supervisor run passed all 38 commands, including 29 shipped
CLI invocations and three service lifetimes. Every process report verified
cleanup, with no timeout, cancellation or cleanup failure. Its directory is
`/private/tmp/weekend-incident-recovery-supervised-final`; `evidence.json`
retains the scoped source inventory, command arguments, expected status,
supervision reports, executable hashes, checkpoint results and hashes of all
129 retained payloads. Its SHA-256 is
`3bb8d358a40385dc588bbc7737cbb2144036a759768b4a09092184c7600181ff`.
The earlier standalone-runner proof applies only to that retired runner.

Offline build, formatting and strict Clippy passed. A focused adapter regression
passes the intended exit-1 rejection and rejects exit 2, the wrong diagnostic,
unexpected success output and output-limit overflow; every control verifies
shared-supervisor cleanup. The new consumer manifest, source and lockfile are
included in `source_hashes`. Its lockfile SHA-256 is
`d6b4d16afc7892ef9d1a68471e448d8927e392e39924c18f3da3fe53bcf7a3ba`.
The frozen binary fixture is separately pinned and checked before and after
the drill.

| Artifact | SHA-256 |
|---|---|
| Frozen logical fixture | `cde9a4fe2d3bc2505148e5001307e973fc10b7d018bd2569e0374edfdd1d1b94` |
| Selected `weekend-admin` | `a54c5d6b82fc80dda62be6eb3d31ffebd3a102962a30a6cb9154317da214af51` |
| Selected `write-coordinator` | `4599ae3fbd2ae332834cbac3e05f8df05c8675a5bf733209eebb8e3407d82e39` |
| Shared xtask runner | `c55c8fb85c979e887ba7471bcf8132bb0d04867095c09ec45c29fda833113b88` |
| Fresh core consumer | `17b48ce1d71ccccf8c8c253e630a469c7beda36480aab7438e9fc578370b57e8` |

The selected executables came from
`/private/tmp/weekend-delivery-7e0e44f-arm64/macos-arm64/build/debug`.
`--artifact-source` is operator-supplied metadata: the drill records exact
selected CLI hashes but does not independently derive their Git provenance or
replace the delivery verifier. The Campaign separately records its runner's
checkout at `6eb5f1a884bd75cfd71cdd75e3fa1cf0be597e63` with `dirty: true` and
the unchanged scoped input hashes. The runner and fresh consumer were built
from that working checkout with the drill addition. The consumer's library
oracle reads only the restored fixture and checks the frozen values above.

## Docker Linux arm64/ext4 follow-up

The same incident inputs passed on Docker Linux arm64/ext4 on 2026-11-16,
using Rust and Cargo 1.99.0 and the already qualified Linux debug CLI artifacts
from `7e0e44f98809b5a82a223604f05425dae3c9ab2b`. All 38 Campaign commands
passed with verified cleanup, no timeout or cancellation, and the same
current/retained, damaged-input, preservation, stale-checkpoint and continued-write
oracles above. The five-case rejection adapter regression, strict xtask Clippy
and both formatting checks also passed.

The runner source was frozen **before execution** from
`6eb5f1a884bd75cfd71cdd75e3fa1cf0be597e63` plus exactly six incident inputs:
the new module, main-command wiring, source-inventory wiring, and the consumer's
manifest, lockfile and source. Their hashes match the macOS report's six inputs.
The offline Git bundle retains the base revision; `incident.patch` supplies the
overlay. The Campaign correctly records the base revision with `dirty: true`.
No clean commit was fabricated. All 736 frozen source-file hashes and the dirty
Git status matched before and after the run; all 326 scoped Campaign inputs
matched that frozen inventory.

The proof directory is `/private/tmp/weekend-incident-linux-arm64-final`.
Its `freeze.json` records the six input hashes, complete source manifest,
bundle and source-tar hashes. The relevant SHA-256 pins are:

| Evidence | SHA-256 |
|---|---|
| `linux-arm64/evidence.json` | `e5ce077073de5c44a2637d6d85a147d03b3fda0938aa4c7ea969cfab661add69` |
| `freeze.json` | `a4721584384f932b7f3b2db135d5e1ef9199ba2f454062035c08862d96aa9945` |
| Exact incident patch | `62d83daf458b1d8c2dc2955b9f8e4935edcd356289a06befc4ee80bf14f7446a` |
| Frozen source tar | `622af2a2feace54fccb6af35997bfbd1319050a473d98ce6f8278d5e440ea295` |
| Selected Linux `weekend-admin` | `f5ef6ed83caeaa1aa54a8244881a19f24d41c1e123667f5a614825230ff1ebd9` |
| Selected Linux `write-coordinator` | `a5697568f64eb8314ac010357358d23eb9213fb8a4b092643eb7a76a1812af32` |
| Linux xtask runner | `25255735ad0a5a48822dda0ab92dc451cca24a8539ac06f3350495741a3155d0` |
| Linux core consumer | `c4f927dc87b1d8e689780a9ec95416e6247786743264fca862eee8813ed5cbfc` |

The immutable image was
`sha256:bda5ef43469a7d63b240adcbc5a5b0599be442bce204d12936f6083224f56b18`.
Source, temporary files and build outputs used one separately owned ext4
volume. The seed/artifact and Cargo registry mounts were read-only. Networking
was disabled; limits were four CPUs, 5 GiB memory, 512 PIDs and a ten-minute
outer deadline with a 15-second kill grace. Docker logs were bounded to two
2 MiB files; child logs used the Campaign limits described above.

`independent-audit.json` confirms all 129 payload hashes, all 38 separate
process reports and stream byte counts, source preservation, credential absence
and retirement of the exact owned container and volume. `docker-run.json`
retains the Docker arguments and empty container/volume absence queries after
cleanup. The frozen source, original CLI artifacts, final runner/consumer
binaries and all proof files remain on the host. A separate not-started attempt
records a logging-driver configuration rejection; it ran no drill and retired
its own container and volume before the successful attempt.

The container ran as root. Mode 0400 and unchanged source bytes were checked,
but this does not establish an enforced permission-denial boundary for root.
This follow-up covers Docker arm64/ext4, rather than a physical Linux host,
x86_64 service deployment, native Windows or hardware durability.

## Limits

This is one small synthetic application/fixture rehearsal on macOS/APFS and Docker Linux arm64/ext4,
with post-acknowledgment process stops. It does not test a mid-publication
crash, power loss, hardware durability, full-volume or permission failures,
`OutcomeUnknown`, damaged retained graphs, salvage, arbitrary application
schemas, pending lost replies or remote duplicate receipt replay, remote replication/CRDT enrollment, external
side effects, or every journal slot. Those paths retain their separate
qualification requirements. Physical-host Linux and native Windows drill executions,
operator handoff, incident response times and support commitments are not
established here. No recovery-time objective, deployment performance budget,
production readiness or owner approval follows from this result.
