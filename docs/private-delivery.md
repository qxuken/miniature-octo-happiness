# Same-source private delivery qualification

## Four-platform checkpoint at `7e0e44f`

The combined and isolated package matrix passes at tested merge
`7e0e44f98809b5a82a223604f05425dae3c9ab2b`, whose branch parent is
`8ddf502b906befe2ad0364ec2246a309d06752bf`. Its Git tree is
`2a5e4065fac6d49b1943adf05fc38b4afbc9c479` and source archive
SHA-256 is
`c9b156fd4025a00d38351151e53a71358fac1ffcf682aa6c3c8b1e5325fc7d07`.
All four package lanes use Rust and Cargo 1.99.0.

| Lane | Workspace passes per debug and release profile | Skips per profile | Rehashed payloads |
|---|---:|---:|---:|
| macOS arm64/APFS | 698 | 6 | 3,515 |
| Docker Linux arm64/ext4 | 734 | 8 | 3,518 |
| Hosted Linux x64/ext4 | 734 | 8 | 3,510 |
| Hosted Windows x64/NTFS | 562 | 6 | 2,025 |

The strict verifier checks all 16 packages with masks 0, 1, 2 and 3.
Their C11, C++17 and Odin consumers and relocation checks pass.
The combined packages also pass the fresh Rust source consumer and
the exact 134-cell trace. The report binds every manifest, shared
library and transcript to the archived source. It retains each
filesystem's reported mount details.

The hosted ZIPs match their published SHA-256 digests. Linux x64
artifact `14216513014` has digest
`9d2a83e0e5a9978b6dba3a27b43cc1f61fb799ca79d52c6d2caf86162383e5aa`.
Windows x64 artifact `14217930125` has digest
`fc2ee6be4ac255bdee67e80a52dc4092d0ecad71adf3a70a104fc427a16603f1`.
[Linux job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43848281064/job/133233954223),
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43848281064/job/133233954359).

All 13 jobs in that [Engine run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43848281064)
also completed successfully. The same-source macOS workspace ASan and
calibrated TSan runs each pass 698 tests with six skips. Both rebuild the
standard library. The Linux x64 shared-runtime C/Rust ASan calibration
detects both intentional invalid controls, while its memory, thread and
file consumers pass. The macOS TSan calibration detects both intentional
races, and the workspace emits no TSan warning. Its retained ZIP is artifact
`14219040799`, SHA-256
`cdd7c97a328e882ca57d8475d040f59138d32b92481917ad82ce8a8dffd2f644`.
The native macOS job also passes actual APFS publication, maintenance,
namespace and cleanup exhaustion checks.

The retained library-only macOS coverage report recomputes 10,972 of
12,108 lines (90.62%) and 20,229 of 22,755 regions (88.90%). Its compiler
reports zero measured MC/DC conditions; these percentages do not measure
MC/DC or all applications. Independent review matches all 322 scoped
source hashes in each sanitizer/coverage report to the exact Git archive
and checks retained command logs and process reports. ASan/C-ASan use
nightly compiler `5f4effd579500175af844241eb179e756190f1e7`;
TSan uses pinned `3ef0c22cdb86ac42dc6b8f0f1ec36857daa6f2a1` with the
documented patched Darwin runtime. These diagnostic artifacts retain
their compiler and instrumentation scope separately from the MSRV packages.

The `6eb5f1a` verifier also binds three retained macOS microbenchmark
samples and an all-22-target ASan smoke campaign to this exact source.
The smoke records 275 actual executions with 1-run/1-second requested
bounds, including seed initialization. The benchmark uses Rust 1.99.1
and includes compaction. The combined report SHA-256 is
`a38adfb4e29130319bd2adfffe10b81929d604d0b9c2c85c2ae0584412a5bb23`.
Changing a completed execution count or removing a target from the
retained campaign makes the public command reject before success output.

