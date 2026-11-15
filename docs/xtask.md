# Private Rust repository commands

The unpublished `xtask` workspace crate owns check, build, private packaging and
cleanup orchestration. Commands use clap's derived parser and generated help.
It is excluded from the workspace's default members; the engine's default
dependency and public API contracts are unchanged. Run from the repository root:

```sh
cargo fetch --locked
cargo xtask --help
cargo xtask check
cargo xtask check --skip-odin
cargo xtask build --release --features crdt,replication
cargo xtask package --output target/private-package --features crdt,replication
cargo xtask package --help
cargo xtask sanitizers address
cargo xtask sanitizers thread
cargo xtask c-sanitizers --cc clang
cargo xtask atomic-model
cargo xtask fuzz --runs 1000 --seconds 20 --output target/fuzz-evidence/private
cargo xtask clean --dry-run
```

`build -r -F crdt,replication`, `package -o NEW-DIR`, and `clean -n` are short
forms. Unknown commands/options, missing values and incompatible feature/profile
options reject through clap before any package or cleanup output is created.
Every failed check/build subprocess stops the command and returns a failure.
No command publishes to crates.io or a container registry.

Rust/Cargo 1.99+ is required for check/build/package/cleanup; these commands no
longer require Python. Qualification pins Rust/Cargo 1.99.1. Remaining tooling
conversion is tracked in the [completion ledger](xtask-conversion.md). The alias
builds xtask offline/locked into `target/xtask`; the initial fetch supplies its
locked tooling dependencies. There are 20 private project manifests, all with
`publish=false`.

## Existing checks and package contract

Sanitizer, fuzz and atomic-model commands run on POSIX through the same bounded
supervisor as fault campaigns. Each requires a fresh evidence directory and
records separate stdout/stderr, exit status, timeout, verified retirement and
source hashes. `--output` chooses another directory for a repeat run. Sanitizers
accept `--toolchain` (default `nightly`); the atomic model also honors
`MODEL_TOOLCHAIN`. ASan/TSan rebuild the standard library and include xtask in
workspace tests. C ASan calibrates valid and deliberately invalid reads on both
sides of the ABI before running memory/thread/file consumers. Choose a compatible
Clang shared runtime explicitly with `--cc`; the local Mac profile uses
`/opt/homebrew/opt/llvm/bin/clang`, since Apple Clang's runtime fails the Rust
link calibration. Inherited flags, wrappers and sanitizer suppressions cannot
override the controlled sanitizer environment. A first instrumented-std build may
need registry inputs; Cargo uses the checked-in lockfile.

Fuzz targets are discovered from the manifest, with repeatable `--target` filters.
Requested run/time budgets do not count as executions: every target must emit a
nonzero completed-run summary and exit successfully. Both lockfiles, reviewed
seeds and source stay bound to the campaign. Model mutants require their exact
stderr assertion and a completed, retired process; timeout or a missing marker
cannot qualify a negative control. Coverage/mutation and the other independent
qualification tools remain tracked in the conversion ledger.

The check planner preserves the original Python pipeline: formatting, strict
workspace Clippy, debug/release workspace tests, task CLI demo, renamed-dependency
consumer, memory/raw/typed/simulated Rust examples, workspace builds, shared-library
ABI smoke, Rust/native file examples and C memory/thread/file consumers. Mac
also runs both Odin examples with the static wgpu archive unless explicitly
skipped. Windows C/C++ compilation remains a separate native CI step.

Golden command plans were captured from the executed `e5c4d95:tools/check.py`
under six platform/option profiles. The test compares every command and argument,
normalizing path separators and the independent Rust ABI consumer that replaces
the former Python ABI smoke command:

| Profile | Commands |
|---|---:|
| Mac, Odin included | 44 |
| Mac, Odin skipped | 36 |
| Linux x86_64/aarch64 | 36 |
| Linux other architecture | 28 |
| Windows | 24 |
| Other platform | 20 |

The package command retains the [private release contract](release-contract.md):
clean-source admission, optional explicitly ineligible dirty builds, source and
lockfile inventories, redacted build-configuration fingerprints, actual compiler
unwind/Windows CRT checks, isolated build directories, exact header/export sets,
C/C++ consumers, ABI and embedded-source identity, native dependency inventories,
and relocation with the original package absent. The temporary library build is
removed **before** consumers execute. Final source/configuration changes reject.
Artifact hashes and commands remain in `manifest.json`; failed qualification
cannot set `verified` or `candidate_eligible` true.

`--odin`, `--rust-source`, `--rust-source-isolated` and `--rust-source-image`
retain their existing meanings. Rust owns the lifecycle and the independent
platform-command, configuration, attribution, Odin-binding, dynamic C-ABI and
delivered-source validators. Windows compiler environment output stays in a
private pipe; only selected identities and hashed overrides enter the manifest.
Checks remain active in debug/release. The old Python modules are retired after
their controls are mapped to Rust.
An isolated source request requires a nonempty image and explicit successful
native/isolation results from the checker. Missing, false or malformed success
flags reject qualification rather than silently omitting a requested profile.

