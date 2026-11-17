# Follower installation, recovery and maintenance

The [authenticated month workload](live-replication-month.md) was repeated with
native macOS arm64/APFS sending to the Linux arm64/ext4 follower. The release
follower built from `a6393e0` with a measurement overlay around its real receive,
verification and manifest calls. The sender used the unchanged `a39de6c`
release binaries. The overlay adds clocks, counts and two optional interruption
markers. It leaves image bytes, protocol rules and filesystem operations intact.

## Installation timing and memory

The run completed 31 installations from 2,396,160 to 36,851,712 bytes and
recovered the four deliberately discarded ship replies by fresh TLS polls. Both
hosts reopened revision 93. The follower verified all 31 retained roots and the
selected image matched the final export byte for byte.

The table uses nearest-rank percentiles over the first 30 daily installs,
excluding the final catch-up. Receive includes transport and writing image
regions. Image sync measures the staged file's sync call. Digest and decode
each include file reads and CPU work. Manifest measures the selection
transaction, including its durable publication.

| Follower phase | p50 (ms) | p95 (ms) | Maximum (ms) |
|---|---:|---:|---:|
| Receive and write | 77.487 | 100.496 | 102.405 |
| Staged image sync | 7.205 | 9.549 | 9.963 |
| SHA-256 readback | 50.124 | 80.764 | 83.022 |
| Graph decode and verification | 45.407 | 81.922 | 84.334 |
| Manifest selection | 0.620 | 0.925 | 1.091 |

The Linux follower's recorded process high-water mark through the final
installation was 68,120 KiB (`VmHWM`). The peak combined core charge for the
two image graphs was 66,041,884 bytes. It sums the prior graph's steady charge
and the candidate's peak while the prior graph remains live. At final
selection the manifest had a separate live charge of 8,457 bytes. These
measurements describe the Docker process and its memory accounting. Filesystem
page cache and host footprint are outside the process RSS figure.

`Limits::max_live_bytes` applies independently when decoding each image. During
installation the follower retains the previous graph while it verifies the
candidate and publishes the manifest. Budgeting the service requires both
graphs, the separate manifest, TLS and allocator overhead. The previous graph
is released after selection succeeds.

## Process interruption at the two durability boundaries

A separate follower installed a verified copy of the saved 31-root revision 93.
The macOS source then acknowledged an empty transaction at revision 94. Both
attempts shipped the exact same saved revision-94 image and descriptor over
pinned mutual TLS. The test killed only the owned Linux follower process at
each marker. Both sender invocations exited 1 without a durable receipt.

| Interruption | Reopened state |
|---|---|
| After staged image sync, before verification and manifest selection | Durable revision 93, exact old selected image and unchanged manifest bytes |
| After manifest selection commits, before the durable reply | Durable revision 94 and exact captured revision-94 selected image |

An independent verifier checked all 31 retained views and the current values
after each reopen. A fresh authenticated poll recovered the exact revision-94
receipt. Sending the same saved export again returned that receipt and left the
manifest and selected bytes unchanged. The result covers process interruption
on the Docker ext4 profile. Physical power interruption and other deployment
platforms remain separate qualification gates.

## Authenticated follower recovery on a full Linux tmpfs

The sender first installs revision 4 with retained revision 2. It then
captures locally committed revision 5 once. The 307,200-byte export adds a
256 KiB value. The same saved export and descriptor are used for the failed
send, retry and replay over pinned mutual TLS.

| Space left before the install | Failed install and reopen |
|---|---|
| 151,552 bytes | The inactive slot stops at that exact prefix of the export. The manifest and selected image stay byte-identical. The server can still report the previous receipt. |
| 307,200 bytes | The entire inactive image matches the export. With no space left for manifest publication, the server returns an error and exits requiring recovery. The previous manifest and selected image stay byte-identical. |

In both cases the sender reports its local revision 5 capture and receives
no follower acknowledgment for it. An independent allocation probe gets
kernel `ENOSPC` with zero available bytes. A new follower process opens
the still-full filesystem and returns the exact previous revision 4 receipt
on a fresh authenticated poll. The test verifies the current record, the
entire retained revision 2 view and the unchanged manifest and selected
bytes while the ballast is still present.

