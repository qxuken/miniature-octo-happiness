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
| Packaging leaves and consumers | release policy, MSVC discovery/consumers, attribution, Odin bindings, dynamic ABI smoke, delivered Rust source | Clean Mac/Linux packages qualify at `06d0a15`; native Windows checkout regression fixed, hosted rerun pending |
| CI assurance and packaging experiment | sanitizers, C sanitizers, coverage/executable inventory, fuzz/corpus runner, atomic-model orchestration, mutation runner, `probe/run.py` | Pending |
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
