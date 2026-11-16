# Follower installation timing and large-snapshot interruption

The [authenticated month workload](live-replication-month.md) was repeated with
native macOS arm64/APFS sending to the Linux arm64/ext4 follower. The release
follower built from `a6393e0` with a measurement overlay around its real receive,
verification and manifest calls. The sender used the unchanged `a39de6c`
release binaries. The overlay adds clocks, counts and two optional interruption
markers. It leaves image bytes, protocol rules and filesystem operations intact.

## Installation timing and memory

The run completed 31 installations from 2,396,160 to 36,851,712 bytes and
recovered the four deliberately discarded ship replies by fresh TLS polls. Both
hosts reopened revision 93. The follower verified all 31 retained roots and the
selected image matched the final export byte for byte.

The table uses nearest-rank percentiles over the first 30 daily installs,
excluding the final catch-up. Receive includes transport and writing image
regions. Image sync measures the staged file's sync call. Digest and decode
each include file reads and CPU work. Manifest measures the selection
transaction, including its durable publication.

| Follower phase | p50 (ms) | p95 (ms) | Maximum (ms) |
|---|---:|---:|---:|
| Receive and write | 77.487 | 100.496 | 102.405 |
| Staged image sync | 7.205 | 9.549 | 9.963 |
| SHA-256 readback | 50.124 | 80.764 | 83.022 |
| Graph decode and verification | 45.407 | 81.922 | 84.334 |
| Manifest selection | 0.620 | 0.925 | 1.091 |

The Linux follower's recorded process high-water mark through the final
installation was 68,120 KiB (`VmHWM`). The peak combined core charge for the
two image graphs was 66,041,884 bytes. It sums the prior graph's steady charge
and the candidate's peak while the prior graph remains live. At final
selection the manifest had a separate live charge of 8,457 bytes. These
measurements describe the Docker process and its memory accounting. Filesystem
page cache and host footprint are outside the process RSS figure.

`Limits::max_live_bytes` applies independently when decoding each image. During
installation the follower retains the previous graph while it verifies the
candidate and publishes the manifest. Budgeting the service requires both
graphs, the separate manifest, TLS and allocator overhead. The previous graph
is released after selection succeeds.

## Process interruption at the two durability boundaries

A separate follower installed a verified copy of the saved 31-root revision 93.
The macOS source then acknowledged an empty transaction at revision 94. Both
attempts shipped the exact same saved revision-94 image and descriptor over
pinned mutual TLS. The test killed only the owned Linux follower process at
each marker. Both sender invocations exited 1 without a durable receipt.

| Interruption | Reopened state |
|---|---|
| After staged image sync, before verification and manifest selection | Durable revision 93, exact old selected image and unchanged manifest bytes |
| After manifest selection commits, before the durable reply | Durable revision 94 and exact captured revision-94 selected image |

An independent verifier checked all 31 retained views and the current values
after each reopen. A fresh authenticated poll recovered the exact revision-94
receipt. Sending the same saved export again returned that receipt and left the
manifest and selected bytes unchanged. The result covers process interruption
on the Docker ext4 profile. Physical power interruption and other deployment
platforms remain separate qualification gates.

## Reproduction and evidence

The proof directory is `target/follower-phase-qualification/`. Its archived
source `source.tar` is the clean `a6393e0241235c6aaaab876dea47f1ab4ed22977`
tree. Extract it into `source/`, apply `profile.patch` with `patch -p1`, and
copy the saved `follower_phase_fixture.rs` into `source/examples/`. Build or
recover the original macOS sender and CLI as described in the
[staging results](fresh-image-staging.md#authenticated-month-replay).
`run-linux-follower.sh` records the archive layout, the previously captured
input and the exact native commands. Run it from the repository root with a
fresh `run/` directory. The Linux image is the pinned Rust 1.99.0 toolchain
used for the exact-source staging runs. The script bounds the sender and
checks process exits, on-disk data, copy bytes, TLS receipts and resource
cleanup. It preserves no generated private credentials.

The staged-allocation sweep is also part of this checkpoint. All 39 injected
failures leave the destination empty and restore the original live charge.
The sweep passed on macOS and Linux and under native macOS AddressSanitizer
with `nightly-2026-11-12` and a rebuilt standard library. Native macOS passed
all 142 core library tests and strict core Clippy.

| Evidence | SHA-256 |
|---|---|
| `source.tar` | `5e9c9718cff807ec9e8cbd9f5ff4d51f2788c62555f3271d929b6a7cabeb7ff2` |
| `profile.patch` | `ab42420d80c31a2e9b37968aab346856bf44a2f262fec2e44ecc459619cab93e` |
| `source/examples/follower_phase_fixture.rs` | `534066841d900b08b847c5d6203adde11fdb8d327a50f2deb1d07a6e086a7863` |
| `run-linux-follower.sh` | `52d3381cd4cf1c70445dc3e1a5b48feb1a7a49aa95ff2a5fccc9fe93b6487dc0` |
| `run/follower-phases.csv` | `ac59dc71910d7dd8ccef6b5e8dc32b0071e027df19dfc873ac4a88cdd8dd4c97` |
| `run/month.csv` | `965a99128631d8469020d951d609ec2f3345ece686c1ea041b83aedb7864f25d` |
| `run/interruptions.log` | `02b19d3e3b5fb58f9010a8f96d48b8250a4b92fffcd8dfe8cc742dc35c894e6f` |
| `run/first.wdb` | `d0da420bcf352c15b550ac00089e6c0f65bb3b114ab8ee33d6b678b03412c040` |
| `run/next.wdb` | `a4f26da47f1812d9e58777c325472fa8a863138990b4ee7235a95a6eb38fe7c1` |
