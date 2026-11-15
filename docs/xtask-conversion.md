# Complete Python-to-xtask conversion

User scope (2026-11-15): remove Python from **all maintained tooling**, including
independent validators and tests. Finish this conversion before continuing the
production-readiness roadmap. Keep every crate unpublished. Historical reports
retain their original commands/source IDs as evidence; they are not current
entry points.

## Completion gates

- Every maintained command has a clap-based xtask entry point; no Python runtime
  is needed for check, package, CI, qualification, cleanup or their tests.
- No maintained `.py` files remain. Retired implementations are available in Git
  history. Rust subprocess fixtures replace Python snippets in Rust tests.
- Independent byte/state/artifact oracles remain independent of engine logic.
  Port their controls, negative cases and deterministic fixtures before retiring
  the old implementation; compare outputs where feasible.
- Root and nested tool builds remain offline/locked with pinned toolchains.
  Every output-admission, timeout, byte limit, source/artifact binding and owned
  cleanup gate is carried forward. Windows environment secrets stay in memory.
- Document the old-test to Rust-control mapping and native platform results.
  Run the final command graph, actual artifact consumers, fault/retained-root
  profiles and hosted CI. Cross-compile-only results remain explicitly scoped.
- Update maintained command examples/workflows; keep historical source-bound
  reports honest. No crates.io/container publication command is added.

## Conversion batches

| Batch | Remaining modules | State |
|---|---|---|
| Core entry points | check/build/package/cleanup, shared bounded POSIX supervision, native and delivered fault parent/worker | Check/build/package/cleanup are Rust-only; bounded supervision/fault orchestration is Rust with remaining adapters/oracles pending |
| Packaging leaves and consumers | release policy, MSVC discovery/consumers, attribution, Odin bindings, dynamic ABI smoke, delivered Rust source | Clean Mac/Linux packages qualify at `06d0a15`; hosted Windows passes at `e3ff5ad` and `cfa08d1` |
| CI assurance and packaging experiment | sanitizers, C sanitizers, coverage/executable inventory, fuzz/corpus runner, atomic-model orchestration, mutation runner, packaging probe | Rust implementations and local qualification pass; hosted qualification is recorded per commit below |
| Independent delivery traces | state model, Rust/C/Odin trace controller, Docker worker, Windows 134-cell trace | Pending |
| Upgrade and distributed qualification | storage/snapshot compatibility, snapshot/study CLI upgrades, replica progress, follower backup/restore, coordinator/peer soaks | Pending |
| Resource/workload qualification | baseline/benchmark/monthly workload, serialization scale, resource observer, compound/recovery/growth/cache probes | Pending |
| Native space failures | quota controller/probe/maintenance and tmpfs controller; preserve native EDQUOT vs ENOSPC distinction | Pending |
| Final retirement | helper registry/test mappings, Rust-only subprocess fixtures, Python shims/old tests, CI/docs command inventory, full qualification | Pending |

The starting 70 tracked Python files comprise 69 under `tools/` and
`probe/run.py`.
The earlier 9,907 lines include both implementation and controls. Keep changes
reviewable and commit verified batches; the conversion remains incomplete until
all gates above pass. Tooling test coverage is separate from product MC/DC.


## Packaging conversion checkpoint

Rust now implements package policy, secret-safe MSVC environment capture, exact
C/C++/Odin command planning, bounded attribution, dynamic ABI smoke and delivered
Rust-source closure/path validation. The original six command plans remain golden
controls, with only the Python ABI consumer replaced by the independent Rust
consumer. No database crate is linked into xtask.

`xtask/tests/legacy-controls.json` freezes all **179** Python control IDs from
`065dbdb`, with source hashes. **17** controls from the retired release/Windows
consumer suites map to named Rust tests; the mapping is checked against actual
Rust test discovery. Remaining IDs stay visible with empty mappings until ported.
Eight Python implementation/shim/test files are removed; a small Windows compiler
transport remains only for the unconverted Windows trace caller. Standalone
supervision fixtures use POSIX programs, without a Python dependency.