The later [extended campaign](fuzz-qualification-7e0e44f.md) passes all 22
targets with 1,442,896 actual executions under per-target 100,000-run and
120-second bounds. The strict public verifier binds it to the same packages
and benchmark samples. That report SHA-256 is
`a70f827e3e846aa7de73767f7258d01db3ab250a1fb4aabf08cd0de749c77cbc`.
A separate Rust 1.99.1 offline/locked build passes all 29 deterministic fuzz
library tests against the same source. The extended campaign retains all 244
seed identities, mutated corpora, logs and process reports; it sets no general
fuzz-duration or production-performance policy.

The local archive
`target/archived-proof/private-delivery-7e0e44f.tar.zst`
has SHA-256
`63ea2a61ee8fb7298983870d184222db9e27a02add2952c973d5c0925137697c`.
A fresh extraction verifies all 22,192 indexed files. Running the public
strict verifier against its restored matrix, benchmark and extended campaign
reproduces `extended.json` byte for byte. Rechecking the restored sanitizer
and coverage source/process/log evidence also reproduces its reviewed summary.
The archive retains exact source archives and Git history, the verifier and
fuzz runner, actual benchmark executable, original hosted ZIPs, all campaign
corpora and logs, negative controls, and the 29-test stable replay log.

## Retained fuzz evidence

`verify-delivery --fuzz PATH` optionally requires a successful clean-source
`cargo xtask fuzz` campaign at the same full Git revision as all delivery
lanes. The verifier compares the complete scoped source inventory, fuzz
lockfile and seed hashes with the exact Git archive, requires every target
in its fuzz manifest once, and checks ASan commands and successful process
reports with verified cleanup. It recomputes completed counts from the
retained stdout/stderr and matching combined logs, and requires each target's
artifact directory to exist and be empty. Requested run budgets do not count
as completed executions. The report adds the evidence digest and each
target's actual completed count and log digest; omitting `--fuzz` preserves
the existing report structure.

Produce a campaign from the clean candidate with evidence and build outputs
outside tracked source paths, then pass its retained directory alongside
`--source`, `--matrix` and any other delivery options. This gate binds the
retained campaign; it sets no minimum duration, execution count beyond one,
or platform coverage policy. Native platform qualification and sustained
fuzzing remain separate release requirements.

## Retained benchmark evidence

The `0fe8ff7` verifier accepts an optional `--benchmark` directory.
It requires a clean successful report from the same full Git commit
as the four delivered lanes. The scoped source inventory and workspace
lock must match the archived tree. The verifier checks the exact
offline, locked release build and sample commands, successful process
retirement, raw CSV hashes, captured stdout, compaction setting and
recomputed summary.

The existing clean `651654c` four-platform matrix passes with three
new macOS compaction benchmark samples from that same revision.
Removing the optional benchmark field reproduces the historical
report. Its only difference from the archived matrix is the newly
built verifier's hash. An actual changed-source report rejects before
success output. Changing a summary value and updating its reported
hash also rejects because the aggregation disagrees with the raw
samples.

```sh
cargo xtask verify-delivery \
  --source 651654c4641def4f97aa2b4388f08a00f9e15f41 \
  --matrix target/private-delivery-651654c --require-isolated \
  --benchmark target/benchmark-651654c
```

The retained benchmark report SHA-256 is
`6dcfa75d73a879ada3b436adbe4a88bc4b075e63caab325faa60773adfb3a8cf`.
Its recomputed summary SHA-256 is
`931e8de65bba63e590b2b31604da865b97350463255bfefde7ec700c2f47ef98`.
The integrated report SHA-256 is
`3d0dcd02b70768a0294930ee0c24bfa1a7e63a9bd24edd7e56378e4d7772a266`.
The local archive
`target/archived-proof/benchmark-delivery-0fe8ff7.tar.zst`
restores all 65 indexed files with matching checksums. Archive SHA-256
`3e38fa41525bb6fc4a830002d922af491f9ad64f2de0fa5cb4aaa93cdddc6f42`.
The complete delivery trees remain in the separately recorded
`private-delivery-651654c.tar.zst` archive.

