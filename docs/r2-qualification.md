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
Study uses the frozen-main controller. Snapshot uses controller checkpoint
`5b03528ce5479d9e7aa477ea620a91ea31bb5ff1`; the incident drill's Windows lease
correction uses `3f2dedfe7057b0a963d18d716c2c566ae609b184`. Each separately
records its 334 scoped input hashes and actual executable hashes. Native builds
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

The corrected operator rehearsal passes all 39 expected outcomes on macOS/APFS and Docker
Linux arm64/ext4, including preserved originals, rejected damaged input, exact
logical export, fresh current/retained reads, stale-history rejection and
continued writes from the coordinated restore. Both runs retain all three
executables, preserve them byte for byte and remove the synthetic credential.
Four separate service lifetimes keep byte comparisons outside an active writer
lease. Every command verifies cleanup without timeout, cancellation or output overflow.
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
| `target/r2/incident-macos-lease/evidence.json` | `332ceb7a271d6fc210b8c640ae453a1c3c0f11705eb6681dd96ba3a84e6d203a` | 135 |
| `target/r2/incident-linux-lease/incident/evidence.json` | `eb350dc4f783529d3cabf95052e8c5f0f7fcfc87cae731fe9888cc97e6a1a164` | 135 |
| `target/r2/study-6b657e5/evidence.json` | `002fa8b7791ce3e2a7e4f6a6b40a349ccfc8de14c019069b7cf85fcedb28a469` | 233 |
| `target/r2/snapshot-month-corrected/evidence.json` | `b6dc705016f63e40e6fa005adfcaced2187da7f1d49b97c004c0a2d07eeb6668` | 124 |

Independent replay verifies every indexed payload hash, both incident command
counts and cleanup records, repeated-sync/restart observations, final snapshot
bytes/digest, credential removal and exact owned-resource absence. The original
reports are unchanged. Proof is local and remains private; durable off-machine
custody is still R7 work.

## Tests, review and failed preparation

The initial macOS checkpoint passes 757 all-feature workspace tests with six
ignored in each of debug and release, plus strict workspace Clippy. Windows
MSVC cross-checks pass for Rust 1.99.0/1.99.1 and strict Clippy. Hosted Windows
subsequently passes the output-limit, source write-protection and exclusive-lease
observation controls in both debug and release; those native results are
separate from cross-compilation.
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

The first hosted Windows operator run passes source protection and the physical
recovery cases, then fails with OS error 32 while the controller reads the raw
restored database during its live service. Windows' writer lease intentionally
uses `share_mode(0)`, excluding that raw read. The correction retires the
read-only service, compares both images and only then starts the continued-write
service. It preserves all assertions and the exclusive lease policy. A native
Windows regression verifies the raw observation fails while the lease exists
and succeeds after release. The failed hosted report remains unqualified.
Engine run `44111198909`, tested merge `994e1637c47a038452a5c8bceeeae0d5ee5116bf`,
passes all three corrected operator jobs: Mac/Linux each have 39 outcomes and
Windows has 41 with its two ACL setup commands. Independent replay checks all
370 indexed payloads, actual executable hashes, stream hashes/lengths, expected
exits and cleanup. Windows verifies read-only write denial, restored receipt
`1 3 42 4`, aligned revision 4/value 42, continued revision 5/value 99 and stale
history rejection at revision 3/value 0. Its evidence SHA-256 is
`95dabab52229991a2c4faa02fd7d6f68a0a3aca65501a73cd82665b197d97c29`.
The preceding 38-outcome Unix runs remain retained with their original
controller identities.

### macOS owned-volume retirement

The same hosted run fails the native maintenance lane when normal `hdiutil
detach` reports Resource busy (exit 16) before the reopen. This is an image
retirement failure, not a database publication assertion. Checkpoint `7bb940c`
allows at most five normal detach attempts, with 250 ms between clean Busy
results. Each attempt rechecks the exact owned image and whole GPT device.
Other exit codes, supervision failures, timeout, missing exit and exhausted
Busy results remain failures; no forced detach or database operation retry is
added. Existing absence, remount, byte, inode and retained-root checks remain.

The native regression holds an owned APFS directory open, observes exit 16,
releases the handle and requires clean detach plus verified retirement. It
fails against the single-attempt implementation and passes the correction.
Directed controls reject persistent Busy and failed supervision. The original
near-cap namespace scenario also passes at `7bb940c`: actual exchange ENOSPC
(raw 28), retained/current reads after remount, preserved scratch inode/bytes,
and successful retry after releasing only owned ballast. All 45 indexed
payloads and 17 command cleanup records verify. Its local report
`target/r2/apfs-namespace-bound/evidence.json` has SHA-256
`8776adad91cfe666cfd4ce67ca73d9f73f761b58e22415206cccb0e3106a01af`.
An earlier preparation without the compiled source identity was rejected by
the unchanged identity oracle before image setup; that failed report remains
unqualified.

