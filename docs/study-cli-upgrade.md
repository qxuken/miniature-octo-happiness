# Coordinated pre-1 StudyProgress CLI upgrade

This private qualification uses **actual historical and current release CLI
binaries**, not a simulated old TLS profile. It covers a coordinated upgrade
from `fb80989d98bd977743168ee10418a434a67a0261` to
`365daf6` (the full hash is bound in the evidence inventory). The former is the
parent immediately before `ed8f9f922bc46bd6e52b7d0776f69dad93f3184a`, which introduced
shared pinned TLS and required StudyProgress ALPN. The historical CLI has no
ALPN requirement; the current CLI requires `study-progress/1`.

The StudyProgress model, CRDT module/containers and study command/enrollment
protocol files are byte-identical between the two archives. The runner rejects
changes to those inputs rather than silently migrating state. Core current
reads additionally verify all persisted records after the scenario. This is a
specific pre-1 pair, not a compatibility promise for every version or an
old-reader rollback qualification after arbitrary new application changes.

## Observed upgrade behavior

Both operating systems passed the following scenario with unchanged credentials,
collection `[9;16]`, policy generation 7 and actor identities `[1;16]`/`[2;16]`:

1. The historical binaries generate credentials, initialize both native files
   and exchange successfully. Independent edits converge to 12 minutes, goal
   40, session limit 50 and lessons `[1,2]`. Additional acknowledgments occur
   without publication. Each stopped database retains its complete original
   record, revision and retained ID using the public core API.
2. Upgrade only A's executable; B remains the actual historical executable.
   Both endpoints are live, use the same trusted leaves/pins and stay responsive.
   A locally commits lesson 3, eleven minutes, goal 80 and limit 45; B commits
   lesson 4, thirteen minutes, goal 70 and limit 20. Those local acknowledgments
   survive independently, leaving A at 23 minutes and B at 25.
3. Enable exchanges. The new sender reports **`peer 0 error study ALPN
   negotiation required`**; the old sender reports `peer 0 error Io`. Both
   report zero acknowledgments. Refusal/time-out alone cannot satisfy the
   oracle: at least one explicit ALPN rejection and both live error reports
   are required. All displayed values and each exact locally acknowledged
   revision remain unchanged, and those revisions survive offline reopening.
4. Stop both processes, upgrade B as well, and reopen the same native files
   without enrollment/schema/credential changes. Both offline edits remain.
   Matching new binaries converge to **36 minutes, goal `[80]`, limit `[20]`,
   lessons `[1,2,3,4]`, empty planned set**. Complete public StudyProgress records,
   including causal metadata, compare equal. Repeated covering exchanges and
   another process restart add no commits.
5. After shutdown, public core reads compare both original retained records,
   revisions and IDs exactly with their pre-upgrade captures. Current records
   satisfy the independent semantic oracle and the final observed revisions.

Final converged revisions were `[7,7]` on Mac and `[9,9]` on Linux. Concurrent
exchange/resolution scheduling can produce different valid publication counts;
the oracle requires each run's own stable exact revisions on repeat/restart,
not one invented cross-platform commit count. Full causal state and semantic
values converge in each run.

The old/new mismatch is an **availability boundary**, not a lost local commit.
Plan a coordinated stop/upgrade/restart of both endpoints before requiring sync.
Local command success never constitutes remote acknowledgment. This test changes
binaries while keeping credentials fixed; [credential cutover](study-credential-cutover.md)
is separate evidence. No runtime, wire format, publication barrier or actor
migration implementation changed.

## Test construction and process boundaries

`tools/study_cli_upgrade.py` extracts two independent Git archives and builds
`cargo build --offline --locked --release -p study-peer` with their original
manifests/locks and separate Cargo targets. The production CLI sources are never
rewritten. It then generates a disposable current integration test from the
frozen credential test's process/relay/public-state helpers plus
`tools/study-cli-upgrade-fixture.rs`. The only helper transformation passes an
explicit executable with the log argument to `Peer::start` and uses that path in
`Command::new`; exact unique extraction/substitution guards fail closed. No
TLS implementation or rejection result is mocked. Dead unused helper code is
allowed in this generated fixture. Strict release integration-target Clippy
passes on both platforms.

Each relay owns its loopback port for the scenario, points to CLI port-zero
listeners after readiness, and forwards opaque bytes. Existing helper deadlines
bound readiness/replies (8 seconds), one-shot commands (10 seconds), convergence
and rejection (20 seconds), and relay I/O (4 seconds). The Python runner bounds
builds/Clippy to 300 seconds, test execution to 180 seconds and Docker execution
to 900 seconds. Owned process groups are killed on every exit, including a failed leader that
has already exited; the direct child is reaped. The fixture kills
and reaps its children, removes private credential trees, and the runner removes
its owned runtime directory even after fixture failure. Logs contain no generated
private-key bytes; only proof and executable copies leave the Docker volume.

