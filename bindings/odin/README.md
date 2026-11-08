# Raw Odin binding (C ABI 1)

Copy this directory into an Odin collection and import it as, for example,
`import db "weekend:odin"` with `-collection:weekend=/path/to/bindings`.
Pass `-define:WEEKEND_DB_LIBRARY=...` with a path **relative to this binding
folder**: Odin foreign imports reject absolute library paths. In a private
package that path is `../../libweekend_db.dylib` on macOS,
`../../libweekend_db.so` on Linux, or `../../weekend_db.dll.lib` on MSVC Windows.
Keep the shared library alongside the resulting executable, with
`-extra-linker-flags:-Wl,-rpath,@loader_path` on macOS or the literal
`-extra-linker-flags:-Wl,-rpath,$ORIGIN` on Linux. Windows loads the adjacent DLL.

This module exposes the C function names and numeric constants directly.
Opaque handles are distinct raw pointers; release every acquired handle once.
The packaged `weekend_db.h` is authoritative for ownership, thread exclusion,
buffer sizes/aliasing, status meanings, uncertain commits, and recovery rules.
Database close requires quiescence and returns BUSY for outstanding handles.
Const pointer qualifications express the C contract; Odin does not enforce them.

Paths are explicit-length native bytes on supported macOS/Linux platforms and
UTF-8 on Windows. Embedded NULs are invalid; file creation is exclusive. File
operations are unsupported outside the C adapter's documented platforms.

Regenerate declarations with `python3 tools/odin_bindings.py`; `--check` rejects
stale function signatures/constants against the C header. The parser fails on
unrecognized signature forms instead of silently omitting functions. The
standalone packaged smoke exercises ABI/source identity, bytes with embedded
NUL, buffer sizing, transactions, and retained file reopening; it does not test
every exposed function or establish whole-binding semantic coverage.

`python3 tools/release.py --output NEW_DIRECTORY --odin` packages this binding
and consumer source, requires an installed Odin compiler, records its version
and executable hash, and runs the consumer against the delivered shared
library. Missing Odin is an error when this profile is requested. Existing
wgpu coexistence examples remain separate.
