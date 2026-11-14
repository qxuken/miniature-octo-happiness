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
cargo xtask clean --dry-run
```

`build -r -F crdt,replication`, `package -o NEW-DIR`, and `clean -n` are short
forms. Unknown commands/options, missing values and incompatible feature/profile
options reject through clap before any package or cleanup output is created.
Every failed check/build subprocess stops the command and returns a failure.
No command publishes to crates.io or a container registry.

Rust/Cargo 1.99+ and Python 3.11+ are required for these profiles; qualification
uses Rust/Cargo 1.99.1 and Mise-pinned Python 3.14.8. `WEEKEND_PYTHON` selects the
Python interpreter. `tools/check.py` and `tools/release.py` forward to the new
commands and preserve the Python interpreter used by the caller. The alias
builds xtask offline/locked into `target/xtask`; the initial fetch supplies its
locked tooling dependencies. There are 20 private project manifests, all with
`publish=false`.

## Existing checks and package contract

The check planner preserves the original Python pipeline: formatting, strict
workspace Clippy, debug/release workspace tests, task CLI demo, renamed-dependency
consumer, memory/raw/typed/simulated Rust examples, workspace builds, shared-library
ABI smoke, Rust/native file examples and C memory/thread/file consumers. Mac
also runs both Odin examples with the static wgpu archive unless explicitly
skipped. Windows C/C++ compilation remains a separate native CI step.

Golden command plans were captured from the executed `e5c4d95:tools/check.py`
under six platform/option profiles. The test compares every command and argument,
normalizing only path separators and the new isolated-Python `-I -B` flags:

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
retain their existing meanings. Rust owns the lifecycle; Python's tested
platform-command, configuration, attribution, Odin-binding, ctypes ABI and
delivered-source validators remain leaf checks. The bridge does not log helper
stdin/stdout, including the Windows process environment; only selected policy
results and hashed overrides enter the manifest. Checkers run with isolated
Python so `PYTHONOPTIMIZE` cannot remove ABI assertions.
An isolated source request requires a nonempty image and explicit successful
native/isolation results from the checker. Missing, false or malformed success
flags reject qualification rather than silently omitting a requested profile.

The library, macros, apps, storage/wire formats, durability barriers and separate
follower-durable acknowledgment are unchanged. This batch begins the migration;
it does not eliminate Python. Bounded/fault-injection runners and independent
oracles remain in Python pending their own migration and regression replay.

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

Next: migrate bounded process supervision and fault-runner orchestration while
replaying their cleanup/deadline/output-bound controls; then consider retiring
Python leaf policies/oracles with independent cross-checks. Windows execution
and final-candidate consolidation retain their source/platform-specific gates.