Existing or dangling output paths reject before any external command. Known
ambient build overrides are removed while `RUSTUP_HOME` and `RUSTUP_TOOLCHAIN`
remain intact. Toolchains, configuration and registry caches are trusted inputs;
this is not a hostile compiler/configuration qualification. Seven Python controls
pass normally and under `-O` on Mac/Linux: fresh-output admission, preserved
selector, real child failure/timeout reaping, cleanup after failed container
removal with authoritative absence checks, exact fixture extraction/seam, an exited leader with a sleeping descendant,
and copied-artifact/source mismatch rejection.

## Native qualification and evidence

Mac arm64 used Mise Rust/Cargo 1.99.1 and Python 3.14.8, native `/dev/disk3s5`
(APFS). Linux aarch64 used immutable local image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
Rust/Cargo 1.99.1 and Python 3.11.2. Actual rustc commit:
`58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM 23.1.1. Linux ran with network
disabled, two CPUs, 3 GiB memory and 256 PIDs. `findmnt` confirmed ext4 on
`/dev/vda1` for the private UUID volume containing build/runtime data. Frozen
seeds and the trusted host registry were mounted read-only. Only the owned
container/volume were removed, and successful live-daemon queries confirmed
both absent after proof copyback.

Each platform passed one release native scenario and strict targeted Clippy.
This is not debug-profile, cross-host or native Windows execution. Source
inventories contain 542 old files and 612 current files. Both exact archives,
original locks, source stability, generated fixture hash and actual binary
hashes are recorded. The controller now rehashes the complete copied artifact inventory, rejects
symlinks, compares both worker source inventories to the frozen seed, checks
binary hashes and regenerates the exact fixture hash before accepting success.
Copied proof files and binaries were also independently rehashed. All generated production binaries
are preserved as evidence; private runtime credentials are not copied.

| Platform | Old CLI SHA-256 | New CLI SHA-256 |
|---|---|---|
| Mac | `044f608d72b8dd928ea52387fdfdd53b5bfa4bb09370d7245b0953a48d8dbd11` | `1655643cee0e933f1650417e0db9403e8605f08deb6fd099498dc9a2f6575d83` |
| Linux | `d2ec90f7bcce2b5557c243d2b5a07edf033495b1347a121e3421ea75c82c14ab` | `63765d82b0cdd569825babe79304c0f34dcd040fb5f7e04acd1d83b0045390b2` |

Final evidence: `target/study-cli-upgrade-365daf6/index-v2.json`, SHA-256
`46066a6f36aa4a6996895879794cdac3c91401f35034ec6dd5557237de66ddb2`.
`mac-qualified-v2/` and `linux-qualified-v2/` are the fresh corrected source-bound runs.
The original `mac-qualified/` and `linux-qualified/` evidence remains historical.
Exploratory missing-path/credential-order failures and earlier scenario-success
but Clippy-failure records are retained separately and are not qualified.
Local ignored artifacts are not a permanent retention or release promise.

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/study_cli_upgrade.py \
  --new 365daf6 --output target/study-upgrade-mac-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/study_cli_upgrade.py \
  --new 365daf6 --output target/study-upgrade-linux-fresh \
  --docker-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
mise exec python@3.14.8 -- python tools/test_study_cli_upgrade.py
mise exec python@3.14.8 -- python -O tools/test_study_cli_upgrade.py
```

No general rolling-upgrade availability, torn configuration replacement,
live reload, dynamic roster, CA hierarchy, power loss, compiler MC/DC, or
unified final-candidate qualification follows from this drill. No remote CI,
repository push or registry publication occurred.

## Qualification-helper correction

Review reproduced two defects in the original helper: a leader exiting with
status 2 could leave its sleeping descendant alive, and controller success
trusted the worker manifest without rehashing copied artifacts or comparing
worker source inventories to the frozen seeds. The saved original artifacts
were independently checked and valid, but those failure paths were not protected.
The v2 gate fixes both and reruns the actual scenarios. The descendant control
requires no live descendant; an already-dead reparented zombie is permitted
because only its OS parent can reap it. Corrupted copied bytes and a mismatched
worker source inventory each reject, alongside a valid-copy acceptance control.
Earlier results are not relabeled under the corrected helper.