Local debug/release, strict Clippy and Windows MSVC cross-target checks pass.
The complete Mac command graph passes both profiles, actual C/Rust ABI consumers
and both Odin/wgpu integrations. Development packages pass C/C++ relocation,
exact identity, complete attribution and fresh Rust-source consumers on Mac and
Docker Linux; Mac also runs Odin. Their command logs invoke no Python, their
source IDs explicitly contain `-dirty`, and they remain candidate-ineligible.
The remaining Python registry passes 320 checks plus four registered skips across
48 workers on each platform. Counts decrease only for the retired/mapped suites.
Later source changes and native/candidate results retain their own scope.

Clean `06d0a1593aed8934c1ea4300a766fb790b23c736` packages now pass on native
Mac arm64 and Docker Linux aarch64, both with `verified` and `candidate_eligible`
true. Mac includes C/C++/Odin relocation, a fresh native Rust-source consumer,
and a Linux consumer with the original source unavailable. Linux includes C/C++
relocation and a fresh delivered-source consumer. The Linux controller independently
confirms container and volume absence. Local proof hashes (SHA-256) are:

- Mac package manifest: `63f04a2816eadb13914b7ac544f61e8166d55cc8ad518c9f5f8093ef5bc28e1b`.
- Linux package manifest: `79df88876c88657534aebc3cc34176c3ee67dea47cd91a2ba8a965dc447fbe13`.
- Linux controller: `1eec336597aad3f29e7c7b204f70b3a16cfecc96e3e4cb3b5178d438ee279542`.

Hosted run 43656698020 also passes native Mac/Linux packaging and both remaining
helper registries. Its Windows Odin golden comparison exposes Git's CRLF checkout
conversion. Binding checks now normalize only CRLF to LF, as the former Python
text reader did; generation still emits canonical LF. A filesystem regression
first reproduces the rejection, then verifies CRLF acceptance, declaration and
constant drift rejection, significant whitespace/bare-CR/final-newline rejection,
and exact LF regeneration. The same normalization applies to the embedded golden
fixture. Windows execution remains a hosted gate; cross-target Clippy alone does
not qualify it. This correction does not change any ABI or serialized bytes.

## Sanitizer, model and fuzz conversion checkpoint

Clap commands `sanitizers`, `c-sanitizers`, `atomic-model` and `fuzz` replace their
Python controllers. Shared source-bound campaigns use the existing POSIX
supervisor, with fresh output admission, 64 MiB per-stream ceilings, finite
deadlines, process retirement and terminal JSON even when a command fails.
Expected failures require a nonzero exit and the exact stderr assertion, while
observation/output/deadline/cleanup failures still reject. New process controls
exercise those distinctions and source drift rather than mocking successful
instrumentation. Inherited flags, wrappers and suppression settings are replaced
by controlled sanitizer settings. Instrumented-std workspace tests include xtask.

All three legacy controls from the retired C-sanitizer and fuzz suites map to
discovered Rust tests; **20 of 179** frozen controls are now mapped, with the
remaining 159 visible. Unix-only fuzz controls have an explicit platform scope;
the sanitizer-environment control still runs on Windows. Six more Python files
are removed, leaving **56** maintained files for the remaining batches. Coverage
and mutation measurement, the packaging probe, independent fault/state/artifact
oracles and the final helper controller remain open. This checkpoint is not the
completion of the conversion or product MC/DC.

The Mac debug/release runner suites pass **55 tests** (48 unit, seven integration),
and strict Clippy passes natively and for Windows MSVC. Docker Linux passes the
same debug tests and Clippy. Both platforms execute the two atomic-model baselines
and detect both deliberate mutants by their required assertions. C/Rust ASan
passes both valid and both invalid calibration reads plus all three real C
consumers on Mac (LLVM Clang 23.1.5) and Linux (Clang 16.0.6, nightly-2026-11-01).
The initial Apple-Clang Mac attempt fails Rust runtime linking and remains failed
evidence; it does not qualify ASan. Linux's offline container and volume are
independently absent after copying proof. The source hash maps match across these
Mac/Linux campaigns.

