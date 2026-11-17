# Coordinator recovery from a private Rust-source package

`cargo xtask coordinator-recovery --package PACKAGE --expected-source SOURCE
--output NEW-DIRECTORY`
qualifies the coordinator source profile of a verified private package made
with `cargo xtask package --rust-source`. It builds and runs the existing
[four pending-pair schedules](coordinator-pending-restore.md) in debug and
release. Nothing is published to crates.io.

## Artifact selection and observations

The command verifies the complete package inventory, expected source ID and clean source identity
before creating evidence. Output must be separate from the package, including
through an alias in an existing parent directory. An overlap regression checks
that rejection leaves the entire package inventory unchanged.

The delivered package must have a verified native Rust-source profile and the
exact recovery fixture compiled into the controller. Cargo builds only from
that delivered source, offline and locked, into a new owned target directory.
Its JSON messages must identify exactly the coordinator and the writers test
executable, with the expected manifest, target kinds, source paths and test
profiles. Executable paths must be regular files within the owned target.

For each profile, the command copies and hashes both executables into the
evidence directory, then removes their original build-path files. It launches
the retained test executable with the selected retained coordinator path.
The test fixture's `WEEKEND_PENDING_RESTORE_BINARY` is an internal selection
control; it is not coordinator configuration. A supplied missing artifact
must fail at invocation and cannot fall back to `CARGO_BIN_EXE`. A regression
checks that failure and retirement of the temporary fixture directory.

The controller first discovers the one exact test. Execution must report all
four ordered schedules, the expected final revisions/values/receipt sequences,
both retained roots, original preservation, a second lost reply and stable
local replay, followed by one actual passing, nonignored test. Zero execution,
skips, missing/duplicate results, incorrect rows and extra output reject.
One correctly placed libtest running notice is allowed during a slow run; it
is never a completion result. Fresh reader processes still verify exact raw
rows through the public core interface and reject the stale-image control.
Both discovery and recovery require empty stderr. A passing stdout transcript
cannot hide unexpected diagnostics on the other stream.

The existing Unix process-group and Windows Job supervisors provide verified
descendant retirement, a one-MiB stream cap, 300-second build deadlines and
180-second recovery deadlines. Each runtime directory must be empty after
the fixture exits. Output is private to the invoking account before its first
evidence write, including initialization failures. Unix creates the leaf with
mode 0700. Windows uses the invoking user's SID and a protected inheritable
grant passed to [CreateDirectoryW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createdirectoryw)
at creation; it rejects an existing leaf and requires local fixed NTFS. The
[descriptor conversion](https://learn.microsoft.com/en-us/windows/win32/api/sddl/nf-sddl-convertstringsecuritydescriptortosecuritydescriptorw)
buffer stays alive through creation and is then released. Privileged system
access remains outside ordinary-account isolation. The selected
executables and full input package are checked again after execution.

`evidence.json` identifies controller inputs, package source and manifest hash,
toolchain, actual retained binaries, original Cargo paths and their retirement,
command transcripts and artifact hashes. Build caches are explicitly excluded
from the proof inventory. Package integrity and source provenance are distinct
from release approval or durable proof custody.

## Reproduction

Start from a clean checkout and use the pinned compiler. Both output paths
must be new; the commands never overwrite an existing package or proof.

```sh
cargo +1.99.1 xtask package --features replication,crdt --rust-source \
  --output target/coordinator-package
cargo +1.99.1 xtask coordinator-recovery \
  --package target/coordinator-package --expected-source FROZEN_FULL_GIT_ID \
  --output target/coordinator-recovery
```

The Linux qualification puts source, package, registry copy, builds, TMPDIR
and runtime files on an owned Docker ext4 volume. Native Windows execution
retains its own CI gate; cross-compilation establishes buildability only.

## Scope and results

This is a source-delivery profile: the coordinator is compiled from the exact
delivered Rust source and then relocated before execution. It does not imply
that an unrelated prebuilt CLI, shared-library package or future source version
has been qualified. It preserves the original matrix's scope: slot zero,
synthetic configured authentication and native process interruption. Device
power loss, deployment-account permissions, operator authority selection and
handoff remain separate gates. Local commit, journal acknowledgment and follower
durable acknowledgment remain separate; this drill adds no follower receipt.

Four controller tests pass in debug and release: strict transcript accounting,
Cargo artifact admission, executable retirement, and package preservation on
overlap/alias or source mismatch. Both pending-fixture tests pass in debug and
release. Strict all-target Clippy passes for xtask and the coordinator, and the
Windows MSVC test cross-check compiles. The overlap preservation test was first
observed rejecting the initial implementation because it had already created
evidence inside the package; admission now runs before any output creation.

Frozen package runs and their executable identities are recorded after
execution. CI adds one source-package job on each native platform; this expands
the usual check set from 19 to 22. The complete R2 and R7 release gates remain
open, and no new tooling record is claimed as an independently covered MC/DC
decision.

The first frozen macOS run at `82cc727` passes all eight schedule/profile
combinations, with zero recovery/discovery stderr bytes. Independent review
then found that nonempty stderr was not rejected explicitly and the evidence
directory was restricted only after constructor logging. Follow-up controls
reject valid stdout plus unexpected stderr and check initialization-failure
privacy. Unix runs that control under umask 000; Windows checks the actual
owner/protected ACL and inherited child-file access in native CI. The initial
run retains its limited scope; the fixed candidate must be qualified again.
The fixed macOS xtask/coordinator suites pass 269 tests with four explicit
ignores in each of debug and release. Strict Clippy and the Windows test
cross-check pass; the new native ACL test still requires Windows execution.
