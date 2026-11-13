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
mutual TLS profile. The historical CLI already implements `status`; this test
uses only common `keygen`, `describe`, `follower-init`, `serve`, `ship`, `poll`
and `status`. It does not invoke newer maintenance commands. There is no
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

Each receipt's SHA256 is independently computed in Python over the entire
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

On a trusted POSIX development machine with the recorded offline dependency
cache and both Git commits:

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/snapshot_cli_upgrade.py \
  --historical 315068fc38b99d8ec616fdb5a8ee81107b471534 \
  --current 365daf6c8a6b7c85fb049fb5a0370b401dde2424 \
  --output target/my-fresh-snapshot-upgrade
```

The output must be fresh, including absence of a dangling final symlink. Safe
archive extraction requires Python's `filter='data'` support; the qualified host
uses Python 3.14.8. The Linux wrapper safely extracts on the host and invokes
`qualify()` with the frozen copied source/hash manifests inside the container;
it does not claim the Python 3.11 container ran the top-level extractor.

Child commands have bounded deadlines and process-group kill/reap on failure.
Server readiness has a ten-second deadline; each finite stage stops its owned
server in a `finally` block. Output goes to files rather than undrained pipes.
The helper preserves the selected Rust toolchain while removing build overrides.
It records failure metadata, source/tool hashes, exact commands, compiler and
binary identities. Generated private keys are removed from evidence on normal
completion and caught failures; public certificates and synthetic database
contents remain. A hard kill of the Python orchestrator can prevent cleanup:
protect its mode-0700 output directory and discard leftover test credentials.
No endpoint process is a production server.

These finite loopback checks do not establish native Windows, cross-host
routing, upgrade during a live writer's lease, hardware power loss, arbitrary
historical compatibility, or a long-term support policy. Existing crash and
credential rotation qualifications retain their own scopes.

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
