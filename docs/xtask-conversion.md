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
| Core entry points | check/build/package/cleanup, shared bounded POSIX supervision, native and delivered fault parent/worker | Rust-only, including delivered package and phase oracles; remaining consumers of the Python supervisor adapter are tracked in later batches |
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

## Native and delivered fault oracle retirement

Both fault commands now run entirely in Rust. The delivered worker uses a strict,
independent six-mode oracle and runs its directed negative controls before the
real consumer. Exercise cannot claim recovery: phase key sets, integer identities,
completion booleans and every observed status/revision/injection counter must
match independently. The new package gate verifies exact source IDs, complete
hash inventories and safe regular paths. Duplicate JSON keys, symlinks/reparse
inputs, self-hashed/escaping paths, excessive JSON/file/entry/depth budgets and
payload corruption reject. There is no engine import in either oracle.

Three retired Python controls map to actual Rust tests, taking the frozen mapping
to **41/179**. Four files are removed, leaving **46** maintained Python files.
The Rust runner and its tests have no remaining Python subprocess; historical
Python command strings remain only in the golden check-plan comparison. Mac
debug/release controls pass **85 tests** (78 unit plus seven integration), and
strict native/MSVC cross-target Clippy passes. The remaining Python registry
passes **272 checks plus four registered skips across 36 workers**.

Actual Docker execution qualifies the old exact `06d0a15` Linux shared library
with today's staged Rust runner: all six C exercise processes and six independent
recovery processes pass, with retained revision 2 and successful retry to 4.
Reopened revisions by mode are **4, 3, 4, 4, 3, 3**. Visible revisions remain
**4, 3, 3, 3, 3, 3**, and status codes **0, 14, 15, 15, 15, 14**. The disabled
injector still fails at the required C assertion (SIGABRT), preventing vacuous
injection success. Loader paths, inputs, package, binaries, copied artifacts and
owned Docker absence all verify. Native permission, append/backup/restore/compact
ENOSPC and retry also pass against frozen engine `570c7df` plus the explicit
current driver overlay. This is bounded native-error/process evidence, separate
from device power-loss qualification and follower durability acknowledgment.

The runner sources were staged before qualification, as required by the existing
runner snapshot gate; archived engine/package and runner identities remain
independent. Local proof under `target/xtask-fault-oracles/` (SHA-256):

- Delivered parent: `c19327927a0936ac80f4e19d30edd47294f0e9853950f65fc654cdeb53248682`.
- Delivered worker: `b338d35f95d5c3dae048a78071c236753c6024536746a4d00a9f79a66115fc21`.
- Native drill: `9785a2122e7255317dc37d2928004ddb8a700a7b84b7d56e23ddda5b2ada2e41`.
- Remaining helper registry: `c3ae64e4592ef310d2cdf772d80650d724885debfe7ae868e76b569131e997c3`.

## Independent POSIX delivered trace conversion

`cargo xtask artifact-trace` replaces the POSIX Python controller and Docker
worker. Its independent Rust model imports neither engine nor FFI code. Frozen
operation fixtures and hand-computed controls preserve the 59-cell v1 and
134-cell v2 formats, all conflict/terminal/history outcomes, and exact original
trace/transcript SHA-256 values. Native Rust rebuilds only delivered private
source, using its original consumer manifest and lockfile plus an explicit
current driver overlay. Optimized C/Odin load the unchanged relocated library;
loader records must identify that exact path. Input/package/runner hashes and
dependency paths remain checked independently.

The Docker worker builds the staged current Rust runner offline, from a read-only
seed and registry cache, on an owned ext4 volume. It has no original-checkout
mount. Its detached provenance explicitly leaves Git identity/dirty state unknown;
it attests source hashes and the actual worker executable instead. The host
verifies frozen inputs, copied package inventories, result hashes, transcript
hashes and independent container/volume absence. Vendor configuration preserves
existing Cargo aliases, supports the legacy config filename, and rejects source
replacement or ambiguous configuration rather than overwriting it. Build caches
are pruned before evidence hashing, including nested copied worker caches.

The prior 300-second build/execution deadlines, 64 MiB native child-file cap and
16 MiB per-stream limit remain; the stream limit is now enforced during capture.
The shared diagnostic campaign latches cancellation across commands: a real
SIGTERM between commands prevents further work, still permits explicit resource
retirement, and forces failed/unverified terminal evidence. A subprocess regression
checks this gap without signaling parallel tests. Separate controls check both
stream caps, inherited file limits, output admission, package corruption/path
escapes, detached identity and cleanup supervision. Existing fault timeout,
output-limit and 15-second cancellation/retirement regressions remain intact.

All 14 retired POSIX trace controls map to discovered Rust tests, taking the
frozen mapping to **55/179**. Three Python files are removed, leaving **43**.
The Windows trace controller, its frozen Python model and its 12 controls remain
until that separate port is verified. Mac debug/release runner suites pass
**103 tests** (96 unit plus seven integration); strict native/MSVC cross-target
Clippy passes. The remaining Python helper registry passes **244 checks plus four
registered skips across 34 workers** with pinned Python 3.14.8.

