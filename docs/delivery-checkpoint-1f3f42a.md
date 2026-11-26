# Arm64 private delivery checkpoint at 1f3f42a

The same clean commit `1f3f42afad0d759b6f692824f750528315b4878a` built
all eight packages with Rust 1.100.0. The macOS lane ran natively on APFS.
The Linux arm64 lane used Docker with the checkout, build, temporary files
and data on ext4. Both platforms exercised the base, replication, CRDT and
combined masks. The build-derived mask and the runtime mask matched for
each package.

| Platform | Mask | Package files checked | Exact shared library SHA-256 |
| --- | ---: | ---: | --- |
| macOS arm64 | 0 | 22 | `ff04b10e099936976765eb31fe1b27e758e869ce612e2b0d2990e708f77db235` |
| macOS arm64 | 1 | 42 | `1ac2a48be3b650c80cbf59e731c18eb0ff2bf5e8b3aee5132e22acbb159798f6` |
| macOS arm64 | 2 | 33 | `d1e4072545542a7df5142118c1587781166a44a47cdc1e3c1dab2ef2a4a9586c` |
| macOS arm64 | 3 | 1,155 | `8997c7a7e00c270cc88a22c1298351c19f913937287062c1086d841eab4998d2` |
| Linux arm64 | 0 | 22 | `fecce054f90ba809639e5cec32d88f5cdb54109fc09be54df7d90b7cc56ae793` |
| Linux arm64 | 1 | 42 | `d27a12c09ba8df02873bdf3d3c407a4ca4a1dc98398e8f1eda621ed87021b731` |
| Linux arm64 | 2 | 33 | `e2920bfeacaeddcb7cfbc949e5450642a5f54c065fc7ff246f7a7f46bc7f824f` |
| Linux arm64 | 3 | 1,155 | `4bd6e05fe8bda21053e60fd050df185d49a666c25cb5c518aaf5151e1a5fcb0f` |

Both combined packages passed all 134 trace cells. Optimized Rust, C and
Odin produced the same transcript as the checked-in oracle. The macOS
minimum-compiler run also passed 1,107 workspace tests per build profile,
with eight ignores in each, strict Clippy, and the native example and ABI
checks. Linux executed the four packaged consumer profiles and the combined
trace.

After an independent extraction, all 24 packaged C, C++ and Odin consumers
passed. Loader records point to the exact extracted libraries and the ABI
probe verifies the expected source ID and mask in each of the eight copies.
After running the consumers the restored package files still match the
original byte inventories. Linux restoration used a fresh container with
networking disabled.

## Proof and scope

The local archive `target/archived-proof/private-delivery-arm64-1f3f42a.tar.zst`
includes the Git bundle, package and trace artifacts, native restore inputs,
loader records, tool binaries and the independent checker. Fresh extraction
verifies all 9,722 indexed files. Cloning the extracted bundle and running
`node verify.mjs /path/to/clean-checkout` verifies 1,070 original source files,
2,504 delivered package files, 6,896 trace file entries and 216 files under
the notice directories. The checker binds the source, mask and exact library
to each package.
It also checks the terminal macOS command records and all restored consumer
loader records. The 14 lock blobs exactly match the [November 25 review](locked-dependency-review-afffd2d.md).
The delivered package notices were checked against their package manifests.

The first Linux run used an image without the linker expected by the Odin
compiler and stopped during the first Odin build. The recorded pass uses
the installed tool image with Clang 16
`sha256:bda5ef43469a7d63b240adcbc5a5b0599be442bce204d12936f6083224f56b18`.
The main verification used a cached Rust 1.100.0 toolchain, the locked
offline registry and the pinned Odin compiler. The exact commands and
both attempts' logs are retained.
[Archive, verifier and report checksums](decisions/private-delivery-arm64-1f3f42a.sha256)

This covers the two arm64 delivery lanes on the recorded commit. The
[earlier four-platform checkpoint](delivery-checkpoint-49ddab5.md) retains
its own source. Same-source x64 delivery, final release qualification and
off-machine proof restoration remain open.

## Hosted checkpoint at 9e5ebee

PR #82's preceding head `9e5ebeeb5fcf968539cba432906cf58bb5cfddef` completed
28 successful checks, including bounded fuzzing, and one planned skip for
candidate compatibility. The engine workflow completed its 27 jobs and
the one skip. The archived test merge `a0aff2d04c98c4ac4eeedeaaeafe33dfd6d9253c`
has the same tree as that head. The terminal records preserve all job
conclusions, including ThreadSanitizer and the R1 shards.
[Hosted CI checksums](decisions/hosted-ci-9e5ebee.sha256)
[Engine run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45765188872)
[Fuzz run](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45765188623)
