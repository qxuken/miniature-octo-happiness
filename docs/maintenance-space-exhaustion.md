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
