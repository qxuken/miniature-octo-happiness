# PR checkpoint qualification at e2d2268

This checkpoint tests merge `e2d226831cbafb380b3a8d4110e662c9ba6d9bed`,
tree `369d47e499f83a0e7af334e473138a8eae9159a5`: branch
`5c5a8e14798067979e889da6b52deac2d61adc09` into base
`693aaef6550867db7a8477dc88e77c26c6e44aa1`. The frozen merge and branch
have the same tree. Subsequent documentation commits do not change these
artifacts' source identity. All 21 Cargo manifests remain `publish = false`.

The work is ready for code review with the
[unfinished production roadmap](remaining-roadmap.md) explicit. Production
release acceptance remains open. This checkpoint refreshes packages, CI,
sanitizers, bounded fuzzing, corpus replay and microbenchmarks; it does not
refresh the historical operator/distributed rehearsals at
[7ec2cb5](candidate-qualification-7ec2cb5.md).

## Four-platform delivery

All sixteen minimum-Rust packages pass the strict delivery verifier: combined
and isolated feature masks 0, 1, 2 and 3 on each lane.

| Lane | Filesystem | Rust | Workspace passes / ignored in each debug and release profile |
|---|---|---|---|
| Native macOS arm64 | APFS | 1.99.0 | 748 / 6 |
| Docker Linux arm64 | ext4 | 1.99.0 | 785 / 8 |
| Hosted Linux x64 | ext4 | 1.99.0 | 785 / 8 |
| Hosted Windows x64 | NTFS | 1.99.0 | 613 / 6 |

Every package passes C11, C++17 and Odin smoke consumers and relocation.
Combined packages also pass fresh Rust source consumers and the independent
134-cell Rust/C/Odin trace. Windows executes the full trace through C++ too;
Unix C++ qualification here is smoke/relocation. Ignored tests retain their
explicit platform/profile reasons and are not counted as passes.

The Linux arm64 checkout, build and runtime use a Docker named ext4 volume,
with source and registry inputs mounted read-only. The container exits zero
and before/after tracked-source checks match. Hosted Linux x64 reports ext4
with `nobarrier`, `data=writeback` and `journal_async_commit`; these are the
actual runner's settings, not a physical durability qualification. The
[snapshot filesystem report](snapshot-flush-filesystem.md) records why the
previous host-shared mount was unsuitable. Zero-progress reads still reject;
no engine check or durability barrier was weakened.

## CI and dynamic checks

All fourteen required jobs pass on attempt 1: thirteen in
[Engine checks 43972698826](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43972698826)
and one in
[bounded fuzzing 43972698853](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43972698853).
Original top-level logs for every job contain the tested merge identity.
The review verifies all 35 downloaded artifact ZIPs against their API SHA-256
digests and retains the original run/job metadata and logs.

These jobs include native Mac/Linux/Windows checks, minimum-Rust Linux x64
and Windows packages, rebuilt-std workspace ASan/TSan, calibrated C/Rust ASan,
Mac/Linux helper controls, Linux quota, Windows owner quota and scoped engine
library coverage. The Windows native job also executes the two RNG guard
mutations in both profiles. Historical compatibility subprocesses keep their
explicit pinned endpoints. Successful CI does not turn their endpoints into
newly qualified candidate upgrades.

Library coverage is scoped line/region evidence. This record makes no new
whole-workspace coverage percentage or measured compiler MC/DC claim.

## Fuzzing, replay and benchmarks

The extended native macOS ASan campaign passes every target under seed 3,
per-target 100,000-run / 120-second bounds, a 2,048 MiB RSS limit and
262,144-byte maximum input. Raw terminal completion lines total
**1,441,898 executions**. Each target exits zero, has verified process cleanup
and leaves its artifact directory empty. The source and all 251 seed inputs
remain unchanged. Stable corpus/model replay separately passes 31 tests in
debug and 31 in release.

| Target | Actual completed executions |
|---|---:|
| open_image_bytes | 100,000 |
| open_structured_image | 100,000 |
| commit_crash | 38,018 |
| txn_schedule | 100,000 |
| value_ops | 100,000 |
| image_and_ops | 48,743 |
| retention_compact_crash | 41,315 |
| ffi_ops | 100,000 |
| crdt_frames | 100,000 |
| crdt_schedule | 44,873 |
| snapshot_frames | 100,000 |
| snapshot_install_schedule | 1,829 |
| schema_index_migration | 7,517 |
| derive_tokens | 66,111 |
| logical_import_export | 100,000 |
| backup_restore | 92,156 |
| bytestore_contract | 100,000 |
| replacement_namespace | 33,443 |
| tree_adversarial_keys | 8,389 |
| coordinator_protocol | 18,035 |
| peer_transport | 100,000 |
| register_causality | 41,469 |

The actual 22 diagnostic executables are retained and independently matched
to the build outputs. The compiler is nightly-2026-11-12 and cargo-fuzz is
0.13.2. These binaries need their matching diagnostic runtime. The fuzz build
does not rebuild std, and bounded execution is not an exhaustive input proof.
Fuzz evidence SHA-256:
`1953e30fa4e4a2e5b7c9d77dd7d6ed86431338c69e6e99b39c33f4a31c69417e`.

Three macOS samples each pass all seventeen compact microbenchmark cases
before the heavier local checks. The verifier recomputes the summary from
retained raw samples. The retained actual benchmark executable has SHA-256
`08f0423d04cdf9d55cec1e56847861fd2b06092924dd2edd68d5ba8b886ab7f7`.
These small-state samples establish no deployment service budget.

The initial local controller/benchmark preparation used the branch checkout
because a login shell changed directories. Source identity checks caught it;
those samples are retained under `preparation/` and receive no candidate
credit. A fresh controller built in the frozen checkout with a non-login
shell produced the qualified samples above. No product failure was hidden
by that correction.

## Evidence review and retention

The strict combined matrix/benchmark/fuzz report has SHA-256
`ab1b686bae5008c3f73e74984976940673d15238d1ab8024677b21ae9d2743f5`.
It binds all sixteen packages and both optional evidence sets to the frozen
merge. A separate raw-data review checks the source/tree and clean checkout,
583 scoped source/seed hashes, actual completion lines, original fuzzer and
benchmark binaries, workspace pass/ignore totals, CI checkout identities and
all 35 artifact digests. The complete checkout has 759 tracked files and
fourteen locks; that scope is distinct from the controller's source inventory.

The local private archive
`target/archived-proof/private-delivery-e2d2268-checkpoint.tar.zst` has SHA-256
`3fd2f04463374bf86afec399492604eaa9193f2b833ebb648759cac4d6198e78`.
Fresh extraction verifies all **21,260 indexed payload files**, totaling
775,583,304 bytes. The relative index SHA-256 is
`9eabfe89748077fcc4bc366f792f4a3dd6cbfa4e0e72cf45c5f9fcb406fd96e1`.
The restored original verifier reproduces the strict combined report above
byte for byte. This is a replay of that verifier, not a fresh independent
compiler build or a rerun of all product tests.

Source tar/bundle, delivered libraries/consumers, actual diagnostic binaries,
logs, corpora, benchmark samples, CI ZIPs and review inputs are retained.
Cargo build caches and the worktree Git pointer are excluded. Reproduction
still requires the recorded compilers, SDKs and matching diagnostic runtimes.
Off-machine custody remains R7 in the unfinished roadmap.

Local commit acknowledgment and follower durable acknowledgment remain
separate. No crate is published and no production-readiness gate is closed
merely by marking the PR ready for review.
