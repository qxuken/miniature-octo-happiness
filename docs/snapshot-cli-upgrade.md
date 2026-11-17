# Private mixed-version snapshot CLI upgrade

This local gate runs **actual unmodified release `weekend-replicate` binaries**
from two exact source archives:

- Historical: `315068fc38b99d8ec616fdb5a8ee81107b471534`.
- Current: `365daf6c8a6b7c85fb049fb5a0370b401dde2424`.

It tests this pair's private pre-1.0 CLI/TLS/storage interoperability. It is not
a supported rolling-upgrade promise for other versions or architectures.
Neither source archive, Cargo manifest nor original lockfile is modified.
The helper builds each CLI offline with `--release --locked`, then separately
compiles a small public-API fixture against that archive's resulting engine
rlib. Its native `SOURCE_ID` must equal the archive commit. The fixture is an
explicit qualification consumer, not a replacement server or wire emulator.

Both actual versions implement the same `weekend-snapshot/1` ALPN and exact-leaf
mutual TLS profile. The common CLI operations are `describe`, `follower-init`,
`serve`, `ship`, `poll` and `status`. The POSIX run also uses the historical
`keygen`. There is no
reachable old/new ALPN mismatch in this specific pair, so no ALPN rejection
result is inferred or fabricated.

## Persisted sequence and independent oracle

One native source and follower enrollment pass through these stages:

1. Old sender to old server ships revision 3, including retained root 1 at
   revision 2. Poll returns the exact export digest.
2. The new CLI independently reopens the persisted follower. The source advances
   locally to revision 4 while a new sender polling the old server still
   observes the revision-3 follower receipt.
3. Against that reachable old server, a new sender attempts a ship with an
   intentionally incorrect server pin. Local capture at revision 4 completes;
   no follower durable receipt is emitted. A matching authenticated poll works
   immediately afterward. Offline reopening still selects exactly revision 3,
   and the manifest bytes remain unchanged. This is a controlled transport
   failure/retry exercise, not a claim that version mismatch causes rejection.
4. New sender to old server successfully retries with correct credentials and
   a fresh export, installing revision 4.
5. The new server reopens that follower enrollment. The old fixture advances the
   same source to revision 5, then the old sender polls and ships to the new
   server. The historical CLI and both public-API fixtures reopen the resulting
   persistent state.
6. Both endpoints use the new CLI. Repeated authenticated polls return the same
   revision-5 receipt. Offline reopening preserves the complete manifest bytes;
   no extra commit is concealed by equal logical values.

The harness independently computes each receipt's SHA-256 over the entire
export. The selected follower image must equal the full exported bytes. Each
version's separately linked fixture scans current and retained views: exactly
one `row` key, all 32,768 bytes, exact current/retained revisions, and the complete
retained-ID list. The source finishes at revision 5 with retained revision 2.
Offline oracle reads must leave the selected image bytes unchanged.

Local commit/capture observations and follower durability acknowledgments remain
separate. Restarting an endpoint selects existing enrollment/data; it does not
reinitialize the follower. Credentials stay fixed across this version upgrade,
so this is separate from [credential cutover](snapshot-credential-cutover.md).

## Running and limits

Use a trusted machine with the recorded offline dependency cache and both
Git commits. On macOS or Linux, run the following command.

```sh
cargo xtask snapshot-cli-upgrade \
  --historical 315068fc38b99d8ec616fdb5a8ee81107b471534 \
  --current 365daf6c8a6b7c85fb049fb5a0370b401dde2424 \
  --output target/my-fresh-snapshot-upgrade
```

### Native macOS client with a Docker Linux follower

The optional Docker mode runs the persisted upgrade sequence with both
client versions on macOS and both follower versions on Linux ext4.

```sh
cargo xtask snapshot-cli-upgrade \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c \
  --output target/my-fresh-cross-os-upgrade
```

Supply an already-local immutable image ID containing Rust and the system build
tools, and retain the offline Cargo registry cache. The controller copies the
same exact historical/current source archives into an owned ext4 volume and
builds both actual CLIs there with `--offline --locked --release`. The native
client builds use the same archives. Each platform links both fixture versions
against its corresponding archived engine rlib and checks the runtime source ID.

