# Mixed-checkpoint passive snapshot qualification

The optional snapshot profile extends the existing native compatibility tool:

```sh
python3 tools/compatibility.py --profile snapshot \
  --historical dcd2402 --current 315068f \
  --output target/snapshot-compatibility
```

The output must be new. The historical default for this profile is `dcd2402`;
current defaults to committed HEAD, never uncommitted engine changes. The original
storage profile and its `a7dc343` historical default are unchanged. Python 3.12+,
Git and the native Rust toolchain are required. Dependencies must already be
available locally: builds use `--offline --locked` and incremental compilation is
disabled. This is private qualification tooling; nothing is published.

Each endpoint is a frozen Git archive. The only build overlays are the same
`tools/compatibility-snapshot.rs` fixture and an explicit example declaration in
its Cargo.toml. The original Cargo.lock remains byte-identical before/after build;
no newly resolved standalone dependency graph substitutes for either historical
lockfile. The original manifests keep their `publish = false` setting. Builds
use a separate temporary target directory, and the resulting executables are
copied into the evidence directory before execution. Temporary source/build
folders are removed afterward. Evidence records both commit IDs, archive hashes,
original/overlaid manifest hashes, lockfiles and hashes, replication source
hashes, exact consumer binary hashes, fixture/runner hashes, toolchain and commands.
Registry source availability and the current toolchain are inputs; this does not
qualify each checkpoint's original historical compiler.

For each direction, one binary is the primary and the other is the follower.
Both run simultaneously as separate processes on runner-owned loopback sockets:

- The primary creates collection 1, writes `key=1` at revision 2, retains root 1,
  and writes `key=2` at revision 4. It exports revision 4, then successfully commits
  `key=3` locally at revision 5 before contacting the follower.
- The receiver initially reports no acknowledgment. Shipping the export returns
  durable revision 4, not the primary's newer local revision. The receiver closes
  and reopens between every request. The current root must contain exactly one
  key with value 2 at revision 4, and retained root 1 exactly one key with value 1
  at revision 2. The retained ID set must be exactly `[1]`.
- Retrying the same immutable export returns the identical receipt. Primary and
  reopened follower receipts must match in revision, stream and digest. Python
  independently calculates SHA-256 over the exported file and compares it with
  both receipts; matching two library-generated receipts alone is insufficient.
- The sender changes the request version byte from wire v1 to 255. Both sender
  and receiver must report UnsupportedFormat. Every file in the follower directory
  remains byte-identical, and a subsequent native reopen still exposes the exact
  prior receipt and complete current/retained state. This is a request-version
  rejection test, not negotiation or an invented future-version implementation.

The runner bounds builds to 180 seconds each and consumers to 90 seconds each;
consumers also bound stream reads/writes and initial handshake waits. Failure
leaves logs and copied binaries for inspection, and exits unsuccessfully. A
successful `evidence.json` is written only after both directions and all oracles
pass. Each fresh primary has a new file identity, so export digests are expected
to differ between directions and reruns.

## Recorded result

On aarch64 macOS with rustc 1.99.1 / LLVM 23.1.1, both directions passed between
`dcd2402adeb034f15a5293c52238524ace9c4070` and
`315068fc38b99d8ec616fdb5a8ee81107b471534`. Their production follower.rs and mod.rs
hashes differ; this is not merely the same replication implementation packaged at
two commits. The fixture hash was
`7bf5d828ef628862d8a1782ef3ef0d7e270c1fe323a9ec589cc1dcbebb157697`.

| Direction | Selected revision | Retained revision | Independently verified export SHA-256 |
|---|---:|---:|---|
| dcd2402 → 315068f | 4 | 2 | `7916a021ec5a1e5ad7567e518b019d00fd587e3484cd52760fa890aaf96659ef` |
| 315068f → dcd2402 | 4 | 2 | `11158cc0240231536e35cddabcb78de7b02119bba8e6452cb40669201a4fe41d` |

Full local evidence is `target/snapshot-compatibility-reviewed/evidence.json` with
adjacent per-endpoint build logs, original lockfiles, executable files, protocol
consumer logs and native images. The final qualification ran with Python `-O`; all runner
validation and process-wait gates remain active in that mode. Raw follower-file
comparisons run with the follower closed, respecting native exclusive file leases.
Readiness files are published by rename after a complete private write. Repeated transfer and unsupported-version
rejection preserved both receivers' receipts and data. The original storage
compatibility profile was also rerun as a regression check.

## Exact boundary

This proves a small native snapshot wire-v1, receipt, retry and reopen compatibility
case for these two pre-1 checkpoints on this host. It does not establish a stable
support policy, all retained-history shapes, rolling upgrades under load, mixed
OS pairs, fault/power-loss behavior, or arbitrary older/newer releases.

The historical core and current core both require the caller to supply an
authenticated duplex channel. Here the test owns both endpoints and authorizes
those specific loopback peers. The fixture provides neither TLS nor a general
unauthenticated deployment example. The newer snapshot CLI's TLS peer identity,
certificate rotation, trust revocation and operator enrollment are separate
contracts and were not exercised by this core wire qualification. In particular,
core compatibility does not imply that a historical CLI can participate in the
new CLI's TLS deployment. See [replication operations](replication-operations.md)
for the deployment requirements and its separate native/TLS evidence.
