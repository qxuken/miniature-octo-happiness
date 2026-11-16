# Snapshot flush schedules and Linux filesystem qualification

Recorded 2026-11-17 on base `9f086d3` with the source overlays below.
This is a scoped test/tooling checkpoint, not a new whole-library release
qualification. All Cargo packages remain unpublished.

## Failure and cause

The first Linux schedule campaign stopped with `Io` where the oracle expected
success. Its retained input is `0000f3f308010f0001010a00`: install revision 3,
truncate revision 4, retry the older offer, reject a malformed header, install
revision 4, then poll. Replaying that input 100 times passed both in a fresh
stable build and in the original ASan binary. It was not a deterministic
schedule-model error.

A fresh instrumented campaign reproduced a zero-byte file read at offset
16,384, requesting 4,096 bytes, while the same file's metadata reported 32,768
bytes. There was no native errno. A standalone `std::fs` program, without
database calls or a sanitizer runtime, reproduced the same premature EOF after
writing and syncing 32,768 bytes: iteration 23, offset 0. The affected Docker
host bind mount reports **`fuseblk`**. The same standalone binary completed
20,000 iterations on container overlay storage, and the diagnostic snapshot
binary completed 10,000 schedules there. This isolates the observed problem to
the host-shared filesystem configuration; it does not identify a particular
FUSE driver defect or qualify overlay storage for durability.

The maintained native-file regression subsequently failed on that host bind
at iteration 20 of its third invocation, again reading zero at offset 0 from
a 32,768-byte file. Earlier invocations passed: the fault is intermittent.
Moving only runtime files was insufficient: compilation with a host-bound
build directory also failed while writing a libc archive (`Bad address`, OS
error 14). Final Linux builds **and** runtime files used a Docker named volume
whose `findmnt` result was ext4.

No engine read, publication barrier, recovery check or acknowledgment rule was
relaxed. `read_all` still rejects zero progress. The correction is to the
qualification setup, with an admission check to prevent recurrence.

## Maintained changes

- Snapshot schedules now distinguish response write failures from response
  flush failures at Ready and terminal acknowledgment. They check accepted
  bytes, write/flush attempt counts, input consumption, and current/retained
  state after every reopen. Accepted local bytes do not prove receipt by a
  peer. A terminal response failure can follow durable follower publication.
- Six flush seeds and the captured interrupted-upload sequence are retained.
  Two new stable tests cover that sequence and standalone native file reads.
  There are now 251 seeds and 31 stable replay tests; the target count remains 22.
- Assertion diagnostics identify the command, prior installed state and native
  storage context. Expected outcomes remain exact.
- `cargo xtask fuzz --linux-ext4-runtime DIRECTORY` creates an owned child
  outside the source checkout, records `findmnt`, and rejects any filesystem
  other than one covering ext4 mount. Each target's recorded command explicitly
  sets `TMPDIR` to that child. Cleanup and retained-report verification bind
  that same directory. Generic fuzzing remains available without this option.

For Docker Linux qualification, place the build directory, runtime parent,
corpus, artifacts and evidence on the named ext4 volume. Mount source and
registry read-only where practical. `cargo fuzz` also creates a default
`fuzz/artifacts/<target>` directory even with an explicit artifact prefix, so
provide writable native storage there or use a source copy on the volume.
For example, inside the prepared Linux container:

```sh
cargo xtask fuzz --target snapshot_install_schedule \
  --toolchain nightly-2026-11-01 --linux-ext4-runtime /runtime \
  --build-directory /runtime/asan --output /runtime/fuzz-proof \
  --runs 1000 --seconds 90 --max-len 12
```

The runtime option does not relocate or admit the build directory; the caller
must place build outputs on native Linux storage too. Ext4 admission is a
profile check, not proof against physical power loss or every filesystem fault.

## Verified results

| Check | macOS arm64 | Docker Linux arm64, ext4 |
|---|---|---|
| Stable Rust 1.99.1 replay tests, debug and release | 31/31 each | 31/31 each |
| Fuzz lib/tests/examples warnings-denied Clippy | Pass | Pass |
| Snapshot schedule ASan, seed 1, 12-byte cap | 1,000 actual / 65 seconds | 1,000 actual / 8 seconds |
| Snapshot frames ASan, seed 1, 262,144-byte cap | 1,000 actual / 0 seconds | 1,000 actual / 0 seconds |

ASan toolchains were `nightly-2026-11-12` on macOS and
`nightly-2026-11-01` on Linux, with cargo-fuzz 0.13.2. The Linux frames build
succeeded, but its first wrapper invocation stopped before execution because
the default artifact directory was read-only; the freshly built target then
passed when executed directly with native output directories. That setup
failure remains in the evidence.

The maintained controller's actual negative control rejected the `fuseblk`
runtime before launching a target and removed its scratch directory. Its
positive ext4 control completed both targets with 1,000 actual runs each,
explicit runtime arguments, unchanged source/seeds/lockfile and verified child
cleanup. This extra controller check used a 12-byte cap for both targets;
the broader frame run above is separate. Six controller tests and strict
all-target xtask Clippy passed, including retained-report tampering checks.

Source SHA-256:

| File | Digest |
|---|---|
| `fuzz/src/snapshot.rs` | `5494ffef0ccb1a202a2b7f0874d38844742d06e2f38a6a95d6819892498b6969` |
| `fuzz/examples/snapshot_seeds.rs` | `4c08993ec89843cc8234d8d042f2fcdfa7108e4bb9ebf4b4ddd63b57e7456e68` |
| `xtask/src/fuzz.rs` | `4ead6868462757555c71f051383530c9b73bc5d3c60d6894ea654e8d41fb4d76` |

The local archive `target/archived-proof/snapshot-flush-filesystem-9f086d3.tar.gz`
has SHA-256 `bf0e0e5e8c19e2a73a5b9f787d9d8eee70febf5303f6f2d2c2bb239c040b4c35`
and retains 199 indexed payloads: failed and successful logs, input, source overlay,
standalone reproducer, controller process reports, container configuration and
the actual Linux/macOS test and target binaries. Binary hashes were captured
after execution. The parent reviewed the raw counts, rejection point, explicit
runtime arguments and cleanup reports, then restored the archive and verified
all 199 payload sizes and hashes. These bounded checks do not replace
the full 22-target extended campaign, Windows qualification, physical-device
testing or the remaining production acceptance gates.
