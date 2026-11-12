# Linux minimum compiler and newer MC/DC capability probe

These checks extend specific toolchain evidence. They do not qualify a final
candidate or native Windows. Engine/application runtime source, formats and
durability barriers are unchanged. No hosted job, remote push or publication ran.

## Declared minimum on Linux

The full existing `tools/check.py --skip-odin` passed against the exact Git
archive of `a56083c3e518abd5fe9196c51beb7bb4bce9d023`, with no source overlay.
All 606 source files, including the original locks, remained unchanged.
The build used actual Rust/Cargo **1.99.0**, the declared Rust 1.99 minimum:
rustc `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`, LLVM 23.1.1, and Cargo
`5f94df478`. Native execution was aarch64 Linux 7.0.14-linuxkit, Python 3.11.2,
GCC 12.2.0, and a private Docker **ext4** volume identified by `findmnt`.

The run exited zero in 180.486 seconds. It includes formatting, strict
workspace/all-feature/all-target Clippy, workspace debug/release tests,
renamed typed consumer, Rust examples/task CLI, shared-library ABI smoke, and
compiled C memory/file/thread consumers. The log has 110 successful summaries
with 846 reported tests across repeated profiles and doctests; this is neither
a unique-test count nor a coverage denominator. Odin was explicitly excluded
on Linux. The existing checker invokes Cargo with `--offline`, **not**
`--locked`; independent source/lock rehashes establish unchanged inputs.

The local image is
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
It extends the recorded c9f225b tool image with exact Rust 1.99.0, rustfmt and
Clippy; its filesystem layer prefix was checked against that base. The setup
selected a dated compiler without changing the host default. The qualification
container had networking disabled, two CPUs, 3 GiB memory and 256 PIDs. Sources,
build products and native temporary files used its UUID-owned ext4 volume;
the host registry was mounted read-only. Seventy cached package archives for
the original root/renamed-consumer locks matched their lockfile checksums
before and after execution. Compiler/configuration/cache inputs remain trusted.

Logs, source archive, full source hashes, rustc-info, image inspection and
orchestration scripts are retained under
`target/msrv-linux-a56083c-final/`. `qualification-index.json` requires successful
execution **and** both exact Docker resources absent after copy/cleanup.
Fourteen build-artifact hashes were recorded before deleting the volume;
those binaries were not copied to the host and are not claimed as retained or
independently rehashed release artifacts. Private package qualification remains
separate. An earlier attempt lacked the registry cache and stopped during
offline resolution before compilation; it remains under
`target/msrv-linux-a56083c/` as failed setup evidence.

To reproduce, extract the named archive, provision the exact compiler plus
rustfmt/Clippy in the recorded image, supply the unchanged locked registry
inputs, and run this on a fresh private native volume with networking disabled:

```sh
CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true \
  CARGO_TARGET_DIR=/work/build TMPDIR=/work/tmp \
  WEEKEND_DB_SOURCE_ID=a56083c3e518abd5fe9196c51beb7bb4bce9d023 \
  python3 tools/check.py --skip-odin
```

This complements the [Mac minimum-compiler check](local-baseline-ffdf506.md).
It predates the new credential-cutover tests and history-axis driver; those
have their separately recorded Rust 1.99.1 qualification. Windows minimum
compiler, other architectures and a support window remain open.

## Dated nightly still rejects MC/DC instrumentation

Mise installed `rust@nightly-2026-11-12` without activating it globally. The
actual compiler is Rust 1.102.0-nightly,
`3ef0c22cdb86ac42dc6b8f0f1ec36857daa6f2a1`, LLVM 23.1.5,
aarch64-apple-darwin. The existing fail-closed driver ran from the frozen
a56083c source:

```sh
mise exec rust@nightly-2026-11-12 python@3.14.8 -- python tools/coverage.py \
  --mode mcdc --toolchain nightly-2026-11-12 --fixture-only \
  --output target/mcdc-probe-fresh
```

The compiler rejected `-Z coverage-options=mcdc` with exit 1, accepting only
`block`, `branch` and `condition`. The driver records **unsupported** before
fixture compilation, profiles or library execution. `rustc -Z help` also has
no MC/DC-named option. This is a capability observation for this exact compiler,
not a universal statement about Rust or a measured MC/DC result. Existing
manual pairs, mutations and branch/condition counts retain their separate scopes.

Evidence is `target/coverage-probe-mcdc-2026-11-12/`. Its supplemental
`qualification-index.json` rehashes the actual compiler executable, probe record,
help output and all 606 frozen source files against the original archive.
The driver's parent-Git dirty flag reflects concurrent agent work; it is not
used to claim a clean instrumented build. No coverage build or percentage was
produced. These ignored local proofs are not a permanent retention promise.

| Local qualification index | SHA-256 |
|---|---|
| `msrv-linux-a56083c-final/qualification-index.json` | `42ef27c2dd3217fea498a7b9689ceafbcbdfe03eb0d34f3f28775a6b95819f44` |
| `coverage-probe-mcdc-2026-11-12/qualification-index.json` | `23fd13047a5c5506971a512046fe7ac6cb7354ed01f92eee42235a6381200cb0` |
