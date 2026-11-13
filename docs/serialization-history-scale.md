# DATA lookup work through 64 retained roots

The explicit `--axis history` experiment qualifies unchanged engine source
`a56083c3e518abd5fe9196c51beb7bb4bce9d023` with the private serialization driver
and launcher overlaid on a Git archive. No production cache, emission order,
publication barrier or format changed. The existing default breadth/8-epoch
experiment and its historical evidence remain separate.

## Workload and oracle

Each fresh process creates eight keys and requests exactly 1, 8, 32 or 64
retained data epochs. Distinct mode allocates eight independent 16-byte values
per epoch; shared mode clones one value identity across those keys. Bytes are
independently constructed from the epoch plus eight `0x5a` bytes. Both shapes
represent the same logical history. Each epoch commits all eight records and
then retains its snapshot. The driver verifies every key and complete payload
in current state and **every** retained root after each epoch, and again after
reopen, backup, restore and compaction. It checks current revision `1 + 2*n`,
retained IDs `1..=n`, retained revision `2*i`, and no recovery requirement.
Final current revisions are 3, 17, 65 and 129. No history case accepts `Limit`.

The launcher independently requires exactly the requested number of
commit/retain/verify phases, all maintenance phases in order and matching final
shape/width/history. Paired clean/probe runs must agree on history and file
sizes. Negative controls omit a whole epoch or restore phase, change the final
history and inject an early Limit. Six controls pass normally and under Python
`-O` on Mac and Linux; the existing breadth outcome checks remain. Strict Rust
Clippy passed. These are finite full-value oracles, not just round trips or
matching summary counters.

## Measured comparisons

The existing disposable instrumentation increments relaxed atomic counters
inside the actual `Encoder::data` and `Encoder::value` linear-search predicates.
It counts every examined entry, including early successful hits. Only the
instrumented copy of `src/storage/{codec,mod}.rs` has these counters/accessor;
clean binaries have neither. A clean zero counter means uninstrumented.

All measured counts agree across Mac/Linux and both paired variants' logical
oracles. DATA counts are independent of identity sharing in this workload:

| Retained roots | DATA comparisons | Distinct VALUE comparisons | Shared VALUE comparisons | Distinct append / compact bytes | Shared append / compact bytes |
|---:|---:|---:|---:|---:|---:|
| 1 | 12 | 112 | 28 | 81,920 / 61,440 | 53,248 / 32,768 |
| 8 | 516 | 8,064 | 1,120 | 397,312 / 319,488 | 167,936 / 90,112 |
| 32 | 16,132 | 130,560 | 16,768 | 1,478,656 / 1,204,224 | 561,152 / 286,720 |
| 64 | 108,036 | 523,264 | 66,304 | 2,920,448 / 2,383,872 | 1,085,440 / 548,864 |

An independent arithmetic check matches all DATA totals:
`1 + sum(k*k + 6*k + 1, k=1..n) + 3*(n*(n-1)/2 + 1)`.
The initial collection serialization contributes one comparison. Each epoch's
commit plus retention publication contributes `k*k + 6*k + 1`, including the
current DATA lookup and every retained reference. Each of backup, restore and
compaction serializes current first, then retained history, contributing
`n*(n-1)/2 + 1`. This is cubic accumulated work across a growing history of
successive publications; **one lookup remains linear**, and a publication with
many retained references can perform quadratic lookup work. The measured
64-root maximum stays far below the unchanged 64 MiB physical image cap.

## Clean timings and sampled memory

These single-run observations use clean binaries; atomic instrumentation can
perturb timings. Commit/retain columns sum all 64 respective operations. Reopen
is the original-file open; each other column is one maintenance operation,
excluding subsequent full-value verification. All intermediate-root timings,
probe timings and RSS samples are preserved in `summary-v3.json` and raw evidence.

| Platform / shape, 64 roots | Commit ms | Retain ms | Reopen ms | Backup ms | Restore ms | Compact ms | Sampled peak RSS bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| Mac / distinct | 627.613 | 591.326 | 20.695 | 39.825 | 41.173 | 58.125 | 8,699,904 |
| Mac / shared | 623.853 | 624.148 | 5.033 | 15.530 | 15.907 | 33.565 | 4,341,760 |
| Linux / distinct | 124.707 | 81.118 | 11.128 | 29.551 | 29.854 | 31.171 | 6,397,952 |
| Linux / shared | 69.596 | 72.584 | 2.882 | 8.256 | 9.725 | 10.598 | 2,994,176 |

