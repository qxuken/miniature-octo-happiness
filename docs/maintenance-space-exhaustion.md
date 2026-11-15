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
inode quotas, physical power interruption, full ext4 devices and native
Mac/Windows exhaustion remain separate gates. Earlier process-kill and simulated
namespace/recovery matrices retain their separate scope. All 19 Cargo
manifests retain `publish=false`.

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

Native execution is pending. The controller archives the executable, phase
records, image hashes and owned-volume cleanup evidence.

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
