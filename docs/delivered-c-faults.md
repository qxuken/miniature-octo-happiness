# Delivered C ABI publication failures and independent recovery

This private Linux gate exercises the **existing packaged shared library** from
`21e5d2638c24c6254ced35d0077c55bb30a287b2`. It does not rebuild or modify the
engine. A C client compiles against the relocated delivered header/library at
`-O2 -UNDEBUG`; assertions, ABI 1, format 2, feature bits 3 and the full embedded
source identity are checked. Loader diagnostics identify that exact library.

## Cases and selected revision

Each case starts with acknowledged current revision 3 and retained ID 1 at
revision 2. The candidate atomically changes a 32,768-byte row, adds a 65,536-byte
payload and records application request 42. The expected outcomes are:

| Case | Mechanism | Commit result | Live revision | Fresh process reopens |
|---|---|---|---:|---:|
| Baseline | No injected failure | `OK`, output revision 4 | 4 | 4 |
| Append sync | Interposed first target `fsync` returns EIO | `IO`, output 0 | 3 | 3 |
| Root write | Interposed root-slot `pwrite` returns EIO before writing | `OUTCOME_UNKNOWN`, output 0 | 3 | 3 |
| Root sync | Interposed second target `fsync` returns EIO after root write | `OUTCOME_UNKNOWN`, output 0 | 3 | 4 |
| Witness sync | Interposed third target `fsync` returns EIO after witness write | `OUTCOME_UNKNOWN`, output 0 | 3 | 4 |
| Disk full | Kernel ENOSPC on a private 8 MiB tmpfs | `IO`, output 0 | 3 | 3 |

EIO is a **single-threaded syscall-interposition fixture**, not a physical device
failure. The root/witness sync cases leave successful writes visible in the
running kernel's page cache. Their fresh-process revision 4 result does not
predict what survives power loss. The simulator's persistence model and real
device qualification retain their separate obligations.

The C oracle scans every expected key and checks every payload byte in current
and retained snapshots. Revision 3 contains only the old row; revision 4 contains
exactly `payload`, `request` and `row`. The complete retained-ID list and old
revision-2 row are checked throughout. A partial candidate cannot pass merely
by reporting the expected revision or request marker.

Every commit error is terminal: a repeated commit returns `CLOSED` and clears
its nonzero-seeded revision output. After IO or uncertainty, a new transaction
returns `RECOVERY_REQUIRED`; it too becomes terminal with output zero. Readable
live snapshots still show complete old revision 3. All values, scans, snapshots,
transactions and database handles are released under the ABI lifetime contract.

The exercising C process closes and exits. A **new C process without the
interposer** then opens the canonical file, verifies the complete selected
state/history and reconciles request 42 before retrying. If its marker is absent,
one retry commits revision 4. If present, no duplicate effect/commit occurs.
Both paths finish at revision 4, close/reopen again and preserve retained
revision 2. This is process replacement after a returned error, not a SIGKILL
inside publication or an operating-system restart.

## Calibration and bounded exhaustion

The interposer filters by the database's actual device/inode. A different file's
`fsync` must neither consume the armed failure nor increment target counters.
The baseline requires exactly three target syncs, one root-slot write and one
witness write. Each injected case requires exactly one hit at its expected
phase. These checks reject a missing hook or an accidentally different syscall
path rather than silently recording an unexercised failure.

A separately compiled shim disables injection while retaining observation.
Running the expected first-sync failure with it must abort at the C status
assertion; normal success cannot satisfy the negative control. Core dumps are
disabled. The shim is not a delivered library feature or API.

The ENOSPC case performs no synthetic error injection: a controlled ballast
file exhausts only the fresh container tmpfs. The C client checks tmpfs type,
capacity at most 8 MiB, an explicit runner flag and exclusive file creation;
ballast writes have an independent 16 MiB ceiling. The observed native errno is
28 and the injection-hit count is zero. Only that ballast is removed before
fresh-process recovery. This qualifies kernel exhaustion on tmpfs, not ext4
capacity exhaustion, EDQUOT, persistent-device behavior or host disk filling.

## Helper verification and source binding

Five controls pass under normal and optimized Python on Mac and Linux. They
check existing/dangling/package-contained output admission before external work,
strict typed case outcomes, separate exercise/recovery phase identities, false
completion rejection, and failed cleanup/daemon queries. Review caught a Boolean
mode accepted by Python's integer equality before merge; explicit phase integer
checks and Boolean/float negative cases now close it.

