# Local commit recovery after ext4 writeback EIO

The Linux profile exercises an actual native `fsync` error at each of the
three local commit barriers. The database uses its normal
file adapter and the core-only `native_quota` fixture. The private controller
injects the fault at the block device, outside the database process.

## Reproduction

Use a dedicated Linux environment with loop devices, ext4 and device mapper
with the `linear` and `error` targets. Install `e2fsprogs`, `util-linux`
and `dmsetup`. Build the executables in an exact source checkout before
elevating only the controller.

```sh
revision=$(git rev-parse HEAD)
cargo build --locked --offline --release -p xtask --target-dir target/xtask
WEEKEND_DB_SOURCE_ID="$revision" cargo build --locked --offline --release \
  --example native_quota --target-dir target/quota-build
sudo "$PWD/target/xtask/release/xtask" quota --writeback \
  --fixture "$PWD/target/quota-build/release/examples/native_quota" \
  --source "$revision" --output "$PWD/target/native-writeback"
```

The controller verifies the source archive, executable hashes, ownership of
the 64 MiB ext4 backing image and its loop device, and the exact mapper UUID
and active table. It creates the database as an unprivileged numeric UID
with empty supplementary groups and capabilities. It uses a fresh database
per barrier and maps only eight 512-byte sectors to the kernel `error` target.
The remaining sectors stay on the same linear backing device.
[Linux FIEMAP documentation](https://docs.kernel.org/filesystems/fiemap.html), [Kernel device-mapper error target](https://kernel.googlesource.com/pub/scm/linux/kernel/git/torvalds/linux/+/refs/heads/master/drivers/md/dm-target.c)

For append, the controller uses FIEMAP to locate one reserved 4 KiB
unwritten block beyond the old end, leaving the logical file length alone.
For root-slot and witness cases, it selects one already allocated block at
the byte offset for revision 4. In each case, the fixture opens the database
and stages the change while the device is healthy, then waits on a gate in
the controller-owned directory. The controller releases it only after reading
back the exact fault mapping. Supervision binds the readiness line and the
failure report to the same process.

The original transaction becomes terminal and the live writer requires
recovery before further writes. The controller restores the linear table
and remounts ext4 before inspecting recovery. Read-only inspection checks
the selected graph and reports whether its matching witness is already on
disk. A separate writable reopen checks every current and retained payload.
A second remount verifies the final state. The controller preserves all
three raw images per case and proves its mapper, mount and loop are gone.

## Initial append baseline on Docker Linux arm64/ext4

At `fdb3ff2bdcb29b181c1e9a3ccc3a0163ba99a64b` with Rust 1.99.0, the
kernel reported `EIO` at the append flush. The original transaction was
closed, and a second transaction on the same database handle returned
`RecoveryRequired`. Both fresh reopens verified retained ID 1 at revision 2.

| Observation | Current revision | Physical bytes | Committed bytes |
|---|---:|---:|---:|
| Seed | 3 | 57,344 | 57,344 |
| Reopen after EIO | 3 | 139,264 | 57,344 |
| Reopen after successful retry | 4 | 221,184 | 221,184 |

Read-only admin checks on the copied images verified the same witnessed
revisions. All three images kept their exact hashes. The failed append
extended the physical image. Its first 4 KiB block read back as zeros and the entire
57,344-byte acknowledged prefix matched the seed. The successful retry
appended beyond the failed tail. All 24 Linux quota unit tests and strict Clippy
checks passed, and the shared fixture passed Windows x64 cross-target Clippy.

Local evidence is under `target/delayed-writeback-validation/docker-fdb3ff2/`.
The controller evidence SHA-256 is
`9ed66da266158338bffaa4e945dfb8c74ab3843043e68371b97addecb4031197`.
The source archive SHA-256 is
`bfb69b2953c698e293166b5f5e33187304f38bc2797d91b0198b3542357c21a8`.
The independent audit matched all 241 evidence payloads and 79 command
records. Its SHA-256 is
`f680a569721afc023eadfb2a1278e50931ca6d9b0aaf7275c4a9284c41212eea`.

## Initial append baseline on hosted Ubuntu x64/ext4

The [Ubuntu 24.04 job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43752541591/job/132929474999)
passed with Rust 1.99.1 at PR head `fdb3ff2` and tested merge
`ca97b30e03f889795bfba95b3e71259f6bbdc5bc`. It observed the same
`Io / Sync / Other / 5` result, remount revisions 3 and 4, retained
revision 2, and exact pre-failure prefix. The failed 4 KiB block read as
zeros, and the three image lengths matched the Docker result. The native
append quota, maintenance block and inode quota, and global inode exhaustion
controls also passed in the same job.

The independent audit matched the 692 archived source files to the exact Git
archive, checked 241 payload hashes and 79 command records, and verified the
uploaded fixture and the two remounts. Read-only admin checks independently
confirmed the witnessed revisions and retained ID while preserving all three
images byte for byte. [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43752541591/artifacts/14177295044)

| Evidence | SHA-256 |
|---|---|
| Hosted ZIP | `01c77020ddb5b39ec27f7f29e145af9b0690afea5e5be8bf9b10750c93344668` |
| Controller | `9f511c87b80f6275684db33129d94ee5695c0007d2c8b01082193b5648823812` |
| Uploaded fixture | `0b2425429e5c1fb78631eef9232a6697b2bc17224bbca99459431a7c1be6cee8` |
| First remount image | `2e58bb2f2ab0d1431f7ec6610c2fc610e7b4dc0ae0fda4039aedd90c26e1f526` |
| Successful retry image | `0730553aa765bbb3f372c4d076d12ac0f1b9a2cb85473e08fdd2088438d66fd1` |
| Independent audit | `11b663af019ad045eff70c5c1129d5f2e7d4642c6a91615d42e36e4fe6aef601` |

## Root-slot and witness results

All three cases exercise a failed buffered-write flush with kernel
`EIO` (5), reported as `Sync / Other`. The current view in the failed
writer remains at revision 3. The first two cases retry to revision 4.
In the witness case, reopening verifies and stabilizes the already
published change. The follow-up changes only its witness block, so the
application mutation occurs once.

| Faulted 4 KiB block | Commit result | First remount, read-only | Final remount |
|---|---|---|---|
| Append at offset 57,344 | `Io` | Revision 3, witnessed | Revision 4, witnessed after retry |
| Even root slot at offset 4,096 | `OutcomeUnknown` | Revision 3, witnessed | Revision 4, witnessed after retry |
| Even witness at offset 12,288 | `OutcomeUnknown` | Revision 4, unwitnessed | Revision 4, witnessed after writable reopen |

All six remount observations retain ID 1 at revision 2. Raw byte comparison
shows the append and root-slot failures preserve the 57,344-byte acknowledged
prefix. With a witness failure, the prefix differs only at the now-durable
candidate slot. The final witness image is the same length as the first
remount image and differs only at the 4 KiB witness block.

Docker Linux arm64/ext4 passed at `2b9f00f7e857a16e1775b5bb41b440b0c48378df`
using Rust 1.99.0. The hosted Ubuntu 24.04 x64/ext4 job passed with
Rust 1.99.1 at the same PR head and tested merge
`8cc013178dafe30a89d28475aff60cf4cfb8815b`. The job also passed its append
quota, maintenance block and inode quota, and global inode exhaustion
checks. [Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43753774702/job/132933286290), [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43753774702/artifacts/14177696025)

On each platform, the independent audit matched 693 source files against
the exact Git archive, checked 20 distinct fixture processes, 156 commands,
all 478 evidence payload hashes, and the raw slot and witness revisions.
Independent read-only admin checks matched the expected revisions and
preserved the hashes of all nine images. Strict Clippy passed for the
Linux controller and fixture and for the Windows x64 cross-target fixture.
All 27 Linux quota unit tests and 14 durability tests on macOS passed.
Local proofs are under `target/publication-writeback-validation/`.

| Evidence | Docker Linux SHA-256 | Hosted Ubuntu SHA-256 |
|---|---|---|
| Controller | `2521dd419e22e3fe469b1be762ea54994e670257272ad050741085e718a92498` | `35b0ae2463b4c1673613c2cad4be99baf057ec67456d206076f14a0d111116e9` |
| Fixture | `ab9177c64dc25d8a82bd70c8f2606ef4a82706fd943d2c41e91d89093639b2e4` | `81cab53288205f95b39ce0535de02624115ed21e9982b6da4a57d10fad8c5633` |
| Independent audit | `3d9a6ddd8608cb39384877cbbc2e3697c07178d6c11bb0c3cbed2d6a96abb84d` | `e29e488cd56f6dce3eded0a83cfb5fcc745e8d4d9c2f1e2d9ad4d61e5f52bc24` |

The hosted ZIP SHA-256 is
`7d0f475a7a56a6d5a3c7989134deaeddf7fac289bbf542673de46b2d5fff6205`.

The results qualify the precise sectors denied by the `error` target on
ext4. The simulator covers other slot-survival outcomes. The
[maintenance writeback profile](maintenance-space-exhaustion.md#kernel-writeback-eio-during-near-cap-maintenance)
qualifies the first copy-data flush. Later maintenance sync failures and
physical power interruption remain open.
