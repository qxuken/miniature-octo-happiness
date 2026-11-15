# Native quota exhaustion and recovery

The `linux-quota` job uses an isolated ext4 loop filesystem on an Ubuntu 24.04
host. It exercises the real Linux file adapter, not an injected ByteStore error.
The existing [tmpfs ENOSPC/permission drill](native-fault-drills.md) retains its
own scope and guards.

## What the gate requires

The root controller creates a fresh 64 MiB image and binds its inode, loop
association and exact mountpoint before configuring a 4 MiB user quota. It
never configures quotas on the host filesystem or detaches unrelated devices.
The formatter explicitly selects ext4 even when `mkfs.ext4` resolves to
`mke2fs`. A separate numeric UID runs the probe and Rust fixture with empty
supplementary groups, zero capabilities and `NoNewPrivs=1`.

The calibration must observe actual Linux errno 122 (`EDQUOT`), with more than
16 MiB of both filesystem free and available space. Errno 28 (`ENOSPC`) cannot
substitute. Removing only the owned ballast must allow the same UID to write
and fsync again. Ballast writes stop at an independent 8 MiB ceiling.

The source-bound, core-only Rust fixture then executes in fresh processes:

1. Create collection 1, commit two records at revision 2 and retain that root.
   Retention publication makes the current revision 3, with retained ID 1 at 2.
2. Exhaust the user's quota again. A transaction replacing the integer and
   writing a distinct 64 KiB payload must fail with `Io`, native `Quota`, raw
   errno 122 and operation `Write`.
3. Require an aborted/terminal transaction (`Closed` on reuse), recovery-required
   database state and rejection of a fresh write with `RecoveryRequired`.
4. Reopen in another process while the quota is still exhausted. Verify current
   revision 3 and retained revision 2, including complete record sets and every
   payload byte.
5. Remove owned ballast, retry once to revision 4, then reopen independently.
   Verify the complete new current data and unchanged retained revision 2.

Both the fixture and independent Python oracle check full rows. The parent
rejects wrong types, missing/extra fields, stale revisions, damaged payloads,
wrong source/features and substituted error categories. Fixture modes emit
their actual process IDs; phases must use distinct processes.

Commands have bounded runtime and output. Proof stays in a mode-0700 directory;
guest-accessible staging is separate. Cleanup attempts unmount and loop detach
independently, checks actual absence, and removes scratch only after both are
absent. A `findmnt` query error cannot prove absence. Cleanup uncertainty fails
qualification and retains its diagnostic evidence.

The Rust controller records the exact source commit and file hashes, fixture and
controller executable hashes, resolved command hashes, kernel, quota policy,
command results and full data oracles. The guest calibrator is a byte-verified
copy of the controller binary and checks its own UID, GID, capability sets and
no-new-privileges flag after `setpriv`. CI records the installed package versions.

## Running it

Use a disposable, quota-capable Linux host with `quota`, `e2fsprogs` and
`util-linux`. Build without engine feature flags:

```sh
source_id="$(git rev-parse HEAD)"
cargo +1.99.1 fetch --locked
cargo +1.99.1 build --offline --locked -p xtask --release --target-dir target/xtask
WEEKEND_DB_SOURCE_ID="$source_id" cargo +1.99.1 build --offline --locked --release \
  --example native_quota --target-dir target/quota-build
sudo "$PWD/target/xtask/release/xtask" quota \
  --fixture "$PWD/target/quota-build/release/examples/native_quota" \
  --source "$source_id" \
  --output "$PWD/target/native-quota"
```

The output must be new. Source files must match the requested commit. Default
execution fails if quota support or cleanup cannot be established. The optional
`--probe-only` mode may record unsupported capabilities, but never sets engine
qualification to verified.

## Verification status

Local controller controls pass normally and under Python `-O`. The full helper
entry point now runs twenty suites: forty workers, 292 passes and four registered
skips (296 reported tests). Twelve quota controls include real child timeout and
output limits, strict data/error oracles, wrong mount identity, cleanup failures,
ambiguous absence and formatter selection.

A frozen Linux aarch64/ext4 Docker run verifies seed, fresh reopen at 3, retry,
fresh reopen at 4 and complete retained bytes. A separate no-quota negative
control rejects the required-failure mode, then independently reopens the real
successful revision 4. Its source/fixture overlay hashes match the current files,
and its owned container and volume are absent. Proof:
`target/quota-development-844c429-v2/evidence.json`. Formatting, workflow lint and
targeted Clippy checks pass. These checks do not establish actual EDQUOT.

