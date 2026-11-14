# Private Windows package qualification

On an x86_64 Windows MSVC host with Visual Studio C/C++ build tools:

```powershell
python tools/release.py --output target/private-windows-package --features crdt,replication
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
then run beside that exact DLL. The existing ctypes smoke and source-identity
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
platform-named package/evidence directory, including failed-check evidence. Python command and
configuration tests run on all three native platforms. Actual MSVC execution is
qualified by the Windows job; tests running on macOS do not establish Windows
runtime compatibility. Odin exact-artifact qualification remains separate.