The Docker server publishes one port on host loopback. Docker bridge networking
provides the real macOS-to-Linux TCP/TLS path. Builds run offline.
The image, registry and frozen archive/fixture seed are immutable or mounted
read-only. Container limits are two CPUs, 2 GiB memory and 256 PIDs. No existing
container, volume or follower enrollment is reused.

After every server exits, both Linux fixtures check the actual selected ext4
image, then both native fixtures check a fresh byte-for-byte follower copy.
Selected images must equal the independently hashed native exports. The final
new/new stage also repeats a successful revision-5 shipment with a fresh export.
its receipt, export and entire follower directory must remain unchanged. This
adds a duplicate successful transfer check to the existing wrong-pin/retry and
repeated-poll checks. Every server is finite and must exit successfully after
its exact exchange count, so terminating a host `docker exec` process is never
used as evidence that its remote server retired.

Reports bind the native runner and fixture hashes, full original source-file
inventories, commit and archive IDs, original lock hashes, both platforms'
compiler identities, four archived CLI/fixture binary pairs and Linux filesystem
identity. Final Linux source copies must match the original archive inventories;
Linux binary hashes and the frozen seed are rechecked before cleanup. Selected
follower data, exports, both platforms' executables and source copies remain in
the output. Private keys are scrubbed. Cleanup always attempts both owned Docker
removals and independently verifies both authoritative resource inventories.

The native sequence supports macOS, Linux and Windows. Docker mode
requires a native macOS client and an image for the
Docker daemon's architecture. It qualifies this archive pair over Docker's local
routing. Physical cross-host routing and additional architectures remain separate
gates.

### Recorded mixed-OS result (2026-11-16)

The optional mode passed locally from base `69afd3b` plus the explicitly hashed
qualification-driver overlay. Native macOS arm64 Rust 1.99.1 clients communicated
with actual Linux arm64 Rust 1.99.0 follower binaries over Docker TCP/TLS. Both
original archive commits listed above remained unchanged. The Linux data volume
was ext4 and the immutable image ID was the one in the Docker command above.

`target/r05-cross-os-20261116/` contains the successful 143-command mixed-OS
sequence. New-client/old-server retry installed revision 4. Old-client/new-server
shipment installed revision 5. All five Linux servers exited 0 naturally.
Independent offline checks preserved the selected image and retained revision 2.
The repeated successful revision-5 shipment returned the same receipt and export
bytes and left the entire follower directory unchanged. The wrong-pin attempt
captured local revision 4 without advancing durable revision 3. Both owned Docker
resources were independently confirmed absent after cleanup. All 1,607 recorded
artifact hashes were independently rechecked and no private keys remain.

`target/r05-native-regression-20261116/` passed the 62-command native
sequence. All 205 artifact hashes were independently rechecked. Both reports bind
the exact runner, fixture, archived sources, locks, CLI/fixture binaries and all
controller source-file hashes, including the new Linux controller module.

| Input/evidence | SHA-256 |
|---|---|
| Runner binary | `def5a884b8ffd271f5fd6fa691e84c645a00054e76b8eea1560a4ed320ec71d3` |
| `snapshot_upgrade.rs` | `a99bcdc33fdc164923c3b20fa0c4e5fc0a7e967b8630689bd1efc9521f6606fa` |
| `snapshot_upgrade_linux.rs` | `fae4a063edd468024ecbef2c97e433010d8a9c6f90f081aa5f730af4421c1d47` |
| Mixed-OS `evidence.json` | `93b995625669ced6f39e8bfdb410f4d48c5021a033ba3e2540b9a81f6cbfd2e8` |
| Native `evidence.json` | `6a4e2e640c5d9ecc32ba88f9e281d1527e2f244ff96db8e25c913a0bd1e3090a` |

The integrated command is committed at `228f5f8`. The local archive
`target/archived-proof/mixed-os-upgrade-228f5f8.tar.zst` retains
the exact controller source and runner, overlay, reports, both platforms'
executables and all indexed report files. Extraction and independent
rehashing match all 1,607 mixed-OS and 205 native artifacts. Archive
SHA-256
`c220eefb6d19e2543dec15a688e93f43139f25ee25538dda28b06295db48e02b`.

