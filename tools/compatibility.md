# Private pre-1 checkpoint qualification

Run with local Git objects, an installed compiler and cached dependencies:

```sh
cargo xtask compatibility \
  --profile storage --historical a7dc343 \
  --current 21e5d2638c24c6254ced35d0077c55bb30a287b2 \
  --output target/compatibility/new-storage-run
cargo test --offline --locked -p xtask --bin xtask compatibility
```

The final output component must not exist: files, directories, existing symlinks
and dangling symlinks are rejected before resolving the output's parent. New
parent directories are allowed. The final mkdir is exclusive; no existing
qualification output is cleaned or overwritten. Failure retains the newly owned
output, logs and explicit `verified:false` metadata.

Both `--historical` and `--current` select committed endpoints.
The storage historical default remains a7dc343; the snapshot default remains
dcd2402; current defaults to committed HEAD. Uncommitted engine changes are never
compiled. The qualification runner and fixture may be under review and are bound
separately by hash rather than attributed to an older engine commit.

Each endpoint is extracted safely from its Git archive into `output/source`.
Only an explicit private example fixture, a runtime identity example and their
Cargo.toml declarations are overlaid. Existing packages retain `publish = false`.
Builds use each endpoint's **original Cargo.lock**, `--offline --locked`, separate
target directories and no incremental compilation. No downstream lock resolution
substitutes for the historical graph. The runner disables Git checkout line
conversion for the archives and checks each extracted lock against its Git
object. Original lock copies, source manifests,
source/lock hashes, exact compiler/Cargo versions, build commands, binaries and
logs remain in the output. Every archived/overlaid source file is checked
unchanged after build and execution.

Storage endpoints require their native version API: a separately compiled
identity probe checks the exact SOURCE_ID, feature bits zero, ABI 1 and format 2.
The probe and consumer have separate recorded binary hashes and share the same
frozen library build. Snapshot enables replication (feature bit 1). A historical
snapshot endpoint that predates the native identity API uses an explicitly
labeled `qualification_build_stamp` probe instead: it reports the compile-time
source stamp/feature selection and exercises Database::new. That is not native
engine version introspection; it remains bound by archived source, lock and
binary hashes. Endpoints with the API still use native introspection.

Known compiler/target/flag/wrapper/profile overrides are removed from child build
environments; local Cargo configuration/cache remains a trusted input. The
recorded installed compiler builds both checkpoints, not their historical
compilers. Registry dependencies are cached separately. The runner builds
offline on macOS, Linux and native x64 Windows with fixed NTFS.

## Storage profile

Each checkpoint writes its own fresh native image. For each writer, both the
current and historical consumers check exact current revision 4, two current
records (`key=21`, `new="current"`), exact retained IDs `[1]`, and retained
revision 2 with only `key=11`. Both reads must preserve every image byte.

For each image, both consumers reject a copy with header version 0xffffffff and
must preserve every unsupported-image byte. This deliberately changes the
version before CRC validation. Byte-preservation checks are explicit in both
debug and optimized builds, including when a consumer exits successfully after
changing the file.

There are two writes, four full logical reads and four unsupported-version
rejections per platform. `evidence.json` reports each writer's results separately;
image hashes differ between writers/reruns because native identities are new.
This is a narrow pre-1 base-feature format-two fixture, not a general release or
API compatibility promise. The [21e5d26 qualification](../docs/storage-compatibility-21e5d26.md)
records native Mac and isolated Linux results.

## Passive snapshot profile

```sh
cargo xtask compatibility \
  --profile snapshot --historical dcd2402 --current 315068f \
  --output target/compatibility/new-snapshot-run
```

The existing mixed-checkpoint snapshot fixture is unchanged. Both sender/receiver
directions retain the complete revision-4/current and revision-2/historical
oracle, independently hashed export/receipt agreement, reopened follower state,
identical duplicate acknowledgment, primary advancement to revision 5, and
unsupported poll-version rejection with closed follower files unchanged.
The peers are specifically owned private-loopback endpoints. This does not
qualify TLS, trust rotation, upload faults, mixed OS pairs or rolling deployment.
See the [historical snapshot report](../docs/snapshot-compatibility.md) for that
protocol's precise boundaries. New runs preserve source/build directories;
that earlier report's temporary-directory layout describes its older runner.

Git/tool probes and archives have 30-second subprocess waits, each build 180
seconds, and storage consumers 30 seconds. Each peer in a concurrent snapshot
pair runs under a 90-second deadline. POSIX retires timed-out process groups
and reaps direct children with a five-second wait. Windows starts each child
suspended, assigns it to a job and verifies job cleanup. Each snapshot peer
has its own job and both results are retained if either peer fails. Metadata
records commands, deadlines, return codes and failures.
These are subprocess bounds under normal OS scheduling, not a kernel/filesystem
liveness guarantee.

The native `weekend-db-typed` process_evolution integration tests and archived
pre1-v2.wdb fixture provide separate schema evolution evidence. Mixed feature
encodings, device power loss and further checkpoints require their own profiles.

## Native Windows

Install the native x64 MSVC toolchain and run from a checkout that contains
the frozen endpoint commits. From PowerShell, cache the locked dependencies
and use a fresh output for each profile.

```powershell
cargo fetch --locked
cargo xtask compatibility --profile storage --historical a7dc343 --current 21e5d26 --output target/windows-storage
cargo xtask compatibility --profile snapshot --historical dcd2402 --current 315068f --output target/windows-snapshot
```

The [typed rollout runbook](../docs/typed-schema-rollout.md#reproducing-the-windows-ntfs-profile)
records the separate release-binary schema migration profile.
See the [recorded Windows storage](../docs/storage-compatibility-21e5d26.md#windows-ntfs-result)
and [snapshot results](../docs/snapshot-compatibility.md#windows-ntfs-result)
for the pinned pairs and artifact hashes.

## Release qualification

A manual Engine checks run or a `v*` tag runs five compatibility
profiles (storage, snapshot, typed, snapshot CLI and Study peer CLI) on macOS, Linux and Windows. Each job checks the GitHub event
revision against its checkout, fetches the historical locks' dependencies
and passes the checked-out commit to each offline qualification. The
`candidate-compatibility` artifacts retain the endpoint identities, locks,
consumer binaries, test files and command transcripts. Pull requests
continue to run the existing fixed-endpoint controls.
