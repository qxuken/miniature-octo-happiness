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

The original five macOS cases inject single syscall returns and recover
through independent processes on a running kernel. Physical power
interruption remains a separate gate.

### Optimized Odin consumer on APFS

Build a clean combined-feature package with the shipped Odin bindings, then
run the Odin and C consumers against the same dylib.

```sh
cargo xtask package --features crdt,replication --odin --output target/private-package
cargo xtask faults delivered --macos --odin "$(command -v odin)" \
  --package target/private-package --output target/macos-odin-faults
cargo xtask faults delivered --macos \
  --package target/private-package --output target/macos-c-faults
```

The Odin consumer calls the packaged ABI directly using the packaged
declarations. It keeps assertions enabled under `-o:speed` and checks ABI,
format, features and the embedded source ID in both processes. The exercise
checks that a full sync of an unrelated inode leaves the injection armed,
reads the exact visible and retained rows, and verifies that failed and
recovery-required transactions are terminal with a cleared revision output.
A fresh process checks request 42 against the reopened state, retries only
when the marker is absent and reopens the complete revision 4 again. Every
retained row remains at revision 2. Loader logs verify the exact relocated
dylib for both phases and the injector for the exercise phase only. The
disabled-injector control must exit 65 at the commit-status comparison.

Both consumers produced identical rows under the independent Rust oracle.

| APFS case | Returned status | Visible revision | Fresh process reopens |
|---|---|---:|---:|
| Baseline | `OK` | 4 | 4 |
| Append full sync | `IO` | 3 | 3 |
| Root full sync | `OUTCOME_UNKNOWN` | 3 | 4 |
| Witness full sync | `OUTCOME_UNKNOWN` | 3 | 4 |
| Root-slot write | `OUTCOME_UNKNOWN` | 3 | 3 |

The runner and package came from clean source
`b2ddef866ee4c6d13b66f0aef3d72a094851551a`. The run used macOS 27.0.1 arm64,
Rust 1.99.1, Apple clang 21.0.0 and Odin `dev-2026-10:a22aa1984`. The eight
Rust helper controls passed in debug and release, and strict Clippy passed.
The same runner passed all six Linux C cases, including tmpfs ENOSPC,
against the historical `bc56084` package on Docker Linux arm64.
The archived artifact lists independently rehashed 94 Odin files, 93 macOS C
files, 109 files from a separate `--apfs-full` run and 788 files from the
Linux worker. The `--apfs-full` C run against the same dylib also observed
kernel `ENOSPC` with no synthetic hit, reopened revision 3 while full and
reopened revision 4 after releasing the ballast and retrying. Its owned
volume was detached and removed.

| Proof under `target/odin-delivered-validation/` | SHA-256 |
|---|---|
| `source.tar` | `f988e22f2d034c8dbc09dd15cae237e428614438785f1768ddd5aa2613b1660c` |
| `macos-odin/evidence.json` | `1d9c1d7447dec295795fd54dd93cdf6336e548eb357d0dbb25c0c30e338940de` |
| `macos-c/evidence.json` | `45a3f8fff86cd25d9a102ea2027aa55ef87a88e2dc7cdcd2cc4d355c79de5934` |
| `macos-c-full/evidence.json` | `af4cc238f3323502a8cd70468607c694b991a44825d959c82f9ebbd25cc1638e` |
| `macos-odin/relocated-package/manifest.json` | `91c9cc5124b34471c43b4a5aab8804f2e344793a1c01ab86e4cd1a1aee67d680` |
| `macos-odin/relocated-package/libweekend_db.dylib` | `68267299bb198361056a77f2e3a16c21e52a846527f269b0791b7c996d6319a5` |
| `linux-c/result/evidence.json` | `b1554154ecb0a0ec8ed8c6a254b5737c3b02b3ad224fa46c0c70012b626bff7d` |

