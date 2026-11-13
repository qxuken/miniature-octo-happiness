# Delivered C ABI publication failures and independent recovery

This private Linux gate exercises the **existing packaged shared library** from
`21e5d2638c24c6254ced35d0077c55bb30a287b2`. It does not rebuild or modify the
engine. A C client compiles against the relocated delivered header/library at
`-O2 -UNDEBUG`; assertions, ABI 1, format 2, feature bits 3 and the full embedded
source identity are checked. Loader diagnostics identify that exact library.

## Cases and selected revision

Each case starts with acknowledged current revision 3 and retained ID 1 at
revision 2. The candidate atomically changes a 32,768-byte row, adds a 65,536-byte
payload and records application request 42. The expected outcomes are:

| Case | Mechanism | Commit result | Live revision | Fresh process reopens |
|---|---|---|---:|---:|
| Baseline | No injected failure | `OK`, output revision 4 | 4 | 4 |
| Append sync | Interposed first target `fsync` returns EIO | `IO`, output 0 | 3 | 3 |
| Root write | Interposed root-slot `pwrite` returns EIO before writing | `OUTCOME_UNKNOWN`, output 0 | 3 | 3 |
| Root sync | Interposed second target `fsync` returns EIO after root write | `OUTCOME_UNKNOWN`, output 0 | 3 | 4 |
| Witness sync | Interposed third target `fsync` returns EIO after witness write | `OUTCOME_UNKNOWN`, output 0 | 3 | 4 |
| Disk full | Kernel ENOSPC on a private 8 MiB tmpfs | `IO`, output 0 | 3 | 3 |

EIO is a **single-threaded syscall-interposition fixture**, not a physical device
failure. The root/witness sync cases leave successful writes visible in the
running kernel's page cache. Their fresh-process revision 4 result does not
predict what survives power loss. The simulator's persistence model and real
device qualification retain their separate obligations.

The C oracle scans every expected key and checks every payload byte in current
and retained snapshots. Revision 3 contains only the old row; revision 4 contains
exactly `payload`, `request` and `row`. The complete retained-ID list and old
revision-2 row are checked throughout. A partial candidate cannot pass merely
by reporting the expected revision or request marker.

Every commit error is terminal: a repeated commit returns `CLOSED` and clears
its nonzero-seeded revision output. After IO or uncertainty, a new transaction
returns `RECOVERY_REQUIRED`; it too becomes terminal with output zero. Readable
live snapshots still show complete old revision 3. All values, scans, snapshots,
transactions and database handles are released under the ABI lifetime contract.

The exercising C process closes and exits. A **new C process without the
interposer** then opens the canonical file, verifies the complete selected
state/history and reconciles request 42 before retrying. If its marker is absent,
one retry commits revision 4. If present, no duplicate effect/commit occurs.
Both paths finish at revision 4, close/reopen again and preserve retained
revision 2. This is process replacement after a returned error, not a SIGKILL
inside publication or an operating-system restart.

## Calibration and bounded exhaustion

The interposer filters by the database's actual device/inode. A different file's
`fsync` must neither consume the armed failure nor increment target counters.
The baseline requires exactly three target syncs, one root-slot write and one
witness write. Each injected case requires exactly one hit at its expected
phase. These checks reject a missing hook or an accidentally different syscall
path rather than silently recording an unexercised failure.

A separately compiled shim disables injection while retaining observation.
Running the expected first-sync failure with it must abort at the C status
assertion; normal success cannot satisfy the negative control. Core dumps are
disabled. The shim is not a delivered library feature or API.

The ENOSPC case performs no synthetic error injection: a controlled ballast
file exhausts only the fresh container tmpfs. The C client checks tmpfs type,
capacity at most 8 MiB, an explicit runner flag and exclusive file creation;
ballast writes have an independent 16 MiB ceiling. The observed native errno is
28 and the injection-hit count is zero. Only that ballast is removed before
fresh-process recovery. This qualifies kernel exhaustion on tmpfs, not ext4
capacity exhaustion, EDQUOT, persistent-device behavior or host disk filling.

## Helper verification and source binding

Five controls pass under normal and optimized Python on Mac and Linux. They
check existing/dangling/package-contained output admission before external work,
strict typed case outcomes, separate exercise/recovery phase identities, false
completion rejection, and failed cleanup/daemon queries. Review caught a Boolean
mode accepted by Python's integer equality before merge; explicit phase integer
checks and Boolean/float negative cases now close it.

The runner copies and hashes its C sources, runner, controls and shared process
helper before execution. Every manifest-listed package file is checked before,
after relocation and after copyback. Binary hashes remain stable. The worker's
complete artifact inventory is rehashed by the controller before success.
Exercise and recovery outputs have separate schemas and process records; neither
an exercise summary nor a saved expected value can substitute for recovery.

Execution uses immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
aarch64 Linux, GCC 12.2.0 and Python 3.11.2. Host orchestration uses Mise Python
3.14.8. The container has no network, two CPUs, 2 GiB memory and 128 PIDs. Build,
injected-failure databases and proof files use a UUID private volume confirmed
as ext4 by `findmnt`; only the actual exhaustion case uses tmpfs. Native commands
have 90-second deadlines, the disabled-shim control 30 seconds and the container
300 seconds. Owned process groups are stopped/reaped; Docker cleanup attempts
both exact UUID removals independently and requires successful absence queries.

## Reproduction and evidence

With the already qualified Linux package and local image available:

```sh
mise exec python@3.14.8 -- python tools/delivered-faults/test_runner.py
mise exec python@3.14.8 -- python -O tools/delivered-faults/test_runner.py
mise exec python@3.14.8 -- python tools/delivered-faults/run.py \
  --package target/qualification-21e5d26/linux-package \
  --output target/my-fresh-delivered-c-faults
```

Final proof is `target/delivered-faults-365daf6/final-v3/`, with six exercising
processes and six separate recovery processes, plus the expected disabled-shim
abort. All 91 copied artifacts were rehashed, and both owned Docker resources
were absent. The original package manifest/library SHA-256 remain
`d0a1bbce2fc5b742fe843ca5e28b68b6dd7a128aca5eb994389a256b7ba990c2` /
`16a927abb639ed68de1fdcc375bc52d41270aa1ac06552b1132fd3942088ac64`.

| Record under `target/delivered-faults-365daf6/` | SHA-256 |
|---|---|
| `index-v2.json` | `bd54a434658c6dec2f713a576663091a2b3ed8d4bc49724fa832fccb8a39560f` |
| `final-v3/evidence.json` | `91b486f09332ade287052ec1d7cc58fbb47938463871ae5fde002974775ce86c` |
| `final-v3/result/evidence.json` | `6a40ad6d41ab1f36bedcac8b96d26dc9aea37c3bd1e584316e42f3c4a74d6136` |

Earlier `initial/` uses same-process close/reopen; `final/` and `final-v2/`
predate the strict phase-type control or Linux helper-control execution. They
remain source-bound historical evidence and are not relabeled as final-v3.
Ignored local proof is not a permanent retention promise. This gate adds no
native Mac/Windows, Rust/Odin fault-consumer, power-loss, compiler MC/DC or
unified release-candidate qualification. No remote or registry action occurred.
