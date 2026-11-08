# Accelerated month workload

This is a deterministic 30-day calendar schedule executed as fast as the native
storage and subprocesses permit. It is not a 30-day wall-clock soak, production
load model, resource cap or device power-loss qualification. The small fixed
workload makes every final and retained state independently checkable.

Build and run a local two-process-peer qualification:

```sh
cargo build --offline --locked --release --manifest-path tools/monthly-workload/Cargo.toml
python3 tools/monthly_workload.py --directory target/monthly-workload/new-run
```

The directory must not exist. The standalone private tool depends on the engine
with `crdt` and on the typed crate. It reuses StudyProgress's record and resolution
policies without changing their encoding. The driver exposes separate `init`,
`day`, `serve`, `sync`, `checkpoint`, `verify` and `backup` process phases. Every
phase opens its database independently. Files, sidecars, backup/restored copies,
JSONL events, CSV statistics and a JSON summary remain in the run directory.

## Schedule and oracle

Each of two nodes owns eight typed tasks with unique slugs and a typed daily
receipt. Tasks are node-local; this driver does not replicate conventional typed
rows. Per day, each node deliberately races two drafts over one existing row,
asserts one conflict, then retries a transaction editing all eight tasks and the
receipt together. Final task scores are `100*day + 10*role + id`. The nested
transactions establish an overlapping draft schedule, not CPU-parallel threads.

Each node then edits its replicated StudyProgress while offline. Node 0 adds
`10+day` practice minutes and node 1 adds `20+day`, marking their distinct daily
completion IDs. Weekly goals conflict and use MaxRegister; session limits
conflict and use MinRegister. Bidirectional sync commits resolution. Repeating
both directions must produce zero further commits. The independent arithmetic
oracle at day d is `30*d + d*(d+1)` minutes, completion IDs 0 through `2*d-1`,
weekly goal `90+d` and session limit `20+d%5`. No CRDT merge implementation is
used to compute those expectations.

Every seventh day, both worker processes terminate deliberately with exit 70
after the typed receipt commit but before CRDT editing or returning a success
response. The runner restarts the phase, reads the durable receipt, skips typed
replay and performs the missing CRDT update. The grow-only daily mark makes an
already committed CRDT edit idempotent too. This is a specific interruption
between durable operations, not random termination inside publication or a
physical power cut. Existing simulator/native interruption suites cover those
other failure windows.

Both nodes retain one snapshot after each day's convergence. Every verification
checks current tasks and unique lookups, the exact CRDT state, and all previous
daily retained task/CRDT states. At day 30 each node creates a verified backup,
restores into a fresh file, opens it independently, and reruns that full oracle.
The schedule contains 480 typed task edits, 60 conflict/retry pairs, 60 offline
CRDT updates, 60 retained roots across the nodes, 120 sync sessions and 8 deliberate
process interruptions. Initialization and receipt/index writes are additional.
Final state is 1830 minutes, 60 completed IDs, weekly goal 120 and session limit 20.

## Linux server and macOS client

The runner defaults to two peers on its own host. It can launch role0 on a Linux
server using a supplied argv prefix, while role1 remains the local macOS client:

```sh
python3 tools/monthly_workload.py --directory target/monthly-workload/cross-os \
  --server-command '["docker","exec","CONTAINER","/work/monthly-workload"]' \
  --server-db /data/server.wdb \
  --server-listen 0.0.0.0:39101 --server-address 127.0.0.1:39101 \
  --client-listen 0.0.0.0:39102 --client-address host.docker.internal:39102
```

Build the same source/lockfile for Linux, prepare its parent data directory and
publish the server port before using that example. Reverse connectivity to the
client port is required for bidirectional sessions. The supplied prefix can
instead be an SSH command, provided stdout carries the driver's READY line and
JSON response without a shell banner. Record the remote build/toolchain and
binary hash separately. Driver modes, database paths and endpoint arguments
are passed as argv, not shell text.

These are test-owned TCP endpoints without TLS or peer authentication. The
`*_authenticated` API assumes the caller has established identity, which this
isolated workload supplies by construction. Do not expose these listeners to
untrusted networks. Actual cross-OS execution must be recorded separately from
the default local run. CRDT replies are exercised; passive snapshot-follower
durable acknowledgment is not used or implied.

## Measurements and limits

JSON records driver phase duration, successful typed/CRDT transaction-call
latency, reopen latency, revision/commit changes and file length. Transaction
latency includes application conversion and validation as well as publication;
it is not an isolated fsync measurement. Backup and restore have separate timers.
The Python runner also measures process wall time and local disk free bytes.
Interrupted operations have no success response, so reported transaction timing
samples omit their committed typed transaction. Scheduled operation totals are
stated separately from observed replies. CSV contains one row per phase;
transaction samples are arrays within the corresponding JSON events.

RSS is the host's cumulative child-process high-water mark, excluding remote
container descendants. It is not summed resident memory, a leak bound or an
allocator quota. File growth is per node/day; local free-space measurements do
not describe a remote filesystem. Long-running load, larger cardinalities,
network partitions beyond daily offline edits, authentication, passive follower
acks and random fault schedules remain separate qualification work.

For a pinned build, pass `--engine-source COMMIT` to bind the declared build
source in the run summary. That label is caller-supplied; retain the archive,
build command and remote binary hash as supporting evidence. The runner rejects
stale driver binaries missing the measurement fields. Socket connection and
I/O timeouts are 10 and 15 seconds; server accept waits at most 30 seconds.
Python phase subprocesses are bounded at 60 seconds.

## Executed Linux/macOS profile

On 2026-11-09, `tools/monthly_workload_docker.py --source 1ca5e5a
--directory target/monthly-workload/cross-os-reproducible` built the same archived
engine plus the current qualification driver for a Linux aarch64 server and
native macOS arm64 client. Linux 7.0.14-linuxkit used a private Docker volume on
ext4 (`rw,relatime,discard,noauto_da_alloc`); the macOS client used APFS. Both
used rustc 1.99.1. The runner records the source tar, tool hashes, Docker image
identity and Linux binary hash, and removes only its private container/volume
after copying synthetic database evidence back to the run directory.

Both the initial cross-OS run and this reproducible rerun passed all 30 days,
all retained-state oracles, zero-commit repeat sync, eight receipt-interruption
retries and both restored copies. The latter took 30.54 seconds of wall time.
Each node ended at 6,668,288 physical bytes with 30 retained roots. Its 112
returned transaction samples had median 7.076 ms, p95 14.710 ms and maximum
93.208 ms. The 60 verify-phase opens had median 12.580 ms, p95 22.227 ms and
maximum 63.347 ms. The two backups took 54.010/62.553 ms and restores
54.160/65.335 ms. These combine unlike hosts and are descriptive samples.
Local child peak RSS was 32,145,408 bytes; remote descendants are excluded.

The prior local macOS pair also passed the same pinned engine and latest driver
in 12.07 seconds. Evidence is `target/monthly-workload/mac-qualified3`. The
cross-OS evidence is `target/monthly-workload/cross-os-reproducible`, including
JSON/CSV events, binary/source bindings and independent restored files. These
profiles exercise real native adapters and TCP but retain the workload/auth,
virtual-device and accelerated-time limitations above. They do not qualify
physical power failure or the later live-allocation-budget implementation.