### Instrumented helper completion

Engine run `44111198909` also fails the recursive detached-worker test in TSan:
the three-second command deadline terminates its early-exit worker before the
expected exit 2. The report verifies descendant cleanup and contains no race
diagnostic. A deterministic delayed-exit case reproduces the same deadline,
exit -15 and preserved cleanup before the fix. Completion cases now have a
30-second bound for instrumented startup/exit; the actual timeout case keeps
its three-second bound. Exact early exit 2, no completion timeout, orphan
retirement and malformed-journal failure remain required. Supervisor runtime
code, default deadlines and production behavior are unchanged.

The corrected five-case test passes native debug and rebuilt-std TSan with
nightly `2026-11-12` and the retained patched Darwin runtime SHA-256
`d2aad029c8e571d9f3861798eb382f4367ef3e6fa78eae41b2bd071c9f619734`.
That targeted local diagnostic is separate from hosted full-workspace TSan and
its freshly calibrated runtime. Full xtask debug/release and strict Clippy
also pass after integration. Failed hosted and deterministic local reports
remain retained without qualification credit.

Clean checkpoint `a93c3e1` subsequently passes the entire local rebuilt-std
workspace TSan campaign: 758 passed and seven ignored, with the same pinned
runtime and all three calibration outcomes checked first. Its report SHA-256
is `1538ec38f71a1fbf2bc68d28381d4d88a44e4c32e67a2c7347f163a98889dc45`.
Hosted workspace ASan also passes that checkpoint's merge tree, with the same
758/seven counts. Three initially unstarted macOS jobs report runner-capacity
failure with runner ID 0 and no executed steps; those are not test results.
ASan passes its targeted retry. The next helper execution exposes the registry
omission below, independently of sanitizer or database behavior.

### Native skip registration

The new ignored APFS control also needs explicit registration in the helper
runner. The first executed macOS helper campaign correctly rejects its absent
registration after all 224 default unit tests pass. The correction adds only
the exact APFS test on macOS; Linux keeps its one TLS skip. The native
maintenance job separately executes the APFS control and passes in 15.47
seconds. Ordinary helper skips provide no native execution credit.

The existing real-libtest regression now discovers and runs one passing test
alongside the ignored APFS test, then validates the actual registry. It fails
with the original registration and passes the correction; unregistered and
missing skips still reject. The corrected local macOS helper campaign passes
all eight suite/profile combinations: 235 passing tests and two registered
skips per profile. It records the exact `helper_controls.rs` and documentation
overlay over `a93c3e1`, without claiming a clean Git checkpoint. Report SHA-256
is `90646120359b40483097f48c33eb63f458e8ab8737d72b78af30fdb07ec4858c`.
The extended real-libtest case also passes targeted TSan; strict Clippy and
decision-manifest reconciliation pass. This correction follows the clean full
workspace TSan result above; subsequent CI retains its own source identity.

The refreshed checked decision manifest has 8,239 records across 133 files and
12 targets: 3,023 runtime, 156 driver and 5,060 tooling. Source-set SHA-256 is
`97d7e49a5cc67053df3392c10de8a2b4674d7b314697a751e6643063c654b768`.
Runtime sources, 721 historical row references and eleven named R1 controls
are unchanged. The 7,506 individual G mappings remain unresolved; new tooling
execution does not establish predicate independence or compiler MC/DC.

The hosted operator results qualify their named execution profile. The
ordinary-account deployment profile remains separate. Each subsequent CI
revision retains its own full-workspace and sanitizer gates.

## Boundaries

This refresh covers named synthetic workflows and post-acknowledgment restarts.
It is not physical power-loss qualification, a wall-clock month, arbitrary
application schema/key coverage, all journal slots or deployment latency-budget
acceptance. Additional pending-intent/lost-reply and operator handoff profiles,
ordinary-account Windows deployment and the release support window retain their
explicit roadmap obligations. The R1 per-decision queue remains open.

### Candidate operator recovery

Manual and tagged Engine checks run the operator drill against the selected
candidate in addition to the fixed historical CLI pair. The existing
three-platform release job builds both binaries in debug and release and
retains each executable alongside its incident report. The job compares
the checkout to the event commit before building. The recovery evidence
records the SHA-256 of each executable. Pull request scope stays the same.

At `2e9f717` the release shell step passed all 85 commands in both profiles
on native macOS arm64 and Docker Linux arm64/ext4. A fresh extraction
verified 286 artifacts per profile and the exact two executables used in
each drill. Restored state included revision 4 with value 21 and retained
revision 2 with value 11. The coordinated handoff returned receipts at
revisions 6 and 7. The [proof manifest](candidate-operator-2e9f717.sha256)
identifies both archives, the source bundle and the executed shell step.
Native Windows and hosted release execution for this candidate are pending.
