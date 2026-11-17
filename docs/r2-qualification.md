# R2 operational rehearsal refresh

Selected engine source is merged main
`6b657e516c13f678cd54c816db8a53ec5a11e395`. Each workload uses newly owned files
and synthetic identities. Local commits and follower durable acknowledgments
remain distinct. All Cargo manifests retain `publish = false`.

The operator and snapshot controllers are qualification tooling added after
that engine checkpoint. Their reports separately identify runner source,
scoped input hashes, actual executables and the selected engine archive;
subsequent documentation does not become a new runtime qualification.

## Maintained commands

- `cargo xtask incident-recovery` reuses the existing physical/logical recovery
  and coordinated-journal oracles. Windows now uses the existing Job-owned
  process supervisor, a one-MiB output cap, current-user directory ACLs and
  native executable suffixes. Source protection uses mode 0400 on Unix; Windows
  sets the read-only attribute and requires a writable open to be denied.
  Actual selected binaries and the fresh consumer
  are retained and checked before/after the drill. The three-platform CI job
  builds selected CLIs from the exact engine checkpoint above; its controller
  and fresh consumer retain their separately reported current source.
- `cargo xtask study-peer-soak OUTPUT --source SOURCE --image IMAGE --rounds 30
  --restart-every 5` retains the existing two-host offline-edit, restart and
  repeat-sync protocol. Its output is private: the existing controller retains
  synthetic private identities inside the mode-0700 evidence directory.
- `cargo xtask snapshot-month --source SOURCE --image IMAGE --output OUTPUT`
  builds the existing live-month example and snapshot CLI from the same Git
  archive on macOS and Docker Linux. Linux build/runtime files use ext4. Only
  public certificates cross hosts; the native private key is scrubbed and the
  owned container/volume are verified absent before success. The CSV oracle
  independently checks local/captured/acknowledged revisions, four missing
  shipment replies and covering polls. Reopened current/retained data and the
  selected image's exact bytes are checked separately.

The snapshot schedule contains 30 logical days, 31 installations, 32 polls and
63 authenticated exchanges. The last receipt is revision 93 with zero gap;
prior daily local revisions are one ahead of the captured/follower revision.
A successful local heartbeat must not be reported as a follower acknowledgment.

## Executed results

The selected engine archive SHA-256 is
`9a7390ca9872ded519100f36dc16e12c48e3cec4ca55e31775b026a4ce80f4dc`.
Study uses the frozen-main controller. Corrected operator/snapshot runs use
controller checkpoint `5b03528ce5479d9e7aa477ea620a91ea31bb5ff1` and separately
record their 334 scoped input hashes and actual executable hashes. Native builds
use Rust 1.99.1; Linux Study/snapshot builds use 1.99.0, and the separate Linux
operator build uses 1.99.1. The installed Docker image is
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.

The frozen-main StudyProgress run passes 30 rounds and six paired process
interruptions on macOS/APFS and Docker Linux/ext4. Both peers finish at revision
64 with 1,395 minutes, weekly goal 230, daily limit 10 and lessons 2–61.
Repeat sync is stable, exact local revisions survive reopening, and the owned
Docker container and volume are absent. All fourteen peer exits satisfy their
specific graceful/native-kill/container-kill contracts. Different local image
hashes are allowed for converged causal histories; displayed values and local
revision stability are checked explicitly.

The operator rehearsal passes all 38 expected outcomes on macOS/APFS and Docker
Linux arm64/ext4, including preserved originals, rejected damaged input, exact
logical export, fresh current/retained reads, stale-history rejection and
continued writes from the coordinated restore. Both runs retain all three
executables, preserve them byte for byte and remove the synthetic credential.
Every command verifies cleanup without timeout, cancellation or output overflow.
The incident source-input maps match exactly across hosts. Linux checkout,
build, TMPDIR and data all use the separately owned ext4 volume, with networking
disabled; its container exits 0 and both container/volume are verified absent.
Linux ran as root: mode 0400 does not establish permission denial against root.

The snapshot run passes all 30 days, 31 installations, 32 polls and 63 authenticated
exchanges. Days 7, 14, 21 and 28 have no shipment acknowledgment, followed by an
exact covering poll. Both sides reopen revision 93 and the follower verifies all
31 retained views. The selected follower image equals the final export byte for
byte; its SHA-256 and durable receipt digest are
`2f83e445de3692c8aa3eed282346d441e33368ee171e9dcbefd068e148419bcc`.
The native private key is absent, no server private key crossed hosts, and the
owned Docker resources are absent.

| Original evidence | SHA-256 | Indexed payloads |
|---|---|---:|
| `target/r2/incident-macos-corrected/evidence.json` | `05d16639d97385869b958959bf5b81f8eaa1b7d53c7051eb3717c7f12acb52bb` | 132 |
| `target/r2/incident-linux/incident/evidence.json` | `6245161a24c4224aee0f655319c5489d395c025a62477fbf0c4c1d694cf1c3fd` | 132 |
| `target/r2/study-6b657e5/evidence.json` | `002fa8b7791ce3e2a7e4f6a6b40a349ccfc8de14c019069b7cf85fcedb28a469` | 233 |
| `target/r2/snapshot-month-corrected/evidence.json` | `b6dc705016f63e40e6fa005adfcaced2187da7f1d49b97c004c0a2d07eeb6668` | 124 |

Independent replay verifies every indexed payload hash, both incident command
counts and cleanup records, repeated-sync/restart observations, final snapshot
bytes/digest, credential removal and exact owned-resource absence. The original
reports are unchanged. Proof is local and remains private; durable off-machine
custody is still R7 work.

## Tests, review and failed preparation

macOS passes 757 all-feature workspace tests with six ignored in each of debug
and release, plus strict workspace Clippy. Windows MSVC cross-checks pass for
Rust 1.99.0/1.99.1 and strict Clippy. The new Windows-only output-limit and source
write-protection controls await native CI; cross-compilation is not execution.
The snapshot CSV controls reject stale revisions, fabricated missing-reply
acknowledgments, wrong gaps, incomplete/extra rows, malformed lengths/digests and
unexpected final writes. Existing incident rejection controls remain intact.

The first snapshot launch failed before transport because the harness omitted
the CLI's required existing empty follower directory. It records `NotFound`,
scrubbed credentials and verified Docker retirement. The setup is corrected and
the complete scenario passes in a new directory; the failed report remains
retained without qualification credit. Independent source review also found
the Windows protection omission; its correction verifies the attribute and
write denial before reporting read-only inspection. Both review axes have no
remaining actionable findings in the corrected code.

The refreshed checked decision manifest has 8,234 records across 133 files and
12 targets: 3,023 runtime, 156 driver and 5,055 tooling. Source-set SHA-256 is
`4cd7e92e5e462847e121a03f64fce4fde6196a5913b941cffa1a94e3f944f920`.
Runtime sources, 721 historical row references and eleven named R1 controls
are unchanged. The 7,501 individual G mappings remain unresolved; new tooling
execution does not establish predicate independence or compiler MC/DC.

Hosted three-platform operator execution remains pending for the new PR. These
are execution gates, not conclusions inferred from source inspection or
cross-compilation. The ordinary-account deployment profile remains separate.

## Boundaries

This refresh covers named synthetic workflows and post-acknowledgment restarts.
It is not physical power-loss qualification, a wall-clock month, arbitrary
application schema/key coverage, all journal slots or deployment latency-budget
acceptance. Additional pending-intent/lost-reply and operator handoff profiles,
ordinary-account Windows deployment and the release support window retain their
explicit roadmap obligations. The R1 per-decision queue remains open.