Native Mac Rust/C/Odin and Docker Linux Rust/C each match all 134 expected lines
against the exact clean `06d0a15` packages qualified earlier. Current runner
sources are separately recorded as `0ccdb2b-dirty`; no clean-current-source or
Windows trace qualification is implied. Linux verifies ext4 for both build and
runtime, original checkout absence, preserved Cargo aliases, copied frozen state
and owned resource removal. This is finite operation/package evidence, separate
from physical power-loss qualification and follower durability acknowledgment.
No crate is published.

Local proof under `target/xtask-artifact-trace/` (SHA-256):

- Mac Rust/C/Odin: `68b6981caf9bdb399b07d422daf3082d0c69efc78032ce36a51f44129f644f7e`.
- Linux parent: `24e815238c04230cf30cac22c737e97f55b8f39e139604f4908b84494581fa96`.
- Linux worker: `ab9e2c72668d9f1e132bdbaa9905fbb3cdcf8e603f517b8fb4952cc973c1274b`.
- Remaining helper registry: `cead4ebc9263b8d7bb758966f672b012ad9223b143a5dbae1d0684d471939300`.

Clean `0ccdb2b` subsequently passed all hosted engine/fuzz jobs, including
TSan, Rust coverage and native Windows/private packaging. This new trace port
retains its own pending hosted gates. Earlier local development invocations and
failed command logs remain separate from the final qualified proofs.

Hosted `fec3bcf` passed native Mac/Linux, both POSIX helper jobs, address sanitizers,
C-ASan/models, quota and coverage. Native Windows exposed CRLF conversion of the
two frozen `.expected` fixtures, causing exactly the v1/v2 fixture comparisons to
fail while independently rendered hashes and hand-computed cases passed. The
fixtures now have an explicit Git `eol=lf` policy; their bytes and expected hashes
are unchanged. A fresh Git index/checkout regression with `core.autocrlf=true`
verifies both original hashes and absence of CR bytes. All nine model controls
pass locally. The new Windows hosted rerun remains the native execution gate.

## Windows Rust trace port: native qualification pending

The next port adds Windows-only `cargo xtask windows-trace`, reusing the frozen
independent Rust model. Its portable policy controls cover the pinned clean x64
MSVC manifest, complete payload inventory, Windows device/path/case aliases,
backslash keys, strict loaded-DLL attestations, feature profiles and compiler
overrides. It schedules the same seven trace cases plus three identity rejections
for each optimized C/C++ consumer: 20 actual executions remain required.

The native process boundary creates each child suspended, assigns a
`KILL_ON_JOB_CLOSE` job, verifies/resumes its single primary thread, bounds both
streams and deadlines, terminates the entire job on every outcome, and queries
zero active processes before reporting cleanup. Ordinary nonzero exits remain
distinct from timeout/output/read/retirement failures. MSVC discovery also uses
this ownership boundary, with a shared 120-second discovery deadline and bounded
in-memory output; raw environment values never enter files, logs or errors. A
Windows-only sentinel regression checks successful and failed private capture.

Mac debug/release controls pass 116 tests (109 unit plus seven integration), and
strict native/MSVC cross-target Clippy passes. These are portable execution and
cross-compilation results, not native Windows process/DLL qualification. The
workflow now runs all 20 exact-DLL cases after private package qualification and
uploads the proof separately; its terminal summary includes the package pin,
actual DLL hash and evidence hash. Crates remain unpublished. The Python Windows
controller/model/transport and its suite are intentionally retained until that
native gate passes, so the retirement ledger remains 55/179 with 43 Python files.

The preceding LF-fixture fix at `cad7154` has now passed the actual Windows
validation, full checks, C/C++ consumers and clean private package/relocation job.
Its fuzz job also passes; the hosted TSan job was still pending at this observation.

## Windows artifact trace migration verified