All **22** Mac fuzz targets pass under ASan, totaling **21,391 actual executions**
with 1,000-run/20-second limits. The snapshot-install target completes 407 and the
schema/index target 984; the other twenty complete 1,000 each. Both lockfiles and
reviewed seeds remain stable. These are bounded smoke results, not an extension
of the older million-execution campaign. Source is explicitly `e3ff5ad-dirty`;
later clean/hosted execution retains its own source identity.

The full Mac workspace ASan command also passes through the new Rust runner with
instrumented std, all features, coordinator capacity/restart/backup/restore checks,
and xtask's own controls. Its supervised child exits zero in 217.94 seconds and
retirement is verified. The remaining helper registry passes **314 checks plus
four registered skips across 44 workers** with pinned Python 3.14.8. That registry
is still maintained Python until its later conversion batch; these results do not
claim the complete repository is Python-free. Hosted TSan for this new runner
remains a subsequent exact-commit gate.

Local SHA-256 proof bindings under `target/xtask-full-conversion/`:

| Evidence | SHA-256 |
|---|---|
| Mac model | `0deac24dc8fccc7a733e5c1c19769caa29eb23bc1b4f5a2b8cae870912898b56` |
| Mac C/Rust ASan | `8d18c2d2fa307a06c04e61ed6ef7c96bee613a0856b4e46eaeba4fe7aa0d2d15` |
| Mac fuzz | `7669e99e23715492ce695c9bea2e81d9b9fff9afab99ea4771b73b89cbd8d34a` |
| Linux controller | `4331ecd7dacc909ecf7684f06d70e88a1c5e2e9451e1ef3c2b188ff567e1e97d` |
| Linux C/Rust ASan | `89a27fbf2b29e0a1d60c618ec789cd437d2d5c14a3dbbdd1ed240491be16d874` |
| Linux model | `392f52d33d9c1a9922a317fcfe85f7f0509abc0b464ac38d404fb71080f0d6e5` |
| Mac workspace ASan | `5d2cd6a36cdec8f7dfe80c580f4d64aa610f1bbc6f07be0a19b5909dd7ac4de0` |
| Remaining Mac helper registry | `28624624875c489a4c9b1c941a9686c8cb0da9b881a9136aa20109ddc23fd748` |

Hosted [engine run 43659047228](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43659047228)
and [fuzz run 43659047126](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43659047126)
are fully green at clean `cfa08d1`, including TSan, ASan, C calibration/models,
native Mac/Linux/Windows, private packages, quota and remaining helper controls.
This verifies both the original cancellation-fixture correction and the first
Rust diagnostic-runner batch on hosted platforms. Crates remain private.

## Coverage conversion checkpoint

`cargo xtask coverage` and `coverage-executables` replace both controllers and
their Python boundary suites. **38/179** frozen controls now map to discovered
Rust tests; 141 remain. Four more Python files are removed, leaving **52**.
The new coverage controls preserve exact calibration counts, rejection of missing
or impossible metrics, unsupported MC/DC, warning rejection even at exit zero,
source/fixture/runner binding, Cargo artifact/harness admission, package cwd,
positive final libtest counts, exact parent PIDs, object hashes and terminal
rejection metadata followed by execution of the next target. Child/build profiles
stay unqualified, with no pooled executable percentage. All 72 runner tests pass
on Mac in debug/release; Linux debug and native/MSVC cross-target Clippy pass.

The Mac library comparison runs both old Python and new Rust controllers before
retirement and gets **identical totals**: 9,873/11,120 lines and 18,218/20,966
regions, with the same 135 unit tests. The initial Mac per-parent reports qualify
durability (15 tests, 281/941 branches) and the core unit object (135 tests,
1,017/1,205 branches). Its two child profiles remain unqualified. The condition
fixture measures 2/6, 6/6 and 6/6 branch outcomes; the actual MC/DC request rejects
with `unsupported`, so these results are not compiler-measured MC/DC.

