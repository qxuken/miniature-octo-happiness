# Near-cap maintenance interruption and repeated recovery

Checkpoint `3136017` qualifies **52 native compaction process kills** and
**108 simulated recovery cases per platform** on macOS arm64/APFS and Docker
Linux aarch64/ext4, 2026-11-14. Each recovery case includes two failed opens
before the real medium returns a successful handle. Every case verifies the
current graph and all **64 retained depth-64 roots**, then verifies a later
successful operation in a fresh process. Production engine code, serialization,
durability barriers and separate follower acknowledgment are unchanged.

## Fixture and oracle

The existing [compound workload](compound-resource-envelope.md) produces a
66,154,496-byte image, **98.58%** of the 64 MiB cap, after 32 contended commits
by eight writers. It contains 48 distinct 896 KiB bulk values, 64 historical
versions of a depth-64 value, and eight worker records. Current revision is
**212**; retained IDs 1..64 capture **53,55,..179**. Their shared large payload
is reachable data; no padding is added to manufacture fullness.

The original native/concurrency/96 MiB budget/copy/fault profiles run first in
the same frozen binary. The new drills reuse their complete Rust oracle: every
bulk byte in current and every retained view, every nested list/leaf byte,
worker sequence/payload, row count, retained ID and captured revision. Independent
Python fingerprints check bulk bytes, deep leaves, current workers and retained
sequences. Retained worker/bulk bytes also undergo full Rust comparisons; they
are not inferred from counts. FNV fingerprints detect accidental mismatch and
are not cryptographic authentication.

Added profiles launch **214 fresh checker processes per OS**: two controls,
two checks for each of 52 native kills, and one final check for each recovery
case. Other checks also run in the supervisors and independent simulated media.
Private test images are retired after their checks; the source and two compacted
control images remain in the copied proof tree.

## Native maintenance interruptions

A successful instrumented compaction first calibrates each hook's occurrence
count. Every chosen occurrence is tested **before and after** the native action.
Offline compaction uses an explicit 256 MiB working-domain budget; fresh
checker opens retain the 96 MiB core budget. The child flushes a readiness marker
and parks; its parent sends **SIGKILL**,
waits/reaps it, and requires signal 9. There is no normal Rust cleanup.

| Native action group | Chosen occurrences |
|---|---|
| Source length / witness write | First occurrence of each |
| Source reads | First, middle, last |
| Source sync | Both stabilization and witness syncs |
| Target length | First |
| Target reads / writes | First, middle, last of each |
| Target sync | All three: append, root and witness |
| CopyMetadata / SyncMetadata / CheckNames | Each action |
| Exchange / SyncDirectory / SyncReplacement | Each action |
| RemoveOld / SyncCleanupDirectory / SyncCleanupFile | Each action |

These are 26 selected occurrences × two edges = **52 kills**. The copy/read/
write profile contains 36; the metadata/namespace publication profile contains
16. Read/write sampling covers large partial copies and both verification
passes. Sync coverage includes every source/target durability barrier, rather
than only the first sync callback. It is not a sweep of every read/write offset
or interruption inside a kernel syscall.

Before exchange, the canonical image must be byte-for-byte identical to the
66,154,496-byte original. After exchange, it must exactly match the independently
compacted **63,074,304-byte** control. Each is opened by a fresh checker, which
verifies revision 212 and all current/retained data. Compaction is then retried
while any interrupted scratch name still exists. The result must match the
compacted control exactly and pass another fresh checker.

Partial target files can remain before exchange; an old source inode can remain
under the scratch name afterward. Tests do not treat these names as authority
or assume engine orphan removal. Only after canonical verification and successful
retry does the supervisor retire its own fresh case directory. The original
source file remains byte-for-byte unchanged across all cases. Real process
termination exercises leases and filesystem namespace behavior; it does **not**
simulate losing kernel/device caches or qualify physical power-loss durability.
The existing small-image namespace persistence model retains its separate scope.

## A second failure during recovery

Each case starts from a fresh copy of the near-cap image and one of six states.
The first operation changes writer zero from sequence four to five. Its
append/root/witness positions come from the actual successful I/O trace.

