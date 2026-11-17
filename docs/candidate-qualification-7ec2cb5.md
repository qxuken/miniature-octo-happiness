# Immutable candidate qualification at 7ec2cb5

This checkpoint tests merge `7ec2cb59b8bba2c9d1dda915f8ee880304fd730c`,
tree `d7cc0023644c2da2fd5526346fa379b9ab3074f0`: branch
`86acb7dc19957433b52bfb94cbee0de862ac5eca` into base
`693aaef6550867db7a8477dc88e77c26c6e44aa1`. Subsequent documentation commits
do not change the identity of these artifacts. All Cargo packages remain
`publish = false`; nothing is published to crates.io.

## Packages and executed checks

All sixteen minimum-Rust packages pass the strict public delivery verifier:
combined and isolated masks 0, 1, 2 and 3 on each of the following lanes.

| Lane | Filesystem | Rust | Workspace passes / ignored, per debug and release profile |
|---|---|---|---|
| Native macOS arm64 | APFS | 1.99.0 | 731 / 6 |
| Docker Linux arm64 | ext4 | 1.99.0 | 765 / 8 |
| Hosted Linux x64 | ext4 | 1.99.0 | 765 / 8 |
| Hosted Windows x64 | NTFS | 1.99.0 | 592 / 6 |

Every package passes C11, C++17 and Odin smoke consumers and relocation.
Combined packages also pass fresh Rust source consumers and the independent
134-cell Rust/C/Odin trace. Windows additionally executes that complete trace
through C++; the Unix C++ result is a smoke/relocation check. The Windows
controller's individual source inventory covers 26 tooling files; complete
application/model source identity is supplied by its retained exact clean Git
checkout, rather than an invented full individual-file inventory.

The strict matrix/benchmark/fuzz report SHA-256 is
`c2b043021348a4ca19d9cf4c595a99298efa3449b393dd63b3e4f991cd333f72`.
Independent review replays the verifier byte for byte and checks 13,666 input
hashes without changing the original reports or rerunning product tests.

## CI, sanitizers and coverage

Engine run `43900770660` and fuzz run `43900770844` bind to this exact tested
merge. All fourteen required job results pass, with one recorded hosted-capacity
retry. The original Engine attempt executes twelve successful jobs; its
coverage job never acquires a runner and is cancelled. Its annotations, empty
step list, runner ID zero and unavailable-log response are retained. One
targeted rerun executes only coverage successfully. The twelve other job
results are reused, with matching original timestamps and byte-identical logs;
they are not twelve additional executions. The original cancelled attempt
remains part of the record.

Pinned rebuilt-std macOS workspace ASan and TSan each pass 731 tests with six
ignored. TSan includes the previously intermittent detached-session control
and all twelve selected Mac regression names. The macOS-only bounded EPERM
retirement fix is exercised on the same source as the packages. Linux executes
the portable error-reason control; Windows keeps its platform-specific path.
The calibrated hosted Linux C/Rust ASan job also passes.

Supplemental local library coverage at Rust 1.99.1 measures 10,972 / 12,108
lines (90.62%) and 20,229 / 22,755 regions (88.90%). It measures zero MC/DC
conditions. This is engine-library coverage, not whole-workspace coverage or a
compiler MC/DC claim, and does not replace the separately retained hosted job.

## Fuzzing and benchmark evidence

The fresh extended ASan campaign runs all 22 targets with seed 3 and individual
100,000-run / 120-second bounds, 2,048 MiB RSS limits and 262,144-byte maximum
inputs. Raw completion lines total **1,442,916 executions**. All 244 seed inputs
remain unchanged, artifact directories are empty, and bounded process cleanup
passes. Stable replay separately passes 29 tests. Actual produced fuzzer
executables and compiler identity are retained; these diagnostic executables
require their matching runtime/toolchain. The fuzz build does not rebuild std.
Fuzz evidence SHA-256:
`6cab135ac80ae15e657b8fd3c63b0cfeea8746945230b1eaa4ce758b1b350d79`.

Three native macOS samples execute all seventeen compact microbenchmark cases
before the heavier parallel qualification work. Raw measurements, independently
recomputed median/minimum/maximum and the actual benchmark executable are
retained. The public verifier binds the measurements and inputs; independent
review additionally checks the retained executable and build metadata. These
small-state measurements establish no deployment latency or throughput SLO.

The benchmark, incident and fuzz controllers each snapshot 330 scoped files and
twelve locks. Separate complete Git/source checks and the Linux before/after
gates cover 746 tracked files and all fourteen locks. Those scopes remain
distinct. All fourteen locks exactly match the fresh
[3db5bcd dependency review](locked-dependency-review-3db5bcd.md), which uses
RustSec `02e2517f` and parses 1,509 advisories with zero reported
vulnerabilities/warnings. This is exact-lock carry-forward, not a new advisory
scan or legal/source-security approval.

