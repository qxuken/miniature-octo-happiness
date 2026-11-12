# Local qualification of 2490631

This report binds local results to **2490631dab43678279f8ba2c56458c8ea908c1b4**.
Its eight hosted jobs were billing-blocked and executed no tests; the passes below
are local evidence, not a reinterpretation of those hosted results. Repository
visibility and all `publish = false` manifests remain unchanged. No crate or
release artifact was published. Private packaging eligibility means its specific
packaging gates passed, not that the library is production-ready.

Core proof records are preserved locally in the SHA-256-indexed bundle
`target/local-qualification-2490631/index.json`.
Its 17 JSON records retain the original evidence independently of `/tmp` cleanup;
large logs and binaries remain at their recorded original locations. The bundle
is a local ignored artifact, not a published or committed distribution.

## Source and toolchain binding

Mac/Linux full checks used Git archives with SHA-256
`8060e8e4178b2aa34459b05df8a722f7af977b7fb2d4f8d0d2649b8f7c4fba8d`.
The original Cargo.lock hash is
`384d128361f436d847264173af62063e151d93b5396e025459e2066c289f881a`.
Detached worktrees and per-file hashes bind the other runs to the same commit.
Incremental compilation was disabled and separate output/build directories kept
qualification artifacts apart from runtime source. This documentation follows the
checks and does not imply that subsequent runtime edits inherit their results.

Mise selected Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1) and Python 3.14.8 for the
stable local/package work. Nightly tools report their actual separate compiler:
macOS sanitizers, branch coverage and fuzz used Rust 1.101.0-nightly
`708c94ef0` / LLVM 23.1.5. Mac C-ASan used Homebrew Clang 23.1.5. macOS was arm64
27.0.1; Docker Linux was aarch64. Docker image IDs, rather than mutable tags alone,
bind the container environments in the evidence below.

Coverage/sanitizer source manifests report `dirty=true` because their disposable
source directory contained an untracked `target` cache symlink. The tracked diff
was empty. Every recorded coverage source hash (131 files per run) and C-ASan
hash (55 files) was compared with the commit's Git objects. This is explained
provenance, not permission to attribute arbitrary dirty builds to a commit.
See `/tmp/weekend-2490631-source-verification.json` and
`/tmp/weekend-2490631-local-qualification.json`.

## Passed checks and their scope

| Check | Observed result | Evidence |
|---|---|---|
| macOS full check | `python3 tools/check.py` exited 0: formatting, strict Clippy, workspace debug/release tests and Rust/C/Odin consumers | `/tmp/weekend-pr1-2490631-native-evidence.json`, adjacent native log |
| Docker Linux full check | `tools/check.py --skip-odin` exited 0 on a native ext4 build volume; 540 archived files remained unchanged | `/tmp/linux-full-2490631-binding.json`, `/tmp/linux-full-2490631-native.log` |
| Deterministic fuzz replay | 25 tests passed | `/tmp/weekend-fuzz-replay-2490631.log` |
| Bounded ASan fuzz | All 22 targets exited 0; 20,956 actual executions | `/tmp/weekend-fuzz-asan-2490631/evidence.json`, `/tmp/weekend-fuzz-2490631-summary.json` |
| macOS Rust sanitizers | Complete `tools/sanitizers.py address` and `thread` runs exited 0 with instrumented standard library | `/tmp/weekend-2490631-local-qualification.json`, address/thread logs |
| macOS C/Rust shared-runtime ASan | C memory, threaded and native-file consumers passed; both valid controls succeeded and both intentional C/Rust errors were detected | `target/qualification-2490631-c-address/evidence.json` |
| Docker Linux C/Rust shared-runtime ASan | Same three consumers and four calibration controls passed with instrumented Rust standard library | `target/linux-asan-2490631/evidence.json`, `provenance.json` |
| Bounded atomic models on Linux | Two baselines passed; two deliberately broken controls were detected | `target/linux-private-2490631/evidence.json`, `/tmp/linux-package-2490631.log` |
| Four release feature profiles | Base, CRDT, replication and combined builds passed exact runtime feature-bit/source-ID checks and FFI smoke | `target/local-feature-profiles-2490631/evidence.json` |

The fuzz campaign requested 1,000 executions and a 20-second limit per target.
Twenty targets completed 1,000 each; `snapshot_install_schedule` completed 326 and
`schema_index_migration` 630 before their time limits. Source, seeds and lockfile
hashes stayed unchanged. The clean worktree remained clean. This is a finite smoke
campaign, not exhaustion of the input/state spaces.

Linux full-check setup first encountered missing rustfmt/Clippy components and
an EFAULT while creating an archive on a host-bound build directory. Matching
components and a native Docker ext4 build volume resolved those infrastructure
failures; the final unchanged-source run passed. The package/model image was
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.

