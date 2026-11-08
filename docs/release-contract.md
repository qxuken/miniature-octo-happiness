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

Candidate eligibility means these packaging checks passed from clean source;
the roadmap adds further acceptance gates. Native Windows compiled C11/C++17
consumers passed at checkpoint 83954e8. The new private Windows packager
requires its own candidate run. Optional `--odin` packages generated bindings
and runs a standalone consumer against the exact artifact. macOS Odin/C/C++
and Linux C/C++ relocation checks passed after fixing a temporary install-name
dependency; the package is moved and its build directory removed before checks.
See [Windows packaging](windows-private-package.md) and
[Odin packaging](odin-private-package.md) for scope and evidence.

Incompatible byte formats require a new version, read-only rejection by
unsupported readers, logical migration, archived source and rollback tests.
Preserve a verified backup before migration. Archived fixtures start in
`tests/fixtures/README.md`.

C consumers compare header ABI version with the loaded library. Status
and kind numbers retain their meanings. The header defines opaque handle
ownership, valid pointers, buffers and close quiescence. Incompatible
signatures or ownership require a new ABI version. Test exact artifacts.

Rust consumers pin the matching workspace, macros and typed runtime.
Optional [Rust source packaging](rust-source-package.md) delivers the exact
source inventory and lockfiles and verifies a fresh typed/CRDT consumer. Registry
dependencies remain external; optional Docker proof isolates the copied source.
Expansion and schema/wire markers require compatibility tests.
Replication and CRDT decoders enforce separate versions. Mixed-version
deployment requires an interoperability test before rollout.

Panic containment requires unwind-enabled artifacts. Refcount exhaustion
intentionally aborts. Pointer validity and close quiescence remain C
caller obligations. Platform/device assumptions in DESIGN.md are part
of the durability contract.

## Ownership and qualification workload

The security-report and release-owner contact is `sec@qxuken.dev`, supplied by
the owner on 2026-11-09. [SECURITY.md](../SECURITY.md) describes reporting and
private patch handling. The primary deployment profile is a Linux server and
macOS client. A reproducible 30-day simulated workload passed with stored task/study updates,
retained history, sync/restart failures and restore. See
[workload evidence](monthly-workload.md).
Native architecture/filesystem evidence and measured latency/resource results
accompany that workload. Accelerated calendar simulation does not establish a
month of continuous wall-clock uptime or device power-loss behavior.