## Recovery and distributed workloads

Fresh macOS/APFS and Linux/ext4 administrative/coordinator incident rehearsals
each pass 38 expected outcomes. They verify current and retained roots,
corrupt-backup rejection, fresh restore, exact journal receipts, stale-history
rejection and retry. Actual CLIs, fixture consumers, images and process reports
are retained. Selected artifact source fields are operator declarations
corroborated by source/build/environment/binary evidence. A local coordinator
receipt is not a follower durable acknowledgment.

The maintained Rust Study peer controller executes thirty offline/sync rounds
between native macOS/APFS and Docker Linux/ext4. Sixty offline local edits and
six paired post-acknowledgment SIGKILL/restart checks preserve the expected
values and revisions. Both peers finish at revision 62, total 1,395, weekly
goal 230, daily limit 10 and lessons 2 through 61. Repeated covering sync adds
no commits. The retained controller evidence SHA-256 is
`8c0a59939abbe29217bd9926b67c041a8a0bff5b6b6ad1b88486d146a11434f7`.
This is the existing default-output profile and an accelerated finite scenario,
not thirty days of continuous uptime.

The authenticated snapshot workload runs a native macOS/APFS primary against
a Docker Linux/ext4 follower on a separate private bridge with a host-loopback
port. Existing Rust `live_month` and TLS CLI binaries execute thirty logical
days: 31 installations, 32 polls and 63 authenticated exchanges. Four terminal
shipment replies are discarded on days 7, 14, 21 and 28; fresh polls reconcile
the exact durable receipts. Local commit, captured revision and follower
durable acknowledgment remain distinct while heartbeat writes advance the
primary. Both finish at revision 93 with gap zero. Independent reopening
checks all 31 retained views against the 32 known payload keys and heartbeat;
the follower's 36,851,712-byte selected image exactly equals the final export.

All twelve unique supervised operations pass, with bounded cleanup and no
timeout/cancellation. Source/lock/binary checks pass, only public certificates
cross hosts, synthetic private keys are removed before retention, and the
owned Docker container, volume and network are absent. The compact proof
retains 107 indexed payloads. Snapshot evidence SHA-256:
`93846021cf407f680a8c32fc8efb0540a9c64acd323bbfc2d22217e6bd97a005`.
This verifies the named finite known-key workload; it is not an arbitrary-key
oracle, WAN deployment, wall-clock month or performance-budget acceptance.

## Retention and fresh restoration

The local private archive
`target/archived-proof/private-delivery-7ec2cb5-checkpoint.tar.zst` has SHA-256
`a2d1b077acc3098f59a043da78a73457859f38f0b2033db98938c004ec860d82`.
It retains the exact source tar/bundle, actual binaries, all four package lanes,
original CI ZIPs/logs and retry history, benchmark/fuzz inputs, incident images,
distributed workloads, dependency-audit inputs and independent reviews.
Fresh extraction verifies all **28,824 indexed files**, totaling 4,889,551,822
payload bytes. The relative SHA-256 index digest is
`b1b9ec1176d8e6c7793b58a05f6a8d671dfbc0339b952157d553687a76bc0e19`.

Strict public verification using the restored original verifier reproduces
report `c2b043021348a4ca19d9cf4c595a99298efa3449b393dd63b3e4f991cd333f72`
byte for byte. An independent offline/locked Rust 1.99.1 build of the verifier
from a fresh exact-bundle checkout also verifies the restored inputs. Its
report differs only in `verifier_sha256`, which correctly identifies the
different actual compiler/path-dependent executable; replacing that one field
reproduces the original report bytes. Both processes complete with verified
cleanup. Initial supervisor-argument and missing-PATH build setup corrections
are retained separately; no product workload was retried.

The fresh verifier avoids depending on the original compiled-in source path.
Reproduction still needs the recorded platform SDKs, compilers and registry
inputs; this is not a hermetic compiler distribution. The archive remains local
and is not checked into Git or published. Original controller reports are
unchanged, and subsequent documentation commits do not become artifact builds.

## Acceptance limits

This candidate supersedes the need to consolidate the named package,
sanitizer, benchmark and extended-fuzz checks after the historical `6859d41`
supervisor failure. It does not relabel that failed checkpoint as green.
Source archives, actual artifacts and original reports retain their identities.

The owner still needs to accept a supported deployment, workload and resource
budgets, version/support policy and attribution review. Additional promised
compatibility transitions, feasible decision/failure pairs, broader workloads
and instrumentation profiles remain scoped obligations in the
[acceptance audit](production-acceptance-audit.md). Physical-device power-loss
behavior requires the actual OS/filesystem/device stack. Finite process kills,
simulator tears, capacity/quota tests and accelerated calendar workloads do not
establish that behavior or SQLite-level operational maturity. Local commit
acknowledgment and follower durable acknowledgment remain separate throughout.
