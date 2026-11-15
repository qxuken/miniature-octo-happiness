# Near-cap maintenance ENOSPC and quota recovery

Checkpoint `74fd4e8d256043f027d4cfa353ec33d957d0d64f` passes actual kernel
space exhaustion during **backup, restore and in-place compaction**. Docker
Linux aarch64/tmpfs supplies `ENOSPC` (28); hosted Ubuntu 24.04 x86_64/ext4
supplies `EDQUOT` (122). Both profiles verify complete current and retained
data while full, successful retry after freeing space, and a later acknowledged
commit reopened by another process. Production engine code, serialized format,
durability barriers and separate follower durable acknowledgment are unchanged.

## Workload and bounds

The existing [compound fixture](compound-resource-envelope.md) supplies a
66,154,496-byte source: **98.58% of the 64 MiB image cap**, revision **212**,
48 distinct 896 KiB bulk values, eight worker records and **64 depth-64 retained
roots**. Retained IDs 1..64 capture revisions **53,55,..179**. Its independently
verified compact backup is 63,074,304 bytes. No padding manufactures image
occupancy. The existing concurrent-draft, reader, allocation/budget and copy
profiles run during seed generation.

Seed generation runs outside the filesystem being exhausted: its bounded
temporary copies require more space than the exhausted fixture permits.
Only the verified source and backup are installed into that fixture. Install
uses new, nonsymlink regular files, checks size/SHA-256 and fsyncs both copies.
The same unprivileged UID creates files and executes all fixture/probe phases;
supplementary groups and all five capability sets are empty, with NoNewPrivs=1.

| Profile | Private exhausted filesystem | Independent ballast ceiling | Error observed |
|---|---|---:|---|
| Local Docker | 256 MiB tmpfs | 384 MiB | 28 / DiskFull / Write |
| Hosted quota | 512 MiB ext4 loop image; 256 MiB UID soft/hard block quota; no inode quota | 384 MiB | 122 / Quota / Write |

The quota failure has **242,409,472 bytes globally free** and **204,832,768
bytes available**, both above the required 16 MiB. An ENOSPC result cannot pass
as a quota result. Local tmpfs reaches zero available/free bytes. Neither
controller fills the host filesystem. The quota controller binds the private
image inode, loop association, exact mount and tool identities before changing
quotas or cleaning up. The earlier 4 MiB quota/64 MiB append profile is unchanged
and also passes in the same hosted job.

## Failure and retry protocol

For each of backup, restore and compaction, the controller:

1. Fills only its owned ballast file to the required kernel error, then truncates
   and fsyncs it to release exactly **8 MiB**. This permits an actual partial copy.
2. Requires maintenance to return `Io` with the expected native error/category
   and exactly one new incomplete output or compaction scratch file. Both source
   and good backup must remain byte-for-byte unchanged.
3. Opens the source and good backup in **separate fresh processes while still
   full**, checking revision 212 and every current/retained value.
4. Removes only ballast, then proves a same-UID 4 KiB write and fsync succeeds.
5. Rejects inspection of the partial image as Corrupt/UnsupportedFormat. Backup
   and restore reject that existing destination with AlreadyExists and preserve
   its bytes, then retry at a new name. Compaction retries with the old scratch
   name still present. Successful output must exactly match the good backup;
   the failed partial remains unchanged.
6. Independently opens source, successful output and good backup in fresh
   processes. Only afterward does the test retire its own partial/retry outputs.

| Operation | ENOSPC partial bytes | EDQUOT partial bytes | Reopens while full | After freeing space |
|---|---:|---:|---|---|
| Backup | 8,388,608 | 8,392,704 | Current 212; all 64 retained roots | Fresh-path retry succeeds; exact backup bytes |
| Restore | 8,388,608 | 8,392,704 | Current 212; all 64 retained roots | Fresh-path retry succeeds; exact backup bytes |
| In-place compact | 8,388,608 | 8,392,704 | Current 212; all 64 retained roots | Retry with orphan present succeeds; exact backup bytes |

The quota allocation boundary allows one additional 4 KiB block in the partial
copy. Every output remains smaller than a valid compact image. Maintenance
preserves revision 212; afterward an acknowledged transaction changes worker
zero from sequence four to five and another process reopens **revision 213**.
All 64 retained roots retain their original values, and the good backup remains
at revision 212. The test's explicit retirement is not automatic engine orphan
collection, and an existing failed backup/restore destination is never silently
overwritten.

Each profile executes **32 distinct fixture processes**, including the seed,
with 30 checked phase records. A required-failure negative control first runs
with ample space: successful backup makes `unwrap_err()` panic (exit 101), its
actual output passes a fresh complete oracle, then only that owned output is
removed. This prevents a failure fixture from passing when maintenance succeeds.

The Rust oracle checks every bulk byte, nested leaf, record/worker sequence,
retained ID and captured revision in current and every retained view. Separate
Python fingerprints check expected values and revisions; they are accidental
corruption checks, not authentication. Copy uses a 256 MiB working-domain
budget; fresh opens use the original 96 MiB core budget. Process RSS and core
admission remain separate.

## Source-bound results and review

The [hosted quota job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43629887189/job/132553924242)
passes on kernel `7.0.0-1017-azure`. Its PR merge source is
`b75be5e2f24d6c498d3af7f723c5df03cae1ff66`; all **662** archived source-file
hashes match checkpoint `74fd4e8`. Local Docker uses the checkpoint directly,
Rust 1.99.1, Linuxkit 7.0.14 and pinned image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.

Root review downloads artifact `14127129087` (`linux-quota-X64`) and verifies its
ZIP against GitHub's API digest:
`d027b08d641de2b90d5b17e3883eefb6f10ed9c0787050a9ab0aa9b8717d65ab`.
The exact uploaded maintenance binary hashes to
`5fed1dbe7edea5b668a1bba2dc81ee521e6986452707aa67ba0748760ad9f4dd`.
Review recomputes all **128 quota evidence payload hashes** and **100 local
payload hashes**, revalidates complete phase order, native errors, probe
privileges, process identities, logged outputs, full-root oracles and negative
controls. Quota cleanup reports no errors and independently proves mount/loop
absence; Docker proves its container and private volume absent.

Ignored local proof trees are `target/maintenance-quota-74fd` and
`target/maintenance-space-final`. Review index
`target/maintenance-space-qualification/summary.json` has SHA-256
`88c7fea09cc2be3bf1a0912604d0b151fa5023b8d2a2e1f27a3577a162352de6`.
These paths preserve local evidence rather than promising long-term archival;
GitHub's artifact has its configured retention period.

Targeted Mac core compaction/durability/headroom/retention regressions pass
**34 tests in debug and 34 in release**. The three new helper controls and 12
existing quota controls pass with normal and optimized Python. Strict private
driver Clippy, root/private formatting and actionlint 1.7.7 pass. The helper
registry includes the new controls and CI runs them on Linux and Mac.

The final local registry run passes **344 controls**, with four explicitly
registered optional skips (348 reported tests, 52 normal/optimized workers).
Its evidence is `target/maintenance-space-helpers-reviewed/evidence.json`, SHA-256
`07c648d3dd8416478c384d193aa7a1a4d05e9422ceb7d8666cc67227f04a2e88`.
Two earlier local attempts stopped in the existing optimized compound
output-limit control: an incomplete observer report lacked `failure`, masking
the original assertion with KeyError. Direct diagnostic reruns and the final
full registry pass. The control now reports the original assertion when that
key is absent, while still requiring an output-limit rejection. The original
intermittent cause remains unproven; logs in
`target/maintenance-space-qualification/helpers-{final,repeat}.log` and their
referenced worker directories retain the failures. This diagnostic improvement
does not claim to fix the underlying intermittent observer issue. The hosted
Mac/Linux helper jobs pass independently at the frozen checkpoint.