Docker Linux exposes two previously assumed installation paths: Cargo lives under
`/usr/local/cargo`, and the dated nightly emits standard-library paths under
`/usr/local/rustup`. Both initial reports reject outside sources rather than
qualifying them. The runner now adds literal, component-bounded exclusions for
the configured Cargo registry and the selected sysroot's Rust source subtree,
including canonical aliases. Regex metacharacters and sibling paths have directed
controls. The allowed project-source prefixes and outside-source rejection remain
strict. Terminal failure also cannot retain a measured/calibrated success label.

The corrected Linux rerun passes library line measurement (**10,191/11,327** lines,
138 unit tests), condition calibration and both independent parent reports:
durability 281/974 branches, core 1,058/1,248. The core's two child profiles remain
unqualified. Counts differ across OS/compiler profiles and are not combined. The
owned container/volume are independently absent. Rust 1.99.1 supplies line
instrumentation; Linux condition reports use nightly-2026-11-01. LLVM tools are
installed for those exact toolchains. Cargo builds remain offline/locked.

The remaining helper registry passes **278 checks plus four registered skips
across 40 workers**. Current command docs and hosted coverage use Rust. Historical
reports retain their Python source IDs and commands. These are source-hashed
`cfa08d1-dirty` development snapshots; the earlier Mac comparison precedes the final
Docker path correction, while the Linux qualified run and final condition probe
include it. No blanket frozen-candidate or full-workspace coverage claim is made.

Local proof bindings under `target/xtask-coverage-conversion/` (SHA-256):

| Evidence | SHA-256 |
|---|---|
| Old Python Mac line comparison | `761dfd5b37a5ccfd54fabeb82447452f69cf7f9eb3c130d492ddaa92f04922e5` |
| Rust Mac line comparison | `80d338c09d59855028d6e903ba78cd9762902a47eb768d21b0d5ab1056d2bd6a` |
| Initial Mac parent reports | `a9b83510834894dc391c35bbd54c30c6fec5e38c09795fca953a1e1e54fe09fb` |
| Final Mac condition fixture | `eb95a21ac396e6baa8440fdf33882f853cb9d370f6c0d82e0595a80f33cda9f8` |
| Mac unsupported MC/DC probe | `d13737d872e165a3f27147f43340415b22d5e5ff14bf5445a2e22bb6f0338480` |
| Remaining Mac helpers | `3a7cf3a54dabebf5176926d90719d2ae79e5894113a0813b3e808f66c928fa79` |
| Qualified Linux controller | `c77e93b0560a1253c567aec448dcb1f476b8241616688d3f7a28bec8dbfa4f9b` |
| Qualified Linux library line | `ed0344a0bd8c8a220b1d844a8cee0cba14e00a72831a9e7223c4869ed5bcfc65` |
| Qualified Linux condition fixture | `e42fa81e78d7d610af889587ef06caf1eb9e3f0eef63ffcab56e41abf5dc8bd9` |
| Qualified Linux parent reports | `10709b272e69e5263a443dfb01be816726d50fd39a7c4e5c288aa8c29a656df7` |

## Mutation conversion checkpoint

`cargo xtask mutations` replaces the last Python mutation controller. Its 56
reviewed cases are frozen, with original source provenance, in
`xtask/tests/mutations.json`; names, replacements, source occurrence counts,
packages and test commands are preserved. Five new Rust controls check the full
inventory/current expression matches, production-only replacement, copy isolation,
symlink rejection, nonempty baselines and genuine failed-test detection. Compiler
diagnostics merely quoting a failure marker cannot qualify: the failed libtest
footer must be anchored and Cargo must exit 101, with clean process retirement.
Root source and the complete copied input hash map must remain stable after
restoration. Owned package artifacts are cleaned before each mutant and after
restoration, preventing coarse timestamps from reusing a mutant in another case.
All subprocesses retain the 120-second limit and shared output/cleanup gates.