The restart coverage is endpoint process retirement/reopen on the same persisted
volume. Container restart, machine crash and power-loss durability are separate
qualifications.

On native x64 Windows with MSVC and fixed NTFS, run from PowerShell.

```powershell
cargo fetch --locked
cargo xtask snapshot-cli-upgrade --output target/windows-snapshot-upgrade
```

The Windows CLI expects externally provisioned credentials. The runner
restricts each fresh credential directory to the runner's Windows account
before using the separately linked fixture to generate its test leaf and key.
The archived CLIs run unchanged on both platforms.

The output must be fresh, including absence of a dangling final symlink. The
Rust controller accepts only regular tracked files in the source archives.
The Linux qualification runs the same xtask command in an isolated Docker
container with its database and build output on ext4.

Child commands have bounded deadlines and owned process-tree cleanup.
POSIX uses process groups. Windows assigns a suspended child to a job before
resuming it. Server readiness has a ten-second deadline. The supervisor stops
each owned server after its stage and verifies process retirement. Its bounded
logs include the terminal status.
The helper preserves the selected Rust toolchain while removing build overrides.
It records failure metadata, source/tool hashes, exact commands, compiler and
binary identities. Generated private keys are removed from evidence on normal
completion and caught failures. CI's artifact filter also excludes private
key files. Public certificates and synthetic database contents remain.

Cross-host routing, upgrade during a live writer's lease, hardware power loss
and additional historical versions require separate qualification.

## Native Windows NTFS result

On 2026-11-15, PR head `44b492d` passed the full sequence on native x64
Windows with fixed NTFS and Rust 1.99.1 MSVC. The controller ran from PR test
merge `bb539b47b73c3b8b6e502b05f787fb39819874e9`. All 66 command records
verified cleanup, including five intentional server job stops. The same
controller passed its 62-command macOS regression, including its separate
POSIX private-key creation path.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43732690108/job/132868430498)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43732690108/artifacts/14169083095).

The revision-4 wrong-pin shipment exited 1 after local capture with an invalid
peer certificate error and no follower acknowledgment. The same authenticated
server then returned the original revision-3 receipt. An offline reopen and
byte comparison confirmed the follower files were unchanged. The new sender's
trusted retry installed revision 4. The old sender then installed revision 5
on the new server. Both archived CLIs verified the reopened receipt.
Both public-API fixtures checked the full current row and retained root 1
at revision 2.
Repeated polling preserved the final receipt and follower files.

| Revision | Independently verified export SHA-256 |
|---|---|
| 3 | `c91640cb411bdc9e87be9819a11bf1cf14643b522c903a365ac0380c157535d6` |
| 4 | `6dbdf88a033424e93f64a1f90b2cfaa2cf69c7c47bcf86521ac9d792ccfe7d15` |
| 5 | `901d3e7151c150778f2b688016ff27a68ec54028af98014e4448e9f51c758b17` |

All 153 downloaded payload hashes matched. The final selected follower image
matched the revision-5 export exactly. Both source archives matched the macOS
rerun byte for byte, both lockfiles matched their committed Git blobs, and
the four executables matched their recorded hashes and x64 PE headers.
The archive contains all three public certificates and no private keys.

| Evidence | SHA-256 |
|---|---|
| Windows ZIP | `5bcd95fa984f2885f464203c54b8f5c945c02af88c1f489086aad756f5712695` |
| Windows `evidence.json` | `f15add08454406c42b267159dedb85ce8f40c3e7793185563ee390690c27a943` |
| macOS regression `evidence.json` | `b854209d8d38dc99415be6b157034825f0e6dea488342ab714fe784eaacce3ad` |

## Initial recorded local results

Mac arm64 and Linux aarch64 completed the entire 48-command sequence with
verified receipts at revisions 3, 4 and 5. Both built with Rust 1.99.1
(`58cf8f9c3`, LLVM 23.1.1), offline original locks and release optimization.
Mac orchestration used Mise Python 3.14.8. Six admission/parser/cleanup controls
passed normally and under `-O` on both Mac and Linux Python 3.11.2; these include
real timeout/child reaping and caught-failure private-key removal, not merely
mocked success responses.