The runner copies and hashes its C sources, runner, controls and shared process
helper before execution. Every manifest-listed package file is checked before,
after relocation and after copyback. Binary hashes remain stable. The worker's
complete artifact inventory is rehashed by the controller before success.
Exercise and recovery outputs have separate schemas and process records; neither
an exercise summary nor a saved expected value can substitute for recovery.

Execution uses immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
aarch64 Linux, GCC 12.2.0 and Python 3.11.2. Host orchestration uses Mise Python
3.14.8. The container has no network, two CPUs, 2 GiB memory and 128 PIDs. Build,
injected-failure databases and proof files use a UUID private volume confirmed
as ext4 by `findmnt`; only the actual exhaustion case uses tmpfs. Native commands
have 90-second deadlines, the disabled-shim control 30 seconds and the container
300 seconds. Owned process groups are stopped/reaped; Docker cleanup attempts
both exact UUID removals independently and requires successful absence queries.

## Reproduction and evidence

With the already qualified Linux package and local image available:

```sh
cargo test --offline --locked -p xtask --bin xtask delivered_faults
cargo test --offline --locked --release -p xtask --bin xtask delivered_faults
cargo xtask faults delivered \
  --package target/qualification-21e5d26/linux-package \
  --output target/my-fresh-delivered-c-faults
```

The current parent, worker, exact-package verification and independent six-mode
oracle are Rust-only. The worker runs directed oracle controls itself; debug and
release Rust tests preserve the retired Python controls. Current conversion
results are in the [ledger](xtask-conversion.md). Historical proof below retains
its original source/tool versions.

Final proof is `target/delivered-faults-365daf6/final-v3/`, with six exercising
processes and six separate recovery processes, plus the expected disabled-shim
abort. All 91 copied artifacts were rehashed, and both owned Docker resources
were absent. The original package manifest/library SHA-256 remain
`d0a1bbce2fc5b742fe843ca5e28b68b6dd7a128aca5eb994389a256b7ba990c2` /
`16a927abb639ed68de1fdcc375bc52d41270aa1ac06552b1132fd3942088ac64`.

| Record under `target/delivered-faults-365daf6/` | SHA-256 |
|---|---|
| `index-v2.json` | `bd54a434658c6dec2f713a576663091a2b3ed8d4bc49724fa832fccb8a39560f` |
| `final-v3/evidence.json` | `91b486f09332ade287052ec1d7cc58fbb47938463871ae5fde002974775ce86c` |
| `final-v3/result/evidence.json` | `6a40ad6d41ab1f36bedcac8b96d26dc9aea37c3bd1e584316e42f3c4a74d6136` |

Earlier `initial/` uses same-process close/reopen; `final/` and `final-v2/`
predate the strict phase-type control or Linux helper-control execution. They
remain source-bound historical evidence and are not relabeled as final-v3.
Ignored local proof is not a permanent retention promise. This gate adds no
native Mac/Windows, Rust/Odin fault-consumer, power-loss, compiler MC/DC or
unified release-candidate qualification. No remote or registry action occurred.

## Native macOS delivered C faults

Use a verified combined macOS package on local APFS.

```sh
cargo xtask faults delivered --macos \
  --package target/xtask-rust-only-clean-private-package \
  --output target/macos-delivered-faults
```

The same optimized C exercise and recovery consumers link the relocated exact
dylib. The macOS fixture observes `fcntl(F_FULLFSYNC)` and `pwrite` on the
target inode. An unrelated file executes a full sync first to verify that it
cannot spend the target's failure. The interposer forwards the known
descriptor operations used by the artifact and aborts on an unexpected command.
The runner clears inherited loader overrides, verifies the relocated dylib in
each process's loader log, and checks that the recovery process loads no shim.

| Case | Returned status | Visible revision | Fresh process reopens |
|---|---|---:|---:|
| Baseline | OK | 4 | 4 |
| Append full sync | IO | 3 | 3 |
| Root full sync | OUTCOME_UNKNOWN | 3 | 4 |
| Witness full sync | OUTCOME_UNKNOWN | 3 | 4 |
| Root write | OUTCOME_UNKNOWN | 3 | 3 |

All five macOS cases pass against the exact `bc56084` dylib. Both retry paths
finish at revision 4 with retained revision 2. The disabled shim exits on the
required status assertion. The package manifest and dylib remain byte-identical.
The existing Linux/ext4 driver also passes all six modes with the shared runner,
including the kernel tmpfs ENOSPC control. Its reopened revisions remain
4/3/4/4/3/3 in mode order.