Mac and offline Docker Linux **each pass all 13 distinct baselines and detect all
56 mutants**, with all 680 copied inputs unchanged afterward. This includes
allocation, FFI, transfer/replacement/simulator, value/tree, live budget, both TLS
endpoints and typed runtime/catalog guards. Root inputs remain unchanged. The
Linux container and volume are independently absent. An initial smoke and full
run are followed by a full rerun after the failed-footer predicate is tightened;
the following hashes bind the final reruns. Mac debug/release controls pass
(70 unit plus seven integration tests); strict native/MSVC cross-target Clippy
and Linux debug controls pass. No mutation is applied to the original checkout.

The retired controller had no separate Python test suite, so the frozen legacy
mapping remains **38/179** and the remaining helper registry stays at 141 controls
before its two execution modes/skips. There are now **51** maintained Python
files. The source-bound runs use `60f03c0-dirty`; this is finite directed mutation
evidence, not measured compiler MC/DC or a new production qualification claim.
Current usage and safeguards are in the [mutation guide](mutations.md).

Local proof under `target/xtask-mutations/` (SHA-256):

- Mac final report: `78c263fe1eb5219af1204255855b12acbf54eddd8321d7cbc3ef4fc928defa02`.
- Linux final report: `4f8a96e207ed1adc80bb61a36ee5768d5f3712a8f702096b5574bb5dcb4a14aa`.
- Linux controller: `35e885afe9980348ad635c73586f60d58329cd25390b827a83ab3339632e365b`.

## Packaging experiment conversion checkpoint

`cargo xtask probe` replaces `probe/run.py`; no Python is needed for the macOS
C-versus-Rust experiment. The original eight debug/release static/shared artifacts
still run both C and Rust consumers. Generated C amalgamation, ASan/UBSan contract
and allocation tests, deliberate C/Rust lifetime mistakes, and all plain/wgpu
Odin integrations retain the complete **29-case** inventory. Explicit Odin/wgpu
skips retain their smaller recorded scope. Rust builds are now offline/locked
with an owned target, dynamic consumers have explicit local rpaths, and every
subprocess has bounded output, a deadline and verified retirement. The exact
wgpu archive is hashed before and after execution. Independent controls reject
the expected duplicate-symbol exception for shared Rust, plain Odin, unrelated
link failures, timeout or failed retirement.

The full Mac run passes all 29 outcomes and reproduces only the two expected
Rust-static/wgpu duplicate-symbol failures, in debug and release. Both shared
Rust/wgpu cases pass. C sanitizer checks and both required lifetime diagnostics
pass; all command outcomes are explicitly accounted for. The tested wgpu archive
SHA-256 is `7d8b688a6b55f0728c847e1c3b028d07ccb1009d8cea4833a6186a5775384248`.
This is the original local-path packaging experiment, not an engine or release
artifact qualification. The source-bound development run is `7a2c4b8-dirty`.

Mac and Docker Linux debug/release runner suites pass **79 tests** each (72 unit
plus seven integration); strict native/MSVC cross-target Clippy passes. Linux
does not execute the Mac experiment. Its owned control container and volume are
independently absent. Source binding now includes the probe's C/Rust/Odin inputs
while excluding the retired `probe/build` cache. No separate legacy Python suite
was removed here: the mapping stays 38/179, and **50** Python files remain for
independent delivery/state/fault oracles, distributed/workload/quota qualification
and final helper retirement. Current probe instructions use Rust; historical
reports retain the original entry point and source identity.

Local proof under `target/xtask-probe/` (SHA-256):

- Mac full evidence: `3bfb3cf17199fb3af51a095943f3bf19ccb57dbbb8196180a6e836333025120e`.
- Mac result inventory: `419262ea6f9dfb01db1d13ebf8b8b5627dd7b4a2135d119a567a06b1278b8815`.
- Linux control controller: `d187ca10139231e18b306f64337e540292e447c050081e26a03a3f8b4a7adaa9`.

Hosted engine/fuzz runs at clean `60f03c0` are subsequently fully green,
including the new Rust library-coverage job, both sanitizers and all three native
package platforms. Later mutation/probe commits retain their own CI gates.
