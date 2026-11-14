# Frozen pre-1 storage compatibility at 21e5d26

On 2026-11-12, private qualification passed between historical
`a7dc34309a8841ab3ee72fd88b9cae9281507b7c` and current
`21e5d2638c24c6254ced35d0077c55bb30a287b2` on native arm64 macOS/APFS and
isolated arm64 Linux/overlayfs. Each checkpoint wrote an image and both
checkpoint readers verified its complete fixture state. Unsupported-version
rejection preserved every byte. This is a small base-feature pre-1 format-two
case, not a stable compatibility, crash-survival or release-readiness promise.

The [runner instructions](../tools/compatibility.md) describe reproduction and
scope. No repository, crate, artifact or result was published; all work was local.

## Runner corrections

Independent fault controls reproduced two assurance gaps before this run:
resolving a dangling output symlink admitted its redirected missing target, and
Python `-O` removed storage byte-preservation assertions. The corrected runner
rejects an existing final path before resolving parents, uses exclusive mkdir,
and performs production checks with explicit failures under either Python mode.
Successful consumer exit alone cannot establish unchanged bytes.

Storage `--current` now selects a committed archive, as snapshot already did.
Both historical/current endpoints are frozen; neither is built from the mutable
checkout. The only overlays are the reviewed Rust fixture, a separate identity
probe and their example declarations. Both examples compile against each
endpoint's original workspace and original unchanged Cargo.lock with
`--offline --locked`. No generated downstream resolution replaces those locks.
Private package manifests retain publish=false. Sources, logs, original lock
copies and build artifacts remain available in the output directories.

Each storage endpoint's native version API probe reports its exact source ID,
features=0, ABI=1 and format=2. The probe and actual consumer have separate binary
hashes and share the same frozen library build. All overlaid/archived source
files are rehashed after build and after consumers complete. Known ambient
compiler flags/targets/wrappers/profile overrides are scrubbed; local Cargo
configuration and existing dependency cache remain trusted inputs.

Git/tool/archive subprocess waits are 30 seconds; builds are 180 seconds;
storage consumers are 30 seconds. Timed-out process groups are killed and direct
children reaped with a five-second wait. Failed qualification retains explicit
verified=false/failure metadata. These are bounded subprocess waits under normal
OS scheduling, not a claim about arbitrary kernel/filesystem liveness.

## Storage fixture and observed result

Each writer creates collection 1, commits `key=11` at revision 2, retains exactly
ID 1, then commits `key=21` and `new="current"` at revision 4. Every reader checks
current revision 4, exactly two current rows and their complete values, retained
IDs exactly `[1]`, historical revision 2, exactly one historical row with value
11, and absence of `new` historically. Python separately compares the complete
image bytes before/after each reader.

For each writer, a copied image receives header version 0xffffffff. Both readers
must return UnsupportedFormat and preserve all its bytes. The mutation precedes
CRC validation; this is not a valid future encoder or a format migration test.
Each platform executes two writes, four logical reads and four unsupported
rejections, plus two builds and two runtime-identity probes: 14 recorded commands,
all exiting zero. Every resulting witnessed image was 69,632 bytes.

| Platform / writer | Readers | Current / retained revision | Witnessed image SHA-256 |
| --- | --- | --- | --- |
| macOS / current | current + historical | 4 / 2 | `2537cddbae8d73ae979e7c17d9fcc5a76c96cb9149d95066977300f2769ed5f2` |
| macOS / historical | current + historical | 4 / 2 | `a17ce0a2c514efc568d9cecba0ed1b94307128aee66a0dd5c2bdc88cc4605ff5` |
| Linux / current | current + historical | 4 / 2 | `497d318dc8c1749a2a6e422734b43d6613e42db2c97c7827b734421c4cb6faf8` |
| Linux / historical | current + historical | 4 / 2 | `bda7071b54f8c960cda5d95cfd1de0f52e68fc092c9b5570c4edae1fad56ecf7` |

Fresh file identities make differing image hashes expected. Both readers also
preserved the corresponding unsupported-image hashes recorded in evidence.
No old/new disk format translation was needed or attempted.

## Source, compiler and platform binding

Native commands used Mise Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1) and Python
3.14.8, debug builds and Python `-O`. This qualifies both sources with the same
installed compiler, not each checkpoint's original historical toolchain.

| Endpoint | Git archive SHA-256 | Original Cargo.lock SHA-256 |
| --- | --- | --- |
| a7dc343 | `c520b04cf6ef6eb0770209bb00d715be651c337e49dd297b1a0b583595fdb56b` | `353ab69da0e2a7ecc9094126773d277a7be1ef64d2f829d9d7be0b9488113d28` |
| 21e5d26 | `a2b8a3160c6521d869473e2801cb897be6f76a870fa96dbce04415245c6a2c21` | `384d128361f436d847264173af62063e151d93b5396e025459e2066c289f881a` |