The benchmark producer records a binary hash and retains measurements.
The optional verifier binds the source and retained measurement files.
Verifying the actual benchmark binary and qualifying deployment service
budgets require further evidence. The local xtask suite passes 203
unit tests and seven integration tests. Native and Windows MSVC
cross-target strict Clippy pass.

## Combined and isolated matrix at `651654c`

The strict delivery matrix passed at
`651654c4641def4f97aa2b4388f08a00f9e15f41`, the tested merge of
`693aaef6550867db7a8477dc88e77c26c6e44aa1` and
`28d1aa4caef901a5860fafea2d697c98a1f3e66d`. The Git tree is
`061c42cb55c5c81aac750e8ab47ab4467f73dbaa`. The verifier recomputed the
Git archive SHA-256
`1e09fe639e5e9d8b8fcf8b97eeba75c009e57a8fd69aba294ac61ab9a2dce898`.
Every lane used Rust and Cargo 1.99.0.

| Lane | Workspace passes per debug and release profile | Skips per profile | Verified payloads | Cargo and runtime feature masks |
|---|---:|---:|---:|---|
| macOS arm64/APFS | 692 | 6 | 3,471 | 0, 1, 2, 3 |
| Docker Linux arm64/ext4 | 728 | 7 | 3,474 | 0, 1, 2, 3 |
| Hosted Linux x64/ext4 | 728 | 7 | 3,466 | 0, 1, 2, 3 |
| Hosted Windows x64/NTFS | 555 | 6 | 2,003 | 0, 1, 2, 3 |

All 16 packages passed C11, C++17 and Odin consumers and relocation.
The four combined builds also passed their Rust source consumer and
exact 134-cell trace. The Windows trace ran all 30 C/C++/Odin outcomes.
The archived `matrix.json` records the manifest and shared-library hashes
for each profile. A negative control changed the recorded runtime mask
for the real base package to 3 and updated the enclosing hashes. The
verifier rejected the mask. The earlier combined-only archive still
verifies in historical mode and rejects when `--require-isolated` is set.

The two hosted delivery ZIPs matched their GitHub SHA-256 digests before
extraction. Linux x64 artifact `14200955909` had digest
`aefbeb56d18d003987c7f40b634663e88088f96c7378d8788b2affdfd75c1d29`.
Windows x64 artifact `14200064276` had digest
`f26c48bd7352668c70092efe6503648c17d9ce2be00bc86c953a3247492283bd`.
[Linux job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43810815103/job/133111846569),
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43810815103/job/133111845822).

The macOS C and Odin fault runners passed against the same combined
dylib. The C runner also exercised actual APFS append exhaustion.
On Linux arm64 both C and Odin passed the injected cases and actual
tmpfs exhaustion. The hosted Linux x64 Odin runner passed the same six
cases. On Windows x64, Odin passed six publication cases and actual
NTFS exhaustion against the exact combined DLL. In each full-volume
test a fresh process reopened revision 3
with retained revision 2, then retry reached revision 4 after space
was freed. The runner reports bind these cases to the package manifest
and to the rehashed library.

The same Git revision passed the hosted Linux x64 C/Rust AddressSanitizer
diagnostics with four calibration controls and the memory, thread and file
consumers. Both fail-required budget mutations exited with the expected
failure and passed the process-cleanup check. The hosted fuzz smoke and
separate replay step passed. The campaign completed 21,016 actual runs
across 22 targets with 242 unchanged seeds. Both hosted jobs recorded
Rust nightly commit `5f4effd579500175af844241eb179e756190f1e7`.
[C/Rust sanitizer and atomic model](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43810815103/job/133111845774),
[fuzz smoke and replay](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43810815166/job/133111846020).

Native macOS workspace AddressSanitizer passed 692 tests, with six
registered skips in 60 summaries. The run used `nightly-2026-11-12`,
Rust commit `3ef0c22cdb86ac42dc6b8f0f1ec36857daa6f2a1`, and rebuilt
the standard library. The matching ThreadSanitizer workspace run failed.
It reported a read and write of `errno` through Rust's standard library
in separate threads during the Study peer credential-rotation suite.
The earlier writer thread was already finished. The focused
credential-rotation replay passed all four tests.