The subsequent [observer finalization fix](helper-observer-finalization.md)
reproduces and fixes the missing-terminal-report mechanism with deterministic
cleanup-probe failures. It retains failed cleanup as a qualification failure,
adds startup/wait/cancellation and composed-error regressions, and passes the
complete Mac/Linux helper registries. That follow-up preserves the qualification
and evidence above at their original source; it does not reconstruct the lost
historical assertion or diagnose a kernel fault.

## Reproduction

Local Docker, with the pinned image already available:

```sh
cargo xtask quota-enospc \
  --revision 74fd4e8d256043f027d4cfa353ec33d957d0d64f \
  --output target/maintenance-space-new
```

The Docker controller overlays the named private harness files and records their
hashes. To reproduce the checkpoint exactly, use those files from that revision.
On a disposable quota-capable Linux host, from an exact clean source checkout
with quota/e2fsprogs/util-linux installed:

```sh
source_id="$(git rev-parse HEAD)"
cargo +1.99.1 fetch --locked
cargo +1.99.1 build --offline --locked -p xtask --release --target-dir target/xtask
WEEKEND_DB_SOURCE_ID="$source_id" cargo +1.99.1 build --offline --locked --release \
  --manifest-path tools/resource-envelope/Cargo.toml --features compound \
  --bin weekend-compound-resource --target-dir target/maintenance-quota-build
sudo "$PWD/target/xtask/release/xtask" quota --maintenance \
  --fixture "$PWD/target/maintenance-quota-build/release/weekend-compound-resource" \
  --source "$source_id" --output "$PWD/target/maintenance-quota-new"
```

Dependencies must already be cached for the offline build. The quota entrypoint
requires Linux root only for its bounded filesystem provisioning; guest engine
operations drop privileges. Missing quota support or unproven cleanup fails the
qualification rather than becoming a native pass. `--probe-only` cannot be
combined with `--maintenance`.

## Remaining scope

This closes the named near-cap **copy-phase** ENOSPC/EDQUOT maintenance gate.
Actual filesystem exhaustion during exchange, cleanup, and delayed writeback,
physical power interruption, and global inode exhaustion on full ext4 devices
remain separate gates. Earlier process-kill and simulated
namespace/recovery matrices retain their separate scope. All 19 Cargo
manifests retain `publish=false`.

## Native Linux inode quota at file creation

The `--maintenance --inode-quota` profile uses the same near-cap images,
private 512 MiB ext4 loop mount, unprivileged UID, and complete reopen
oracle. It sets a four-inode user limit with unrestricted block usage.
The data directory, source, backup and one owned ballast exhaust the
UID's inode allowance. A separate exclusive create must return kernel
`EDQUOT` while global free inodes and data blocks remain available. The
same UID must still write and fsync 4 KiB in the existing ballast file.

Backup, restore and in-place compaction must return `Io` with native
`Quota` and operation `Create`. The controller checks that the directory
still contains only the source, good backup and ballast, and compares
the exact source and backup bytes. A fresh process opens each image
while the limit is active. After removing its ballast, the same UID
checks exclusive creation again and retries maintenance. Backup and
restore retry at the original destination name. The compact retry uses
the original canonical file. Each successful image must match the good
backup's bytes and reopen with the complete retained history.

Build the controller and compound fixture as in the block-quota profile
above, then run the same `quota --maintenance` command with `--inode-quota`
and a fresh output directory. The hosted quota job runs both profiles.

### Recorded native inode quota result

PR head `f3d42b8` passed on Ubuntu 24.04 x86_64 with kernel
`7.0.0-1017-azure`. The hosted checkout was
`a69c3b9ea5bdd74464680071ac30cb631682690a`. Each calibration returned
errno 122 from exclusive create with **32,753 free inodes** and
**344,027,136 bytes available**, and fsynced the existing ballast.
The same job also passed the previous append and block-quota
maintenance profiles. [Hosted quota job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43745342960/job/132907295547)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43745342960/artifacts/14173049779).

All three maintenance operations returned `Io / Quota / Create`, raw
errno 122. No destination or scratch file appeared. The source and
backup reopened at revision 212 with **all 64 retained roots** while
the limit remained active. Each retry passed after releasing one inode.
A fresh inspector then verified witnessed revision 213 in the advanced
source and revision 212 in the unchanged backup. Retained IDs 1–64
still reported revisions 53, 55, …, 179 in both images. Read-only
inspection preserved the original hashes.

The independent audit checked 689 archived source hashes, 195 evidence
payload hashes, 64 command cleanup records, and all 30 maintenance phase
records. Its separate payload oracle recomputed the expected current and
retained digests. Cleanup proved the owned mount and loop were absent.
Docker Linux passed 19 quota controls and strict Clippy for the
controller and fixture. The Windows-target fixture Clippy check passed.
Compaction, inspection and retention tests passed on macOS with 18 tests
each in debug and release. Local proofs are under
`target/linux-inode-quota-f3d42b8/`.

| Evidence | SHA-256 |
|---|---|
| Hosted ZIP | `689e8ae36c5dda1c8604e74ca8b528dc83a1b2b49a019ea6ff5b3dc06d4f151e` |
| Inode quota controller evidence | `d1596104eb8768f29eed8602cc00a24cb4bd44f5d5770a0653df05b009118664` |
| Exact uploaded fixture | `f6c88ff99e7a31ed9f121527b9c2d8d47e3974ff8ddd06ee53e527d8518ba531` |
| Advanced source at revision 213 | `236eb1c4f73012036b05ecc7d84f36819cfd96083ef6062e034fd6fd93ef9ebe` |
| Backup at revision 212 | `5dde4f8628feda0cdc422362b9f92dcaf4c3f0bebdc5c533e7ccbb022df8c11e` |
| Independent audit | `c8af1e112a3a9ef2619f3632e023436948550fb2d2e600fa5c7b831fa91970cb` |

## Recorded native global inode exhaustion result