| Proof | SHA-256 |
|---|---|
| macOS dylib | `d5f9b9315064876f07013a698f654de131e462b5ef2487416202b60d79285705` |
| macOS evidence | `0658490f36a72616482559b9e70306d184374ca86e1d267e32b9a3aa888a66fa` |
| Linux evidence | `eda1ec9681de28a05d24db3278521a9fb805e5ac1c31259ec214c66f6342588a` |

The macOS case injects single syscall returns and recovers through independent
processes on a running kernel. Physical power interruption and Darwin kernel
ENOSPC need separate evidence.

## Native Windows delivered C gate

Use a manifest-pinned combined-feature package and a fresh output path on
local fixed x64 NTFS. The Windows CI job runs the gate against the same
packaged DLL as the delivered C/C++ trace.

```powershell
$pin = (Get-FileHash target/private-package/manifest.json -Algorithm SHA256).Hash.ToLowerInvariant()
cargo xtask windows-faults --package target/private-package --manifest-sha256 $pin --output target/windows-faults
```

The optimized C11 consumer uses the same row, retained-root and transaction
oracle as the Unix gate. Each process verifies its loaded DLL path, DLL
hash, embedded source identity, ABI, format and feature mask. The exercising
process patches the loaded DLL's two import slots for `FlushFileBuffers`
and `NtWriteFile` in memory. The injector filters calls by the case database
path, observes an unrelated handle without consuming a fault, and restores
both slots before normal exit. The fresh recovery process opens the tested
file with its original import table. A disabled-injection executable must
fail the expected-status assertion.

| Windows case | Injection | API result | Visible revision | Fresh process reopens |
|---|---|---|---:|---:|
| Baseline | Observe all three publication flushes | OK | 4 | 4 |
| Append flush | `FlushFileBuffers` reports an I/O device error | IO | 3 | 3 |
| Root flush | `FlushFileBuffers` reports an I/O device error after the root write | OUTCOME_UNKNOWN | 3 | 4 |
| Witness flush | `FlushFileBuffers` reports an I/O device error after the witness write | OUTCOME_UNKNOWN | 3 | 4 |
| Root write | `NtWriteFile` returns the I/O device error before writing the slot | OUTCOME_UNKNOWN | 3 | 3 |
| Append write | `NtWriteFile` returns disk full before appending | IO | 3 | 3 |