After removing only the ballast, one retry produces the exact revision 5
receipt. Its digest matches the saved export. The installed file matches
the export byte for byte, the current and retained values verify, and a
second send of the same export leaves every follower file unchanged.
The source export and descriptor stay byte-identical through the attempts.

The test is intentionally opt-in. Set `WEEKEND_TEST_FOLLOWER_TMPFS` to a
dedicated empty Linux tmpfs mount no larger than 8 MiB, and keep the sender,
build and logs on a separate filesystem. The test checks the mount and
capacity before creating its owned directories.

```sh
cargo test --offline --locked -p weekend-replicate --test cli \
  full_storage::authenticated_follower_reopens_at_the_old_receipt_when_tmpfs_is_full \
  -- --exact --ignored --nocapture
```

The source archive and Docker reproduction script for
`4fbfaa27c6ca75ce09ea57779a6d85786253512b` are under
`target/follower-native-full-proof/`. The script uses the pinned Rust 1.99.0
Docker image, puts the sender and build on an ext4 volume and gives the
follower a separate 8 MiB tmpfs. This checks operating-system exhaustion
and process reopen. Persistent-device power interruption remains a
separate gate.

Both failure cases passed in debug and release on Docker Linux arm64
with Rust 1.99.0. The 24 ordinary application tests and strict Clippy
passed on Docker Linux and native macOS. The owned Linux container and
volume were absent after the run. The exhaustion case ran on Linux.

| Reproduction input | SHA-256 |
|---|---|
| `source.tar` | `2c96616607a4e47860eea80aa133c3399e23bf41d30b4cf4ebb400ec2abc0b78` |
| `run-docker.sh` | `61ff051e688be638757a1f2c34e0f175ca4fdcf12521b56a7f9a029b053e6792` |
| `final.log` | `f303d243a6df25a90895145882a8438aa42066cea1b6a936b98b1d72c10e96c0` |
| `macos.log` | `52213ef9e2ffda9223e93fb7f0c210c2b6b19ed5b7dfd5327b8873f8d05f42e5` |

## Torn staging and manifest prefixes

The simulator installs revision 3 and offers revision 4. Both images retain
revision 2 and the test verifies the retained row at each reopened head.
Each run starts from identical durable bytes for its fixture. The test
advances the manifest with an empty transaction for the other parity, so
both sets of root and witness regions are reused.

| Simulated failure | Cases | Receipt after reopening |
|---|---:|---|
| Only one prefix of a staged image write persists when sync fails | 15 | Revision 3 |
| Manifest root or witness write tears or its sync persists a prefix | 320 | Revision 3 with an incomplete new root, otherwise revision 4 |

The staging test chooses five lengths from each of the first, middle and
last writes. It checks the entire surviving image, including any zero-filled
gap, and proves that manifest bytes and the selected image did not change.
The manifest test takes every prefix that changes the persisted bytes plus
the physical full-region endpoint. It combines immediate durable tears,
volatile tears followed by either crash policy, and failed syncs that
persist a prefix before the next crash drops or persists pending writes.

Each manifest case compares the exact expected bytes with the surviving
manifest. Read-only inspection checks the selected revision and witness
before writable reopen can stabilize it. The first poll after reopening
returns the selected receipt. An upload occurs only when retry finds the old
receipt. Every successful retry survives another simulated crash, preserving
the exact new receipt and retained view.

```sh
cargo test --offline --locked -p weekend-db --lib --all-features \
  replication::follower::tests -- --nocapture
```

