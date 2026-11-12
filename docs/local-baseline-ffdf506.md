# Minimum compiler, dependency and quota-capability qualification

These local checks are independently bound to committed source
`ffdf506fcdb347b787d66c1e8194d15206dc71a2` or the explicitly named kernel/advisory
snapshot. They do not qualify a new production release, later source edits or
native Windows. No repository, crate or artifact was published.

## Declared minimum compiler

The manifests declare `rust-version = "1.99"`; README requires Rust/Cargo 1.99+.
Mise installed the exact 1.99.0 toolchain without changing the default project
selection. A safely extracted Git archive ran the complete existing
`tools/check.py` under Rust 1.99.0 (`b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`,
LLVM 23.1.1) on macOS 27.0.1 arm64/APFS. The check exited zero in 237.933 seconds.
All 590 archived files stayed unchanged. No driver or engine overlay was used.

The checks include formatting, strict workspace/all-feature/all-target Clippy,
workspace debug and release tests, renamed typed consumer, Rust examples/task
CLI, shared-library ABI smoke, compiled C memory/thread/file consumers and Odin
memory/file consumers linked with static wgpu-native. The log contains 110
successful test summaries totaling 838 reported tests across profiles, including
repeated tests/doctests; this is not a unique-test or coverage denominator.
Cargo commands in the existing check script use `--offline`, **not** `--locked`;
original lockfile bytes were independently checked unchanged before/after.

This establishes the declared minimum on this Mac configuration. It does not
establish 1.99.0 behavior on Linux/Windows, every downstream build configuration,
compiler security equivalence to later patches, or a support period. Private
release packages remain separately qualified with their own compiler inputs.

Evidence: `target/msrv-ffdf506/evidence.json`, SHA-256
`df06e37a961dd6649d2e2715141ca01072de283c25f9fc01d625f758f8749819`;
`check.log`, SHA-256
`45400174e0a028568a10fadd3d67fe12e04b8aed4e71a9de20d964a87791e70d`.
Independent review rehashed the Git archive, all source files, log and Cargo
rustc-info. A separately labeled post-run tool inventory records installed
Cargo 1.99.0, Python 3.14.8, Apple Clang 21 and Odin dev-2026-10:a22aa1984;
it is corroboration of the unchanged installation, not a substitute for the
execution log. Reproduce from a frozen checkout with isolated targets:

```sh
CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true \
  mise exec rust@1.99.0 python@3.14.8 -- python tools/check.py
```

## Locked dependency review

Mise-pinned cargo-audit 0.22.2 scanned all twelve tracked Cargo.lock files at
ffdf506 against an immutable archive of RustSec advisory database commit
`af7b3ec33460b8e2de8d422e16d628518891a519`, containing 1,490 advisories. Every
scan exited zero with no reported vulnerabilities or warnings and no ignored
advisories, platform filter or severity filter. The database is the snapshot
fetched by the initial default cargo-audit run on 2026-11-13; subsequent scans
used `--no-fetch --db FROZEN_DATABASE --deny warnings`.

| Lockfile | Reported dependency count | Vulnerabilities / warnings |
|---|---:|---:|
| `Cargo.lock` | 77 | 0 / 0 |
| `crates/weekend-db-typed/tests/renamed/Cargo.lock` | 8 | 0 / 0 |
| `examples/rust_package/Cargo.lock` | 8 | 0 / 0 |
| `fuzz/Cargo.lock` | 26 | 0 / 0 |
| `probe/rust/Cargo.lock` | 1 | 0 / 0 |
| `tools/atomic-model/Cargo.lock` | 31 | 0 / 0 |
| `tools/decision-inventory/Cargo.lock` | 23 | 0 / 0 |
| `tools/monthly-workload/Cargo.lock` | 8 | 0 / 0 |
| `tools/resource-envelope/Cargo.lock` | 2 | 0 / 0 |
| `tools/serialization-scale/Cargo.lock` | 2 | 0 / 0 |
| `tools/soak/Cargo.lock` | 2 | 0 / 0 |
| `tools/sqlite-baseline/Cargo.lock` | 2 | 0 / 0 |

Counts are per lockfile and overlap; do not add them as unique packages. These
are database matches, not a source/unsafe audit, compiler/OS security guarantee,
license approval, or promise that no undisclosed defect exists. New tool
lockfiles added after this checkpoint require a supplemental scan.

With `--no-fetch`, cargo-audit's JSON reports null Git commit/time metadata even
for this database. The final wrapper therefore safely extracted the exact Git
archive, recorded its hash and every one of its 1,509 files, supplied that path
explicitly, and rehashed it after all scans. Original lockfiles and the actual
cargo-audit executable were also rehashed. An initial successful scan without
that independent database binding is retained as unqualified preliminary output;
a first bound wrapper stopped before scanning on a Mise lookup issue. Neither
is relabeled as the final qualification.

Final local index: `target/dependency-review-ffdf506-final/index.json`, SHA-256
`8ae9ef90435b025543b0804dcccf6afa561552a5992e838a16e4213459f510f1`.
Individual JSON/stderr records and the frozen advisory files remain adjacent. A
supplemental scan of the later private replica-progress fixture lockfile at
`1c0b72fcd7c7300c733c169a75bc75fd4cdcbf2f` also passed against the same frozen
snapshot with no reported vulnerability/warning and unchanged lock bytes;
`supplemental-progress-index.json` and its JSON/stderr are retained beside the
main index. The twelve-file table above keeps its original checkpoint scope.
Independent review regenerated the advisory archive and verified all source,
report, settings and tool hashes. It found no mismatch.

## Actual quota capability remains unqualified

The available Docker Linux kernel is `7.0.14-linuxkit`. A bounded provisioning
attempt used a fresh private 32 MiB file, ext4 with quota metadata, and an
atomically allocated loop device in a disposable privileged container. The mount
returned status 32 before the quota-setting program or any engine operation ran.
The owned loop mapping/container were removed; no database EDQUOT test passed.

A separate read-only kernel configuration query records `CONFIG_QUOTA=y` and
`CONFIG_QUOTACTL=y`, but `CONFIG_QFMT_V1`, `CONFIG_QFMT_V2`, `CONFIG_XFS_QUOTA`
and `CONFIG_TMPFS_QUOTA` are unset. This establishes a missing capability for
the attempted quota profile; the generic mount error alone is not a unique-cause
diagnosis. Changing a container's userspace cannot qualify that missing kernel
configuration. Native quota handling still needs execution on a quota-capable
Linux filesystem/kernel. ENOSPC tests and synthetic `QuotaExceeded` category
mapping are separate evidence and do not substitute for EDQUOT.

The source image was immutable local Docker image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
with mke2fs 1.47.0. No filesystem was created on a physical device. Capability
and failed-provisioning records are preserved at
`target/quota-capability-ffdf506/index.json`, SHA-256
`42d9c02ddd8bd149271016863f80af7b0ee258b47b69c4528980f6b6a5709b4b`.
Independent review verified its hashes, configuration and pre-engine failure.
These local ignored artifacts are not a permanent retention promise.