Each recovery reconciles request 42, finishes at revision 4 and verifies
retained revision 2. The gate covers injected write and flush returns on the
native filesystem. [Actual NTFS exhaustion](#actual-ntfs-disk-exhaustion)
has a separate execution below. Physical device persistence remains open.

All six cases passed on Windows Server 2025 x64 on fixed NTFS at PR head
`47915eaac4a21ef4e6dd450929e0c8925f3533e8`. The packaged DLL identifies the
tested PR merge as `e4be226fc3eb0683876f302e950ac61cbfbb8d6e`. The consumer
used MSVC 14.51.36231 and SDK 10.0.26100.0. The disabled-injection control
failed the required status assertion. An independent archive check rehashed
all 132 listed files, confirmed byte equality of both DLL copies, and checked
the 24 before/after DLL records in the 12 exercise and recovery processes.
All 15 compiler and fixture command reports verified process cleanup. The
normal 20-case, 134-cell C/C++ trace passed with the same DLL and manifest.
The downloaded package, fault and trace DLL copies compared byte for byte.
[Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43723029255/job/132838901168)
· [Windows fault evidence](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43723029255/artifacts/14165960124)
· [C/C++ trace evidence](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43723029255/artifacts/14165267291)

| Evidence | SHA-256 |
|---|---|
| Hosted ZIP | `9ae2800858357153741d309a560dcfd68dbe79f1b17c0e412e768b29a67ced36` |
| Evidence JSON | `ba515c5b949a3013771c05f1cf9f9a61d123d40dd6210e35f5019e949628f26b` |
| Delivered DLL | `a5b64177bd021ced37096fd4f881455fd2e66799868753dab172af899636a16b` |
| Package manifest | `883f69927be6dd114599d42e9405892d9746ccb5fb091ecb80819b2306f72c22` |
| Exercise and recovery executable | `875d39bd649fd5f65fa16afe26cd32777d61280e24da7b659cccdbcaaecf78ac` |
| Disabled-injection executable | `801ccc62637e6c9d82581537c8a57ccd90a66a36f2a42568b7c91800cc553606` |
| Independent archive rehash | `66b24f762730486b172ba91b067c8d24b9c4b33f5135dcdfffaf39ed87180184` |
| Normal C/C++ trace evidence JSON | `9e98a5b2c9ad64ae442823a05b1712db8de670e8bdc2954b0ffa75dd58b418d9` |
| Windows job log | `4a05ef5c41aec0a4968db0473af333c5b23ce6d4f42a5b6c7683691187070514` |

The initial hosted run at `86ff6c6` stopped during compilation when MSVC
`/W4 /WX` rejected shadowed fixture variables. `47915ea` corrected the
names while preserving the compiler flags and the fault oracle.
[Initial compiler evidence](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43722762171/artifacts/14165555158)

## Actual NTFS disk exhaustion

Run from an elevated x64 Windows session with the Windows Virtual Disk
provider and DiskPart. Use a verified combined-feature package and a new
evidence directory on fixed NTFS.

```powershell
$pin = (Get-FileHash target/private-package/manifest.json -Algorithm SHA256).Hash.ToLowerInvariant()
cargo xtask windows-disk-full --package target/private-package --manifest-sha256 $pin --output target/windows-disk-full
```

The controller creates a fixed 64 MiB VHD in the evidence directory. It
holds the attachment handle while DiskPart partitions and formats the disk
number obtained from that handle. Before allocating ballast, it verifies
that the new drive has one extent on the owned VHD and uses 4 KiB NTFS
allocation units. The DLL, the executable and the logs stay on the host
volume. The controller verifies detach, drive-letter disappearance and VHD
file removal before marking the run successful.
[Windows virtual disk attachment](https://learn.microsoft.com/en-us/windows/win32/api/virtdisk/ne-virtdisk-attach_virtual_disk_flag)
· [Volume extent verification](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-ioctl_volume_get_volume_disk_extents)

| Phase | Required observation |
|---|---|
| Exhaustion and commit | The volume rejects a 4 KiB ballast write. The DLL's positional append receives `STATUS_DISK_FULL` from NTFS. The C API reports `IO`, clears the output revision, and keeps live revision 3 with retained revision 2. The hook only observes the native return. |
| Reopen with the ballast in place | A second process opens revision 3 and checks every current and retained payload byte. The database hash stays fixed and no allocation unit becomes free. |
| Free space and retry | The controller deletes the ballast. A third process finds request 42 absent, retries once, and reopens revision 4 with the complete request and retained revision 2. |

The gate passed on Windows Server 2025 x64 at PR head `ebfd099`, with
MSVC 14.51.36231, SDK 10.0.26100.0 and DiskPart 10.0.26100.1150. The
packaged DLL identifies the tested PR merge as
`ac1bc1a68b722423bacd1284aa77f012ad827a50`. The exercise observed native
error 112 with zero injected hits and no root write, witness write or
publication flush. Windows reported zero free bytes at the failure and
after the fresh reopen. The ballast removal restored 53,506,048 free bytes.
The third process retried and reopened revision 4 with retained revision 2.

The volume and image cleanup checks passed. The archive rehash verified
all 79 listed files and six DLL attestations across the three processes.
The packaged DLL and manifest compared byte for byte with the ones used by
the six injected-fault cases in the same job. NTFS quota, maintenance
exhaustion and physical power interruption retain their own gates.
[Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43724963499/job/132844797192)
· [NTFS exhaustion archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43724963499/artifacts/14165178920)

| Evidence | SHA-256 |
|---|---|
| Hosted ZIP | `8fe5c81ff236d458e460da34b5643b3cec2cf854ff2034bed49f8dfaf55889f8` |
| Evidence JSON | `eb4f24e9e0b11c4c2499e3c1be593548088c72d738bddfe788dc9ad50325a11e` |
| Delivered DLL | `d5dc3f7970b6ac5a02caffd91430478d11ae023557b17388b06647245faec71b` |
| Delivered manifest | `d7cc4a4541f26ccc0b1ac851ce5854132e798c459a98422f87e3b618deed5838` |
| Database after failure and full-space reopen | `b217d3d57f129c8f795ed645c68272557073af5bc0cf4399b432d62188806949` |
| Database after retry | `85a55ef35281bb798339d88febe87ffb1f1435116ab652a2bb28d79008b406ec` |
| Independent archive rehash | `36a3dc1a4e302daf9a420f36f905034095dae12f0e3c27aae9fceb65c8db1209` |
