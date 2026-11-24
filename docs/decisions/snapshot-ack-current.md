# Snapshot follower acknowledgments

A primary's committed revision and the follower's durable acknowledgment are
separate. The follower sends `Ready` before reading an upload. It sends its
terminal `Installed` receipt after syncing and verifying the staged image and
durably selecting it in the manifest. The receipt binds the stream, revision
and digest. Following a transport error, a fresh poll reports the recovered
selection.

## Decision pairs

| Decision | Accepted and rejected controls |
| --- | --- |
| Follower offer older or identical | Older and identical offers independently return the installed receipt without upload. A different digest at the same revision conflicts. A newer offer proceeds to staging. |
| Follower Ready write and flush | A successful flush permits the upload. An injected flush error returns `Io`, leaves the image body unread, and leaves the candidate empty and the manifest untouched. |
| Follower image sync and manifest selection | Successful barriers select the new image. Image sync failure preserves the prior receipt. A manifest barrier failure withholds progress until recovery. |
| Follower terminal flush | Success returns the new receipt. Failure returns `Io` after the bytes were written to the test channel. Reopen and poll still recover the new receipt. |
| Sender Empty reply | Empty is accepted before any receipt. It conflicts once progress exists. |
| Sender enrollment and upper revision bound | A wrong stream and a revision above the verified source head independently reject. The comparison advances with the admitted stream and revision. |
| Sender rollback and equal revision | A lower revision conflicts. At an equal revision only the exact receipt is idempotent. A newer revision with a different digest advances. |
| Sender immediate Installed reply | A newer receipt or one exactly matching the export is accepted subject to stream and head checks. An older receipt and one with the export revision but a different digest reject. |
| Sender post-upload Installed reply | Only the exact export receipt passes. A newer receipt accepted in the immediate case conflicts here. |

## What reopens

The flush cases share one channel type and the same call site. The scripted
channel holds writes and flush results separately. Capture of bytes in that
channel shows the order of operations. A failed flush leaves network delivery
uncertain, so the test uses a fresh exchange for recovery.

| Failure or control | Receiver result | Reopened source revision | Retry |
| --- | --- | ---: | --- |
| Ready flush fails | `Io` | 3 | `Ready`, full upload, then `Installed` revision 4 |
| Terminal flush fails | `Io` | 4 | Immediate `Installed` revision 4 |
| Both flushes succeed | `Installed` revision 4 | 4 | Immediate `Installed` revision 4 |

The fixture verifies the exact receipt and the selected current value after
reopen. All three cases also verify retained snapshot ID 1, revision 2, with
its original value. The older, identical and equal-revision conflict replay
checks preserve the same retained snapshot and the selected manifest bytes.

The existing [image and manifest barrier cases](replication.md#r2-selected-image-and-recovery-barriers)
use a fixture without retained roots. They reopen source revision 2 after
image or manifest append sync failure. Root sync failure reopens 2 or 3
according to whether that root survived. Witness sync failure reopens 3.
This batch reuses those cases for the image sync and manifest selection
controls.

## Compiler condition counts

The same-object arm64 macOS library test run used nightly `708c94ef0` with
condition coverage. Counts below are for the nonzero fixture instantiation of
the generic `poll` and `ship` call sites, and the simulator `handle` and
ordinary `observe` functions.

| Source condition | True | False |
| --- | ---: | ---: |
| `follower.rs:198` older offer | 1 | 709 |
| `follower.rs:198` identical offer | 210 | 499 |
| `follower.rs:201` conflicting revision | 1 | 498 |
| `mod.rs:304` Empty before progress | 8 | 1 |
| `mod.rs:338` immediate receipt is newer | 1 | 11 |
| `mod.rs:338` immediate receipt is exact | 8 | 3 |
| `mod.rs:347` post-upload receipt is exact | 2 | 4 |
| `mod.rs:355` receipt has wrong stream | 1 | 91 |
| `mod.rs:355` receipt is above verified head | 2 | 89 |
| `mod.rs:359` receipt is older | 23 | 38 |
| `mod.rs:359` receipt revision is equal | 26 | 12 |
| `mod.rs:359` receipt differs at equal revision | 1 | 25 |

Counts establish condition execution. The input pairs and isolated named
controls supply the decision evidence.

## Verification

Checks ran against `1cf30143da15896a93457b4d1a21835067aaf19d` with Rust
1.100.0 on arm64 macOS and Docker Linux arm64 on ext4. Each debug and release
run passed 25 replication unit tests and eight native integration tests. The
existing manifest-growth integration case remains assigned to maintenance
qualification. All 16 controls reached the named assertion in both profiles
on both hosts, with five passing distinct baselines per campaign. Every
isolated mutant ran one test, exited 101 and had verified process cleanup.

Strict Clippy passed on both hosts and for the MSVC cross-target. macOS
strict-provenance Miri passed the three new flush controls in one test.
Native Windows runtime and deployed transport qualification remain open.

Eight previously open source records now have a direct or overlapping
disposition. The inventory also upgrades five historical follower records
to named source controls and binds their compound guard to its overlap.
The full inventory stays at 10,602 records.

## Reproduce

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --lib replication::
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --test replication
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-snapshot-ack- \
  --output target/r1/snapshot-ack-debug
```

Repeat in release with `--release` and a separate mutation output directory.
The terminal flush is a tail-result control. It has a named mutation without a
separate explicit decision record for the tail expression in the inventory.
