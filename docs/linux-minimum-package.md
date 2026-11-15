# Linux arm64 minimum compiler and Odin delivery

Clean source `44bd5a6ca9f313745b62d09083671d7c092ea562` passed the
workspace, combined-feature private package and delivered trace checks
using Rust and Cargo 1.99.0. The two bounded Docker runs used aarch64
Debian 12, Linux 7.0.14-linuxkit, GCC/G++ 12.2.0 and a private ext4 volume
for the checkout, compilation and native test data. Rustc reported commit
`b940084d7eb6a299eb4bfeb8e34901bc051e7ac4` and LLVM 23.1.1.

`cargo xtask check --skip-odin` passed formatting, strict workspace
Clippy, debug and release workspace tests, the task demo, the renamed
consumer, native Rust and C examples, and shared-library ABI checks.
Each workspace profile reported 666 passes and six ignored tests in 60
test summaries, including doctests. All 687 tracked sources and 13 locks
matched the exact Git archive before the check, after it and after delivery.
The standard check uses offline Cargo commands. The package and delivered
Rust consumer builds explicitly use the locked dependencies.

The private `crdt,replication` package exercised C11, C++17 and Odin
consumers and reran all three after deleting its build directory and
renaming the package. The original package path was absent during that
check. The packaged Rust source built and ran its own typed and CRDT
consumer. The packager verified the C exports, source identity and feature
mask 3, and recorded complete attribution inputs. The `.so` advertises
`libweekend_db.so` as its SONAME and the packaged Odin consumer has
`$ORIGIN` in its ELF runpath.

The separate [134-cell trace](artifact-trace-validation-modes.md) matched
all 134 lines of the independent oracle individually in optimized Rust,
C and Odin. Both C and Odin loader logs show initialization of the exact
relocated `.so`. Rust compiled from the copied package source with the
trace driver overlay and original lock. The original checkout was present
for the runner, and compiler dependency records exclude it from the
consumer build. The independent audit matched 772 payload hashes in each
of three package copies, all 2,299 trace payloads and the cleanup records
for all 16 trace commands.

## Reproduce the profile

Start with the named revision in a clean Git checkout on arm64 Linux
with ext4. Select Rust 1.99.0 with rustfmt and Clippy, cache the locked
registry inputs, and install a working C/C++ toolchain. The workspace ran
in `sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
For delivery, the tool image supplied Debian Clang 16.0.6 from
`sha256:25d3b9a14d0b56cb21a43022b08304da91fa669e0297e30984a71f45dab33249`
with the complete 1.99.0 toolchain copied in from the workspace image.
The derived image was
`sha256:a4e81b7bbe846b2dac43c27564b23fa4fd49098a56d8bcf06ec62b13ddf3b142`.
The two runs had networking disabled and mounted the Cargo registry
read-only. The Rust trace used the same registry at the default Cargo
cache path after the runner stripped its explicit override.

Extract the `dev-2026-10` arm64 Odin tarball. Verify the archive hash
below and put its `odin` and the link driver's `clang` on `PATH`. The
compiler reported `dev-2026-10-nightly:84bc3fc` and ran with the license
and runtime files shipped in the tarball. The first package attempt
stopped at compiler discovery until the extracted Odin directory was
added to `PATH`. [Odin release](https://github.com/odin-lang/Odin/releases/tag/dev-2026-10)

```sh
export CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0
cargo xtask check --skip-odin
cargo xtask package --output target/linux-minimum-package \
  --features crdt,replication --odin --rust-source
cargo xtask artifact-trace --package target/linux-minimum-package \
  --odin "$(command -v odin)" --output target/linux-minimum-trace
```

The raw package, trace, workspace log and independent rehash are retained
locally under `target/msrv-linux-44bd5a6/`. Both owned containers and their
volume were removed after the copy and verification. The recorded profile
covers this Linux arm64 checkpoint. Linux x86_64 at the minimum compiler
and the final release candidate remain separate. See the
[native Windows Odin results](windows-private-package.md#recorded-native-windows-odin-result).

| Evidence | SHA-256 |
|---|---|
| `odin-linux-arm64-dev-2026-10.tar.gz` | `99c43e9670a285901d6cfee5f56197b74c7d6f7c537b35ecba0c1b44ac2e4df8` |
| Odin compiler | `73b9e13f4239aa695de32b8de3513b7b1150d821659920b2f1b837cdb7c2e454` |
| Workspace check log | `bb86723465732ff3b722fb066386f954386c056ba3e309ef578358493d93f760` |
| Package manifest | `afbc8ac67e1b292d173820916b026c01d9323365c3bd9955a980a358f8b8e672` |
| Delivered `libweekend_db.so` | `b1d2e9001d5679983b6d24634c18c25f70aad9a6a43b9450d48ae9f3b33d45d6` |
| Trace evidence | `a37093aae2808721c8733a41d8c25d901204a71d52e2a7967b13a0aad46c9df4` |
| Common 134-line transcript | `0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3` |
| Independent audit | `ff984ae3f53cc85f6d752dc784e9744131d061645bbfa450fd9858d75a2cdead` |

## Native Linux minimum-compiler runner

The `linux-msrv` command shares the compiler, lockfile and workspace checks
with `windows-msrv`. On Linux it requires GNU Rust for the running x86_64
or aarch64 architecture and ext4 for the runtime data. It binds the
combined-feature package, original root lock, packaged Rust source,
relocated `.so` and common 134-cell trace in one report. With `--odin`, the
trace must use the same Odin binary as the package. Compilation and
execution of the workspace tests have separate deadlines.
Let `TMPDIR` point to an existing ext4 directory outside the checkout.
The runner creates and retires its own child directory there.

On a clean x86_64 Linux checkout, install the pinned `dev-2026-10` Odin
toolchain from the upstream `odin-linux-amd64-dev-2026-10.tar.gz`
with SHA-256 `c3c8b095621fd0c75f7f73e3a0829f1b4d45324225f20ba11ed8dc4da310a8ab`.
Make its compiler and Clang available on `PATH`. [Odin release](https://github.com/odin-lang/Odin/releases/tag/dev-2026-10)

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy
cargo +1.99.0 fetch --locked
cargo +1.99.0 fetch --locked --manifest-path crates/weekend-db-typed/tests/renamed/Cargo.toml
cargo +1.99.0 fetch --locked --manifest-path examples/rust_package/Cargo.toml
cargo +1.99.0 xtask linux-msrv --odin --output target/linux-msrv
```