The native Windows job for PR head `b0e0d73` passed Rust process/private-capture
controls, engine checks, private exact-package relocation and all **20 actual
C/C++ executions** on x64 NTFS. The job emitted the exact manifest pin, DLL hash,
source identity and proof hash and uploaded the separate trace evidence. Those
values and the reproduction command are in the
[Windows trace report](windows-artifact-trace.md#rust-runner-and-native-ci-2026-11-15).
The actual package source is the CI PR merge checkout. This proof does not replace
or recreate the earlier Windows agent's archived package proof.

The 12 legacy controls now map to Rust, including exact model bytes, native
process limits/exit classification, compiler isolation and package/attestation
rejections. The regression suite also tests a real hashed junction on Windows.
The old Windows Python controller, model, controls and MSVC JSON transport are
removed, together with the now-unneeded hidden environment-dump CLI. The frozen
ledger tracks **67/179** controls mapped; **39** maintained Python files remain.
The remaining controller families retain their existing gates until converted.

Mac debug/release and native/MSVC cross-target Clippy also pass after removal.
The residual Python helper registry passes **220 checks plus 4 registered skips
across 32 workers** (evidence SHA-256 `b5af2429533b8090c2f7ff2287141b3cd1a8c8a7254a85f20f244bac4ee69824`).
Hosted engine and fuzz runs for `b0e0d73` are fully green, including TSan.

## Benchmark and paired serialization conversion

`cargo xtask benchmark` replaces the release benchmark aggregation script. It
keeps raw sample CSVs and the median/min/max output, rejects inconsistent case,
operation and image-size inventories, and binds the benchmark binary and source.
Three native macOS release samples with compaction pass. Final image sizes remain
3,190,784 bytes before compaction, 86,016 with both roots and 45,056 after release.
Timing samples are diagnostic; the concurrent qualification work used this
execution as a runner check, not a fresh latency baseline.

`cargo xtask serialization-scale` preserves both clean/instrumented lanes, current
and retained-root checks, exact maintenance phase order, limit rejection and
paired image sizes. It archives a selected commit, overlays the three private
driver files, and modifies only the two codec/storage files in the instrumented
copy. Original archive, driver, binaries, logs, actual filesystem and sampled RSS
are recorded separately. The Rust supervisor now samples its owned main child
(approximately 50 ms cadence rather than the former Python 20 ms loop); these
observations are neither exact peaks nor measurements of every process.

On macOS with frozen engine `0f46c2a`, all 12 breadth and 16 history executions
pass. The distinct 2,048-row case rejects epoch eight at 7,806,976 bytes of
remaining append headroom, preserving seven roots and the original physical
sizes. Docker Linux/ext4 repeats all 16 history executions with this engine;
image sizes and DATA counters match, while identity-index probe counts can vary
with allocation addresses. A separate Docker Linux/ext4 run uses historical
`032dadf` and exactly reproduces every original linear VALUE/DATA counter and
before/after image size, including 440,169,243 VALUE comparisons in the limited
case. Frozen inputs, copied result hashes and owned-resource removal verify.

Seven legacy serialization controls now map to Rust (**74/179**). Two maintained
Python files are removed, leaving **37**. The legacy serialization file is still
required byte for byte by the existing compound/resilience runners: the residual
helper check caught this import when it was temporarily removed. It is retained
unmodified until those runners move, so existing frozen resource evidence and
reproduction remain usable. Current serialization instructions use Rust.

Mac release runner controls pass 125 tests (118 unit plus seven integration);
strict native/MSVC cross-target Clippy passes. No crate is published. Local
proof SHA-256 values:

- Mac release benchmark: `49b38f2448aa0014b6bf91e6991bf11a1160b5f35b1efc4bacb14fd4ecdb7312`.
- Mac breadth: `27368984b47f671836232a17e86e2d1f0d18d5351070d7a6cd5c0410a7a9d7dc`.
- Mac history: `3d3d72cd5ac2b855faca7712a3258766bcaa321671c3ab9387a964bf03503d4d`.
- Linux history controller: `29f505f40f8edd07f691bca1171076fbbaf6a7f8503b8fd4d0e958aa8f726e84`.
- Linux historical breadth controller: `e97c9c194a41d29db25ec0d32160522380c92fad8cf6595630185a265a359fd6`.

With the still-required shared file restored, the residual Python registry
passes **206 checks plus four registered skips across 30 workers** (SHA-256
`4114be98df0ad177b8657faacf829efec27d21cc94ba2ef51e842a6553d0e8fd`).

## Linux quota controller and maintenance ENOSPC

The Rust quota controller provisions an owned ext4 loop mount and runs the
engine fixture and the calibrator under a numeric UID with cleared capabilities.
It binds both the current checkout and the fixture to the exact source commit.
Cleanup checks the mount and backing image before removal and requires fresh
proof of both absences. The separate Docker command shares the guest calibrator
and independent maintenance oracle on a fixed 256 MiB tmpfs.

The frozen Docker replay at engine source `7d45f4810cc3a40bc610e2daa4914de15e9eac5b`
passed backup, restore and compaction. Each reported errno 28 after 8,388,608
bytes of partial output. Each input reopened with all 64 retained roots while
space was constrained. After retry, the final source reopened at 213 and the
backup at 212. The required-failure control also verified the successful copy
on a non-full filesystem. The copied report, executable and artifact hashes
verified, and resource absence checks passed.

- Docker controller evidence SHA-256 `ef0af7b47a96ba97839a4e70bffeb5b35e8f97a4c563d8d95855e75f265d4aeb`.
- Docker worker evidence SHA-256 `fac068efc66c295cb2a4069521701baa826617ac0fbe50ef9eb8a8c6dc457633`.

The hosted native quota job now invokes the Rust controller for append and
near-cap maintenance. The Python files and their helper registrations stay
available during that first native verification. Windows CI for the preceding
coordinator fixture fix (`7d45f48`) is green, including the full capacity run
and all 20 exact-DLL cases. The engine and fuzz workflows both passed.

The initial hosted Rust run verified the source archive and native quota
setup, then rejected the staged controller binary at the fixture size gate.
The runner and the engine fixture now have separate 128 MiB and 64 MiB limits,
respectively. CI builds the controller in release mode. A Linux control tests
both boundaries, and x64 Linux and macOS strict Clippy checks pass after fixing
a Linux-only redundant closure. The first failing native run remains recorded
as a failed qualification.