The macOS Odin profile covers the five interposed syscall outcomes. Actual
APFS exhaustion below uses the C consumer. The [Linux arm64 Odin profile](#optimized-odin-consumer-on-linux-arm64)
also exercises actual tmpfs exhaustion.

### Actual APFS append exhaustion against the dylib

Add `--apfs-full` to run all five existing cases and the actual disk-full
case against the same verified package:

```sh
cargo xtask faults delivered --macos --apfs-full \
  --package target/private-package --output target/macos-delivered-apfs
```

The controller uses the [owned 256 MiB APFS image](maintenance-space-exhaustion.md#native-macos-near-cap-copy-profile).
The consumer seeds current revision 3 and retained revision 2. The controller
fills its own ballast until kernel `ENOSPC`. The exercising process loads the
existing shim in observation mode and checks what the dylib actually receives
from `pwrite`. The independent oracle requires error 28, zero synthetic hits,
no root or witness write, no publication flush, `IO` and a cleared output
revision. The exercising process still reads the acknowledged revision 3.

A fresh consumer opens the database with the ballast in place and verifies
every current and retained payload byte. The controller checks the file hash
and kernel exhaustion before and after that open. After it removes only the
ballast, a fourth process finds request 42 absent, retries once and reopens
revision 4 with retained revision 2. Only the exercise process loads the
shim. The loader log binds each phase to the exact relocated dylib, and the
controller verifies disk-image detachment and removal.

All six cases passed with the clean combined `6053e8c` release package on
macOS 27.0.1 arm64, Rust 1.99.1 and Apple clang 21.0.0. For the append
case, APFS reported 5,849,088 available bytes before and after the fresh
open, with a real `ENOSPC` on the 4 KiB ballast probe. The file hash stayed
identical from seed through the failed commit and full-volume reopen.
Removing the ballast restored 266,764,288 available bytes. The retry
reopened 4 with retained revision 2. All four consumer processes loaded the
same artifact, and only the exercise loaded the observer. The volume and
process cleanup checks passed.

Independent review rehashed all 103 listed files, checked the loader logs
and outcomes, and compared the original and relocated dylib and manifest
byte for byte. The local proof is `target/macos-append-proof/local`. The
same shared fixture passed all six Linux regressions against exact `f70e405`,
including kernel tmpfs exhaustion. The hosted Windows rerun for `6053e8c`
passed the six injected DLL cases and the actual NTFS exhaustion case.
All 211 listed Windows files rehashed and the two DLL copies matched.
[Windows injected cases](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43728606525/artifacts/14167139786)
· [Windows exhaustion case](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43728606525/artifacts/14167656243)

| Evidence | SHA-256 |
|---|---|
| Dylib | `839bdb55ba9d4d7a874b24963b3921f60aa2615b72739ff8f5bf33aefe5fea5f` |
| Package manifest | `e0063b6297260b42d59a51f5ecbd3fafabe9c15a837dbd623a9246835192a653` |
| Combined macOS evidence | `84609b4f136affecbee3a243faf280a7916ea433688b86d66ed348e05855cb8a` |
| APFS phase evidence | `0cd9c2d2e63297cb9d9d71c96d9a5d2c9c529f4c59383a0db15de459ed832785` |
| Independent rehash | `7f9b8af10c1c09851f3267deb9b038d8d30a1da11159269cd16e357198ac82f8` |
| Seed, failed commit and full reopen image | `077f9eb0545e4c5dc02af46c06b06604d034073838d045874e50860d5a04886b` |
| Retry image | `8397e1f8bb05e6caf2e30278b9f27cfe9e983b5c8f61a1153aea0d1a551c6812` |
| Linux regression evidence | `b13a52f29771e90d1ba69912435ac7e0149eb67d1188710736bd77e49805a196` |

## Optimized Odin consumer on Linux arm64

Pass the unpacked Linux compiler to the Docker runner. Its toolchain directory
is mounted read-only with the compiler, core library and runtime files. The
controller checks the worker's compiler digest, consumer language and exact
package manifest before accepting the copied proof.

```sh
image=sha256:bda5ef43469a7d63b240adcbc5a5b0599be442bce204d12936f6083224f56b18
cargo xtask faults delivered --image "$image" --odin /path/to/linux/odin \
  --package target/linux-package --output target/linux-odin-faults
cargo xtask faults delivered --image "$image" \
  --package target/linux-package --output target/linux-c-faults
```

The Linux Odin and C consumers use the same guarded ballast helper. It requires
explicit runner authorization, an actual tmpfs of at most 8 MiB and exclusive
creation of the ballast file. The two consumers independently check their ABI
outputs and data. In the exhaustion case the commit receives kernel errno 28,
returns `IO` with zero output revision and leaves the live view at revision 3.
A fresh process in this run opens revision 3 after the exercise removes its ballast.
Request 42 is absent, so one retry commits revision 4 and preserves retained
revision 2. The independent oracle also requires zero injection hits, root-slot
writes, witness writes and sync calls for that case.

The other five cases use the worker's ext4 volume and the same independent
oracle and expected revisions as the Linux C table above. The Odin consumer
uses the packaged bindings and `-o:speed`, checks the unsigned request marker
and every current and retained payload byte, and rejects the disabled injector
at the commit-status comparison. The recovery processes open the exact
relocated shared library with no interposer loaded.

The private combined-feature packages and runner were built from clean
`b4a88a5517c579407aa1cff928a3d1cf411b6304`. The Docker worker ran on Linux
arm64 with Rust 1.99.0, Clang 16.0.6 and Odin `dev-2026-10-nightly:84bc3fc`.
The pinned Odin archive and compiler digests are in the
[Linux minimum-package ledger](linux-minimum-package.md).
The local proof directory is `target/odin-linux-fault-validation/`. It
includes the Dockerfile and image build log, source archive, package manifests,
shared libraries, loader logs and each phase's observed values.

All six Linux Odin and C result rows matched. The five interposed rows also
matched the native APFS Odin rerun using the macOS package from the same
source. The eight Rust fault-runner controls passed in debug and release,
and strict Clippy passed. Direct guard controls with the built Linux Odin
consumer refused an unarmed 8 MiB tmpfs, an ordinary filesystem and a
9 MiB tmpfs. No ballast file was created in those controls. Both Docker
campaigns verified container and volume removal.

| Evidence under `target/odin-linux-fault-validation/` | SHA-256 |
|---|---|
| `source.tar` | `709d6732c966dc100ab61564ab0e9fede91061b0224d43c037d86ff225e22758` |
| `linux-package/manifest.json` | `36a99ddbf073f43f746386562966ac39e3b487f1c98ef1f95cd2e1601880c27c` |
| `linux-package/libweekend_db.so` | `560b1850013dab6c2936301458ca5679766071fdeb629e8e799294ee582a39be` |
| `linux-odin/evidence.json` | `aaf21533ca144791fd7406d3d5797b356a4c7025b7238a55a3572e2eba21ea54` |
| `linux-odin/result/evidence.json` | `a41e2e60ae507a9e42e906af63a1f3831fa0e004b928de050615af64eb4128a4` |
| `linux-c/result/evidence.json` | `ac9b6edc3a19d3254dbd6494d459e77e7b53555a9e187c4b82944964ada714f8` |
| `macos-odin/evidence.json` | `9728258a65012cb9e39238d9324856c3e42faa6d249b088b29f55a0f26d767db` |

The [Linux x86_64 profile](#optimized-odin-consumer-on-linux-x86_64) runs
the same fault oracle. The [Windows Odin runner](#optimized-odin-consumer-on-windows)
also covers native persistent-volume exhaustion. Physical power
interruption remains a separate gate.

## Optimized Odin consumer on Linux x86_64

The native Linux minimum-toolchain job reuses the package and pinned Odin
compiler from its verified state trace. It checks that the package identifies
the tested source and that the compiler digest matches the package manifest.
The test image uses an amd64 Rust 1.99.0 base pinned by its OCI manifest and
Debian Clang 16.0.6. The campaign runs in an offline container with its own
ext4 volume and an 8 MiB tmpfs. Its proof includes both the image ID and the
loader observations from the phase-specific consumers.

```sh
# From a native x86_64 Linux checkout after linux-msrv --odin succeeds
docker build --platform linux/amd64 \
  --file tools/delivered-faults/linux-x64.Dockerfile \
  --iidfile target/linux-odin-fault-image.txt tools/delivered-faults
cargo xtask faults delivered --image "$(cat target/linux-odin-fault-image.txt)" \
  --odin "$(command -v odin)" --package target/linux-msrv/package \
  --output target/linux-msrv-odin-faults
```

The local Docker Desktop run on arm64 used QEMU to execute the x86_64 Odin
consumer against the previously qualified `10feae4` x86_64 package. The
unmodified consumer and all six phases passed, with the same observations
as on arm64. Actual tmpfs exhaustion returned errno 28 and reopened
revision 3. One retry reopened revision 4 and preserved retained revision 2.
The disabled-shim control exited 65. All 817 artifact hashes rechecked
and the owned container and volume were removed.

Rust's compiler crashed under QEMU, so the local worker used the same
`b265740` Rust source cross-built with Rust 1.99.1 and Zig. Local proof is
under `target/odin-linux-x64-fault-validation/`.

| Local evidence | SHA-256 |
|---|---|
| Exact x86_64 packaged `.so` | `075212f21b1824a9b205533247c8408d21630c3282e93058b8f2693a122b3870` |
| Odin compiler | `49e21d51855d60a618c493e929fe2b236d44de3daeb3a2c2f03f4c8aef566644` |
| Docker image ID | `bf00f5b6b8f43b9efb1b9e11cc887df286b36952f6c28f21fd2bf6004627b7e2` |
| Cross-built worker | `c70aea057b721954364b22083b354cd56ef302f122c22d64b5fc03633e54f543` |
| Result evidence | `ba1e0c98f578b139484817d80ec952e62870e8b4bb56b21811d573a01fa5aa72` |

The native x86_64 job also passed all six cases with matching observations
using its exact same-source package, Rust 1.99.0, Clang 16.0.6 and the pinned
Odin compiler. The tested merge was `aa7b6c405823a1d2794d19e98cbbb2e2df4ed708`,
with `8dce9bd` as its branch parent. The worker reported ext4 on the Linux
7.0.0-1014-aws host. The native archive rehash verified 841 worker
artifacts. The package manifest and shared-library copies were identical and
the worker source matched the branch parent. The owned resources were absent.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43789791365/job/133045376286),
[native evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43789791365/artifacts/14191579902).

| Native evidence | SHA-256 |
|---|---|
| Hosted ZIP | `7db6c4ec92e1780f68f0367962072b22cbb99856163fb1ef3339799b277f8917` |
| Worker image ID | `911ee0fbf30670fc57eea1271b156b975b1295c4fc8ec2481fc80ae18c35f141` |
| Package manifest | `4ffd5af71437ccb6a70fd3b2670271fe6232e0a48c7aa71f5cc886589af5a699` |
| Exact `.so` | `9b1a7ba02a0809dd605ddb3ac207320ef3c246b601278c6dc31b500a2b193747` |
| Worker result | `8493bb52dfb28ab295b162433827b1125744dfe00097147e561d905b23e1a879` |

### Reopen while Linux tmpfs is still full

The runner at `6d9b089` keeps the ballast in place after the failed append.
A second, uninstrumented process loads the exact same `.so` and opens
revision 3. It checks every current and retained payload byte. The runner
independently verifies zero available KiB and an unchanged database hash
after that open. It then removes only the ballast. A third process verifies
that request 42 is absent, commits it once and reopens revision 4 with
retained revision 2. The full and retried database images are saved for review.

Both optimized consumers passed the three distinct process phases on Docker
Linux arm64/ext4 against the exact `b4a88a5` package. The available space
was zero through the full-volume reopen and 8,108 KiB after ballast removal
in both runs. All six outcome rows matched. The eight Rust fault controls
passed in debug and release, strict Clippy passed, and macOS C/APFS
exhaustion and the five Odin injection cases still passed. The hosted
x86_64 result above used the earlier two-process tmpfs case.

| Local evidence under `target/odin-tmpfs-full-reopen/` | SHA-256 |
|---|---|
| `source.tar` | `3a8da845151b999a12720d31ea450189c127715b3757f9caa9568cbdde839c72` |
| `linux-odin/result/evidence.json` | `6aa8180b6533f13ce5bdc93e36d544f14306f4315fea0ff9fbfa8287924484ac` |
| `linux-c/result/evidence.json` | `3943430514630fa3285d1a1363a5c1dcd046d10a5450195ee2df7a1d7e8cdf1a` |
| `macos-c/evidence.json` | `011b80ce2f91ffab0ee18cbc5b75c39b00e5701b7f168bc723ae5b296237500e` |
| `macos-odin/evidence.json` | `140dd0560e7e407587998e36c970b61184cf15d4fd6702d04ca35a43b2008352` |

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
the six injected-fault cases in the same job. [NTFS maintenance copy exhaustion](maintenance-space-exhaustion.md#native-windows-near-cap-copy-profile)
uses the separate Rust fixture. [NTFS owner-quota append recovery](native-quota.md#native-windows-ntfs-owner-quota)
also has a separate native Rust gate. Physical power interruption remains open.
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

## Optimized Odin consumer on Windows

Use an elevated x64 Windows session, a verified combined-feature package
built with `--odin` and the same Odin compiler on `PATH`.

```powershell
$package = 'target/private-package'
$pin = (Get-FileHash "$package/manifest.json" -Algorithm SHA256).Hash.ToLowerInvariant()
cargo xtask windows-faults --odin --package $package --manifest-sha256 $pin --output target/windows-odin-faults
cargo xtask windows-disk-full --odin --package $package --manifest-sha256 $pin --output target/windows-odin-full
```

The consumer builds with `-o:speed` and the packaged bindings and import
library. The controller checks the compiler digest from the package manifest.
Each process attests the path, bytes, ABI and source identity of its loaded
DLL. Only the exercise process loads the private shim. It reuses the C
consumer's import-table interceptors on the exact loaded DLL. The disabled
shim control requires Odin exit 65 and the commit-status diagnostic after
the before-attestation. Fresh processes verify current and retained payloads,
and use request 42 to decide whether to retry.

The same independent Windows oracle checks all six injected outcomes.
For real exhaustion, the controller requires a separate open while the
owned NTFS volume remains full, byte-identical database contents across that
open, a successful retry after freeing space, and verified VHD cleanup.

Local checks built the optimized x64 Odin PE, linked it against the qualified
Windows import library, and built both x64 shim DLLs. Cross-target Rust
Clippy passed. The refactored Odin consumer also passed the existing macOS
and Docker Linux fault profiles.

The native x64 NTFS minimum-toolchain job passed both campaigns with Rust
1.99.0, MSVC 14.51.36231, Windows SDK 10.0.26100.0 and Odin
`dev-2026-10-nightly:84bc3fc`. The tested clean merge was
`a542d004c33fba2308655dd9cc9c1b06548c7e8b`, with PR head `b265740` as its
branch parent. The six fresh-process revisions were **4, 3, 4, 4, 3, 3**.
The disabled shim exited 65 at the commit-status comparison.

Actual NTFS exhaustion returned native error 112 with zero synthetic hits.
The failed commit cleared the output revision and left revision 3 visible.
A second process reopened revision 3 while the volume still reported zero
free bytes and the database hash stayed fixed. Releasing the ballast
restored 53,506,048 available bytes. A third process found request 42 absent,
retried once and reopened revision 4. Retained revision 2 survived every
phase and the owned VHD cleanup passed.

The archive rehash verified 1,729 listed files and 31 before/after DLL
attestations. Both native campaigns used byte-identical copies of the same
manifest and DLL. The source hashes for the Odin consumer, attestation
and shim matched the branch parent.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43789019576/job/133042994358),
[native evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43789019576/artifacts/14192330183).

| Evidence | SHA-256 |
|---|---|
| Hosted ZIP | `fc34ff43f406941f0ef32084decb206010c273f87d52e6dbc5cfa3203bddeaba` |
| Package manifest | `b7aa125b9918b4b058a321ee39d3c1d03e7fb354fa97de023864e7ba5427d649` |
| Exact DLL | `844f8c075dd91f3b37fe4fd1bca485db9e44f7930a8d24c356dc2370b38e301c` |
| Injected fault evidence | `949f26faf75d84d6a95fa0ff0577aaf21b3ebe207dc9fb92808be70786851dec` |
| NTFS exhaustion evidence | `ccf7dd2b1a719b23581e9324dca1e5613e556958377bc38cd751dd069f247af2` |
