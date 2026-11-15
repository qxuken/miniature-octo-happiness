# Private Windows package qualification

On an x86_64 Windows MSVC host with Visual Studio C/C++ build tools:

```powershell
cargo xtask package --output target/private-windows-package --features crdt,replication
```

The destination must be new. A clean tree is required by default. `--allow-dirty`
permits a development package whose manifest remains candidate-ineligible even
if every runtime check succeeds. Nothing is published to a registry.

The tool builds in a unique temporary target directory with locked dependencies,
checks the effective panic strategy, and packages `weekend_db.dll`, its
`weekend_db.dll.lib` import library, the C header, project licenses, Cargo lock,
and collected Rust/dependency attribution. It runs `dumpbin /exports` against
the packaged DLL and requires exact agreement with the header's exported names.
Both C and C++ clients compile against the packaged header and import library,
then run beside that exact DLL. The dynamic ABI smoke and source-identity
checks also load the packaged DLL. The manifest records source/configuration
hashes, feature selection, commands, compiler output, MSVC/SDK versions,
compiler/tool binary hashes, artifact hashes, and dependency listings.

The package does not bundle Microsoft runtime DLLs. The Rust artifact must use
dynamic CRT linkage; an effective `crt-static` configuration is rejected. The
clients use `/MD`, which selects the DLL runtime library. Target machines need
compatible installed Microsoft Visual C++ runtime dependencies. See Microsoft's
[runtime option reference](https://learn.microsoft.com/en-us/cpp/build/reference/md-mt-ld-use-run-time-library)
and [Redistributable deployment guidance](https://learn.microsoft.com/en-us/cpp/windows/deploying-visual-cpp-application-by-using-the-vcpp-redistributable-package).

`native-dependencies.txt` and the two client dependency listings distinguish
external runtime prerequisites from redistributed package contents. Choosing to
bundle vendor runtime binaries would require a separate inventory and review;
Microsoft documents the [redistribution conditions](https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files).
The current package makes no blanket assertion that compiler-produced project
code requires bundling vendor licenses. Existing concrete missing attribution
inputs still prevent candidate eligibility without skipping runtime checks.

Every native CI job invokes the private packager and uploads its complete
platform-named package/evidence directory, including failed-check evidence. Rust command and
configuration tests run on all three native platforms. Actual MSVC execution is
qualified by the Windows job; tests running on macOS do not establish Windows
runtime compatibility. Odin exact-artifact qualification remains separate.

## Minimum Rust compiler on native Windows

Use a fresh, byte-preserving checkout on fixed NTFS with the native x64 MSVC
toolchain. The hosted job disables Git line-ending conversion before checkout.
Run the pinned compiler and cache the locked inputs before qualification.

```powershell
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy
cargo +1.99.0 fetch --locked
cargo +1.99.0 fetch --locked --manifest-path crates/weekend-db-typed/tests/renamed/Cargo.toml
cargo +1.99.0 fetch --locked --manifest-path examples/rust_package/Cargo.toml
cargo +1.99.0 xtask windows-msrv --output target/windows-msrv
```

Add `--odin` to the last command to include the native Odin package and
optimized trace. The hosted job installs the Windows x64 build from the
`dev-2026-10` Odin release on `PATH`, checking that the ZIP SHA-256 is
`b6b2e426a8b6f147b74ec7e18bef8df27cf40fe16193159df7ca09a0cc0c2011`.
[Pinned release](https://github.com/odin-lang/Odin/releases/tag/dev-2026-10)

The runner reads the declared minimum from the root manifest and checks the
actual Rust and Cargo versions. It verifies the working copies of every
tracked Cargo lock against the indexed Git blobs before and after qualification.
The standard Windows debug and release profiles run with locked, offline
builds. The same invocation qualifies C/C++ consumers against both workspace
DLLs, builds a private combined-feature package, runs the Rust consumer from
its copied source, and exercises the relocated package's exact DLL with the
existing 134-cell C/C++ trace. If the package contains a verified Odin
consumer, the trace requires the same Odin compiler hash. It also runs
all ten cases with optimized Odin. Before and after a successful run,
the Odin process verifies its loaded DLL path and hash, ABI, format,
feature mask and source identity under the same evidence rules as C/C++.
The evidence binds the package manifest, DLL and trace report by their hashes.

The controller runs delivery first once it admits the toolchain and locked
inputs. It records workspace compile-only preparation and test execution
as separate commands, each with a 300-second deadline and owned Windows job
cleanup. At `078a0ba` a single combined release command exhausted that
deadline after building in 2 minutes 42 seconds and entering the
coordinator backup/restore test. Its process cleanup was verified. The
separate deadlines retain the full workspace test suite and preserve the
delivery result if workspace verification later fails.

### Recorded Rust 1.99.0 qualification

PR head `ec08f39` passed on native x64 Windows and fixed NTFS on
2026-11-15. The checkout was the clean PR test merge
`6962da64360adf6ae359bca7ae21c5024229c9c2`. Rustc and Cargo reported
1.99.0. Rustc reported commit `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`
and LLVM 23.1.1. The C/C++ compiler was MSVC 14.51.36231 with SDK
10.0.26100.0. [Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43734689131/job/132874586809)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43734689131/artifacts/14170102057).

All 13 tracked locks matched their Git blobs before and after the run.
Formatting, strict workspace Clippy, the renamed typed consumer, Rust
examples, ABI smoke and native C/C++ consumers passed. The debug and release
all-features suites each reported 511 passes across 60 test summaries.
The optimized private package passed relocation, and the copied-source Rust
consumer verified typed indexes, CRDT convergence and repeat sync.

All 20 exact-DLL C/C++ cases passed. Ten successful transcripts matched their
independent expectations byte for byte. The other ten returned errors before
creating a database. The independent audit matched all 1,734 downloaded
payload hashes and the stdout, stderr and cleanup records for all 37
top-level commands. All 686 package source hashes matched Git.

The same source checkpoint passed the macOS debug/release workspace and C
checks at 1.99.0, with all 13 locks unchanged. A separate clean arm64 macOS
worktree built its combined-feature private package with 1.99.0. Its C/C++,
Odin and copied-source Rust consumers passed, and all 771 package hashes
were rechecked.

| Evidence | SHA-256 |
|---|---|
| Windows ZIP | `e839e07c8b43d53a0e319c8a150ca4f55df5a97628036a0ebf78564479434e57` |
| Windows controller | `391f1963d0dac9cdfa6aac4311b8c56d83aa05ba71a71a84b03b6c6587577563` |
| Windows package manifest | `7cdc29905c7284873cff1657f08dcccb4d4d0b28329f2fd5b28097899e41edf1` |
| Windows delivered DLL | `3fbe30f56425b210146460e0945d30eb22f9b1fe3008d8add6980687d14caeca` |
| Windows delivered trace | `0b920f81c66cf264e6fb9f7dcdc2f50824e803df6838e1fbc1e732d09e263137` |
| macOS workspace check log | `38f65cfc08498b04a00f21b2cf74d058783b2d0fd46add6e2b62130070d27f59` |
| macOS package manifest | `f3f55f1857e1d7c3fd38d6b42bcc3a1939bbe3b2fc910d8c66d995adad528f64` |

### Recorded native Windows Odin result

PR head `bf7cc30` passed on native Windows x64 with fixed NTFS. The
hosted checkout was `fd161270963a42f69dfa221c9bf924166f7dabf0` and used
Rust/Cargo 1.99.0, MSVC 14.51.36231, Windows SDK 10.0.26100.0 and Odin
`dev-2026-10-nightly:84bc3fc`. Both the relocated Odin smoke consumer and
the copied-source Rust consumer passed. [Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43741573249/job/132895675756)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43741573249/artifacts/14172524854).

All 30 exact-DLL cases passed across optimized C11, C++17 and Odin.
Each consumer matched the full 134-cell oracle and four shorter expected
transcripts. All 15 rejections left the destination absent. The five Odin
rejections covered invalid validation mode, unknown trace version, wrong
DLL hash, wrong loaded path and wrong source ID. The format and validation
controls reached the initial loader attestation. The three identity
controls left both attestation files absent. Successful runs recorded
matching process IDs and exact loaded paths, DLL digests, ABI, format,
features and source before and after running the database operations.

The independent audit checked 1,814 artifact hashes, 689 package source
hashes against Git, all 13 original locks and the output and cleanup
records of 39 controller commands and 33 trace commands. The standalone
Odin compiler and license hashes matched the pinned upstream ZIP. Each
workspace profile recorded 516 passes across 60 test summaries. Release
compilation took 219.52 seconds and execution took 87.66 seconds under
their independent deadlines. The macOS regression with the earlier
`ec08f39` package also matched the 134-cell oracle in Rust, C and Odin
after the shared Odin driver change.

| Evidence | SHA-256 |
|---|---|
| Windows ZIP | `599daccd7475e3c2bd13529e1b5f43f8c478fde7e85368f69d459420ad0a741b` |
| Windows controller | `4fa90ecbf941c677db18d82ffe3b5cfff66ae7f3f71ab02ea901947d4a002df8` |
| Package manifest | `13702b3e17623bfe4b03cd265af78ce3cf7c972ee2e317bbc9ed1f61c515c9f7` |
| Delivered DLL | `23250b1fd6cec0704867ab382a58d3a8d76027796389e40dc86d0f025d457c95` |
| Delivered trace | `3b514ff4e7db1ff0976609283d77d24c44348ae4b6d1a10363cb51d8687a1718` |
| Odin compiler | `19e23dbe5153cbb317e3efdac1a43951a524e775ac9054dcb937fec48b8f2d5b` |
| Common 134-line transcript | `0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3` |
| Independent audit | `5c8a6d1e49742f1df2ea00375423fbc7c327cf697ca292fff51c2cd75ce47f11` |