External sampling spans the entire process, including maintenance and oracle
allocations, at a requested 20 ms interval. Failed samples are skipped and a run
with no samples rejects. Sampling misses short peaks, particularly tiny cases;
this is not a process RSS ceiling or per-phase allocation attribution. The
requested 128 MiB core budget does not cover all caller, backend, copy or runtime
memory domains. Concurrent work used the host; these are not cross-platform
performance rankings, latency distributions or service guarantees.

The larger history reveals DATA search work hidden by the eight-root profile.
It justifies considering DATA lookup when evaluating a future cache index, but
these small clean end-to-end costs do not independently establish a bottleneck
or a necessary optimization. No CPU attribution or index speedup was measured.
Any proposed index still requires charged allocation/failure tests, cache-reuse
qualification and fixed-identity byte/emission-order preservation.

## Reproduction and source binding

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/serialization-scale/run.py \
  --revision a56083c --axis history --output target/history-mac-fresh
mise exec rust@1.99.1 python@3.14.8 -- python tools/serialization-scale/run.py \
  --revision a56083c --axis history --output target/history-linux-fresh \
  --docker-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
mise exec python@3.14.8 -- python tools/serialization-scale/test_runner.py
mise exec python@3.14.8 -- python -O tools/serialization-scale/test_runner.py
```

Mac arm64 used Mise Rust 1.99.1/Python 3.14.8 and `/dev/disk3s5` (APFS).
Linux aarch64 used that immutable local image, Rust 1.99.1/Python 3.11.2;
`findmnt` recorded **ext4**, `/dev/vda1`, on the UUID-owned private volume.
Rust commit is `58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM 23.1.1.
Builds are release/overflow-checked, locked and offline; known ambient build
flags are removed and Cargo configuration hashes recorded. Compiler/config
inputs remain trusted. Linux had no network, two CPUs, 2 GiB memory and 128 PID
limits. Workloads have 180-second deadlines, builds 120 seconds, container run
900 seconds. There is no physical-device, Windows or power-loss qualification.

Final local evidence is `target/serialization-history-a56083c/`:

| Record | SHA-256 |
|---|---|
| `index-v3.json` | `e7a2f6b3a03dd3d1b82d3203fadfee9ce625ca52f7b53a46bf8a59cc642f97bb` |
| `mac-final-v3/evidence.json` | `5484ea38503b4ebc0d29361126963c610c177629409b7fae7fd18ee68babed95` |
| `linux-final-v3/evidence.json` | `05c755be708bb589ef10908bcb9e9362547ca2da0e2d69825ac3647f3da0df19` |
| Driver `src/main.rs` | `0909b8979b84c66038abec340e86539bf10ada0df1f3d72e65672fc7708c01e8` |
| Launcher `run.py` | `6a2d94cea9e84b2e1544a6a646d58fa29b6b30604414479fd923c3044c4115f3` |
| Controls `test_runner.py` | `877f28c453cf09c13bc02fe5854132e5dfd02ecb3fec52ddb2ef1c6b24a23ffd` |

Each OS completed 16 processes (two shapes × four histories × clean/probe).
Archives, five explicit driver/package overlays, original locks, complete
source inventories, binary hashes and logs are retained. Source and binary
bytes were rechecked after execution and again from copied Linux artifacts.
The Linux launcher copied results before removing only its UUID-owned container
and volume, then checked both absent. Review added a cleanup failure control: failure to remove the container must
not skip volume removal, and only successful live-daemon queries proving both
absent can qualify cleanup. The fresh `*-final-v3` runs also bind the compiler-selector correction below;
earlier `*-final` and `*-final-v2` results remain historical. Preliminary `mac/` and `linux/` runs used
the earlier filesystem probe; they remain historical, not relabeled as final.
Ignored local evidence is not a permanent retention or release promise. No
registry publication, hosted CI or remote repository action occurred.

Final review also reproduced a compiler-selection bug in environment cleanup:
stripping `RUSTUP_TOOLCHAIN` made a Mise 1.99.0 rustup proxy fall back to the
global 1.99.1 compiler. Cleanup now preserves both `RUSTUP_HOME` and
`RUSTUP_TOOLCHAIN` while removing build overrides. The environment control
checks both; an actual Mise 1.99.0 invocation through `clean_env` reports
1.99.0 (`selector-v3.log`). Fresh v3 runs above observed the requested 1.99.1
on both platforms. Earlier history measurements also observed 1.99.1, so their
counts remain valid in that stated compiler scope, but their launchers did not
reliably honor other requested selectors.

The subsequent [charged VALUE index](serialization-identity-index.md) is now
implemented and qualified separately. It preserves exact baseline bytes and
reruns both axes, allocation/budget/crash and retained-root checks. This report
continues to describe the original linear encoder and its original evidence.