The library, macros, apps, storage/wire formats, durability barriers and separate
follower-durable acknowledgment are unchanged. This batch begins the migration;
remaining non-packaging automation and independent oracles are being converted
under the [complete conversion](xtask-conversion.md). The
[supervision batch](xtask-supervision.md) supplies bounded POSIX ownership.
Historical results below retain their original commands and source revisions.

## Cleanup and retention

The alias reserves `target/xtask` so cleanup keeps its own running executable and
cache, including on Windows. `clean` removes root `debug`/`release`, standard
Cargo metadata and existing nested `target` directories associated with tracked
Cargo manifests. Other root-target directories, including qualification proofs,
are retained. `--proofs` explicitly selects every other root-target entry too.
`--dry-run` performs the same admission checks and lists paths without removal.

All selected paths are checked before deletion. A symlink in a cleanup path,
tracked source beneath a selected output, or the running executable beneath a
selected directory rejects the operation. External `CARGO_TARGET_DIR` values
are honored for check/build and are never followed by cleanup. Other repositories
and Docker resources are not selected. Unit controls test dry runs, proof/runner
retention, nested caches, tracked-source/self protection and symlink escape.

## Qualification and remaining scope

The implementation includes command-plan parity, export mismatch/profile checks,
cleanup safety and real subprocess-failure controls. Native workspace, private
artifact and helper qualification results are recorded in the roadmap checkpoint.
Product coverage excludes `/xtask/` alongside the existing tooling exclusions;
the new tool has its own functional controls, not a claimed MC/DC score.

Linux validation exposed a pre-existing delivered-source false positive for a
short checkout root: `/src` matched a component of `/registry/src/...`. The
oracle now matches absolute Make path tokens, retaining real checkout/sibling
rejection and normalizing Windows extended-path syntax. A regression first fails
against the old oracle, then passes under both Python modes. Windows command
paths use `dunce::canonicalize` for compiler-compatible canonical paths.

The [bounded supervision migration](xtask-supervision.md) carries forward the
cleanup/deadline/output-bound controls and native/exact-delivery fault runners.
Further profile/OS-policy or leaf-oracle migration requires independent
cross-checks. Windows execution and final-candidate consolidation retain their
source/platform-specific gates.

## Verified first-batch results

Checkpoint `964efef0cd2c68bef7f65f1b23b57254debc2764` passes the full native
check/build pipelines on Mac arm64 and Docker Linux aarch64. Independent review
parses the actual execution logs and matches all **44 Mac commands** and
**36 Linux commands** against the original Python plans, including every
argument after the documented path/Python-isolation normalization. Mac includes
both Odin/wgpu integrations. Both helper registries pass **356 controls** and
four registered optional skips (360 reported tests, 52 workers per platform).
All 180 copied helper payload hashes per platform and worker cleanup checks pass.

Review then reproduced an empty-image bypass of the old source-isolation
selection. Checkpoint `c17cd76099336f279d4be731b14f8ee3409b3bf0` rejects the
empty value through clap and requires explicit success for each requested source
profile. The latest Rust control suite passes **11 tests** in debug and release
on Mac, with strict all-target Clippy and formatting. Existing pipeline/helper
results above retain their original source; only package guards/documentation
change in this follow-up. The hardened package profiles are rerun from clean,
frozen `c17cd76` source on both platforms:

| Profile | Result | Manifest SHA-256 |
|---|---|---|
| Mac C/C++/Odin, relocation, native Rust source and Docker source isolation | Verified; packaging candidate eligible | `f9548125a409d62538d4ace9d2659cf2110d407eddc56670b258082ea866c22e` |
| Linux C/C++, relocation and native Rust source | Verified; packaging candidate eligible | `1dd951082fcace18a5a1b7818dc30d3664f82dbc5a5df84fded56f6c5a26cbd8` |

Both package source inventories match all **674** archived input files. Review
recomputes **758 Mac** and **752 Linux** artifact hashes, independently compares
header/export sets, verifies source/header bindings, and checks owned Docker
container/volume absence. The exact Mac library independently reports ABI 1,
format 2 and feature mask 3. Candidate eligibility here means packaging checks
passed; it is not production qualification or publication.

Negative controls refuse dirty source, preserve an existing destination, reject
an actual wrapper-selected `panic=abort`, reject an empty isolation image before
creating output, and reject the wrong embedded source ID even with
`PYTHONOPTIMIZE=2`. Profile-result controls also reject missing, false and
malformed confirmations. Earlier development attempts include a formatter
disagreement, the short-root false positive, and source-stability rejections
when checkers were edited during qualification. Those attempts are retained as
failures; the frozen reruns above supply the qualified results.

The actual cleanup command removes normal and nested caches, retains package
proofs and `target/xtask`, and successfully runs the tool afterward. The main
target decreases from **1,754,324 to 586,052 KiB** (about 572 MiB remaining),
with additional nested caches removed. No other workspace is selected.

Ignored proof is under `target/xtask-qualification/`. Independent review index
`final-review.json` has SHA-256
`37cc65e6444e3188103aeb9118aa57a5626b825222bc7e7dd42fec61928d92fe`.
The source IDs, platform scopes and artifact retention remain explicit; local
ignored evidence is not permanent archival. Native Windows execution of the
new entry points remains for CI. All 20 project manifests stay unpublished.