This is fault-model evidence. The full-filesystem, process-interruption and
[ext4 writeback drills](#kernel-writeback-eio-during-follower-installation-and-recovery)
exercise different failure mechanisms. Physical power interruption remains
a separate device qualification.

The exact `4ad1832c7ecd70d4deead37242f6ffcbc54aa664` source passed 145
macOS and 148 Docker Linux arm64 core tests in debug and release. Both
platforms passed eight replication integration tests and strict Clippy.
macOS also passed all ten follower tests under AddressSanitizer with a
rebuilt standard library from `nightly-2026-11-12`. The Docker run used the
pinned Rust 1.99.0 image and its owned container and volume were removed.

| Evidence under `target/follower-torn-prefix-proof/` | SHA-256 |
|---|---|
| `source.tar` | `c76b7697050d074e2e2d468ea6cedb93a836d346d5c1e6d768e8d436d582ffaf` |
| `macos.log` | `94f841a80287f5f5c81a643ff554e6e071a71fc71a980aa3beaa3dfdc1edba9f` |
| `linux.log` | `b49a0e8ae58735c530ee772db23d17a045ea8929b49783bb6f31433d0f6efc51` |
| `asan.log` | `ed3f2758712047360196218f7e210203a82f7dad4553cd3d4916eae4d3cbd8da` |

## Kernel writeback EIO during follower installation and recovery

The unchanged release CLI from `af2b4572282f712d1f227784ad47dfd2e0ecb380`
shipped pinned mutual TLS snapshots on Docker Linux arm64 with kernel
`7.0.14-linuxkit`, Rust 1.99.0 and ext4. Each case started on a fresh 64 MiB
ext4 loop image. The follower had installed primary revision 4. The offered
revision 5 contained a new 256 KiB value. Both images included retained
revision 2. The same saved export and descriptor were used across the
failed send, retry and replay. An extra initial install shifted the odd
cases to the other manifest slot and image filename.

An independent FIEMAP helper checked the inode and the physical mapping of a
4 KiB file block while the process paused before its write. The harness
directed only those eight sectors through the kernel device-mapper `error`
target. All writes and syncs went through the real Linux filesystem.
The pause hook forwarded the calls and recorded the returned
`fsync = -1, errno = 5`. The sender announced the local revision 5 capture
and exited with no follower acknowledgment in all eight cases.

| Failed sync | File offset for even / odd target manifest (bytes) | Sender error | Manifest revision and witness after remount (even / odd) | First successful authenticated poll |
|---|---:|---|---|---:|
| Staged image | 0 / 0 | `Io` | 3 / 4, witnessed | 4 |
| Manifest append | 61,440 / 73,728 | `Io` | 3 / 4, witnessed | 4 |
| Manifest root | 4,096 / 8,192 | `RecoveryRequired` | 3 / 4, witnessed | 4 |
| Manifest witness | 12,288 / 16,384 | `RecoveryRequired` | 4 / 5, unwitnessed | 5 |

After the staged-image error, the same running server could still return
the exact revision 4 receipt. Each manifest error made the server exit.
For each witness case, a second block error during writable reopen stopped
startup before the server listened. Another remount and read-only inspection
found the same manifest bytes and unwitnessed root. With the fault cleared,
writable reopen changed only the corresponding witness block and then
reported revision 5.

The test compared the selected image with its saved export, verified every
current and retained row in the fixture and compared TLS receipts with
the export's independently computed SHA-256. In the six cases that
reopened at revision 4, retry committed the selection and returned
revision 5. After the two witness cases, retry returned the recovered
receipt with identical manifest bytes. A duplicate transfer and a final
remount preserved the witnessed manifest and both exact image files.
The owned mapper devices, loops, container, volume and generated
credentials were removed.

The local archive `target/archived-proof/follower-native-writeback-af2b457.tar.zst`
contains the source tar, runner, pause and FIEMAP helpers, Rust fixture,
exports, manifest copies and raw logs. Extract it into a fresh directory and
run `bash run-linux.sh` with its pinned Docker image already present and
locked Cargo dependencies cached in `~/.cargo/registry`. The included
`image/Dockerfile` records the image preparation. The runner requires a
disposable Linux Docker VM with loop and device-mapper support. It runs the
release CLI from the archived tree with only an extra fixture example.
The same build passed 10 follower unit tests, eight native replication
integration tests and 24 CLI/application tests in release mode. The two
opt-in integration cases were skipped in that regression command.

| Evidence | SHA-256 |
|---|---|
| Local compressed archive | `6fa8aad5230196683c3c747e9432b9b4d9ddc5c15cba34a5bec42b53c2db2a08` |
| `source.tar` | `5c8b61e8b945710fd7defe083018617162b5eda6d01f37a6cb647ab3c44a02fd` |
| Docker image ID | `d276cf4b0967c0337be30eb9c302bf3b77794a7ade6b667968ebe5cbc037317c` |
| Release `weekend-replicate` | `b19cff9929990830d38aa184fbfcec7b03ea08923af9912bda0722667dd2529b` |
| `run-linux.sh` | `5c27417d1a76b83e87c173bed29abcfb4795ca02219e21b27c0891dc6140c34c` |
| `even.log` / `odd.log` | `7b7b3ea78d46c0f3779caef5449761476b129796b982f40b4a79d5186e2d5885` / `a90334e16da8218d52eca5f7bc42ee3bd83d33b89957b3703c1eed5c9a0d5658` |
| `regression.log` | `e11fecafb27f1b13368a9c1809be768503bc6754b59638b1e1f0b24e113a7520` |

This run qualifies sector writeback errors on the stated Linux profile.
The next drill covers journal failures during staging. Other platforms
and physical device power interruption retain separate gates.

## Ext4 journal errors during follower staging

The release CLI from `9ede235bff91779dd8552cbb8cf23153e33ad96a` ran on Docker
Linux arm64 with kernel `7.0.14-linuxkit` and Rust 1.99.0. For each case the
follower used a fresh 256 MiB ext4 loop image with 4 KiB blocks and fast
commits disabled. The test started with follower revision 4 and offered
primary revision 5 over pinned mutual TLS. It ran once with an empty
inactive filename and again with an older revision 3 image in that slot.
Both selected images retained revision 2.

The Linux file adapter synchronizes the newly staged file, its parent
directory, and the file again before the follower verifies and selects
the image. The test paused before the chosen `fsync` and checked the
descriptor's inode, kind and mount against the staged path. It mapped the
owned ext4 journal's contiguous transaction extent through the kernel
device-mapper error target, leaving the journal superblock and the rest
of the filesystem addressable. Earlier syncs at each gate returned zero.
The selected kernel `fsync` returned `EIO` in all eight cases.

| Failed sync | Inactive filename after remount if initially empty | Inactive filename after remount if it previously held revision 3 |
|---|---|---|
| First staged-file sync | Absent | Original revision 3 bytes and inode |
| Parent directory sync | Exact revision 5 bytes at the staged inode | Exact revision 5 bytes at the staged inode |
| Parent directory sync with a pending sibling directory creation | Exact revision 5 bytes, probe directory absent | Exact revision 5 bytes, probe directory absent |
| Second staged-file sync | Exact revision 5 bytes at the staged inode | Exact revision 5 bytes at the staged inode |

In every case the sender reported its local revision 5 capture, returned
`Io` and received no follower acknowledgment for the failed transfer.
The live server still returned its exact revision 4 receipt. After the
mapper was restored and ext4 remounted, read-only inspection found the
old witnessed manifest with identical bytes and the selected revision 4
image still matched the saved export. A fresh authenticated poll returned
the same receipt. The test checked every current and retained row in
that image and in the revision 5 image selected on retry.

Retry shipped the same saved export and descriptor and obtained the
exact revision 5 receipt. Duplicate shipment left the manifest unchanged.
Another remount preserved both exact image files and the final witnessed
manifest. These are observed ext4 journal outcomes. The next drill
covers initial enrollment. Maintenance errors, other platforms
and device power interruption retain separate gates.

The local archive `target/archived-proof/follower-ext4-journal-9ede235.tar.zst`
holds the source, runner, fixture, gate, exports, ordered syscall traces,
manifest copies and audit. Extract it and run `bash run-linux.sh` with the
same pinned Docker image and cached Cargo dependencies as the sector
writeback drill. The shell runner bounds both endpoints and verifies removal
of the owned ext4 devices, container, volume and generated credentials.
All 10 follower unit tests, eight native replication integration tests and
24 CLI/application tests passed in release mode. Two opt-in integration
cases were skipped by those ordinary regression commands.

| Evidence | SHA-256 |
|---|---|
| Local compressed archive | `b97deb1855190db2300521b530b7b057bce025489e8eb515cdc18aa2ef46bebc` |
| `source.tar` | `50f506167dcd251c5b46fdc7fe87036ad28cdc3cb574e9a438772f47aad2bb20` |
| `run-linux.sh` | `a1c95644346c76ac7792ab1562a026c64974010db30b35ca663146a388275f76` |
| Release `weekend-replicate` | `b19cff9929990830d38aa184fbfcec7b03ea08923af9912bda0722667dd2529b` |
| `namespace-gate.so` | `75d465d3db5a2dd23cf4cb103e537a801d97b05280e00bebb7e9b71e906be6db` |
| `audit.log` | `88ceb78b20cf5524fee35e5a99b6169216223a0a2a6ac01a81f2854ea222b34c` |

## Interrupted follower enrollment on ext4

The release CLI from `f5d3650a2019e705195079ff9b95773f4856684d` ran
on Docker Linux arm64 with kernel `7.0.14-linuxkit`, Rust 1.99.0 and
fresh 256 MiB ext4 loop images. The enrollment creates a revision 0
manifest, a revision 1 collection and a revision 2 enrollment row.
The row records the stream and primary identity with no image selected.
The test failed 11 real syncs and killed the initializer at three
metadata boundaries. No attempt printed enrollment success or a
follower receipt.

The three initial sync faults targeted the owned ext4 journal
transaction extent. The directory case created a pending sibling
directory after the first file sync. The second file-sync case changed
the manifest mode just before the call. Both pending metadata changes
rolled back after remount. The eight later faults targeted one 4 KiB
block at a time, using FIEMAP to check the file offset and physical
sectors before arming the device-mapper error target. Each intended
`fsync` returned kernel `EIO`. For process interruption the controller
killed only the gated initializer, before the targeted write.

| Fault or process interruption | Read-only manifest after remount | Old directory at follower startup |
|---|---|---|
| First file sync during creation | Absent | `NotFound` before listening |
| Parent sync, second file sync, or initial root sync, or kill before the initial root write | `Corrupt` | `Corrupt` before listening |
| Initial witness sync | Revision 0, unwitnessed | `Corrupt` before listening |
| Collection append or root sync | Revision 0, witnessed | `Corrupt` before listening |
| Collection witness sync | Revision 1, unwitnessed | `Corrupt` before listening |
| Enrollment append or root sync, or kill before the enrollment root write | Revision 1, witnessed | `Corrupt` before listening |
| Enrollment witness sync or kill before the enrollment witness write | Revision 2, unwitnessed | Starts after witness stabilization and authenticated poll reports `follower empty` |

A valid storage root can precede the enrollment row. Writable startup
can stabilize an unwitnessed root before checking that row, so the
revision 0 and 1 cases can repair a witness and still refuse to listen.
In both revision 2 cases, successful startup changed only the witness
block. The old directory never had a replica image or reported a
durable snapshot. Repeating enrollment in a nonempty directory returned
`AlreadyExists` and preserved its bytes.

Using a fresh private directory succeeded in all 14 cases. A pinned TLS
poll first reported an empty follower. Shipping the same saved primary
revision 5 export then returned its exact receipt. A fresh server after
another remount returned the same receipt for poll and duplicate delivery.
The verified image matched the saved export byte for byte, including
the live rows and retained revision 2. The old directory stayed as
observed after the startup check.

The local archive `target/archived-proof/follower-ext4-enrollment-f5d3650.tar.zst`
contains the source, runner, syscall calibration, gate and fixture, export,
manifest copies and audit. Extract it and run `bash run-linux.sh` with the
pinned Docker image and cached dependencies used by the staging drill.
The run passed 10 follower unit tests, eight native replication integration
tests and 24 CLI/application tests in release mode. The two opt-in
integration cases were skipped by the regression commands. The runner
removed its owned mapper devices, loops, container, volume and credentials.

| Evidence | SHA-256 |
|---|---|
| Local compressed archive | `f1d05ae97cd2b1d8e473c232d5a9bb00589ff032c0059c7227f6f10d6c4ff701` |
| `source.tar` | `32c9f4b6a50acbcc0212d1d65441e07df5ca515a1a3e1cdd111636200f388c57` |
| `run-linux.sh` | `55d337ef8217742d9bd5121e8d379ef49b552cdd87a30acdddc5b27ae18bae80` |
| Release `weekend-replicate` | `b19cff9929990830d38aa184fbfcec7b03ea08923af9912bda0722667dd2529b` |
| `audit.log` | `ab134e35a1f1760798c605fce6da64a4e50809dcc476ceed1a1447b53c8bbbd2` |

## Ext4 writeback errors during follower maintenance

The Linux arm64 release CLI from `0be5d61adc7d7619c8603dcf522cfd5ac49925e9`
installed 81 snapshots over pinned mutual TLS. That reached the follower
manifest admission threshold at 1,044,480 bytes, with selected primary
revision 84, inactive revision 83 and retained revision 2. Fault-free
compaction produced a 32,768-byte control manifest at the same manifest
revision 83. Both image files and the durable receipt matched the capped
fixture.

Each case started with byte-identical copies of the capped files and an
unrelated operator file. The harness synchronized and remounted a fresh
256 MiB ext4 volume before arming any fault. It confirmed the two image
files, the selected receipt and the canonical inode on the remounted
fixture. The sector and journal gates used real kernel device-mapper
errors. At the two file syncs after exchange, the controller added a
pending inode-mode update at the gate to make the target call commit
metadata. After those faults the recovered file had its original mode
`0640` in both cases.

| Compaction sync returning kernel `EIO` | CLI result | Canonical manifest after remount | Scratch after remount |
|---|---|---|---|
| Scratch data block at offset 0 | `Io` | Exact original | Incomplete header, `UnsupportedFormat` |
| Scratch root at offset 8,192 | `Io` | Exact original | Incomplete root, `Corrupt` |
| Scratch witness at offset 16,384 | `Io` | Exact original | Manifest revision 83, unwitnessed |
| Scratch file metadata | `Io` | Exact original | Exact compacted bytes, mode `0600` |
| Directory after exchange | `OutcomeUnknown` | Exact original | Exact compacted bytes |
| File after exchange | `OutcomeUnknown` | Exact compacted bytes | Exact original |
| Directory after removing old scratch | `OutcomeUnknown` | Exact compacted bytes | Exact original |
| File after removing old scratch | `OutcomeUnknown` | Exact compacted bytes | Absent |

The harness verified the canonical and scratch inodes and compared bytes
against the capped fixture and fault-free control. The directory faults
followed successful kernel exchange and unlink calls. Each failed command
exited without printing a receipt. Fresh TLS poll and duplicate shipment
returned the exact selected revision 84 receipt. The original canonical
manifest still rejected revision 85 with `Limit` before changing either
image slot. A successful compaction retry preserved the receipt and any
leftover scratch, reproduced the exact compacted bytes and enabled
installation of revision 85. Another remount verified the new receipt,
selected bytes, current row and retained revision 2.

Two other cases exercised `cleanup-inactive`. Kernel `EACCES` at unlink
returned `Io` and preserved the file. With the journal fault armed, unlink
returned zero and the CLI reported one removal alongside its existing
revision 84 receipt. A separate directory `fsync` then returned `EIO`.
After remount, the inactive revision 83 file returned with the same
inode and exact bytes. In both cases authenticated poll still reported
revision 84. Cleanup retry reported one removal, and another retry
reported zero. With a successful directory sync, the next remount
preserved the removal, the selected image, the manifest, the retained
view and the unrelated operator file. The CLI's snapshot receipt and
its cleanup count describe separate outcomes.

The local archive `target/archived-proof/follower-ext4-maintenance-0be5d61.tar.zst`
holds the source, seed and replay scripts, fixture, error gates, syscall
logs, manifest and image files, and audit. Extract it and run
`bash run-linux.sh` with the pinned Docker image and cached Cargo dependencies
used by the staging drill. The same run passed 10 follower unit tests,
eight native replication integration tests and 24 CLI/application tests in
release mode. The two opt-in integration cases were skipped by the regression
commands. The runner removed its mapper devices, loops, container, volume and
generated credentials. Other platforms and physical power interruption remain
separate qualification gates.

| Evidence | SHA-256 |
|---|---|
| Local compressed archive | `5486238e580529e93a3a201e8ea259761ebcc8c029e8242242e19aa917c6f1ea` |
| `source.tar` | `94b2721c5262cc5bd32d270731313d8db2fdcc49c84daa8ac7f39a2bc0753fef` |
| `run-linux.sh` | `d8a508a2a616cbbc3e685f513fd1b654bacc81f020bb57e250c4318dc7155c7e` |
| Release `weekend-replicate` | `b19cff9929990830d38aa184fbfcec7b03ea08923af9912bda0722667dd2529b` |
| `audit.log` | `9f6afb452f021ba3028c4fd3dbd58d1ca80b757abccc7286ac69b7ae64015830` |

## Reproduction and evidence

The proof directory is `target/follower-phase-qualification/`. Its archived
source `source.tar` is the clean `a6393e0241235c6aaaab876dea47f1ab4ed22977`
tree. Extract it into `source/`, apply `profile.patch` with `patch -p1`, and
copy the saved `follower_phase_fixture.rs` into `source/examples/`. Build or
recover the original macOS sender and CLI as described in the
[staging results](fresh-image-staging.md#authenticated-month-replay).
`run-linux-follower.sh` records the archive layout, the previously captured
input and the exact native commands. Run it from the repository root with a
fresh `run/` directory. The Linux image is the pinned Rust 1.99.0 toolchain
used for the exact-source staging runs. The script bounds the sender and
checks process exits, on-disk data, copy bytes, TLS receipts and resource
cleanup. It preserves no generated private credentials.

The staged-allocation sweep is also part of this checkpoint. All 39 injected
failures leave the destination empty and restore the original live charge.
The sweep passed on macOS and Linux and under native macOS AddressSanitizer
with `nightly-2026-11-12` and a rebuilt standard library. Native macOS passed
all 142 core library tests and strict core Clippy.

| Evidence | SHA-256 |
|---|---|
| `source.tar` | `5e9c9718cff807ec9e8cbd9f5ff4d51f2788c62555f3271d929b6a7cabeb7ff2` |
| `profile.patch` | `ab42420d80c31a2e9b37968aab346856bf44a2f262fec2e44ecc459619cab93e` |
| `source/examples/follower_phase_fixture.rs` | `534066841d900b08b847c5d6203adde11fdb8d327a50f2deb1d07a6e086a7863` |
| `run-linux-follower.sh` | `52d3381cd4cf1c70445dc3e1a5b48feb1a7a49aa95ff2a5fccc9fe93b6487dc0` |
| `run/follower-phases.csv` | `ac59dc71910d7dd8ccef6b5e8dc32b0071e027df19dfc873ac4a88cdd8dd4c97` |
| `run/month.csv` | `965a99128631d8469020d951d609ec2f3345ece686c1ea041b83aedb7864f25d` |
| `run/interruptions.log` | `02b19d3e3b5fb58f9010a8f96d48b8250a4b92fffcd8dfe8cc742dc35c894e6f` |
| `run/first.wdb` | `d0da420bcf352c15b550ac00089e6c0f65bb3b114ab8ee33d6b678b03412c040` |
| `run/next.wdb` | `a4f26da47f1812d9e58777c325472fa8a863138990b4ee7235a95a6eb38fe7c1` |

The [CRC-64 replay](crc64-performance.md#authenticated-follower-replay) repeats
the phase and interruption controls after changing the storage checksum
implementation. Daily graph verification p95 is 31.854 ms in that replay.