The `quota --maintenance --inode-enospc` profile formats a private 512 MiB
ext4 loop image with 128 inodes and quotas disabled. It uses the same build
and source-binding procedure as the inode-quota profile above. Replace
`--inode-quota` with `--inode-enospc` and choose a fresh output directory.
The formatter sets the inode count explicitly. [e2fsprogs mke2fs manual](https://kernel.googlesource.com/pub/scm/fs/ext2/e2fsprogs/%2B/29cebc07bd8fa5ec30f75269183b1130861f3acc/misc/mke2fs.8.in)

An unprivileged guest allocated the 113 remaining inodes in an owned sibling
directory, leaving the database directory alone. It required a fresh creation
to return Linux `ENOSPC` (28) and fsynced 4 KiB on an existing inode while
global free inodes were zero. All three maintenance operations returned
`Io / DiskFull / Create` without leaving a partial file. Directory entries
and the bytes of both input files stayed unchanged. Separate processes verified
revision 212 and all 64 retained roots for both images while free inodes
remained at zero.

Removing one owned ballast file left exactly one free inode. A same-UID
control created and fsynced 4 KiB, then removed its file. Each maintenance
retry used the available inode and produced the exact verified backup bytes,
including the in-place replacement from compaction. The run reestablished
zero free inodes before each subsequent fault. The final source reopened
at revision 213 and the backup at 212, both with retained IDs 1 through 64
covering revisions 53 through 179. This qualifies creation-time exhaustion
on the tested ext4 profiles. [Kernel permission denial during later namespace
operations](#kernel-permission-denial-at-exchange-and-cleanup) has separate
evidence. Namespace space exhaustion and physical power loss remain open.

The [hosted Ubuntu 24.04 x64 job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43749868875/job/132921247478)
passed with Rust 1.99.1 at PR head `6a8bd8e` and tested merge
`50a9fcc724207c40b83b2dc86c9f9632fb376285`. Docker Linux arm64/ext4 with
Rust 1.99.0 passed at source `6a8bd8e356dfb7cd6171c2c95d9eaf804fb30951`.
Both observed 389,963,776 bytes globally free and 352,387,072 bytes
available at creation failure. Both retired the owned loop and mount.
The hosted append and maintenance block-quota and inode-quota controls
also passed. The Docker controller passed 21 quota tests and strict Clippy
passed for the controller and near-cap fixture.

The independent hosted audit matched 691 source files against the exact Git
archive, verified 192 payload hashes and 63 command cleanup records, and
recomputed the current and retained data digests for all 30 maintenance phase
records. Read-only admin checks on the uploaded images confirmed both
witnessed revisions and every retained ID and revision. Both image hashes
matched before and after inspection. Local evidence is under
`target/inode-enospc-build/`.

| Evidence | SHA-256 |
|---|---|
| Hosted ZIP | `d7dfab69f71a6d29b3d34737d6c4d67567e9dcdc0d837c4371dc58b815a68aff` |
| Hosted controller evidence | `44dd5483cb8b3bd2bf26a083929641eb0ba89c09a57e04507672875c54217b3f` |
| Exact uploaded fixture | `ccc2558f1bacac51fee281b659a263bbeab4484f88aa23defafcad2dc90c920f` |
| Advanced source at revision 213 | `6d3e46245c1d9c0664e083813eb6a8e27a7382b0c275f185dc94ebe177c3f7fa` |
| Backup at revision 212 | `0606b5d2c21c82bef96eced555e498a1125c06d5ccdd853326ddc5b4a208289b` |
| Hosted independent audit | `e445c334aadcc970cd3d48599cfc01d45437824ca0d6270ca3a925d350da2d2d` |
| Docker Linux controller evidence | `0c525d81e79201fb3ee862c5ed2f00fd04364af8b0c8f8c9d1dc346485709758` |

## Native Windows near-cap copy profile

Run from an elevated native x64 Windows session on fixed NTFS. The Windows
controller reuses the same 212-revision, 64-retained-root seed and creates a
fixed 256 MiB VHD. It verifies the NTFS extent and the 4 KiB allocation unit
before installing both source and independently verified backup.

```powershell
$env:WEEKEND_DB_SOURCE_ID = (git rev-parse HEAD)
cargo build --offline --locked --release --manifest-path tools/resource-envelope/Cargo.toml --features compound --bin weekend-compound-resource --target-dir target/maintenance-win-build
cargo xtask windows-maintenance-space --fixture target/maintenance-win-build/release/weekend-compound-resource.exe --source $env:WEEKEND_DB_SOURCE_ID --output target/windows-maintenance-space
```

Windows exercises `backup_file`, `restore_file` and `Database::compact_to`
with a new file. The in-place compaction and namespace exchange API remains
scoped to macOS and Linux. The Windows fixture asserts native error 112
(`DiskFull`, `Write`) after a partial copy. It starts each operation with
8 MiB free, consumes any remaining space after the failed write, and reopens
the unmodified source and good backup in separate processes at zero free
bytes. Every retry keeps the failed destination at its original path and
produces bytes identical to the good backup in a fresh destination. A later
commit on the original reopens at revision 213 with all 64 retained roots.

The hosted native Windows x64 run at PR head `cd3d14a` used Rust
1.99.1 and executable source `35725723c08a6b6a8fa991956499eadc37580e2d`,
the tested PR merge. All three operations wrote exactly 8,388,608 bytes
before returning 112 / `DiskFull` / `Write`. Each fresh full-volume open
verified revision 212 and all 64 retained roots. Removing the ballast restored
114,339,840 free bytes per case. Retried output matched the verified backup
byte for byte. The later source commit reopened at 213 and the good backup
stayed at 212. All 27 command trees retired and the VHD detached and was
removed. The required-failure control with ample space produced a valid
backup and failed at the intended assertion.

The independent archive check rehashed 60 files, checked the fixture
executable and verified that the final backup and original verified backup
were byte-identical. It confirmed 26 distinct fixture processes and all
24 checked phase records. The [Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43726145853/job/132848384551)
provides the [maintenance evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43726145853/artifacts/14166034096).

| Evidence | SHA-256 |
|---|---|
| Windows archive | `5af2efd943f5b27e9296f1f27d9f2e967123775fbfb8897ce415eabde3217502` |
| Windows evidence JSON | `3561751093999cb451236ccfa1847769f78d87d7fe837db207cdad93c3fc1370` |
| Independent archive rehash | `be2eedf5ea485b9eedbe58f36bc186e9e5a377890540afd754218ed23f436a70` |
| Windows advanced source | `4c9982b7f4caa6e687118458112cf0131fba71221b56868e686c1eecd9086bc3` |
| Windows preserved backup | `7bb1f25890ca7281c1e7d084bb7b6eb45940be2f52c057915403b35d8d83ddbe` |

### NTFS owner quota during maintenance copies

The owner-quota profile uses a separate fixed 256 MiB VHD and the same
near-cap seed. The controller assigns both seed images to Local Service.
Each dedicated fixture process sets its token's default file owner to
its own user SID. The controller verifies that SID on a calibration file,
the partial image and the retry image. It configures an 8 MiB hard quota
for the copy owner on the bound test drive only. The elevated fixture
and controller share the hosted runner account. This requires no host
account provisioning. [NTFS disk quota limits](https://learn.microsoft.com/en-us/windows/win32/fileio/disk-quota-limits)

The fixture must receive the same Win32 write error as the bounded
calibration while the volume still has physical space for a complete copy.
After fresh opens verify the source and backup, a same-owner probe must
still hit the quota. The controller raises only the owner limit to 80 MiB
with the partial file present. A same-owner 4 KiB fsynced write must now
succeed. The copy retries at a new path while the original partial and
both good images remain byte-identical.

Build the Windows fixture as above from a byte-preserving checkout and
run the same command with `--owner-quota` and a new output directory.

The first two attempts used a restricted impersonation token. The
calibration could read the data directory and received Access Denied on
file creation, including before quota enforcement. The controller detached
and removed both VHDs. The process-owner approach keeps the runner's elevated
token and changes only the default file owner within each fixture
process. A later run also showed Windows reusing a PID after a fixture
exited. The supervisor now records the kernel creation time together
with the PID. The quota checks use the pair for process identity.

#### Recorded native NTFS owner-quota result

PR head `b5988c7` passed on native Windows x64 with Rust 1.99.1 on a fixed
NTFS VHD. The hosted checkout was `04838eff8763fef1f13fb69241b78413d62d7184`.
The byte-preserving checkout matched all six source and lock hashes
expected by the controller.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43748445011/job/132916838914)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43748445011/artifacts/14175422212).

Backup, restore and compact-copy each wrote **8,388,608 bytes** and returned
`Io / DiskFull / Write` with Win32 code 112. Calibration returned the same
code while NTFS still had **114,335,744 physical bytes free**. Both good
images reopened at revision 212 with all 64 retained roots. The same-owner
probe confirmed the quota was still full. Raising the quota to 80 MiB with
the partial file present permitted the 4 KiB write and each copy retry.
Retries matched the good backup byte for byte. A later commit reopened at
revision 213 with all retained history. The backup stayed at 212.

The audit rehashed 112 payloads, checked 53 supervised commands using the
PID and kernel creation time, and independently recomputed current and
retained digests for the 24 maintenance phase records. A separate
read-only inspector verified witnessed revisions 213 and 212 in the
uploaded images and retained IDs 1–64 at revisions 53–179 in both. Its
reads preserved the file hashes. The controller detached and removed the
private VHD. The Linux append, block and inode quota profiles also passed
in the same workflow. A macOS/APFS disk exhaustion regression passed at
`e78e127`. A standard Windows account remains a separate qualification.

| Evidence | SHA-256 |
|---|---|
| Hosted archive | `6d2ebb602a660a53e4f49a199fa511ca280466726eefedcceca59d29e8ea41f3` |
| Owner quota report | `1bae58d446ef1aba9af72292899bf24b7595ace60cc81dc76066ce9a4d4e15e6` |
| Uploaded fixture | `7cd3a0f97f7cd593c9b6b041eeda1d8a368043a265b9d7ded8f84d91a9d1f706` |
| Advanced source at revision 213 | `00fa1f33383275379a71c867423a168e5ff8e5211a15d43f8347e0e7a95e907f` |
| Backup at revision 212 | `2a024795cdcf04bbbdff0b8d8514b0def685185cd7522f981a820761105ebf5e` |
| Independent audit | `f1bd9bd0b1a66952cd45dad6e63850ab9cbd904d51cea3f0236c1efe43a2f6a9` |
| macOS regression report | `47629c94ece43232dc28dfbfc3b4728bc3ffbd43fe937989afc7eda4ae58890f` |

At `cd3d14a`, the macOS/APFS seed rerun passed all 11 phases, including
native in-place compaction. The Docker Linux ENOSPC controller and worker
passed backup, restore and in-place compaction, full-volume reopen and
retry. Linux finished with source revision 213 and backup revision 212.
The test-owned container and volume were absent after execution.

| Regression evidence | SHA-256 |
|---|---|
| macOS seed log | `87f3a7be341a887b7663ec73ebbfd40ac5666f5572e194f7049c157a575137ff` |
| Docker Linux controller | `5c029ee6b0e876d3838993b5251c18c2304bcc6ac0a4bc1fed147957edbd4ef8` |
| Docker Linux worker | `391102346d55d2c2f7ca64576b3ec892f3f853032f1a3418cfdd161b52c493ec` |

## Native macOS near-cap copy profile

Use the same compound fixture and an APFS backing volume. The controller owns
a fixed 256 MiB writable APFS disk image and verifies the image path, physical
disk and mounted volume before writing. It uses an 8 MiB reserve file to make
room for the partial copy. APFS can reject a 4 KiB write with `ENOSPC` while
still reporting available bytes. The test repeats that real write check and
records the filesystem counter before and after each fresh reopen.
[Creating a blank APFS disk image](https://support.apple.com/en-ca/guide/disk-utility/dskutl11888/mac)

```sh
source_id=$(git rev-parse HEAD)
WEEKEND_DB_SOURCE_ID="$source_id" cargo build --offline --locked --release \
  --manifest-path tools/resource-envelope/Cargo.toml --features compound \
  --bin weekend-compound-resource --target-dir target/maintenance-mac-build
cargo xtask macos-maintenance-space \
  --fixture target/maintenance-mac-build/release/weekend-compound-resource \
  --source "$source_id" --output target/macos-maintenance-space
```

Backup and restore retry at new names. In-place compaction retries with its
earlier partial scratch still present and verifies that the canonical file
equals the good backup. Cleanup detaches only the test image and verifies
its absence before removing the backing file. The macOS job archives the
same fixture and phase reports as the Windows gate.

The local arm64 run passed at `df61d559dd0aa2b4e37043b21a66c91f9de6f507`
on macOS 27.0.1 (26A434) and Rust 1.99.1. Backup, restore and in-place
compaction each returned 28 / `DiskFull` / `Write` after writing 8,388,608
bytes. APFS reported 5,849,088 bytes available at the full-volume opens,
and 4 KiB ballast writes returned `ENOSPC` before and after them. Deleting
the ballast restored 129,232,896 reported available bytes per case.

For each error the source and good backup reopened at revision 212 with all
64 retained roots. Each retry produced the exact good backup bytes and
preserved the failed output or scratch. The later source commit reopened at
213 with all retained data. The good backup stayed at 212. The negative
control with ample space produced a working backup and hit the required
assertion. All 29 logged command trees retired, including 26 distinct fixture
processes. The image detached, its file was removed, and an independent
inventory query found no attached test image.

The local proof is `target/macos-maintenance-qualified/arm64`. The independent
check rehashed all 63 listed files and verified the case records. Local test
controls passed 182 xtask cases with one registered ignore. Strict macOS and
Windows x64 cross-target Clippy and actionlint passed.

The shared controller also passed all three NTFS cases in the hosted Windows
rerun for `df61d55`. All 60 listed artifacts rehashed and the preserved backup
stayed at revision 212. Evidence JSON SHA-256 is
`a227642de59fbb1542bd7be43cfeaa4a305a55b031ce308a36675fb39fd9150f`.
[Windows regression archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43727689600/artifacts/14167411697)

| APFS evidence | SHA-256 |
|---|---|
| Evidence JSON | `28d87de11542806a6dc66ab001584dd839d23b678f339a5de3a25f9e61152925` |
| Independent file rehash | `6f3c0b7b66844a343723204196ac634e855f02cf77bac723d202d101c51bb6d6` |
| Fixture executable | `86314887b4238579a06794d1db73f99e12a5634f9ad8cd8e08053054f8ca790c` |
| Controller | `200631a7f5c2d85af45ea2d98eb3b0b9d30ca967c954872cd9cfb45f12f0fced` |
| Advanced source | `61ac286e4c393865e5da765319cf7f83752fd7ba9882c376b0b70cb995f98920` |
| Preserved backup | `cc77c7bbeef10a0d9ff41db0db598f2deb407a8ed3952df3ea54ba37da032288` |

## Injected namespace and sync errors

The `namespace-failures` resource profile runs the production
`Database::compact_file_with_diagnostics` API in a fresh native child. A
private interposer targets the case directory and its original/replacement
inodes. The calibration observes the exchange, both directory syncs, both
replacement syncs and old-file removal in the required order. The shim hooks
`renameatx_np`, `unlinkat` and `fcntl(F_FULLFSYNC)` on macOS, and
`renameat2`, `unlinkat` and `fsync` on Linux. On the selected macOS Rust
toolchain, directory `File::sync_all` also calls `F_FULLFSYNC`. A control
with injection disabled must fail the child's expected-error assertion.

All injected cases expect `OutcomeUnknown` with the named native operation
and errno. The scenario that runs the exchange and then surfaces EIO
exercises an ambiguous result with a changed namespace.

| Case | Injected errno | Native diagnostic | Canonical file after error | Scratch file after error |
|---|---|---|---|---|
| Exchange before the syscall | ENOSPC | Exchange / DiskFull | Original | Compacted |
| First directory sync | EDQUOT | Sync / Quota | Compacted | Original |
| First replacement sync | EIO | Sync / Other | Compacted | Original |
| Old-file removal | ENOSPC | Remove / DiskFull | Compacted | Original |
| Cleanup directory sync | EDQUOT | Sync / Quota | Compacted | Absent |
| Cleanup replacement sync | EIO | Sync / Other | Compacted | Absent |
| Exchange returns EIO after real syscall succeeds | EIO | Exchange / Other | Compacted | Original |

The fixture compares inode and exact canonical and scratch bytes with the
66,154,496-byte original and 63,074,304-byte compacted control. A fresh
process checks current revision 212 and every byte of all 64 retained roots.
The uninjected retry produces the exact compacted image and a second fresh
checker verifies the same complete state. Any scratch name already present
survives retry with identical bytes. Only the private test case retires it
after verification. A Rust controller independently checks the observed
diagnostic, selected revision, state fingerprints and fresh process records
for every case. It also checks that the source and shim hashes stay stable.

```sh
cargo xtask resource namespace-failures --revision 17135d56f722aa9aebb06368ce2abb530df91722 \
  --output target/namespace-failures-mac
cargo xtask resource namespace-failures --revision 17135d56f722aa9aebb06368ce2abb530df91722 \
  --output target/namespace-failures-linux \
  --docker-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
```

The syscall returns are injected. The pathname changes, sync calls preceding
the error, reopening, verification and retry use the real filesystem. This
profile provides native error propagation and retry evidence. Actual
filesystem exhaustion at the named phase and physical persistence on power
loss still require their own evidence.

### Native results

macOS arm64/APFS and Docker Linux arm64/ext4 passed all seven errors and
the disabled-injection control on Rust 1.99.1. Both hosts reported errno
28 for ENOSPC and 5 for EIO. EDQUOT reported 69 on macOS and 122 on Linux.
Each host passed 15 fresh-image checks, including the compacted control.
Each worker verified process retirement, complete current and retained data,
and source stability. Docker also verified its container and volume were
absent after copying the evidence. The full macOS xtask debug suite passed
185 tests with one default ignore. The six resource oracle tests passed in
debug and release. Strict Clippy passed for xtask and the compound fixture.

The engine archive uses `17135d56f722aa9aebb06368ce2abb530df91722`. The
resource campaign overlays the current private fixture and shim onto that
archive, then records their hashes. Evidence is under
`target/namespace-proof/full-mac` and `target/namespace-proof/full-linux`.

| Evidence | macOS SHA-256 | Linux SHA-256 |
|---|---|---|
| Controller | `060d443fbba18144208fdc2732a7fb1240e733577489b92e705f752346e80c92` | `51659d91b6c93401d8a4f025d18c7dbc98ec8d46bfef3d15324707e48b170053` |
| Worker | `6b0869a7fba40964962e3b9a21353fbc21b0499f67d5703f1fd47a383bb829fc` | `aea9c9c9680b02c6935326cf68675991cc1c1a3a0c8c9425d35aaccee212d674` |
| Namespace observation | `82ccc43ae4259c8a5f09d9bd5eff28602a9680e4286715401078d3c906e71ab4` | `8dcd1c3e88adeb4429739c9e434db24b1a435cdc71b25dfe26ba47fc7d3881a5` |
| Shared shim source | `b65034ea3c5d70ad0920a7028ecc2b699c7f6fa4a1550c7f38cb2f5337606230` | `b65034ea3c5d70ad0920a7028ecc2b699c7f6fa4a1550c7f38cb2f5337606230` |

## Kernel permission denial at exchange and cleanup

The extended `namespace-failures` profile adds two real `EACCES` (13)
outcomes to the seven injected cases. At the selected call, the private shim
changes the exact case directory from mode `0700` to `0500`, calls the OS
operation, and passes through its return and errno. The directory stays
searchable throughout. The fixture checks its mode before and after a fresh
database reopen, then restores write permission for the retry. The Docker
worker runs with all capabilities dropped so the directory mode applies to it.
[Linux rename and permission behavior](https://man7.org/linux/man-pages/man2/rename.2.html), [Linux unlink permission behavior](https://man7.org/linux/man-pages/man2/unlink.2.html)

| Kernel operation | Database result / native diagnostic | Fresh reopen while denied | Retry after restoring write permission |
|---|---|---|---|
| Exchange canonical and scratch names | `OutcomeUnknown / Exchange / Permission / 13` | Original 66,154,496-byte canonical image, compacted scratch preserved | Canonical becomes byte-identical to the 63,074,304-byte compacted control. Existing scratch survives unchanged |
| Remove the old image after exchange and both initial syncs | `OutcomeUnknown / Remove / Permission / 13` | Compacted 63,074,304-byte canonical image, original bytes preserved at the scratch name | Canonical again matches the compacted control. Existing scratch survives unchanged |

Both outcomes reopen current revision 212 and verify all data for all 64
retained roots before and after retry. The private fixture retires its
scratch files after checking them. The disabled-injection control and all
seven injected cases still pass, for 19 fresh-process checks per platform
including the compacted control.

The runs used exact source `73bc3b6b39d31ea5c0caf8ad88c3c4ac0a591356`,
macOS arm64/APFS with Rust 1.99.1, and Docker Linux arm64/ext4 with Rust
1.99.0. The independent audit matched both source archives against Git,
rehashed 32 worker payloads per platform, and matched each diagnostic and
fresh reopen to the captured observation and frozen data fingerprints.
The owned Docker container and volume were absent after evidence copyback.
The macOS xtask unit suite passed 185 tests with one default ignore.
Strict Clippy passed for the macOS controller and compound fixture.
Evidence is under `target/namespace-permission-validation/`.

```sh
cargo xtask resource namespace-failures --revision 73bc3b6 --output target/namespace-permission-macos
cargo xtask resource namespace-failures --revision 73bc3b6 --output target/namespace-permission-linux \
  --docker-image sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c
```

Run the native test as a regular user on a filesystem that supports the
adapter's exchange operation. Actual `ENOSPC` during exchange or unlink,
delayed writeback errors, and physical persistence after power loss
remain separate qualification cases.

| Evidence | macOS SHA-256 | Linux SHA-256 |
|---|---|---|
| Controller | `987829b4a88e7cfb5782cdf53c8d1e7a7bc6186b3f7f86e0165a12c43caeaa6f` | `cee0fc78bf5a5b71e6eaf6609a880355e3b1b76079d249090d531f23cbfb5b12` |
| Worker | `f17076659cd89ca3c9b0fb443d4df75614106458b1f1d1e5253fa81eca28b307` | `36ee6ab2fe319c0cdeaab22d88ddaf3df04370c7ddeb7e0e47ed5f335d72221d` |
| Namespace observation | `bf04add8a524f083dae2c38652a0f44ff927c2529feea97526312c2d97f5b2fc` | `fe74c738d1e2b3fcde727e6b335245cb42bc6f9ab6b149af59108e1bbf9fabcd` |

## Kernel writeback EIO during near-cap maintenance

The `quota --maintenance --writeback` profile runs the same near-cap
workload as the space-exhaustion profiles on a private 512 MiB ext4 volume.
The controller reuses the [commit writeback profile](native-writeback.md)
to verify the loop, device-mapper table, mount and process identities.

The fixture runs `backup_file`, `restore_file` and in-place
`compact_file` as an unprivileged UID. A small preload library pauses
the operation after creating its exclusive destination or compaction
scratch file. It preallocates one 4 KiB block at offset zero and leaves the
file's logical size at zero. The controller checks the inode and its FIEMAP
extent, then routes just that block's eight sectors to the kernel
`error` target and releases the process. The shim gates creation. The
kernel supplies the error at the first destination `fsync`.
[Linux preallocation semantics](https://man7.org/linux/man-pages/man2/fallocate.2.html)

| Operation | Native result | Failed output | Successful retry |
|---|---|---|---|
| Backup | `Io / Sync / Other / EIO (5)` | Full-length invalid backup destination | New destination, exact verified backup bytes |
| Restore | `Io / Sync / Other / EIO (5)` | Full-length invalid restore destination | New destination, exact verified backup bytes |
| In-place compaction | `Io / Sync / Other / EIO (5)` | Full-length invalid scratch file, original canonical file | Canonical image replaced with exact verified backup bytes |

After each fault, the controller restores the linear mapping and remounts
ext4. The 63,074,304-byte failed outputs have zero-filled header, root-slot
and witness regions. Their payload bytes match the good compacted backup.
Read-only inspection returns `UnsupportedFormat` at the header. The source
and good backup reopen at revision 212 with every current value and all
64 retained roots intact.
Their failure-time hashes match the pre-fault files. The retry preserves
the failed output, and the controller remounts again to verify the exact
retry bytes. The fixture then removes its own failed outputs. A final
commit and remount reopen the source at revision 213 with all retained
roots and the preserved backup at revision 212.

To reproduce on privileged Linux with ext4 and device mapper, install the
tools listed in the [commit writeback instructions](native-writeback.md#reproduction)
and a C compiler. Use an exact source checkout.

```sh
revision=$(git rev-parse HEAD)
cargo build --locked --offline --release -p xtask --target-dir target/xtask
WEEKEND_DB_SOURCE_ID="$revision" cargo build --locked --offline --release \
  --manifest-path tools/resource-envelope/Cargo.toml --features compound \
  --bin weekend-compound-resource --target-dir target/maintenance-quota-build
sudo "$PWD/target/xtask/release/xtask" quota --maintenance --writeback \
  --fixture "$PWD/target/maintenance-quota-build/release/weekend-compound-resource" \
  --source "$revision" --output "$PWD/target/maintenance-writeback"
```

Docker Linux arm64/ext4 passed at `e21d081f5dabe4c2d40b7b8450003867dca40399`
using Rust 1.99.0. Hosted Ubuntu 24.04 x64/ext4 passed at the same PR head
with Rust 1.99.1 and tested merge `7a7dd2a399befb5cef578ea1db2a90b41b89b6cb`.
Both ran the three commit writeback cases. The hosted job also passed the
append quota, maintenance block and inode quota, and global inode
exhaustion profiles. [Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43755114238/job/132937387945), [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43755114238/artifacts/14178041975)

On each platform the audit matched the exact Git archive with 694 source
files, checked all 532 payload hashes, 175 bounded command records,
30 maintenance phase records, and the exact compiled preload library.
It independently compared the failed targets and recomputed current and
retained data digests. Admin checks confirmed all five image outcomes and
preserved their hashes. The owned mapper, mount and loop were removed.
Strict Clippy passed for the Linux controller and Linux and Windows-target
compound fixtures. All 28 Linux quota tests and 18 macOS compaction,
inspection and retention tests passed. Evidence is under
`target/maintenance-writeback-validation/`.

| Evidence | Docker Linux SHA-256 | Hosted Ubuntu SHA-256 |
|---|---|---|
| Controller | `9101a0673bf51994d74e1936ae0cdbf6ab95494acc0eb6cb4f411d834deec6f6` | `a2e0b83c3730e1af50330e22c53b7ec1ff3503463a7ab87729992945be54e56d` |
| Fixture | `eb547aebb3cdd8fd56d61e0e9a7047d26f663286d5b8b6a67487f79a85a6eefd` | `89fb0b2f4450d2cf46557f7b10614625fce74d34385ffb8fd4eca9efe30b1b1e` |
| Independent audit | `7f4b98d86d04599570fc006422fca7271a8b1bb1e0490eb1783df90b4991ea14` | `e84b03d144cfd47b4b809e5dfebb3381759490ab3b25e43cfc9cc81476f8e3a7` |

The hosted ZIP SHA-256 is
`072d21b3b23a6e0251dbdd113b4fa94507205c2a3dd0f188f29d332631a06a5e`.

This result covers the first copy-data flush. The next profile covers the
root-slot and witness flushes in the same destination image.

## Kernel EIO at maintenance root-slot and witness flushes

Add `--copy-barrier slot` or `--copy-barrier witness` to the preceding
reproduction command and choose a fresh output directory for each run.
Each barrier runs backup, restore and in-place compaction, with a fresh
near-cap source and verified backup.

The same shim records the newly created destination inode, then pauses just
before the populated slot or witness write to that inode. The payload is
already synchronized. The controller verifies the initialized FIEMAP extent
and arms the kernel block error before allowing the write. A remount after
restoring linear I/O makes persisted bytes available for inspection.

| Kernel fault | Backup / restore result | In-place compaction result | Failed destination after remount |
|---|---|---|---|
| Root-slot block at byte 4,096 | `OutcomeUnknown` | `Io` | `Corrupt` at root selection. The header and payload match the verified backup. The slot and its witness are blank |
| Witness block at byte 12,288 | `OutcomeUnknown` | `Io` | Revision 212, unwitnessed, all 64 retained roots verified. Only the witness block differs from the verified backup |

Each native diagnostic reports `Sync / Other / EIO (5)`. The failures
precede in-place exchange, so the canonical path retains the original
source bytes. The source and good backup reopen at revision 212 with every
current and retained value. The retry preserves the failed output and matches the
verified backup byte for byte after remount. The final source reopens at
revision 213, and the good backup reopens at 212, both with the same retained
history.

Independent admin checks on the Docker images rejected all three slot
outputs and verified all three unwitnessed outputs without changing their
bytes. Explicit `weekend-admin backup` and `restore` from one unwitnessed
output each created a fresh durable image byte-identical to the verified
backup. The original unwitnessed output kept its hash.

Docker Linux arm64/ext4 passed with Rust 1.99.0 at
`c902334fae9ef6ac546f7a3710eafe89209294dd`. Hosted Ubuntu 24.04 x64/ext4
passed at the same PR head with Rust 1.99.1 and tested merge
`3faf6e6af47306310ebb82a06e789ccd1065270c`. The hosted job also passed
the first copy-data and all three commit writeback barriers, append quota,
maintenance block and inode quotas, and global inode exhaustion.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43756445904/job/132941481260), [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43756445904/artifacts/14178601948)

Each audit checked 694 source files, 532 evidence payloads,
175 bounded command records and 30 maintenance
phase records. It compared the entire failed images against the good backup
with only the expected metadata blocks replaced by zeros. It also rehashed
the exact fixture and preload library. The hosted admin check verified
the final revisions, all retained roots, and both failed-output dispositions
without changing the copied images. Every mapper, mount and loop was removed.
The 29 Linux quota tests, strict Linux Clippy and the Windows x64 fixture
cross-target Clippy check passed. Local evidence is under
`target/maintenance-publication-writeback/`.

| Evidence | Root slot SHA-256 | Witness SHA-256 |
|---|---|---|
| Docker controller | `8dff89cfdd7783191178fa1967e601aa5d449e0b9f6562b85883fc5d9b6b9aeb` | `0501f45b21943a731edb3a056514bae5baa4b0131ac409a2b88a38c29c19dcd9` |
| Docker audit | `5cf8e314e7e6d0e7e7bafec345dd472cab2087fac43700583fcbe641e32b12b3` | `cab1776f777bc7fd1174b019ff60fa6bb407b3699e675f02fd675293f664ada5` |
| Hosted controller | `1cc1847f073934aee8f8bb00c0cff7dd78cbfc4c3e15168549648af38053913b` | `d7b97cbc4a6613a01c2ec9347606df0a14ac92dfbd1490f6586e36e29beda58b` |
| Hosted audit | `c62a15a2a3e8b0eba1d5d0df9601856174fabb0aebe6d28f409eee872437da9a` | `189d879238fe5c59597c1968bf0491a8b4d1f3b6ade97944c2f1260794d061c0` |

The hosted ZIP SHA-256 is
`fc3c4a4da9808f2903a1b098ead3d8018190164108c687929f7ec8b7615862df`.

The next profile exercises the subsequent replacement file metadata sync.

## Kernel EIO at compaction file metadata sync

Add `--copy-barrier file-metadata` to the [writeback reproduction](#kernel-writeback-eio-during-near-cap-maintenance)
and choose a fresh output directory. This profile runs in-place compaction
on an owned ext4 image with `debugfs` from e2fsprogs. The controller sets
and synchronizes source mode `0640`. It validates the internal journal's
single contiguous extent and leaves the journal superblock addressable.
The target's data, root slot and witness are synchronized before the
shim pauses at the requested scratch-file mode change from `0600` to
`0640`. The controller routes the owned journal transaction sectors to
the kernel device-mapper `error` target. The shim confirms that the actual
mode change succeeded. The following `fsync` returns kernel `EIO`.
[Ext4 journal format](https://www.kernel.org/doc/html/latest/filesystems/ext4/journal.html)

The compaction call returns `Io / Sync / Other / EIO (5)` before exchange.
The canonical path and known backup match their pre-fault bytes. After
restoring linear I/O and remounting, the canonical file retains mode
`0640` and the original scratch inode has mode `0600`. Fresh checks verify
revision 212 and all 64 retained roots in the canonical file, backup and
scratch. The scratch image is witnessed and byte-identical to the verified
backup. A retry publishes another byte-identical copy with mode `0640`,
preserves the failed scratch, and reopens after remount. The final commit
and remount leave the canonical file at revision 213 and backup at 212.

Docker Linux arm64/ext4 on Rust 1.99.0 passed at
`47f674c37ce6aa6663f79c83f18330b9d1d80f8d`. The same fixture also passed
the existing slot and witness cases. All 31 Linux quota unit tests and 23
macOS admin, compaction, inspection and retention tests passed. Strict
Clippy passed for the Linux controller and the compound fixture on Linux,
macOS and the Windows x64 cross-target.

Hosted Ubuntu x64/ext4 passed with Rust 1.99.1 at tested merge
`0ee2da664b0da1c76914181a0872cd5a48dda31f`. The independent audit on each
platform checked 694 source hashes, 299 payload hashes, cleanup for 98
command records, the exact fixture and the exact gate library. Read-only
admin verified all three archived images and kept their hashes. The owned
mapper, mount and loop were removed.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43761010174/job/132955605814), [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43761010174/artifacts/14179923400)

| Evidence under `target/maintenance-journal-validation/` | Docker arm64 SHA-256 | Hosted x64 SHA-256 |
|---|---|---|
| Controller | `440a0905c2ba6d8373a1428130bf7946cbd9cbd8d3beff22dca2e1716d775bfa` | `0f23155b9191fd7b2422e4045f67335e13fba33516fbec949e99f0e79089e98d` |
| Exact fixture | `6514fecc22a55bacc430d984cb3f1f70d3228a1cf8a4552332c8c20f29902f0a` | `47c65bf14bad9d41b6fa343c1c3266bcb32170c37dea2a2c42c6e8f35dc03660` |
| Exact gate library | `375010e9f05370fd91f1fb1a3e78dec8a6ddd99265471f85f64595a1bf1aab58` | `54ea2d68e6d2d196ea14fcd6d47c3149f7c751e36b9ac2d3dc9d5885c3962eb3` |
| Independent audit | `d7b7c4f29385a3cd548fb50f2b993c7bbd8ebb300ea69eaaf82ac31ae243a052` | `0a1e857207a54f54f477cfe1f5c583c18c24d2c650482983120b36fe58db31e2` |

The hosted ZIP SHA-256 is
`2c7a437acfae4fa1df05efee0f744de6dc75f20f16e8299c7d1e8ee7db971758`.

The next profile exercises the two directory syncs after exchange and cleanup.

## Kernel EIO at compaction directory syncs

Add `--copy-barrier exchange-directory` or `--copy-barrier cleanup-directory`
to the [writeback reproduction](#kernel-writeback-eio-during-near-cap-maintenance).
Run each barrier on a freshly formatted private ext4 image. The controller
uses the same internal journal extent as the file metadata profile. The
shim checks both file identities and pauses immediately before the real
`renameat2(RENAME_EXCHANGE)` or `unlinkat`. The selected syscall succeeds
and the shim verifies the live directory entries. The following directory
`fsync` returns kernel `EIO (5)`. The engine reports `OutcomeUnknown`
with the native operation `Sync` and category `Other`.

| Failed directory sync | Live canonical file and scratch | Canonical file and scratch after linear I/O is restored and ext4 remounts |
|---|---|---|
| After successful exchange | Compacted image at the canonical path, original image under the scratch name | Original image canonical, compacted image at the scratch name |
| After successful unlink | Compacted image canonical, scratch name absent | Compacted image canonical, original image restored at the scratch name |

The controller checks the inode mapping at the fault gate and verifies
the exact inodes and file hashes after remount. Each canonical file,
recovered scratch and verified backup reopens witnessed at revision 212
with all 64 retained roots.
The canonical path matches the appropriate image byte for byte. A retry
publishes the verified compacted bytes, keeps mode `0640`, and preserves
the old scratch. The next commit and remount reopen the canonical file
at revision 213 and the backup at 212. Those checks also verify all
retained roots. These outcomes describe the two ext4 journal faults tested.

Docker Linux arm64/ext4 with Rust 1.99.0 passed both barriers and the file
metadata regression at `bcbae975a8b715ff60adb6b67cdf528e5460df04`.
Hosted Ubuntu x64/ext4 with Rust 1.99.1 passed the kernel quota and writeback
matrix at tested merge `43619a316bfa9de266a6f3ebc53387f4864a7d71`.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43762341176/job/132959697452), [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43762341176/artifacts/14181152343)

The 32 Linux quota unit tests passed. Strict Clippy passed for the Linux
controller and compound fixture on Linux and macOS, plus the Windows x64
fixture cross-target. The C gate compiled with warnings treated as errors.

The independent audit for each phase matched the exact Git archive and
rehashed 694 source files and 299 evidence payloads, including the compiled
gate. It checked cleanup for all 98 command records and all 15 phase
records. A separate admin process verified every retained revision in the
three archived images and kept the original hashes. The controller
confirmed removal of the owned mapper, loop and mount.

The Docker fixture SHA-256 is
`b4c4606db9fb679449f716b65053262c71e606f0f070b27d78730274965c5ab8`.
The hosted fixture SHA-256 is
`bb50bbaf6f651c2da62a808139e1a516d42436acc250c1e85201c55592ef2f01`.
The hosted ZIP SHA-256 is
`32854c92b711cd0a7c4ea8014284e37678a64c9b28a798388d37d3f9da1c0dd0`.

| Evidence under `target/maintenance-journal-validation/` | Exchange directory SHA-256 | Cleanup directory SHA-256 |
|---|---|---|
| Docker controller | `e0db8ef5eb37ca9c4c16d3baab7d9ff98363272394ac5c15a07985b44f25aaa4` | `0817f201e8169219a8a02ec04c2772591a610aa705a1deb371607789875e0e20` |
| Docker audit | `55295834a6c78ddeb5c23eb4c18a2b173631755ee4fa60cd00803330ebc02ac6` | `a03ffea8dfda1d3a6c4795b05fe98167314244eb4120ba60a6080a2b1b7da001` |
| Hosted controller | `57648c20d38e2a8caa7c8f29261951f64f422797fd8a1c8d2fe8d579205aa1fc` | `0e5efa3216e7731867ce2dd05bb95f309dc6e6b33139301f5b87a387d4fad6b7` |
| Hosted audit | `f7dcdbe14db5a3c3cf6b982f9d4926e48ff0173e97c9167fa8322fa2a32ffc21` | `3e8b57a78c463a3a246cd7290fc6cdb0d9bf4e27b877e526b7710a7f7056833e` |

The next profile exercises namespace publication after the replacement is
durable and the ordinary user runs out of blocks and inodes.

## Ext4 compaction with no remaining user blocks or inodes

Build the controller and fixture as in the [writeback reproduction](#kernel-writeback-eio-during-near-cap-maintenance).
Use an owned private Linux ext4 volume through the controller. Choose a
fresh output directory.

```sh
sudo "$PWD/target/xtask/release/xtask" quota --maintenance --namespace-enospc \
  --fixture "$PWD/target/maintenance-quota-build/release/weekend-compound-resource" \
  --source "$(git rev-parse HEAD)" --output "$PWD/target/maintenance-namespace-enospc"
```

The unprivileged compaction process pauses just before its real exchange.
By then it has durably written and verified the replacement, whose bytes
match the verified backup. The controller checks exact file identities and
source and scratch hashes. A separate process under the same UID exhausts
the global inode inventory and writes an existing ballast file until the
kernel returns `ENOSPC`. An exclusive create also returns `ENOSPC`. A
small remainder can fit in a separate existing inode when extending the
large file requires more space. The fixture consumes that remainder before
resuming. The interposer verifies zero available blocks and inodes at
both the real exchange and the unlink. The filesystem can still report
reserved blocks in its global free count.

The exchange, unlink and all publication syncs succeed. The canonical
file is the exact 63,074,304-byte compacted image and keeps mode `0640`.
The old scratch name is absent. The result survives remount. Fresh
processes verify revision 212 and all 64 retained roots in the canonical
file and backup. A subsequent commit and remount reopen revision 213 on
the source with the same retained roots. The backup remains at revision
212. An earlier full-inode profile separately exercises failure during
scratch creation and retry after freeing an inode.
[Creation-time ENOSPC](#recorded-native-global-inode-exhaustion-result)

Docker Linux arm64/ext4 with Rust 1.99.0 passed at
`3acc7f8cb279542388e494d12c4acf92bb06321d`. Hosted Ubuntu x64/ext4 with
Rust 1.99.1 passed at tested merge `c7fc34e7b7105314eddd6996363e8d15c3874ebb`.
The fill on each platform wrote 289,308,672 bytes,
including a final 4,096 bytes in the separate inode, while 37,576,704
bytes remained in the global free count. The exact inode-exhaustion retry
profile also passed. All 33 Linux quota unit tests passed, along with strict
Clippy for the Linux controller and fixture and macOS and Windows x64
cross-target fixture checks. The C gate compiled with warnings as errors.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43763861937/job/132964470395), [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43763861937/artifacts/14182160363)

The independent audit on each platform checked 694 source files, 154 evidence payloads,
cleanup for 50 command records and all eight phase records. Read-only
admin verified current and every retained revision in both archived images
and kept their hashes. The controller verified that the owned ext4 mount
and loop device retired.

| Evidence under `target/namespace-enospc-validation/` | Docker arm64 SHA-256 | Hosted x64 SHA-256 |
|---|---|---|
| Controller | `bec251f78daa205dde0ab882f9fce6d41ab27c368a1222511d3c97d349620191` | `5f60026e6e49d6d7cf6f752333c3033d117270abdf8f3da410ec9bdf7d005e4a` |
| Exact fixture | `02cd69bbd16265e4061956b2ff87fc4e98e1f9467535fad0b906d8d1807343f7` | `6ffbfa9f5ad313cb916dfcc3537a7ce9ecca9498961740179da5f488145f6fff` |
| Exact gate library | `e8688b3e67a8ef982abb64d801b7f12f202a06112e420da9aa8db4fe00233787` | `803e3d10b5ee35155490d086882a0a673e5b8183637e5f2ae41e0972f32dc7ca` |
| Independent audit | `f3c2a4932a2bd786223d573c555d3acb3649fbabdcf526f4434f1ef53eeff190` | `665a18cb5ed33a83a2ca35b758db697594c4eaa27a5185859acfd291e46ec360` |

The hosted ZIP SHA-256 is
`332b5136eec623683b47a7d8b15522719a288fcb9edecbb86daef334db0a0176`.

The next profile exercises a native namespace refusal on APFS.

## APFS ENOSPC at compaction exchange

Run the exchange profile on a Mac with room for a private 256 MiB APFS disk
image and the evidence files. The fixture and controller use the same source
checkout. Choose a fresh output directory.

```sh
export WEEKEND_DB_SOURCE_ID=$(git rev-parse HEAD)
cargo build --offline --locked --release \
  --manifest-path tools/resource-envelope/Cargo.toml --features compound \
  --bin weekend-compound-resource --target-dir target/maintenance-mac-build
cargo xtask macos-maintenance-space --namespace-enospc \
  --fixture target/maintenance-mac-build/release/weekend-compound-resource \
  --source "$WEEKEND_DB_SOURCE_ID" --output target/macos-namespace-enospc
```

The fixture pauses at the compaction exchange hook after synchronizing the
replacement data, witness and copied file metadata. The controller verifies
that the scratch inode holds the exact bytes of the trusted backup and that
the canonical inode still holds the original image. It fills an owned ballast
file until APFS returns `ENOSPC` and releases the child. The existing exchange
call gets kernel `ENOSPC (28)`, and compaction reports
`OutcomeUnknown / Exchange / DiskFull`. The hook only coordinates timing.

The canonical inode keeps the original bytes and the scratch inode keeps the
verified compacted bytes. With the ballast still in place, separate processes
reopen both and the backup at revision 212 with all 64 retained roots. After
the controller removes the ballast, a fresh compaction publishes the exact
backup bytes in a new canonical inode with mode `0640`. Detach and reattach
verify the canonical file and the single preserved scratch under the original
scratch name. A fresh commit reaches revision 213 with all retained roots.
The backup stays at 212.

The local macOS arm64/APFS run passed at
`9fe5035d2493c95f050a3a84b231fb49dbb3d00f` with Rust 1.99.1. The controller
wrote 68,706,304 ballast bytes. APFS reported 5,849,088 available bytes after
the fill and 74,555,392 bytes after ballast removal. The original, scratch,
and retry inodes were 19, 21, and 23. The same fixture passed the existing
three copy-exhaustion cases. The macOS xtask unit suite passed 188 tests with
one registered ignore. Strict Clippy passed for xtask on macOS and for the
compound fixture on macOS and the Windows x64 cross-target. The hosted macOS
run was queued when this evidence was recorded.
[Hosted run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43765323051/job/132969001635)

The independent audit matched 305 source hashes against the Git archive,
rehashed all 44 evidence payloads, bound the exact fixture and runner, and
checked process retirement for 17 outer commands and the gated child.
It verified all 11 phase records. Read-only admin checked each archived
image and all 64 retained revisions without changing their hashes. The
private APFS image was detached and removed.

| Evidence under `target/apfs-namespace-validation/` | SHA-256 |
|---|---|
| Exchange controller | `13ddcafbc875bc52de129c0a759a80de191b56f8e20087f75d812aba4333d4b2` |
| Gated child controller | `20a1f2579534798d634f7b23e3b1ccb1adf585d05d715dcc632ff1d4a7c49dfe` |
| Exact fixture | `7d519e3bb05673a8e03bfc0c73f12b6a4836752291db9ecd69ec0eca62f087cd` |
| Existing copy-exhaustion regression | `b527aea73ed1d83a279dcfa8e321f8658740c1bb5d720f952e226976a476c0b4` |
| Independent audit | `68a4150ef005fefab68d00d72a4830717f6594a1deb9ae170f28a22f9e42d878` |

The next profile fills the volume after exchange, before unlinking the old image.

## APFS compaction cleanup after allocation reaches ENOSPC

Follow the [APFS exchange reproduction](#apfs-enospc-at-compaction-exchange)
with `--cleanup-enospc` and choose a fresh output directory. The fixture
pauses immediately before removing the old image, after the exchange,
directory sync and replacement-file sync. The controller confirms that the
canonical inode contains the exact compacted bytes and that the original
inode and bytes are under the scratch name. It fills the private volume
until APFS returns `ENOSPC` on the ballast allocation, then releases the
child. Unlink and both cleanup syncs complete successfully.

After detach and reattach, the canonical inode still matches the verified
backup byte for byte, retains mode `0640`, and the old scratch name is
absent. The controller verifies both canonical and backup images at
revision 212 with all 64 retained roots. Compaction reclaims enough space
for another commit with the ballast still in place. The source then
reopens at revision 213 with all 64 retained roots and the backup at 212.

The local macOS arm64/APFS run at
`e9cd705283d5bc29c99e655fd6cb09ebec4d4b62` passed with Rust 1.99.1. The fill
wrote 68,706,304 bytes before the 4 KiB allocation failed. APFS reported
5,849,088 available bytes before unlink and 71,970,816 after remount with
the same ballast. The old inode was 19 and the canonical inode was 21.
The exchange refusal profile passed again with the same fixture. The
macOS xtask suite passed 189 tests with one registered ignore, and all
22 compaction, inspection and retention tests passed. Strict Clippy
passed for xtask on macOS and the compound fixture on macOS and the
Windows x64 cross-target. Hosted macOS was queued when this was recorded.
[Hosted run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43766087115/job/132971377005)

The independent audit matched 305 source hashes to the Git archive,
rehashed 35 evidence payloads, bound the fixture and runner binaries and
checked retirement of 13 outer commands and the gated child. It verified
all eight phase records. Read-only admin independently checked the archived
original, backup and advanced source, including all 64 retained revisions,
and preserved their hashes. The private disk image was detached and removed.

| Evidence under `target/apfs-cleanup-validation/` | SHA-256 |
|---|---|
| Cleanup controller | `dfb7be5908b428a8eb75ec690be44c37b86842654b17d6510a101af0db0cc239` |
| Gated child controller | `0077378cacd8cc88e0899a269866b1ebb8c8e6566927f6814491f9d1c8e5e3a3` |
| Exact fixture | `bb58a934e7b53f63cf3cd603a76aa5b2213c04b59f0c2a6d96c960b7d41c378a` |
| Exchange refusal regression | `08bb55a8de3bf25ba4e78da530107a6a84017bbd85de23a0f5fe98499c94eb1a` |
| Independent audit | `32f57a8f20de577611909c58e571aa9c33abbd71bd2816d272d283c24297e484` |

Native device failures at the later file syncs and physical power
interruption remain open.
