# Five compatibility profiles at 1f3f42a

Native macOS arm64/APFS and Docker Linux arm64/ext4 qualified the same
candidate `1f3f42afad0d759b6f692824f750528315b4878a` with Rust 1.100.0.
The historical endpoints are the five selected by the release workflow.
Their original lock blobs and full archive identities are recorded.

| Profile | Historical endpoint | Verified outcome on both hosts |
| --- | --- | --- |
| Storage | `a7dc343` | Both readers verify both writers' current revision 4 and retained revision 2. Unsupported versions reject with identical file bytes. |
| Snapshot wire | `dcd2402` | Both sender directions reopen the revision-4 receipt with retained revision 2. Local progress reaches revision 5. Duplicate acknowledgments and unsupported-version rejection preserve the files. |
| Typed schema and indexes | `1ca5e5a` | The precommit interruption reopens the original image. An older update preserves new and unknown fields, and an older insert rejects once the registry is installed. Restore revives the old indexes and writes. |
| Snapshot CLI | `315068f` | Authenticated transfers observe receipts 3, 4 and 5 through both upgrade directions. With the wrong pin, local capture reaches 4 while the follower stays at 3. Repeating the final poll preserves the files. |
| Study CLI | `fb80989` | The old peers converge, and mixed ALPN deliberately rejects with zero acknowledgments and both local commits intact. The upgraded peers converge on the complete causal record. Both retained roots survive, and repeat sync and restart preserve the revisions observed at convergence. |

The Study peers converged at revisions 7/7 on macOS and 9/9 on Linux.
Each run checks its observed revisions again after restart and verifies the
same complete causal record and the original retained roots.

## Study protocol source guard

The original controller rejected the candidate before the Study build on
both hosts. A byte comparison confirmed that the protocol files' production
prefixes are identical and all changed bytes lie in the inline test module.
The controller now accepts this exact ordered pair of whole-file digests.
Every other changed pair still stops at the guard.
[Source digests and decision pairs](decisions/study-protocol-upgrade-source.md)

The regression fails with whole-file equality alone. In both debug and
release, macOS and Linux each passed four Study controller tests and detected
all three named mutations. Strict Clippy passed on both, and the controller
cross-compiled for Windows MSVC x64. The original
scenario then passed on both hosts with controller `8c85281` plus the
recorded guard patch. The old and new Study binaries still build from the
exact historical and candidate archives. The source reconciliation covers
10,803 decisions in 225 files, records the new admission pair and leaves
7,368 tooling gaps. The full macOS xtask suite passed 267 tests in each
profile with three existing ignores. Its logs are retained beside the
archive under `target/archived-proof/compatibility-1f3f42a-xtask/`.

## Reproduction and evidence

Build the controller from a checkout with the guard and use the old
endpoints above. The archive includes the five-profile runner. The targeted
Study command and named controls are

```sh
CARGO_NET_OFFLINE=true cargo +1.100.0 xtask study-cli-upgrade \
  --new 1f3f42afad0d759b6f692824f750528315b4878a --output target/study-upgrade-fresh
cargo +1.100.0 xtask mutations --case r1-study-protocol-source- --toolchain 1.100.0
cargo +1.100.0 xtask mutations --case r1-study-protocol-source- --toolchain 1.100.0 --release
```

The same frozen candidate passed all 55 committed corpus and model replay
tests in both profiles on both hosts. All four runs report the same test
names and preserve the same 253 committed seed files.

The local proof `target/archived-proof/compatibility-1f3f42a.tar.zst`
retains both hosts' executables, phase reports and tests, the original
rejections, the guard patch and the source bundle. Fresh extraction checks
8,112 indexed files, including all 1,110 declared compatibility artifacts,
and validates the replay, Study terminal receipts and named controls.
[Archive and report checksums](decisions/compatibility-1f3f42a.sha256)

Native Windows execution for these exact endpoint pairs and agreement
on the supported version window remain open.