| Initial state | First operation result | Locally acknowledged floor | Root before failed recovery |
|---|---|---:|---|
| Append sync fails, drops pending persistence | Io | 212 | 212; new append only volatile |
| Append sync persists but returns error | Io | 212 | 212; durable unreachable append |
| Root sync fails without persisting pending root | OutcomeUnknown | 212 | 213 visible, root not durable |
| Root sync persists but returns error | OutcomeUnknown | 212 | 213 durable, unwitnessed |
| Witness sync fails without persisting witness | OutcomeUnknown | 212 | 213 durable |
| Commit acknowledged | Ok(213) | 213 | 213 durable and witnessed |

The volatile-root case deliberately closes the old handle **without a crash**.
Recovery can therefore see a complete root that its first sync has not yet made
durable. Treating it as already durable would hide the critical second-failure
case. No first-error result exposes an advanced in-process acknowledged head.

Nine recovery failures are crossed with both external crash policies, DropAll
and PersistAll:

1. First metadata read returns Io.
2. Last graph read returns Io.
3. Stabilization sync returns Io without persisting pending bytes.
4. Stabilization sync persists pending bytes but returns Io.
5. Witness write tears after 63 bytes, volatile prefix, and returns Io.
6. Witness write tears after 63 bytes, durable prefix, and returns Io.
7. Witness sync returns Io without persisting pending bytes.
8. Witness sync persists pending bytes but returns Io.
9. A **40 MiB core budget** rejects open with Limit, returning no handle.

The first/last read and all recovery mutation positions are calibrated from an
actual successful open. Injected I/O cases require the exact trace prefix through
the fault. The budget case is core admission rejection, not a fabricated system
OOM or a physical memory exhaustion claim. Normal recovery/checker opens use
the existing 96 MiB requested-core limit.

For the volatile 213 root, revision after the second failure/crash is:

| Failure point | External DropAll | External PersistAll |
|---|---:|---:|
| First/last read, failed nonpersisting stabilization, or Limit | 212 | 213 |
| Persisting stabilization, either witness tear, or either witness sync failure | 213 | 213 |

Append-only initial states always reopen **212**. Durable-root and acknowledged
initial states always reopen **213**. The acknowledged state never drops below
its 213 floor. An error while opening returns no database; it does not promise
that the earlier uncertain commit failed or authorize falling back from a
verified durable newest root.

A further failed open is then injected at the recovery witness sync. Its trace
is calibrated in a **separate medium**, so the actual medium has no successful
recovery handle between the two failed opens. DropAll follows again; normal
recovery must preserve the same selected revision and all retained roots. These
steps also verify lease release: a leaked writer lease would yield Busy rather
than the required I/O failure or successful reopen.

Finally, if 212 survived, a fresh transaction applies the intended update and
acknowledges **213**. If 213 survived, a metadata-only transaction acknowledges
**214**. Another crash and a fresh-process full oracle require the corresponding
213/214 state, writer zero at five, other writers at four and all historical
workers at zero. No follower durable receipt is manufactured by these local
operations; acknowledgment boundaries are unchanged.

## Continuous tests and qualification

`repeated_recovery_failures_preserve_retained_roots_and_acknowledged_floor` adds
42 smaller cases to the normal durability integration target. It reuses the
existing model, trace and barrier helpers, captures two distinct retained roots,
and checks independent copied current/retained values and captured revisions
after two failed opens, then current state/catalog after a later acknowledged
publication. The large fresh-process matrix checks full retained values again. The large matrix
lives in the private resource harness, outside default fast workspace tests.

