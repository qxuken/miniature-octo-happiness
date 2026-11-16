# Fresh image staging and repeated export memory

The [authenticated month workload](live-replication-month.md) measured about
663 MiB sender RSS with a final compact export of about 35 MiB. An isolated
release probe ran directly against that exact revision-93 primary, with all
31 retained roots. On macOS arm64/APFS, 16 inspections and 16 open/drop cycles
each peaked near 34 MiB RSS in separate processes. Sixteen exports from one
open source reproduced the high-water mark. Source live core charge stayed at
33,007,752 bytes throughout. An allocator snapshot taken after both source and
export handles had been dropped showed 562.0 MiB resident in `Malloc Large
(empty)` and 629.9 MiB of physical footprint. This isolates the repeated
fresh-copy path from TLS, follower selection and new source commits.

## Staging and publication

Fresh creation, compaction copies and logical import use charged 64 KiB staging
segments. The last segment can be smaller. The encoder calculates the same
object CRC over its header with the checksum field zeroed, its payload and its
zero padding. Both storage layouts share that encoder. Ordinary append commits
retain contiguous staging.

The full candidate still prepares before writing the destination header.
Segments end on protected 4 KiB boundaries, so the adapter sees the same
ordered write calls and the same data sync. Root-slot and witness writes each
have their own sync. The writer publishes in-memory state after the witness
sync, and the follower's durable receipt still depends on its verified image
and durable manifest selection. A fresh copy undergoes independent native
readback verification before it is returned.

## Evidence

The paired release probe uses baseline `1fdb642` and candidate `a39de6c` on
macOS arm64/APFS (Rust 1.99.1) and Docker Linux arm64/ext4 (Rust 1.99.0).
Both revisions produce the exact same 36,851,712-byte export as the original
month workload. The source image hash also remains equal.

| Sixteen exports | Baseline | Segmented |
|---|---:|---:|
| macOS process maximum RSS (`time -l`) | 660,832,256 B | 108,052,480 B |
| macOS wall time (`time -l`) | 5.40 s | 6.20 s |
| Docker Linux RSS after the 16th export (`ps`) | 66,728 KiB | 66,692 KiB |
| Docker Linux probe elapsed time | 6.13 s | 4.92 s |

These are finite runs. Elapsed time increased in this macOS pair. Linux shows
nearly equal sampled RSS. The Linux snapshot is a phase
observation and may miss peaks.

The baseline rejected the same 35 MiB export with a 64 MiB copy budget and
left the destination empty. The candidate succeeded with identical bytes.
The committed portable regression rejects an export at 512 KiB before writing
any durable bytes, then succeeds with the same destination at 800 KiB. That
800 KiB case failed with `Limit` before the change. The simulator comparison
also checks exact bytes, descriptor locations and ordered write/sync traces
between contiguous and segmented staging for objects crossing segments.

macOS passed all 662 all-feature workspace tests with six registered ignores,
plus strict workspace Clippy. The exact Linux archive passed 210 tests with
one maintenance ignore, covering allocation, compaction, inspection, logical
import, native interruption, replication, retention and golden bytes. Linux
strict core Clippy and Windows x64 strict workspace cross-Clippy also passed.

### Authenticated month replay

The `a39de6c` replay against the regular TLS follower on Docker Linux/ext4
again shipped 625,446,912 image bytes and reconciled all four lost replies.
Both endpoints reopened revision 93 and verified every value in the 31
retained roots. The 36,851,712-byte final export matched the follower's
selected image. This run used a fresh random file identity and recorded digest
`d0da420bcf352c15b550ac00089e6c0f65bb3b114ab8ee33d6b678b03412c040`.

| macOS sender | Original run | Segmented run |
|---|---:|---:|
| Maximum RSS | 695,173,120 B | 111,656,960 B |
| Wall time | 12.85 s | 12.70 s |
| Verified capture p95 | 315.796 ms | 317.058 ms |
| Shipment p95 | 279.809 ms | 292.585 ms |
| Post-upload p95 | 176.276 ms | 185.346 ms |

The original and replay are separate single runs. Follower RSS, disk-only
installation latency and deployment thresholds remain open.

The later [Linux follower profile](follower-installation-profile.md) measures
RSS, staged image sync and verification separately. It also kills the follower
before and after manifest selection and checks the same retained-history oracle.

The allocation sweep at `a6393e0` fails each of the 39 charged allocations in
the multi-segment creation fixture. It verifies zero destination writes or
syncs and exact live-charge rollback, then succeeds. The sweep passes native
macOS, Docker Linux and macOS AddressSanitizer (`nightly-2026-11-12`).

### Reproduce and inspect

The release probe source is recorded at
`target/live-copy-phase/source/examples/live_copy_phase.rs`. Copy it into
`examples/` in each archived checkout and build with
`cargo build --locked --release --features replication --example live_copy_phase`.
Use the revision-93 primary from `target/live-month-8539e7e/macos/primary.wdb`
or a freshly reproduced one from the month workload. Use a new empty staging
directory for each invocation. The arguments are the phase, source, staging
directory, repetition count and copy budget in bytes.

```sh
/usr/bin/time -l target/release/examples/live_copy_phase copy \
  "$source" "$staging" 16 9223372036854775807
cargo test --locked -p weekend-db --all-features --lib \
  replication::tests::live_copy_stages_large_values_within_the_copy_budget -- --exact
cargo test --locked -p weekend-db --all-features --lib \
  storage::codec::tests::segmented_append_preserves_the_bytes_and_adapter_write_boundaries -- --exact
```

The `inspect` and `open` modes use the same source and repetition count.
`WEEKEND_COPY_PROBE_PAUSE=1` holds the process for twelve seconds after dropping
the handles, allowing `vmmap -summary` to sample the resident allocator regions.
Compare the final `copy-15.wdb` to `snapshot-31.wdb` from the same month run.

| Evidence under `target/live-copy-phase/` | File or SHA-256 |
|---|---|
| `source.tar` | `af32348d3a993e8ff5643758889b08856be26c8779179f9be250c32135692798` |
| `candidate.tar` | `4efb632e1a677df35961a113d54b0c442f342c5d3d20f262c8a2254472e5a344` |
| Probe source | `aa0fa56521e1533a2cf4ad5d1de7c740ff7de260f039aebd84e40716486ca76d` |
| Original primary image | `840ceb2ff88b5743b251ff86c0534957eda63909b39dc1012a768561a009e181` |
| Reference export and both paired copies | `1844dfe2862bad11adf039ab70a30925c1fb18402662b28550df149198b52a97` |
| macOS isolation and allocator summary | `inspect.*`, `open.*`, `copy.*`, `vmmap.txt` |
| Paired macOS and Linux logs | `paired-baseline.*`, `paired-candidate.*`, `linux.log` |
| Budget and trace controls | `budget-before.log`, `budget-after.log`, `budget-64m*.csv`, `write-parity.log` |
| Authenticated replay `month/live-month.csv` | `ab935256e36d38135316e925344d65a4787affae2797db3b3025d4acc745778d` |
