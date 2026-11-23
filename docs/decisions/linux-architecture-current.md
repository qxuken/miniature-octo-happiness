# Linux architecture admission

The Linux file adapter admits x86-64 and AArch64. Other Linux targets
return UnsupportedPlatform from the file factories. The portable engine
can use memory or a caller-supplied ByteStore on those targets.

| Guard | Supported AArch64 | Emulated RISC-V 64 |
| --- | --- | --- |
| Entropy in Database create | Create a file, commit a record, and reopen the acknowledged revision | Return UnsupportedPlatform before obtaining a path from the caller |
| Path admission in FileStore | Return the appropriate error for invalid, absent and occupied names. Accept a fresh create and an existing read-only open. | Return UnsupportedPlatform for all path shapes. Existing bytes stay intact and no destination appears. |

The RISC-V case checks database open, inspection, backup and compaction
as well as the three file-store modes. The portable tests commit and read
a row using Database::new and commit and reopen the exact revision on the
simulator. The two guard conditions are compile-target constants. Each
tested binary exercises its own literal value. The tests and mutation
results are bound to the architecture of the running executable.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/linux.rs` | `512931ec540b580a4cf83706878f73aeae4358e74fefd1e0b15fd74a3fcf0220` |
| `src/storage/file.rs` | `0cfd7817b5af48fdcddc8d8b521cead6a4a47583c525aadb137c9c72f01264a2` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P | weekend-db::weekend_db | src/storage/linux.rs:84:5– | 3349..3351 | if | identity | Refuse unsupported architecture before the native entropy request. A supported target reaches native creation. |
| P | weekend-db::weekend_db | src/storage/file.rs:175:9– | 5634..5636 | if | new_in | Refuse unsupported architecture before path validation and native admission. A supported target distinguishes invalid, missing, occupied and accepted paths. |

## Verification

Frozen source `dbf19a563ecb518f056d8a8b90edf7b2804f6884` used Rust 1.100.0.
Linux AArch64 on Docker ext4 passed 291 engine and storage tests in each
profile, including the three architecture tests. macOS arm64 passed 277
engine and storage tests in each profile. The portable RISC-V lane passed
53 tests per profile, including transaction validation, retention,
durability, ByteStore, both golden-image fixtures and all three
architecture tests.

The RISC-V executables ran under Debian's QEMU user mode 7.2.22 with
GCC 12.2.0 and the Debian glibc 2.36 RISC-V sysroot. Each Linux target
detected both exact guard reversals in both profiles. The executable
architecture was checked from the ELF headers and the test runner
reported both target-specific test executions. Strict Clippy passed on
macOS and Linux AArch64, and for the cross-target RISC-V architecture test.

The two compile-target guards now have dispositions. The inventory adds
the integration test to the list of excluded test targets. Its production
decision count remains 10,588, with 9,445 open records
(2,076 runtime, 26 driver and 7,343 tooling). x86-64 shares the accepted
source guard. The fresh Linux evidence here covers AArch64 and emulated
RISC-V. Native Win32, remaining Unix descriptor failures and deployed
filesystem and device behavior retain their own gates.

### Reproduction on Linux

Install Debian's `qemu-user`, `gcc-riscv64-linux-gnu` and
`libc6-dev-riscv64-cross`. Build the host mutation runner before setting
the cross target. Run the test and mutation commands with and without
`--release`. Use distinct output directories for those proof runs.

```sh
cargo +1.100.0 build --offline --locked -p xtask --target-dir target/xtask
rustup target add --toolchain 1.100.0 riscv64gc-unknown-linux-gnu

cargo +1.100.0 test --offline --locked -p weekend-db --all-features \
  --test linux_architecture
target/xtask/debug/xtask mutations --toolchain 1.100.0 \
  --case r1-linux-architecture- --output target/linux-architecture-native

export CARGO_BUILD_TARGET=riscv64gc-unknown-linux-gnu
export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_GNU_LINKER=riscv64-linux-gnu-gcc
export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_GNU_RUNNER='qemu-riscv64 -L /usr/riscv64-linux-gnu'

cargo +1.100.0 test --offline --locked -p weekend-db --all-features \
  --test linux_architecture --test transactions --test retention \
  --test durability --test bytestore
cargo +1.100.0 test --offline --locked -p weekend-db --all-features \
  --lib storage::codec::tests::serialization_
target/xtask/debug/xtask mutations --toolchain 1.100.0 \
  --case r1-linux-architecture- --output target/linux-architecture-riscv64
```
