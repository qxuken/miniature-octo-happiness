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

## Frozen package execution

Fixed source `f2e4d35ce6dc374d6333c4370de4e12a35247a70` passes all four
schedules in both profiles on macOS arm64/APFS and Docker Linux arm64/ext4
with Rust 1.99.1. Both checkouts are clean. On each platform the audit verifies
all 850 package payloads, all 771 declared source files against that Git
revision, and all 336 controller input hashes. The controller input maps are
identical across hosts. Discovery/recovery stderr is empty, both original
build-path executables are absent, runtime directories are empty, and the
retained executables match their before/after hashes.

| Platform/profile | Coordinator SHA-256 | Test executable SHA-256 |
|---|---|---|
| macOS/debug | `8ed21633c5d5fc1de3bcc3446ebcf0dac3fa43ba9621bb4a0e7e85b3c693180d` | `28c3a6fc01956d24babfa5228c771a5ce4b268eab0a89cb2a049e67cb01a3262` |
| macOS/release | `349a6da94480997c447d7033ee9c05443cc361e3d8ee4ad6d858f043836e1b5c` | `c5e759f765c263a26de7109b0c2337da3947e690ab5fb56e711a5b4e77ee6099` |
| Linux/debug | `cc1a8422ae644fc23ae39d7013e51e45db1dadb72ecb08f2d49a1a4ac1300eb3` | `a5677987eac19c315005d34c2d57ae78ccbdafbc3bea8b364e91edfd6517833e` |
| Linux/release | `62244f9a31199cd6ceaed598c5fc3ce8a48912f904ccba77be5d15f9607ff18d` | `1c3536734d68fa220f7276c162eccdc1568ae5fd88934270d91e627d354cc9ef` |

| Platform | Package manifest SHA-256 | Recovery evidence SHA-256 |
|---|---|---|
| macOS | `f4f92d1a6047ab7d1a07b7f55f05851da059b38f71bacf445f1bc7a5b342450b` | `edb2a6b9ca5df87010e895e8fbe1f4ef2684fd625cbdadcf69dad82339d364db` |
| Linux | `eba05f5cb17fd667145e11b07ab61c22f3767740b43f605f8b110f94212d52a3` | `5d91435f8544c1c5bad4e0f00038b300e631640c61a133c21f9ff09e2ab080a9` |

Linux uses immutable image
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`,
with source, registry copy, builds, package and runtime files on the owned
ext4 volume and no external networking. The verified source bundle SHA-256 is
`4c0c538bdd6d706e6df62d1371ee26ed65d38977a74204a8416b186ebe17d8e8`.
The worker runs as UID 0: mode 0700 does not prove access denial against root.
Its container exits 0; the owned container and volume are retired after the
audited proof is copied out.

Local proof and the independent audit are under
`target/r2/coordinator-artifact/`, including `frozen-audit.json` and the retained
source bundle. These ignored files are not durable proof custody. Native
Windows execution and its ACL control, other promised delivery profiles,
operator handoff and final release consolidation remain open. Subsequent
documentation commits do not become a new runtime qualification.
