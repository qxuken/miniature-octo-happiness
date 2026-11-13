# Native Linux quota exhaustion and recovery

The `linux-quota` job uses an isolated ext4 loop filesystem on an Ubuntu 24.04
host. It exercises the real Linux file adapter, not an injected ByteStore error.
The existing [tmpfs ENOSPC/permission drill](native-fault-drills.md) retains its
own scope and guards.

## What the gate requires

The root controller creates a fresh 64 MiB image and binds its inode, loop
association and exact mountpoint before configuring a 4 MiB user quota. It
never configures quotas on the host filesystem or detaches unrelated devices.
The formatter explicitly selects ext4 even when `mkfs.ext4` resolves to
`mke2fs`. A separate numeric UID runs the probe and Rust fixture with empty
supplementary groups, zero capabilities and `NoNewPrivs=1`.

The calibration must observe actual Linux errno 122 (`EDQUOT`), with more than
16 MiB of both filesystem free and available space. Errno 28 (`ENOSPC`) cannot
substitute. Removing only the owned ballast must allow the same UID to write
and fsync again. Ballast writes stop at an independent 8 MiB ceiling.

The source-bound, core-only Rust fixture then executes in fresh processes:

1. Create collection 1, commit two records at revision 2 and retain that root.
   Retention publication makes the current revision 3, with retained ID 1 at 2.
2. Exhaust the user's quota again. A transaction replacing the integer and
   writing a distinct 64 KiB payload must fail with `Io`, native `Quota`, raw
   errno 122 and operation `Write`.
3. Require an aborted/terminal transaction (`Closed` on reuse), recovery-required
   database state and rejection of a fresh write with `RecoveryRequired`.
4. Reopen in another process while the quota is still exhausted. Verify current
   revision 3 and retained revision 2, including complete record sets and every
   payload byte.
5. Remove owned ballast, retry once to revision 4, then reopen independently.
   Verify the complete new current data and unchanged retained revision 2.

Both the fixture and independent Python oracle check full rows. The parent
rejects wrong types, missing/extra fields, stale revisions, damaged payloads,
wrong source/features and substituted error categories. Fixture modes emit
their actual process IDs; phases must use distinct processes.

Commands have bounded runtime and output. Proof stays in a mode-0700 directory;
guest-accessible staging is separate. Cleanup attempts unmount and loop detach
independently, checks actual absence, and removes scratch only after both are
absent. A `findmnt` query error cannot prove absence. Cleanup uncertainty fails
qualification and retains its diagnostic evidence.

The evidence records the exact source commit and file hashes, fixture hash,
controller/guest Python identities, resolved command hashes, kernel, quota
policy, command results and full data oracles. Apt package versions are recorded
by CI; they are not represented as immutable package pins. Controller Python
3.14.8 and Mise 2026.11.6 are pinned; the guest's distro Python is recorded
separately.

## Running it

Use a disposable, quota-capable Linux host with `quota`, `e2fsprogs` and
`util-linux`. Build without engine feature flags:

```sh
cargo +1.99.1 build --locked --release --example native_quota --target-dir target/quota-build
quota_python="$(mise exec python@3.14.8 -- python -c 'import sys; print(sys.executable)')"
sudo "$quota_python" tools/quota/run.py \
  --fixture "$PWD/target/quota-build/release/examples/native_quota" \
  --source "$(git rev-parse HEAD)" \
  --output "$PWD/target/native-quota"
```

The output must be new. Source files must match the requested commit. Default
execution fails if quota support or cleanup cannot be established. The optional
`--probe-only` mode may record unsupported capabilities, but never sets engine
qualification to verified.

## Verification status

Local controller controls pass normally and under Python `-O`. The full helper
entry point now runs twenty suites: forty workers, 292 passes and four registered
skips (296 reported tests). Twelve quota controls include real child timeout and
output limits, strict data/error oracles, wrong mount identity, cleanup failures,
ambiguous absence and formatter selection.

A frozen Linux aarch64/ext4 Docker run verifies seed, fresh reopen at 3, retry,
fresh reopen at 4 and complete retained bytes. A separate no-quota negative
control rejects the required-failure mode, then independently reopens the real
successful revision 4. Its source/fixture overlay hashes match the current files,
and its owned container and volume are absent. Proof:
`target/quota-development-844c429-v2/evidence.json`. Formatting, workflow lint and
targeted Clippy checks pass. These checks do not establish actual EDQUOT.

Hosted quota execution is pending at this checkpoint. The earlier Linuxkit
quota provisioning limitation remains historical evidence; neither synthetic
quota errors nor ENOSPC are credited as an actual quota pass. This gate covers
native append exhaustion/recovery only, not physical power loss, quota failures
during maintenance, other filesystems or other operating systems. Engine
publication barriers and separate follower durable acknowledgments are unchanged.