The [torn metadata sweep](../DESIGN.md#pub-v2-persisted-prefixes-in-reused-root-and-witness-regions)
later extended those 42 small-fixture cases to 156 and added 280 publication
cases. It covers every distinct persisted prefix for the chosen reused metadata
regions, including both slot parities during publication. The near-cap matrix
above retains its original selected offsets and case count.

- Core-package all-feature debug/release: **308 tests per Mac profile** and
  **312 per Linux profile**, including the new 42-case regression. Each profile
  has one intentionally ignored parent-invoked subprocess helper. Root/test
  sources are unchanged between the initial verification checkpoint 2b42c1c
  and final harness-only sync expansion 3136017.
- Strict workspace/core Clippy and private-driver Clippy pass on Mac; full
  Linux core tests and strict final-driver Clippy also pass. Root/private-driver
  formatting and whitespace checks pass. No new unsafe allocator/API code or
  dependency is introduced; no new sanitizer/extended fuzz campaign is claimed.
- Two new helper controls pass normally and optimized under Mise Python 3.14.8.
  The complete Mac helper registry passes **338 tests + four registered skips**,
  342 reported across **50 workers**. Actual Linux records also pass the same
  source-bound matrix/byte validators in the Docker observer.
- Final native/recovery workers run on macOS arm64/APFS and Linux aarch64/private
  ext4 `/dev/vda1`, using immutable image
  `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
  Rust 1.99.1 is pinned; host Python is Mise 3.14.8, Docker observer Python is
  image-pinned 3.11.2. All owned native groups and Docker containers/volumes are
  verified retired. Source images and all 19 private manifests are rechecked.


| Profile | Cases per OS | Mac supervisor seconds | Linux supervisor seconds |
|---|---:|---:|---:|
| maintenance-copy | 36 | 63.29 | 59.72 |
| maintenance-publish | 16 | 35.36 | 33.23 |
| recovery-0 | 18 | 26.99 | 28.02 |
| recovery-1 | 18 | 27.33 | 28.35 |
| recovery-2 | 18 | 27.15 | 28.19 |
| recovery-3 | 18 | 27.23 | 28.38 |
| recovery-4 | 18 | 27.30 | 28.47 |
| recovery-5 | 18 | 27.32 | 28.49 |


Supervisor durations include calibration, file copies, process startup, complete
oracles and cleanup. They are test durations, not API latency percentiles. The
new state records retain `elapsed_ns=0` for schema compatibility; this is not a
measured zero-latency result. Supervisor RSS sampling excludes its children;
fresh-checker rows contain their own process sample. No whole-job RSS, stack or
worst-case latency bound follows from these measurements.

The [replacement ledger](decisions/replacement.md#near-cap-native-interruption-follow-up)
and [native ledger](decisions/native-storage.md#near-cap-maintenance-and-repeated-recovery)
record the new scope. Production source/AST decisions are unchanged: the prior
2,604-record / 319-blind-spot inventory remains historical source evidence, not a
new compiler MC/DC percentage. The existing namespace and graph/slot ledgers
are reused; these added drills do not manufacture new independent conditions.

### Sparse append recovery

The durability suite also simulates an append sync failure across eight pending
writes. It tests all 256 masked subsets and five torn prefixes per write, for
296 cases. The byte oracle verifies the exact physical end and sparse holes,
and read-only inspection finds the previous witnessed revision. Reopening
preserves the current rows and two retained snapshots. A successful retry
starts at the aligned physical end, preserves the abandoned bytes and reopens
with the expected rows. Its compact copy matches the uninterrupted control
byte for byte. A separate ByteStore regression verifies that persisting an
empty prefix leaves the durable length unchanged, including during a failed
sync. Both tests passed on macOS arm64 with Rust 1.99.1 in debug and release.

## Reproduction and provenance

```sh
cargo xtask resource resilience --revision 3136017 --output target/resilience-new
cargo xtask resource resilience --revision 3136017 --output target/resilience-linux-new --docker-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
cargo test --offline --locked -p xtask --bin xtask complete_resilience_matrix
```

The runner archives an explicit Git revision, overlays named private harness
files, builds locked/offline with Rust 1.99.1, overflow checks and feature bits
4 (fault injection), and checks the full runtime source ID, ABI 1 and format 2.
Sources, binaries and copied artifacts are rehashed after use. Each worker has
the existing 180-second deadline and an 8 MiB output bound; Docker has a
1,200-second overall deadline, no network, two CPUs, 2 GiB and 256 PIDs. Full
Linux tests use a separate 3 GiB volume/container and read-only registry cache.
All owned runtime groups, containers and volumes are verified retired.

Local proof index: `target/resilience-qualification/summary.json`, SHA-256
`4e9008ff2404122e0d9b3e71ae52bf9aa5f55158a7127165e8549513dbd33fa1`.
Frozen source/binary/report/control-image hashes and cleanup checks are retained
there. Temporary per-case images are retired after verification;
local ignored evidence is not a permanent hosted release bundle. Earlier smoke
and first-occurrence-only profiles are superseded by the full sync cut set;
their counts are not credited to the final 52-kill qualification.

Remaining work includes other graph/history/transaction shapes, longer schedules,
all-offset/torn-prefix fault sweeps, maintenance ENOSPC/quota/device failures,
arbitrary longer recovery-failure sequences, delivered/network/native Windows
profiles and physical power loss. These finite Mac/Linux results do not qualify
those unexecuted gates. One immutable candidate still needs its complete artifact,
sanitizer/fuzz/compatibility matrix and owner budgets. All **19** Cargo manifests
retain `publish = false`; crates remain unpublished.
