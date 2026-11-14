# Private pre-1 checkpoint qualification

Run with local Git objects, an installed compiler and cached dependencies:

```sh
mise x rust@1.99.1 python@3.14.8 -- python3 -O tools/compatibility.py \
  --profile storage --historical a7dc343 \
  --current 21e5d2638c24c6254ced35d0077c55bb30a287b2 \
  --output target/compatibility/new-storage-run
mise x python@3.14.8 -- python3 tools/test_compatibility.py
mise x python@3.14.8 -- python3 -O tools/test_compatibility.py
```

The final output component must not exist: files, directories, existing symlinks
and dangling symlinks are rejected before resolving the output's parent. New
parent directories are allowed. The final mkdir is exclusive; no existing
qualification output is cleaned or overwritten. Failure retains the newly owned
output, logs and explicit `verified:false` metadata.

Both `--historical` and `--current` select committed endpoints in both profiles.
The storage historical default remains a7dc343; the snapshot default remains
dcd2402; current defaults to committed HEAD. Uncommitted engine changes are never
compiled. The qualification runner and fixture may be under review and are bound
separately by hash rather than attributed to an older engine commit.

Each endpoint is extracted safely from its Git archive into `output/source`.
Only an explicit private example fixture, a runtime identity example and their
Cargo.toml declarations are overlaid. Existing packages retain `publish = false`.
Builds use each endpoint's **original Cargo.lock**, `--offline --locked`, separate
target directories and no incremental compilation. No downstream lock resolution
substitutes for the historical graph. Original lock copies, source manifests,
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
compilers. Registry dependencies are not vendored. No network fetch, remote job,
publication or core source change is performed. The process-group runner
currently qualifies macOS/Linux, not native Windows.

## Storage profile

Each checkpoint writes its own fresh native image. For each writer, both the
current and historical consumers check exact current revision 4, two current
records (`key=21`, `new="current"`), exact retained IDs `[1]`, and retained
revision 2 with only `key=11`. Both reads must preserve every image byte.

For each image, both consumers reject a copy with header version 0xffffffff and
must preserve every unsupported-image byte. This deliberately changes the
version before CRC validation; it is not a valid future encoder or a migration
test. Byte-preservation checks use explicit failures and remain active under
Python `-O`, including when a consumer returns success after changing the file.

There are two writes, four full logical reads and four unsupported-version
rejections per platform. `evidence.json` reports each writer's results separately;
image hashes differ between writers/reruns because native identities are new.
This is a narrow pre-1 base-feature format-two fixture, not a general release or
API compatibility promise. The [21e5d26 qualification](../docs/storage-compatibility-21e5d26.md)
records native Mac and isolated Linux results.

## Passive snapshot profile

```sh
mise x rust@1.99.1 python@3.14.8 -- python3 -O tools/compatibility.py \
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
seconds, storage consumers 30 seconds, and a snapshot pair a shared 90-second
deadline. Timed-out process groups are killed and direct children reaped with a
five-second wait. Metadata records commands, deadlines, return codes and failures.
These are subprocess bounds under normal OS scheduling, not a kernel/filesystem
liveness guarantee.

The separate native `weekend-db-typed` process_evolution integration tests and
archived pre1-v2.wdb fixture retain their own scope and evidence. This runner does
not replace typed-schema migration, mixed feature encodings, device/power-loss,
arbitrary checkpoint or broader lifecycle qualification.
