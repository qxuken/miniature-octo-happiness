# Private Rust source profile

Add `--rust-source` to `tools/release.py` to deliver a source tree and compile a
fresh private Rust consumer against it. This is source delivery, not a portable
precompiled rlib or proc-macro binary. Nothing invokes Cargo publication.

```sh
python3 tools/release.py --output NEW_DIRECTORY --rust-source
# Optional stronger Linux-only isolation proof (Docker must be available):
python3 tools/release.py --output ANOTHER_NEW_DIRECTORY --rust-source --rust-source-isolated
```

A clean source tree is required unless `--allow-dirty` explicitly marks the
package ineligible. The copied inventory contains tracked regular files, their
workspace manifests and lockfiles, plus the explicitly named private consumer
files during development. It excludes `.git`, build `target` directories,
Python caches, and arbitrary untracked files. Symlinks, escaping paths, missing
tracked files, and publishable Cargo package manifests are rejected. Original
source-file bytes are preserved and individually hashed; their hashes must
match the initial packaging inventory. Exact project license texts are added
to local MIT/Apache package roots as separately recorded distribution metadata.
All source-package Cargo manifests declare `publish=false`.

The standalone consumer at `rust-source/examples/rust_package` uses relative
path dependencies on the copied core, typed crate and proc-macro crate. Its own
committed lockfile pins registry versions/checksums. Cargo metadata must keep
all local manifests within the delivered source; unqualified Git dependencies
are rejected. A unique target directory and `CARGO_INCREMENTAL=0` prevent
reuse of the original project's build products. Compiler dependency files are
checked for references back to the original checkout. The actual executable
must belong to the isolated build target. Its embedded source ID is checked
while running the typed Model/index/update flow and derived CRDT Record and
Resolution behavior: two enrolled peers edit offline, synchronize over trusted
test-owned loopback streams, converge on a committed goal, and repeat sync
without another commit. The transport fixture is not a production authentication
implementation. Encode/decode roundtrip is checked as well.

Registry crates are **not vendored**. Rebuilding requires a suitable Rust
compiler and the locked registry inputs, fetched by Cargo or already cached.
This profile makes no offline self-contained SDK claim. Attribution inventories
cover the actual compiled packages, proc macros, and matching Rust standard
library notice input. Source delivery includes project licenses and explicit
additional license copies; a concrete missing attribution input blocks
candidate eligibility without suppressing the runtime checks.

`--rust-source-isolated` additionally rebuilds in a bounded fresh Linux Docker
container from the selected locally available image (pull it explicitly first
if needed). The immutable image ID is resolved before execution. Only the delivered source is mounted, read-only. The original
checkout path must not exist in that container. Registry dependencies can be
fetched using the committed lockfile; compilation and test artifacts remain in
the disposable container. The manifest records the image ID, and the log records
the Linux Rust toolchain. Memory is capped at 2 GiB, processes at 256, CPUs at 2,
and the run at 300 seconds, with forced cleanup on timeout. Docker is not a
prerequisite for ordinary source delivery on macOS or Windows.

Evidence deliberately separates `native_compile_verified` from
`linux_isolation_verified`. A native copied-source compilation is not represented
as an originals-unavailable proof. The existing native C/C++ and optional Odin
exact-artifact/relocation checks remain independent. The source consumer does
not establish whole-library behavior, cross-version compatibility, or support
for every target where Rust itself runs.
