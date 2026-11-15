# Linux minimum compiler and Odin delivery

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
covers this Linux arm64 checkpoint. The later x86_64 result is recorded
below. The final release candidate remains separate. See the
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

### Recorded native x86_64 and arm64 results

PR head `5a6ffd2d3a22d949abf99c3f804c19e49289f801` passed on native
x86_64 Ubuntu 24.04 with kernel 7.0.0-1014-aws and ext4 runtime data.
The hosted source was `10feae471dda0c65a82e9e70feb35f32af7e7d02`.
Docker arm64 also passed the same command against the byte-identical
PR head with Debian 12, LinuxKit 7.0.14 and private ext4 volume.
Both used Rust/Cargo 1.99.0 and Odin `dev-2026-10-nightly:84bc3fc`.
The hosted ext4 mount used `nobarrier,data=writeback`. These compiler
and delivered-artifact checks qualify the software on the recorded
profiles. Physical power-loss qualification remains open.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43743610769/job/132901951201)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43743610769/artifacts/14173209158).

Each architecture passed 666 tests across 60 summaries in both debug
and release. C11, C++17 and Odin consumers passed package smoke and
relocation. The packaged Rust consumer built from its copied source.
The optimized Rust, C and Odin traces each matched all 134 oracle lines.
On both architectures the C and Odin loader logs confirmed initialization
of the exact relocated `.so`. The rehash matched all 689 source files and
13 locks against Git, all 52 controller commands and 16 trace cleanup
records per platform, and 3,231 x86_64 and 3,237 arm64 payload hashes.

The shared controller also passed on Windows x64/NTFS from the same hosted
source. Both workspace profiles passed and all 30 C/C++/Odin exact-DLL
cases held. The independent audit checked 1,814 Windows payload hashes.
[Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43743610769/job/132901951439)
and [Windows evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43743610769/artifacts/14173088700).

The first controller at `ce57e3f` placed Rust build outputs in an
in-checkout scratch directory. The existing copied-source checker
rejected the dependency records on both architectures. The corrected
controller keeps scratch outside the checkout and retains that checker.
An arm64 negative control sets `TMPDIR` inside the checkout and rejects
at admission. Both scoped Docker containers and volumes were removed
after retrieving the results. Local evidence is under
`target/linux-msrv-ce57e3f/`.

| Evidence | SHA-256 |
|---|---|
| x86_64 hosted ZIP | `f92ea6311c7962eb5cdad5f8cbb675b11716b367cc21b66aa1f9a100dab25484` |
| x86_64 controller | `c10ea0b9672b6941951dfdf0b4d7ed686cb182ef68b930dfc809a79e8759815b` |
| x86_64 package manifest | `444740bc44bfb2c7e406f8c11f6490e64b3b7ec1eb175a2f99547a7feab9a86e` |
| x86_64 delivered `.so` | `075212f21b1824a9b205533247c8408d21630c3282e93058b8f2693a122b3870` |
| x86_64 delivered trace | `fb67b56afb0dda54f3f43009523df9730a0f23ac362572ce7ae51a654996c835` |
| x86_64 Odin compiler | `49e21d51855d60a618c493e929fe2b236d44de3daeb3a2c2f03f4c8aef566644` |
| x86_64 independent audit | `15cf7ee937805becf596f4a65499fe0fd35b8d4f6075c33e32f71a67cf343f70` |
| arm64 controller | `6b9661db2c8492cc52a89bf24f95ca28bf1afd75ad9fe421c5db189b02199e1e` |
| arm64 delivered trace | `ccb27e3a89111a3ef1f276079bd740edcd18dc51c171208950334f88c057d23a` |
| arm64 independent audit | `84e8d2c38172d928bca3603843b737c57f133a68ac2244263d90fcdfec2c64d7` |
| Same-source Windows ZIP | `21d9bd7252513e349a22185ea22039d05f5d6a87cc360867efc7e1598fa24d67` |