A standalone Rust program using only `std::fs::read_dir` and
`std::io::Error::last_os_error` across short-lived threads reproduced
the warning in four runs with the same toolchain. The archive contains
the repro, command and logs. The [runtime follow-up](#macos-threadsanitizer-runtime)
isolates the change that clears the warning and keeps the deliberate
race controls effective.

The preceding hosted Linux job at `94c3782` recorded one unexpected
`CliFailure` in the replica-progress malformed-input test. The native
Linux configuration tests at `651654c` passed that test. Four diagnostic
workspace-tool runs on Docker Linux arm64 also passed, leaving the
earlier failure's cause unresolved.
[Preceding Linux job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43810346853/job/133110400654).

The local archive `target/archived-proof/private-delivery-651654c.tar.zst`
retains the Git source, four delivery trees, fault evidence, sanitizer
results and standalone TSan repro, and original hosted ZIPs. Its SHA-256 is
`4cdec52c040c12ed210f5be273b901b104c3fb0b309a7f6c5a64389e88db2a0c`.
Restoring the archive and rerunning the strict verifier reproduced the
complete matrix report byte for byte.

## macOS ThreadSanitizer runtime

The local nightly and the 2026-11-16 Rust nightly share LLVM snapshot
`0a5579eb649d3634a9c30ac873300cc2944fa070`. Its Darwin ARM64 runtime
reports an empty thread-specific data (TSD) range. The LLVM fix in
`4a3ac30` gives the runtime the TSD bounds. Applying that fix alone
still reproduced the standard-library `errno` warning in four runs.
The loader reported the exact patched dylib on every run. A native
probe measured `errno` 52 bytes before the TSD start on macOS 27.0.1.
The earlier instrumentation history at that address can survive the
retirement of its pthread. [LLVM TSD fix](https://github.com/llvm/llvm-project/commit/4a3ac30a53ba0417941cab04f51c6ce8a8a67feb),
[Darwin pthread source](https://github.com/apple/darwin-libpthread/blob/main/src/pthread.c).

The local compiler-rt patch resets the shadow-memory cell for `errno`
when a new macOS arm64 thread starts and the address lies outside the
TSD range. The same instrumented safe executable passed four times with
that patch. A deliberately unsynchronized ordinary write and a race on
a live thread's `errno` still produced reports. The hosted macOS job at
`63a3d17` had reproduced the original warning using its bundled runtime.
[Hosted failure and retained diagnostics](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43813246788/job/133119401849).

The full macOS workspace at `651654c` passed 692 tests with six
registered skips in 60 summaries using the patched runtime. The
runtime SHA-256 was
`0cb807f73e2431a6ed6fee228e235aa75564426a770269d199a9a06025d10597`.
The linker audit checked the runtime and search path in 54 native test
binaries. Cargo compiled one proc-macro test for the host. The
supervisor verified process cleanup and the workspace logs contain
zero ThreadSanitizer warnings.

The ThreadSanitizer workflow selects `nightly-2026-11-12` and rebuilds
compiler-rt from the pinned Rust fork with both patches. The runner
checks the runtime SHA-256, confirms the library loaded in all three
calibration controls, and runs the entire all-feature workspace with
instrumented std. The patch applies only to the diagnostic runtime.

The maintained command passed on a clean `a16d10e` checkout with
the independently rebuilt runtime SHA-256
`d2aad029c8e571d9f3861798eb382f4367ef3e6fa78eae41b2bd071c9f619734`.
All 692 workspace tests passed with six registered skips. The safe
calibration exited successfully, both deliberate race processes
reported the expected warning, and process cleanup verified for every
command. The preceding local run at `730df6f` had no sanitizer warnings
and failed the [sampled-poll fixture deadline](replica-progress.md#sampled-poll-qualification).

The hosted macOS arm64 run at tested merge `2ee9552` passed all three
calibration controls and the full workspace. It recorded the same 692
passes and six skips, with zero workspace TSan warnings and verified
process cleanup. The rehashed hosted ZIP SHA-256 is
`e331f25e2374ebb5da34fbbe3abd4a6328ce6465147ee416e94864b80fadf0c0`.
Its runtime SHA-256 is
`69788c428a6d870b501240428cf88ea0f19b6712f0689165853d614cf6fbaba1`.
[Hosted macOS job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43830635422/job/133174864250).

The local archive
`target/archived-proof/macos-tsan-and-native-controls-a16d10e.tar.zst`
retains the candidate sources, runtime variants and patches, loader
controls, workspace reports, and hosted and local APFS evidence. Its
SHA-256 is
`47994d8ac77ea4acbbcd32ec629f0dc0fffe2cd7ed2ff9ae78a5830fae6a1296`.
After extraction, the runtime files and both APFS gate records
matched their recorded SHA-256 digests.

```sh
mkdir -p target
bash tools/macos-tsan/build-runtime.sh target/macos-tsan-runtime
runtime=target/macos-tsan-runtime
pin=$(awk 'NR == 1 { print $1 }' "$runtime/runtime.sha256")
cargo +1.99.1 xtask sanitizers thread --toolchain nightly-2026-11-12 \
  --runtime "$runtime/libclang_rt.tsan_osx_dynamic.dylib" --runtime-sha256 "$pin"
```

## Four-platform result at `c5970bd`

The minimum-Rust delivery matrix passed at
`c5970bd5a73d1e15471ce4f843c8b1806b5fdac3`, the tested merge of
`693aaef6550867db7a8477dc88e77c26c6e44aa1` and
`adfa390ee947acd5c6b7a9fadbb7d60bda17bb48`. Its tree is
`6f257c7b81691a6c4388b66121638aaf231d3828`. All lanes used Rust and Cargo
1.99.0 with the combined replication and CRDT feature mask (3).

| Lane | Workspace passes per debug and release profile | Skips per profile | Verified payloads | Shared library SHA-256 |
|---|---:|---:|---:|---|
| macOS arm64/APFS | 686 | 6 | 3,358 | `40c2d659493e7ffe8c7b337350ca2ec0bbbcfadc626220d80b9db95ad4795981` |
| Docker Linux arm64/ext4 | 722 | 7 | 3,361 | `fbdcb298abc0b2d3ed8e3d1d1b544af55b20c4013fc5d45c413f102c375c4b7c` |
| Hosted Linux x64/ext4 | 722 | 7 | 3,355 | `ea0e07ed95e63e99233a9aa4858232634cd147ab70e74483b1503c81ca5da726` |
| Hosted Windows x64/NTFS | 550 | 6 | 1,876 | `09db485578bf5b3c4e1e0d634b1f41389528bf6c77f03e5441d18958a43081ce` |

The Rust verifier independently checks the 720 tracked files and 13
locks against the Git archive, rehashes the controller, package and trace
payloads, and binds each trace to the delivered library and Odin compiler.
The macOS and Linux Rust, C and Odin transcripts and the Windows C, C++
and Odin full traces matched the same 134-cell oracle byte for byte.
Windows passed all 30 outcomes, including the legacy formats, whole-
collection modes and five rejection cases per consumer. Odin's Windows
assertions return `0xc000001d`; the verifier checks the nonzero status,
empty stdout and the expected absence of a database and final
attestation. A substituted Git source and a modified dylib each caused
the verifier to exit with no success report.

The hosted minimum jobs supplied ZIP `14199198921` (Linux x64, SHA-256
`ab364551b003010e757dc88c82cd7f451e7aa6d7c1d41b713846cfa238acb39d`)
and ZIP `14200036176` (Windows x64, SHA-256
`042ab82fcd9420222a8441fb5f7e5850c0e3f0abd72cd846a43387c08b591ff2`).
Both downloads matched the recorded digests before extraction.
[Linux job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43808059882/job/133103261390),
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43808059882/job/133103261687).

The same macOS source passed package relocation for the three isolated
feature masks (0, 1 and 2). The combined dylib passed five injected cases
each through C and Odin. The C test also exhausted an owned APFS volume
and observed kernel `ENOSPC` with zero injection hits. Reopening while
full recovered revision 3 and retained revision 2. After space was freed,
retry advanced to revision 4 with retained revision 2 still readable.

Place the four extracted evidence directories directly under one
directory named by `--matrix`. Use the Git bundle in the proof archive
to recover the tested merge object if needed. Run the verifier from the
checkout that contains the command.

```sh
cargo +1.99.0 xtask verify-delivery \
  --source c5970bd5a73d1e15471ce4f843c8b1806b5fdac3 \
  --matrix /path/to/extracted-proof
```

The local archive `target/archived-proof/private-delivery-c5970bd.tar.zst`
retains the source archive and bundle, both original hosted ZIPs, all
four evidence trees and the macOS fault runs. Its SHA-256 is
`10c1c9787b4bcb23adfa9df43ad31f5ce1b472aaf355caa4a04371ff62872758`.
Re-extraction reproduced the complete verifier report byte for byte.

This result covers combined private delivery across the four lanes and
the isolated feature masks on macOS. Isolated Linux and Windows feature
profiles and the final candidate's sanitizer, fuzz, workload, recovery
and owner approval gates retain their own evidence requirements.

## Collecting isolated feature packages

Pass `--isolated` to the native `macos-msrv`, `linux-msrv` or
`windows-msrv` command with `--odin`. Each controller adds
`isolated/base`, `isolated/replication` and `isolated/crdt` to its
evidence tree. Each package runs the exact shared library with C11,
C++17 and Odin before and after relocation, and records the source,
compiler, artifact hashes and Cargo/runtime masks (0, 1 and 2).
The combined package exercises the optimized trace and relocated Rust
source consumer.

Use `verify-delivery --require-isolated` when checking the four trees.
This requires all three mask-specific manifests and libraries on each
platform and checks their toolchain, lockfile, source and Odin compiler
against the combined package for that platform. Omit the flag for
historical archives with only the combined package.

## Earlier arm64 result at `384ca0a`

The macOS arm64/APFS and Docker Linux arm64/ext4 lanes ran from clean
`384ca0a225b46da863094192701d32a26409e73d` with Rust and Cargo 1.99.0.
The new macOS command uses the same checks and output structure as
`linux-msrv` and `windows-msrv`. It records the native arm64 architecture
and APFS runtime volume. A different compiler or a temporary directory
inside the checkout rejects before packaging.

| Evidence | macOS arm64/APFS | Docker Linux arm64/ext4 |
|---|---:|---:|
| Workspace passes per debug and release profile | 686 | 722 |
| Workspace skips per profile | 6 | 7 |
| Bounded controller commands | 51 | 52 |
| Controller payload hashes | 3,354 | 3,357 |
| Locked inputs | 13 | 13 |

Both lanes passed strict workspace Clippy and the debug and release
examples, compiled C11, C++17 and Odin package consumers, and built the
Rust consumer from the shipped source. The independent audit matched all
719 source-file hashes against the same Git archive. It rehashed controller,
package and trace payloads, checked process cleanup, and matched both
relocated copies of each library and manifest. Each optimized Rust, C
and Odin transcript matched the same 134-cell oracle byte for byte,
with SHA-256 `0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3`.

| Combined package and trace SHA-256 | macOS arm64 | Linux arm64 |
|---|---|---|
| Controller | `db9f75b5c6b09ce5dd42b1863d8dc19bfb02bd3db4424cc9ff78858f860b9a80` | `64cd74ec574afdfb036687095a6724e7a3548fdb44a61a77fb98b0adb08a28f8` |
| Package manifest | `e66ea9049fcb5957b68e6c48dbf1cc39d64645d98d65ad76e4d1adf87c96cdc9` | `330de5475ec971fa56b81b84341aa9c8c0cd180b5a84275facf00b2e86cf986e` |
| Shared library | `8568bda3fe7be4c23e2cbb7b470319540492de0620f40e28ac1d77436698717e` | `3a34774b2fc77563aaa8fd4992dacc09bdf8d66d862d2318dcd03ead1ab89a2a` |
| Trace report | `ece6e188adfe69c78bb37baa80dd1867a197dab55236cc2d5b214e2b23377031` | `0c5515dcaff010f023594b35c0bc68e812efbb79c1a5f0789b202439b778c4ed` |

## macOS feature flags and failure recovery

The base, replication-only and CRDT-only packages pass C11 and C++17
smoke and relocation. Cargo and the exact runtime agree on masks 0, 1
and 2 respectively. The combined package reports mask 3 and carries the
Rust source and Odin checks above.

| Isolated macOS package | Manifest SHA-256 |
|---|---|
| Base | `ca360100e14a9be78eb6528d84e647c4184ff616be54f05f454a1db142d3dcb1` |
| Replication | `591eee22f37778faecff72be669adda1202873decdc5822101db6837b950acf4` |
| CRDT | `3cb645d94325b7887a21a1b89e6bf7f715ab67b5c2fdbc24156be603c5baf060` |

C and Odin each pass five injected publication and recovery cases against
the combined dylib directly above. C also receives real kernel `ENOSPC`
on an owned APFS image, with no injection hits. A fresh process reopens
revision 3 with retained revision 2 while still full. The retry reaches
revision 4 with retained revision 2. The owned volume and process cleanup
checks passed. The C and Odin evidence hashes are
`c41a515666088ce6aaadd9990e2623fe620e99bbff7944d495b32c390a22c5fe` and
`f8bd9861354888db9d73257b8f1ed6ed9acbff6bb7cd2f6de0da8c98ac08ee32`.

## Reproduction

Use the exact clean checkout and select the declared Rust compiler with
rustfmt and Clippy. Cache the workspace, renamed typed consumer and
private Rust consumer locks before the offline run. Supply the native Odin
compiler on `PATH`. Choose fresh output directories. On Mac, keep the
temporary directory on APFS and outside the checkout.

```sh
cargo +1.99.0 xtask macos-msrv --odin --output target/macos-msrv
cargo +1.99.0 xtask package --output target/macos-base
cargo +1.99.0 xtask package --features replication --output target/macos-replication
cargo +1.99.0 xtask package --features crdt --output target/macos-crdt
cargo +1.99.0 xtask faults delivered --macos --apfs-full \
  --package target/macos-msrv/package --output target/macos-c-faults
cargo +1.99.0 xtask faults delivered --macos --odin "$(command -v odin)" \
  --package target/macos-msrv/package --output target/macos-odin-faults
```

The Linux command was `cargo xtask linux-msrv --odin --output /tmp/evidence`
inside the isolated arm64 image
`sha256:d276cf4b0967c0337be30eb9c302bf3b77794a7ade6b667968ebe5cbc037317c`.
The checkout and temporary directory were on a private ext4 volume.
Mount the locked registry cache at the configured Cargo home and the
account's default Cargo home. The trace deliberately drops the configured
`CARGO_HOME`, and a run with only that mount failed offline metadata
resolution. The rerun used the same cached registry at both paths.
The Linux Odin executable SHA-256 is
`73b9e13f4239aa695de32b8de3513b7b1150d821659920b2f1b837cdb7c2e454`.
The controller records the Mac Odin executable hash in its own manifest.

The local archive `target/archived-proof/private-delivery-384ca0a.tar.zst`
contains the exact Git archive, packages, traces, fault results and
independent audit. Its SHA-256 is
`5f3c308e8624c306477c587c75d39cea687c4589548ce541ac8e7a51d73d76b4`.

These two lanes establish one shared source identity for their packages
and tests. Native x86-64 Linux and Windows runs on other commits retain
their recorded scope. The full candidate also needs the final platform
matrix, feature profiles, sanitizer and fuzz evidence, deployment workload
and recovery evidence, and owner approval.
