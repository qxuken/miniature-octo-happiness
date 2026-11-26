# Authenticated live replication across an accelerated month

The `live_month` example exercises an open Rust primary against the regular
`weekend-replicate serve` process over pinned mutual TLS. It runs 30 logical
days on a macOS APFS primary and a Docker Linux ext4 follower. The sender
keeps its native database lease from the first poll through the last shipment.
The [reverse lane](#linux-primary-and-macos-follower-at-9f942db) repeats
the schedule and checks duplicate delivery after reopening a macOS follower.

## Workload and oracle

The seed creates 32 keys with distinct 32 KiB values and a heartbeat, commits
revision 2 and retains it at revision 3. Each logical day `d` adds one payload
transaction, one retention commit and one heartbeat commit.

1. Replace all 32 values in one transaction at revision `3*d+1`. A row's
   bytes are `5*d+row`, so every retained day's values can be checked directly.
2. Retain that day's snapshot and capture revision `3*d+2`. The export includes
   all previous retained roots.
3. Send the offer over TLS. Before reading the follower's first reply, a
   source thread advances the heartbeat at revision `3*d+3`.
4. Complete the transfer. After the upload is flushed on days 7, 14, 21 and
   28, the test client discards the first authenticated byte of the reply
   and reports I/O failure to `Primary::ship`. The shipment row leaves
   `ship_revision` empty on those days.
5. Reconnect and poll. The current local revision is one ahead of the
   follower. The poll checks both the exact captured revision and its digest,
   including after each discarded reply.

The final exchange sends revision 93 and polls a gap of zero. The primary and
follower both reopen and verify every byte of the 32 current values, every
retained catalog entry and all 31 retained views. The selected follower image
is compared byte for byte with the final export. The schedule covers 30
payload commits, 30 retention commits, 30 writes while an offer is in flight,
31 new installations and 32 authenticated polls, including the empty poll.

## Reproduce

Build both executables from the same archived source on each host. This example
uses the same TLS pins, ALPN and socket deadline as the normal CLI.

```sh
cargo build --locked --release -p weekend-replicate --bin weekend-replicate --example live_month
```

On the primary, create a fresh source and generate a client credential in a
private directory. Keep the printed `identity` for follower enrollment.

```sh
identity=$(target/release/examples/live_month seed primary.wdb)
target/release/weekend-replicate keygen client-credentials
```

On Linux, generate separate server credentials, copy the client's **public**
`cert.der` to `client.der` and set `identity` to the printed value. Have a
private empty `follower` directory on the target filesystem. For Docker, put
that directory on an ext4 volume and publish the listener on loopback.

```sh
stream=11111111111111111111111111111111
target/release/weekend-replicate keygen server-credentials
target/release/weekend-replicate follower-init follower "$stream" "$identity"
target/release/weekend-replicate serve follower "$stream" "$identity" \
  0.0.0.0:9443 server-credentials client.der --max-exchanges 63
```

After the server prints `ready`, copy its public `cert.der` to `server.der`
on the primary. Set `address` to the published socket address. `exports`
must be a fresh path. Use a process watchdog for the server and the sender
in addition to the individual exchange deadlines.

```sh
target/release/examples/live_month send primary.wdb exports "$address" \
  client-credentials server.der > live-month.csv
```

After the server exits, run `live_month verify-follower follower "$identity"`
and the CLI `status follower "$stream" "$identity"` on Linux. Find the
selected slot in the status and compare its `replica-N.wdb` with the sender's
`exports/snapshot-31.wdb`. Check the digest against the last CSV record.

## Recorded result

The exact archive at `8539e7efb245604aa1b5ecc5000d5ab05c341288` built in
release mode with locked dependencies and passed. The sender ran natively on
macOS arm64/APFS with Rust 1.99.1. The follower ran on LinuxKit arm64/ext4
with Rust 1.99.0 in Docker image
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`.
The listener was published on the host's loopback. Strict app Clippy passed on
both platforms and for Windows MSVC x64 cross-compilation. Native macOS passed
all 34 app and example checks.

The run transferred 625,446,912 image bytes across 31 installs. The primary
file ended at 37,359,616 bytes, the final export and selected image were both
36,851,712 bytes, and the follower manifest occupied 430,080 bytes. The final
durable receipt digest was
`1844dfe2862bad11adf039ab70a30925c1fb18402662b28550df149198b52a97`.
All four lost replies were reconciled by fresh polls, and the final exact-byte
and history checks passed after both sides reopened.

The daily latency percentiles below use nearest ranks over 30 samples. Capture
includes encoding, sync, verification and hashing. Shipment includes connection,
TLS, the heartbeat write, upload and the durable reply. Post-upload time runs
from the sender's TLS flush to the first byte of the authenticated response.
It includes remaining transfer, follower verification and durable selection,
and the reply path. Poll includes a new TLS connection. The sample count makes
p99 the observed maximum.

| Sender phase | p50 (ms) | p95 (ms) | p99 (ms) |
|---|---:|---:|---:|
| 32-row transaction | 14.706 | 17.958 | 19.322 |
| Retention commit | 8.043 | 9.970 | 10.009 |
| Verified capture | 191.335 | 315.796 | 324.898 |
| Heartbeat write | 9.253 | 9.644 | 9.785 |
| Shipment | 190.821 | 279.809 | 283.511 |
| Post-upload wait | 105.472 | 176.276 | 181.222 |
| Fresh poll | 1.243 | 1.679 | 2.383 |

The full sender took 12.85 seconds and had a maximum RSS of 695,173,120 bytes
(about 663 MiB) with default limits. These are whole-sender observations across
the complete sequence, including reopen and history verification. The run
did not isolate allocator or follower-process memory and exercised one profile
over Docker's local network path. Device power loss, WAN interruption, steady
write rates and deployment memory/latency targets remain separate gates.

| Evidence under `target/live-month-8539e7e/` | SHA-256 |
|---|---|
| `source.tar` | `6a55503fe617b7b3b8288f8df33298a96718986b3ddd314d25a9918de7ec69a2` |
| `live-month.csv` | `8e0ca8ff56b3b840b7d765fe36c54b80ee84f9528b12bf14857e278cfda37e9d` |
| `percentiles-us.txt` | `a43cd15ce3fe7fe1462eb20cce8508ae857e8ca4049d4eba625dd3f37d6d5f3c` |
| `follower-verify.log` | `0e326f761d6bd3ad8bfea2f85f8e69559e56ce9d706765c81366701e2809d680` |
| `macos-sender.stderr` | `45f0ccb60a7337160f55125ba2bb5a9c0e870b4ae7f95c4a7eca17f075da477b` |

The run directory also retains platform/build logs, the final export and a
cleanup record. The Docker container, data volume and test credentials were
removed after verification.

The [segmented staging replay](fresh-image-staging.md#authenticated-month-replay)
repeats this schedule and measures about 106 MiB peak sender RSS. It retains
the 31-root and exact selected-image oracle and recovers the same four lost
replies by fresh polling.

## Bounded admission with a stalled TLS handshake

At `8127de34f3fe84a4cd2d1ce2f668e24ff6635076`, the release sender ran
on macOS arm64/APFS with Rust 1.99.1, and the release follower ran on
Docker Linux arm64/ext4 with Rust 1.99.0. The follower's manifest and
images were on a Docker volume reported as ext4 in mountinfo. A raw
TCP connection opened first and stayed open without sending a
ClientHello while the logical month completed. The follower listened
on a host loopback binding with a finite accept count of 64.

All 63 authenticated exchanges completed before the raw connection was
closed. The sender shipped 625,446,912 bytes across 31 installations.
Four dropped terminal replies were recovered by fresh authenticated polls.
The server exited after the stalled socket closed. Reopen verified
revision 93, all 31 retained views and exact bytes of the selected
36,851,712-byte export. The verified digest was
`375dc420a33a0433f7c34dd81b54d1da4d9e82c15ed9bbf031e5149dafda1dc6`.
The 31 recorded post-shipment polls had p50 11.459 ms, p95 20.036 ms and
maximum 23.049 ms in this local Docker network run.

The reproduction exposed the previous serial listener blocking a
healthy CLI poll behind the stalled handshake. Admission now bounds
concurrent handshakes and the ready queue separately. The Unix and
Windows socket-readiness calls are shared with the coordinator. The
queue test holds the follower and verifies that exactly four authenticated
channels are queued. The excess connection may close before the client
observes local TLS completion or during its first read. Another test
injects a follower worker recovery error and checks that pending raw
handshakes close during both finite and unlimited service.

macOS and Docker Linux each passed 79 ordinary application tests per
build profile for snapshot, study and coordinator in debug and release,
plus strict all-target Clippy. The opt-in coordinator cases and Linux
full-storage case were skipped by the ordinary suites. The opt-in
native follower manifest rotation test also passed on Linux. All nine
Windows MSVC x64 app test executables cross-linked, with native Windows
execution still pending for this source.

The local archive `target/archived-proof/follower-admission-8127de3.tar.zst`
contains the source, launch and verification scripts, process and test
logs, exact final export and audit. Extract it, unpack `source.tar` into
`verified-source/` and run `bash verify.sh macos` or `bash verify.sh linux`
for the corresponding checks. Run `bash run-month.sh` from macOS with the
pinned Docker image and locked Cargo cache for the cross-platform replay.
The archive SHA-256 is
`3e8b9ac4f8d097af25090bcea4ddbfe3f91e786db92b5d9aa032e2f615ed127e`.

## Linux primary and macOS follower at 9f942db

The exact archive at `9f942db9d0f3b62765d1ba7ff46408ea815672f3` ran the same
30-day schedule in the opposite direction. The native macOS arm64 follower
stored data on APFS and listened on an ephemeral loopback port. The Docker
Linux arm64 primary reached that port through Docker Desktop's host gateway.
The Linux source, build, temporary files, primary database and exports were
on ext4. Both builds used Rust 1.100.0, release optimization and locked
offline dependencies.

The 31 installations transferred 625,446,912 bytes in 63 authenticated
exchanges, including 32 polls. On days 7, 14, 21 and 28 the sender recorded
no shipment acknowledgment. The fresh polls returned revisions 23, 44, 65
and 86 respectively, one behind those days' local commits. The final poll
returned revision 93 with no gap. Both sides verified all 31 retained views
and the current values. The selected 36,851,712-byte follower image matched
the final export exactly. A fresh copy of the follower also reopened on
Linux/ext4 with the same revision, retained views and selected image digest.

A second macOS follower process opened a fresh copy of those durable files.
With separate synthetic credentials, the ordinary Linux CLI polled,
resent revision 93 and polled again. All three commands observed the same
durable receipt. The repeated export matched the original export, and all
three follower files and the Linux primary file were byte-identical after
the exchange. Reopening the copied follower again verified revision 93 and
all retained views. The selected digest was
`f88227f62ace073b551ddc644771b30522348cbd15a8073e38c13161800c5ba3`.

| Linux sender phase | p50 (ms) | p95 (ms) | p99 (ms) |
| --- | ---: | ---: | ---: |
| 32-row transaction | 2.410 | 4.124 | 4.376 |
| Retention commit | 0.194 | 0.999 | 1.165 |
| Verified capture | 78.538 | 148.724 | 151.210 |
| Heartbeat write | 0.620 | 0.851 | 0.855 |
| Shipment | 164.487 | 294.672 | 294.842 |
| Post-upload wait | 69.373 | 119.007 | 119.981 |
| Fresh poll | 0.672 | 0.813 | 0.897 |

These are nearest-rank observations from 30 logical days on one development
host. The macOS receiver's sampled peak RSS was 68,736 KiB. The Docker CLI
runs on macOS, so its reported RSS describes the launcher process.
Deployment timing, device durability, operational account approval and
off-machine custody remain separate release gates.

The first launch returned `NotFound` during enrollment because the harness
omitted the required empty follower directory. The successful run created that
directory and used a fresh primary and follower. The two bounded sessions
retired their worker processes and containers, and both sides' ephemeral
private keys were removed. The archive contains the app binaries, source,
reports and restored data. The `weekend-replicate` all-targets suite passed
111 tests per profile on macOS and 112 per profile on Linux, with one Linux
test requiring separate full-tmpfs setup ignored. Strict all-targets Clippy
passed on both hosts. Running the [opt-in full-storage test](follower-installation-profile.md#authenticated-follower-recovery-on-a-full-linux-tmpfs)
separately on an 8 MiB Linux tmpfs passed in both profiles. It verifies the
previous receipt at revision 4 while storage is full and the exact revision-5
receipt after space is available, including the retained snapshot and
duplicate-delivery checks.

To reproduce, use the build, enrollment and shipment commands above with
Linux as the sender and macOS as the follower. Start with an existing empty
follower directory and bind the macOS listener to `127.0.0.1:0`. Resolve
`host.docker.internal` to IPv4 in the sender container and use that address
with the printed port for the `live_month send` call. Once the first follower
has exited, use a fresh copy of its selected image, previous image and
manifest for the second follower. Allow three exchanges and use the normal
`poll`, `ship --attempts 1` and `poll` commands against revision 93. Compare
both poll receipts, the duplicate export and every durable file with the
originals.

The local `target/archived-proof/snapshot-linux-primary-9f942db.tar.zst`
archive includes the exact launch and repeat scripts. Fresh extraction
verified all 141 indexed payloads and ran the retained macOS binary against
the extracted follower files. The independent checker validates all 31 CSV
records, 18 terminal command receipts, both cleanup outcomes and six
negative CSV controls.
[Archive and evidence checksums](decisions/snapshot-linux-primary-9f942db.sha256)