The Linux run used local image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
network disabled, 2 GiB container memory and 256-process containment. It reports
the same Rust 1.99.1 compiler on aarch64 Linux and uses writable-layer overlayfs.
The existing host registry was mounted read-only; no package was fetched.
Container removal and daemon-live absence checks passed.

Installed container Python 3.11 lacks safe tar extraction filters. Accordingly,
Linux compiled the already safely host-extracted, frozen Mac input sources,
mounted read-only, rather than weakening extraction safety. A retained private
wrapper uses the shared bounded command runner and storage_cases, repeats the
checked Cargo/runtime-identity steps, and compares
every source hash against the Mac snapshot before compilation, retains original
lockfiles and checks every storage oracle under `-O`. Linux did not exercise the
top-level CLI's archive extraction step. The wrapper, commands and containment
metadata are preserved beside the Linux result. An initial attempt without the
required cached registry failed offline and is separate from the final result.

| Artifact | SHA-256 |
| --- | --- |
| `target/storage-compatibility-21e5d26-mac-final/evidence.json` | `6e91a57a747b623ae1fdd0d839a59f42f91630ccb89269dc70b34ece64feae5c` |
| `target/storage-compatibility-21e5d26-linux-final/result/evidence.json` | `0a32ffdf1662509cc23659c4f198c4c6b4ccd8ac150893ca8cd61119bff97042` |
| Linux `docker-evidence.json` | `b5e2dec324a8c75251370044abd25613ff2240909babe47f500bb470d351da4c` |
| Runner | `a7a4a8b7c66cf66d2546ccf6fcfc1dcd12f9ae8e51554eff009efa5b4bd51839` |
| Storage Rust fixture | `248c4ea0971467c14fc51b00b6026f0f535ff890f050b246361aa378074d2c62` |
| Python regression controls | `00a2c0fa57f80d27ed42449789f6199a7c42db31f754e1fd211a3852d9efa3a8` |

Exact consumers are independently hashed in each manifest: macOS historical
`943d685c3c303bc0c125856a53ef4e19ff3310e169338b6a94976135f93d94e9`, current
`39c17e19a7dee042cd61eecc2c2909aa2e897f507b5b2ed2ae918845e7bfeb1e`;
Linux historical `02520b2e114fb2cfa1722bab4c62e9d8ce53a04cc94c4ee8c95b07359cfd11f0`,
current `d7af7353479336510e8bce3a299fc8f27aef196093c11c3c0b21d1a09b3cb56c`.

## Snapshot-profile regression and negative controls

The snapshot fixture was not changed. Native Mac `-O` runs passed both directions
for dcd2402↔315068f (the prior recorded pair) and dcd2402↔exact21e5d26. Both retain
the full current/retained row oracle, source revision 4 despite primary local
advancement to 5, independently hashed export/receipt agreement, reopened
follower state, identical duplicate acknowledgment and unsupported **poll**
version rejection with all closed follower files unchanged. This remains
caller-owned private loopback, without TLS, trust rotation, upload faults or a
mixed-OS transfer claim.

| Mac snapshot result | Evidence SHA-256 |
| --- | --- |
| `target/snapshot-compatibility-runner-regression-21e5d26-final/evidence.json` | `7aff534a6752a1cffd5c4a0401c6a5de69a22d6eab369c070cbf547272c2adf7` |
| `target/snapshot-compatibility-21e5d26-mac-final/evidence.json` | `68f183fea39bf517e9fbfb4ca473a6f545d8398302b8fc531515d744490d66c0` |

dcd2402 predates native version introspection. Its separate probe is explicitly
labeled qualification_build_stamp, reports its compile-time source/replication
selection, and exercises an empty Database. Archive/lock/binary hashes bind it;
it does not claim a nonexistent native SOURCE_ID API. 315068f and 21e5d26 use
their native version APIs with replication feature bit 1. This distinction adds
provenance without weakening the unchanged snapshot protocol oracle.

Six Python regression tests passed normally and with `-O`: both-profile
existing/dangling-output rejection before build; successful controls covering
both storage writers; zero-exit reader and rejector mutation controls that fail
and retain failure evidence; requested-commit/original-lock/locked-example
controls; and a real process timeout/cleanup control. These are runner tests;
the actual native consumers above separately exercise engine behavior.

No core source changed. Broader data shapes, feature combinations, arbitrary
checkpoints, Windows execution, cross-platform transfer, fault/power-loss
behavior, typed-schema migration and stable support-policy commitments remain
separate qualification obligations.