The [hosted quota job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43484032226/job/132087623169)
passes for head `5e665f1b0e32b65ec9f8aec16ced089a34353165`. Its actual checkout
is PR merge `22fa29e0426dfad990ab302d542f8d4bcb63ac3b`; all 636 source hashes
match the pushed head's Git archive. The fixture uses Rust 1.99.1, core features
zero, Ubuntu 24.04 x86_64 and kernel `7.0.0-1017-azure`. Recorded package versions
are e2fsprogs `1.47.0-2.4~exp1ubuntu4.1`, quota `4.06-1build6` and util-linux
`2.39.3-9ubuntu6.6`.

Both calibration and engine ballast observe errno 122 after 4,128,768 bytes,
below the independent 8 MiB ceiling. Engine exhaustion leaves 49,758,208 bytes
available globally. The same UID's 4 KiB fsynced control succeeds after ballast
removal. Five distinct fixture processes report these complete-state outcomes:

| Phase | Current revision | Retained revision | Commit result | Requires recovery |
|---|---:|---:|---|---|
| Seed | 3 | 2 | Success | No |
| Quota failure | 3 | 2 | `Io`, `Quota`, OS 122, `Write` | Yes |
| Fresh reopen while full | 3 | 2 | Success | No |
| Retry after freeing ballast | 4 | 2 | Success | No |
| Fresh reopen after retry | 4 | 2 | Success | No |

The failing transaction reports `Closed` on reuse; a fresh staged transaction is
blocked with `RecoveryRequired`. Complete current/retained rows match the
independent oracle in every phase. Cleanup has no errors and independently
proves both mount and loop associations absent.

The downloaded artifact's SHA-256 matches GitHub's API digest
`7f387ff3d02bb26489a323a57f5a2a19ddb70c7e1ae7fe1c79e2290bebfb7a6b`.
Root review rechecks all 56 evidence payload hashes, the uploaded fixture binary,
source/tool bindings, privileges, command outputs, full rows and cleanup.
Local proof: `target/native-quota-5e665f1/index.json`, SHA-256
`2bec1ac25e442b7e379178555a13a42a0c221614e7032967e642958edd7ca253`.

The earlier Linuxkit quota provisioning limitation remains historical evidence;
synthetic quota errors and ENOSPC retain their separate scopes. This closes the
named native append quota/recovery gate, not physical power loss, quota failures
during maintenance, other filesystems or other operating systems. Engine
publication barriers and separate follower durable acknowledgments are unchanged.

## Near-cap maintenance extension

The subsequent [maintenance qualification](maintenance-space-exhaustion.md)
closes the separate copy-phase backup/restore/compaction EDQUOT gate at
`74fd4e8`, with a 256 MiB user quota on its own 512 MiB ext4 image. It verifies
all 64 retained roots and current 212 while full, partial-output rejection,
retry after freeing ballast and a later acknowledged 213 reopening. Actual
EDQUOT occurs with ample global space. The existing append profile above is
unchanged and passes again in the same hosted job. Other maintenance error
phases, filesystems and physical power loss retain their separate obligations.
## Native Windows NTFS owner quota

The Windows controller provisions a fixed private 64 MiB NTFS VHD. It
changes ownership of its database and ballast to the built-in Local Service
SID, verifies both owner SIDs, and sets a 1 MiB hard quota for that SID on
the owned volume. Windows charges NTFS quota to the file owner. The controller
and fixture run under the elevated test runner account.
[Windows NTFS disk quota limits](https://learn.microsoft.com/en-us/windows/win32/fileio/disk-quota-limits)
· [fsutil quota](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/fsutil-quota)

Run from an elevated native x64 Windows session:

```powershell
$env:WEEKEND_DB_SOURCE_ID = (git rev-parse HEAD)
cargo build --offline --locked --release -p weekend-db --example native_quota --target-dir target/windows-quota-build
cargo xtask windows-quota --fixture target/windows-quota-build/release/examples/native_quota.exe --source $env:WEEKEND_DB_SOURCE_ID --output target/windows-quota
```

The ballast probe must fail a 4 KiB write within the independent 2 MiB
write bound while the volume has more than 32 MiB globally free. The
controller passes the calibrated Win32 code to the independent append
oracle. It requires the same native code from the database append and the
matching diagnostic category. Windows code 112 reports `DiskFull`, and code
1295 reports `Quota`. The fixture verifies terminal transaction behavior,
current revision 3 and retained revision 2. A fresh process reopens both
revisions with the quota still enforced. The controller raises the hard
limit to 4 MiB with the ballast in place, proves a synchronized write
succeeds, retries to revision 4 and verifies the final file in another
process. The VHD and its quota configuration are retired together.