Linux used immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
network disabled, two CPUs, 3 GiB memory and 256 PIDs. Copied sources, builds and
native database files were on a UUID-owned ext4 Docker volume (`findmnt`
recorded the filesystem). The host source seed and registry cache were mounted
read-only. Results, logs and binaries were copied before removing the owned
container/volume; live Docker inventories confirmed both absent. The frozen
host seed remained unchanged.

`target/snapshot-cli-upgrade-365daf6/` contains exact archives, every original
source-file hash, unchanged lock/manifests, linked fixture/CLI hashes and source
identities, command logs, synthetic data/export artifacts, and the Linux
orchestration script. All recorded artifact and binary hashes were independently
rechecked after execution, including both copied Linux CLIs and both fixtures.
No generated `key.der` remains in either evidence tree.

Initial runner SHA256:
`281608fbaa1e5a6caa7faa8250aac47a1295928418c259290dbdb042df146cb5`.
Fixture SHA256:
`372c4d5c1f444c1ba8754dbe009b553ce7cb614a25ba6c4a53da19c25d379079`.
Top-level index SHA256:
`8de1b72db5caa1bf00a5aa7b51b155669826952fb87de2d5c71b041c6505257c`.
The Mac and Linux manifests have SHA256
`0193048ab206897a3cd1bb88602f2ca0efe6fdd9fae3e1955a22196e2f39c0b7`
and `54f49a15ec1c39f94aae2e9437a5165aaf32083bdc95fdaf42427374e807f7ef`.

The initial Mac attempt, retained as `initial-rlib-discovery-failure/`, built the
historical CLI but stopped before fixture or transport execution because the
harness expected a hashed rlib filename. This crate emits `libweekend_db.rlib`;
explicit single-match discovery was corrected. Only the later complete runs
support interoperability claims. No crate publication, remote CI or visibility
change occurred.


## Process cleanup requalification

Review found that the shared `compatibility.stop` helper killed a process group
only while its leader was alive. A failing CLI that spawned a descendant and
then exited could leave that descendant running. The helper now always signals
the owned new-session process group and reaps its direct child with the existing
five-second bound. A real early-exit-2 leader/sleeping-descendant control passes
with the corrected helper; restoring the old predicate makes it fail because
the descendant remains running. The test distinguishes a stopped orphan zombie
from a running process and cleans up its fixture even on assertion failure.
Seven compatibility controls passed normal and `-O` on Mac; the two actual
process-cleanup controls passed normal and `-O` on Linux Python 3.11.2.

Review also identified that the ignored Linux wrapper could skip volume removal
when container removal raised. The committed `cleanup_docker` helper now always
attempts both owned UUID removals, then independently checks both authoritative
Docker inventories. A removal error can be accepted only if both resources are
actually absent. Daemon/query failure cannot claim absence. A negative control
checks timeout, second cleanup attempt, both inspections, and invalid ownership.
The final frozen Linux wrapper invokes this helper.

Both full 48-command Mac/Linux upgrade sequences were rerun successfully with
these corrections. Seven snapshot controls passed normal and `-O` on both
platforms. Current proof is `target/snapshot-cli-upgrade-365daf6-v2/`, including
all source/tool/binary hashes, command logs, the wrapper, and cleanup controls.
All output artifacts and both versions' CLI/fixture binaries were independently
rehashed. Both final Docker resources were absent with no cleanup errors, the
source seed remained unchanged, and generated private keys were removed.
The prior complete runs remain under the original path and
`target/snapshot-cli-upgrade-365daf6-v2-pre-wrapper-fix/` with their own hashes;
they are not relabeled as testing the later cleanup implementation.

Final runner SHA256: `697a5ab5856083bb82ab40a61a7712b199c8b5552535a5d991c9ede5b6d53335`.
Shared helper SHA256: `799a2ef524163d6b664b64e3919bf99f204a9fc5bbe1246bdfbb3765bf371c4a`.
Final v2 index SHA256: `af9cd13ee46e40a5784c6618397210b1b264e832f32c52647942bdbc004a6d49`.
