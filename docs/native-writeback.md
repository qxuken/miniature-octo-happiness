# Local commit recovery after ext4 writeback EIO

The Linux profile exercises an actual native `fsync` error during the
first append barrier of a local transaction. The database uses its normal
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
with empty supplementary groups and capabilities. The controller reserves
one 4 KiB unwritten block at the old end of the database without extending
its logical length. It requires FIEMAP to identify that exact allocation,
then maps only its eight 512-byte sectors to the kernel `error` target.
The remaining sectors stay on the same linear backing device.
[Linux FIEMAP documentation](https://docs.kernel.org/filesystems/fiemap.html), [Kernel device-mapper error target](https://kernel.googlesource.com/pub/scm/linux/kernel/git/torvalds/linux/+/refs/heads/master/drivers/md/dm-target.c)

The fixture checks the old current and retained contents before the write.
The failing transaction must report `Io / Sync / Other / EIO (5)`, become
terminal, and require recovery before further writes. The controller restores
the linear table and remounts ext4 before each persistence check. The first
fresh reopen must preserve the exact acknowledged prefix, current revision 3
and retained revision 2. A successful retry commits revision 4, which
another fresh process verifies after a second remount. Every check compares
both records and all payload bytes in each view. The controller retains
small raw images and proves its mapper, mount and loop association are gone.

## Docker Linux arm64/ext4 result

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

## Hosted Ubuntu x64/ext4 result

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

This profile covers the pre-publication append barrier on ext4. Native
writeback faults during slot publication, witnesses and maintenance,
as well as physical interruption and storage-device behavior, need
their own qualification.
