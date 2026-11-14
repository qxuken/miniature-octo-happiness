# Odin private package profile

`python3 tools/release.py --output NEW_DIRECTORY --features crdt,replication --odin`
adds the raw ABI binding, standalone consumer source, compiled consumer, and
Odin toolchain/license evidence to a private native package. Missing Odin fails
qualification. Nothing is published. See [binding installation](../bindings/odin/README.md)
for collection imports and platform-specific library paths.

The generated binding exposes all 49 current C functions and all numeric
status/kind/validation/version constants. `tools/odin_bindings.py --check` and
release tests compare the complete generated declaration file against the
packaged C header's names and type/pointer signatures. Unsupported declaration
forms fail closed. This verifies declaration alignment, not the semantics of
every function. The consumer exercises runtime ABI/format/source identity,
embedded-NUL bytes and BUFFER sizing, memory transactions, current values, and
retained snapshots after closing and reopening a file. It does not import wgpu;
the older wgpu coexistence examples remain separate checks.

The packager now deletes the isolated Cargo build directory before any compiled
consumer runs. It then renames the entire package to a different path, verifies
the original package path is absent, and reruns C/C++, ctypes, and requested
Odin checks there. The directory is restored afterward so evidence stays at the
requested output path. macOS uses a library install name of
`@rpath/libweekend_db.dylib` with executable `@loader_path`; Linux uses SONAME
`libweekend_db.so` with literal `$ORIGIN`. Windows uses the adjacent DLL and
import library. Dependency listings are captured with otool/readelf/dumpbin.

Observed qualification: macOS ARM64, Odin dev-2026-10:a22aa1984 passed the full
Odin and C/C++ profile, including relocation with the original path absent.
A fresh Linux ARM64 rust:1-bookworm container passed C/C++/ctypes and actual
SONAME/$ORIGIN relocation; Linux Odin execution was not tested. Windows Odin
command construction is tested but native Odin execution remains pending.
Dirty development packages correctly report `verified=true` and
`candidate_eligible=false`; compiler hashes and Odin's runtime license hash are
recorded in the manifest. If the compiler root's LICENSE is unavailable,
runtime verification continues but that concrete missing attribution input
blocks candidate eligibility.

This work exposed a previous macOS packaging limitation: the copied dylib's
install name pointed into the temporary build directory, and earlier consumer
checks ran before that directory was deleted. Those older results do not prove
standalone package execution or relocation. The new loader identities and
post-cleanup, renamed-directory executions address that false positive.
A fresh-cache Linux check also exposed missing unbuilt-workspace dependencies
for offline attribution metadata; the tool now fetches the locked dependency
set before its build and metadata checks.

Default macOS/Linux/Windows CI now runs the C/C++ private package and relocation
checks at `target/private-package`, uploading platform-named evidence. Native
jobs have a 30-minute budget. Odin remains an explicitly requested local profile
until a native CI compiler installation is qualified.
