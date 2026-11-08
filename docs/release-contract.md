# Private release contract

Packages use `MIT OR Apache-2.0` and retain `publish = false`.
Consumers pin source or verified private artifacts. Project name and
0.1 version remain provisional. Disk, wire, C ABI and Rust APIs have
independent compatibility boundaries.

Runtime queries expose package version, format 2, ABI 1, source ID and
feature flags through `weekend_db::version` and the C header. Ordinary
builds report unknown source. Artifact tooling embeds the revision and
marks development builds dirty.

```sh
python3 tools/release.py --output /absolute/new/package-directory --features replication,crdt
```

The tool builds with the lockfile, checks exports against the header,
compiles/runs C and C++ consumers, exercises the delivered ABI, checks
identity and records hashes and commands. Source changes during packaging
reject. The tool forces the release profile to unwind and queries the
actual compiler's panic configuration before building. A remaining
`panic=abort` from Rust flags or Cargo configuration rejects the package.
The manifest records the selected panic strategy and SHA-256 hashes of
build-related environment overrides and discovered ancestor/Cargo-home
configuration files. Configuration contents and registry credentials
are omitted. Those hashes identify inputs without retaining their
contents, so keep the configuration securely for later reproduction.
Compiler wrappers and the compiler remain trusted tools.
The package includes `notices/inventory.json` and copied attribution
files for the actual Cargo compiler-artifact package IDs emitted during
the isolated library build. This includes build dependencies and proc
macros. Each entry records its name/version, declared license expression,
Cargo source, lockfile checksum where available, manifest hash, and
copied license/notice hashes. Project-level notices are explicitly
identified for workspace packages. Notice paths must resolve within
their package or the separately admitted workspace root, with bounded
file and aggregate copy sizes.

The matching toolchain's `COPYRIGHT-library.html` supplies standard
library attribution and is copied with its hash. Missing declarations,
missing notice text, invalid notice paths, missing registry checksums,
or absent standard-library notices produce review items and prevent
candidate eligibility. License expressions and copied texts remain
inputs for legal review. The tooling does not approve their terms.
Metadata and compiled package IDs are joined using Cargo's opaque
package identity. [Cargo metadata](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html)

Candidate eligibility means these packaging checks passed from
clean source. The roadmap adds further acceptance gates. Windows compiled
packaging and full Odin delivery remain tracked. Windows CI now compiles
and executes C11 and C++17 memory consumers against both debug and release
MSVC DLLs using `tools/windows_consumers.py`, and archives the import
library with the DLL. This job still needs a native run before claiming
Windows compiled-consumer qualification for a candidate.

Incompatible byte formats require a new version, read-only rejection by
unsupported readers, logical migration, archived source and rollback tests.
Preserve a verified backup before migration. Archived fixtures start in
`tests/fixtures/README.md`.

C consumers compare header ABI version with the loaded library. Status
and kind numbers retain their meanings. The header defines opaque handle
ownership, valid pointers, buffers and close quiescence. Incompatible
signatures or ownership require a new ABI version. Test exact artifacts.

Rust consumers pin the matching workspace, macros and typed runtime.
Expansion and schema/wire markers require compatibility tests.
Replication and CRDT decoders enforce separate versions. Mixed-version
deployment requires an interoperability test before rollout.

Panic containment requires unwind-enabled artifacts. Refcount exhaustion
intentionally aborts. Pointer validity and close quiescence remain C
caller obligations. Platform/device assumptions in DESIGN.md are part
of the durability contract.