The original Linux packaging image lacked Clang/nightly, so its evidence still
contains a correctly scoped sanitizer-unavailable note. That limitation was
subsequently addressed by an actual calibrated run, not a Python configuration
test: Linux ASan used dated `nightly-2026-11-01` (`f9ceb4a4d`, LLVM 23.1.4), Debian
Clang 16.0.6 and one shared external Clang ASan runtime. Both valid calibration
processes exited 0; intentional C and Rust errors each exited 86 as required.
The qualified container snapshot was
`sha256:25d3b9a14d0b56cb21a43022b08304da91fa669e0297e30984a71f45dab33249`.
Its source hashes were independently checked against 55 Git objects. These
sanitizer builds are diagnostic configurations, not the release binaries.

## Coverage: metric and exclusions

**Aggregate qualification withdrawn on 2026-11-13.** The
[retained-object audit](historical-coverage-audit.md) reproduces 1,046 line-mode
and 660 branch-mode mapping mismatches. LLVM returned zero while rejecting
mappings; the previous driver incorrectly accepted that as measured coverage.
The table preserves historical printed totals, not valid coverage percentages.
Functional, sanitizer and artifact checks retain their independently stated scope.

Both runs executed the all-feature workspace test suite. Reports exclude paths
under tests/examples/tools, Rust toolchains and registry dependencies. This does
not remove inline unit-test code in production source files from instrumentation,
or turn the report into a production-only semantic decision denominator. Platform
cfgs, macro expansion, monomorphization and compiler version affect the measured
set. These mismatched aggregates cannot establish a trustworthy gap inventory.

| Historical raw output (unqualified) | Covered / total | Printed percentage |
|---|---:|---:|
| Stable line coverage | 13,702 / 14,136 lines | 96.93% |
| Stable region coverage | 25,517 / 26,960 regions | 94.65% |
| Nightly branch coverage | 1,633 / 1,751 branches | 93.26% |

Branch calibration distinguished the partial fixture (2/4 branches) from its
complete fixture (4/4). The stable and nightly denominators differ and are not
merged into one score. Both reports have **zero MC/DC denominator**: neither is a
measured MC/DC result. Directed decision ledgers remain separate source-to-test
evidence with their stated delegations and exclusions. Exact flags, source hashes,
raw metrics and calibration are in
`target/qualification-2490631-coverage-{line,branch}/evidence.json`.

## Private artifact and source-package qualification

The macOS `crdt,replication` package passed C/C++ and Odin consumers against the
exact copied shared library after relocation, runtime ABI/source identity,
symbol/header and attribution gates. Its Rust source/typed/derive/CRDT consumer
compiled and ran natively and inside an isolated Linux container without access
to the original checkout. Manifest `verified` and `candidate_eligible` are true.
Odin was `dev-2026-10:a22aa1984`; the compiler hash is recorded. The Linux source
isolation image was
`sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c`.
Registry dependencies remain lockfile-pinned but **not vendored**; this is not an
offline self-contained dependency distribution. Evidence:
`target/private-2490631-mise/manifest.json`.

The independent Linux base-feature package passed exact C/C++ consumers and
relocation with GCC/G++ 12.2.0, and reports `verified=true` and
`candidate_eligible=true`. Its 540 source hashes and 16 artifact hashes were
checked. The original lockfile was unchanged. Evidence:
`target/linux-private-2490631/package/manifest.json` and its parent evidence.
The 15 release-tool, one Windows-consumer configuration and one C-sanitizer
configuration tests also passed; these Python configuration tests are not native
Windows or sanitizer executions. No Linux Odin package profile was requested.

The separate macOS feature sweep checked bits 0 (base), 2 (CRDT), 1 (replication)
and 3 (combined), using locked offline optimized library builds and exact source
identity plus FFI smoke. It does not substitute for full platform-specific
execution under every feature combination.

## Windows: compiled, not executed

The exact clean commit cross-compiled and linked **47 workspace test executables**
with all features for `x86_64-pc-windows-msvc`. The optimized snapshot CLI also
built as PE32+ x86-64. Mise Rust 1.99.1 used fixed cargo-xwin 0.23.1, cached
Windows SDK 10.0.26100 / CRT 14.44.17.14 and LLVM clang-cl/lld-link 23.1.5.
The missing Windows standard-library target was installed for that pinned Rust
toolchain; no VM or global configuration change was made. Evidence and executable
hashes: `/tmp/weekend-windows-2490631-evidence.json`.

**Zero Windows tests executed for this commit.** No Windows runtime/VM was
available locally. Native Windows/NTFS persistence, alias/reparse handling, file
locking, compact-copy behavior, process interruption, TLS/socket behavior, ACL
assumptions, actual C/C++ consumer execution and exact private DLL/import-library
package relocation remain unverified
for this head. Cross-linking Rust tests cannot close these gaps. Earlier native
Windows passes (including historical 939d) apply only to their recorded revisions;
billing-blocked hosted jobs supply no replacement evidence.

The maintenance CLI's Windows rejection of in-place manifest compaction also
needs execution on Windows. That operation uses explicit fresh-directory reseeding
in the supported Windows workflow.

This report closes the listed local checks, not all production-readiness work.
Device power-loss qualification, longer/broader workloads, remaining decision
pairs and the exact-head native Windows gates remain separate obligations.
