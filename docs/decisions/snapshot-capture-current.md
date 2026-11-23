# Live snapshot capture and saved-image reopen

Live capture verifies the source and pins its roots before calling the
destination factory. It encodes, synchronizes, verifies and hashes the copy
before advancing the sender's verified revision. Accepting a follower's
durable receipt remains a separate operation.

## Admission pairs

| Decision | Independent controls | Observable result |
| --- | --- | --- |
| Live source identity or backward revision | Same identity at a supported revision, foreign identity at a supported revision, same identity below the sender's bound | Each rejection returns `ForeignStore` before the destination factory runs. |
| Saved descriptor enrollment or upper bound | Exact descriptor, changed enrollment only, revision above the bound only | Both invalid cases return `ForeignStore` before verification. |
| Ship enrollment or upper bound | Exact enrollment and admitted revision, changed stream only, revision above the bound only | Both rejections leave the request channel empty. |
| Exact byte length | Matching length or length mismatch | Mismatch returns `Corrupt` before the visitor runs. |
| Exact image digest | Valid digest or changed digest at the same length | Mismatch returns `Corrupt`. An independently matched digest still requires the selected revision, identity and witness to verify. |

The live destination fixture also checks invalid limits, a database without
durable storage and factory refusal. Invalid limits and missing durable
storage leave the factory uncalled. For an admitted source the factory runs
once, with a failing call preserving the sender's bound and a healthy call
advancing it. Saved reopen checks offer bounds, invalid limits and Busy while
the original export still holds its lease.

## Fault sweeps

The live fixture pins current revision 4 with value 2 and retained revision 2
with value 1. It records a successful simulator trace. The exact full-image
read at the end identifies the digest pass. Separate witness and graph reads
between the final sync and the digest identify independent verification.

| Stage | Operations faulted individually | Result |
| --- | ---: | --- |
| Preparation through append sync | 12 | `Io` |
| Destination root and witness writes and syncs | 4 | `OutcomeUnknown` |
| Independent witness and graph verification | 16 | `Io` |
| Digest length check and reads | 11 | `Io` |
| Saved capture reopen length checks and reads | 18 | `Io` |

Each fault stops at its expected trace prefix. An uncertain outcome concerns
the capture destination. The source remains healthy with byte-identical
durable contents and the same live-byte charge. The sender keeps its old
verified bound and follower receipt, and the new receipt still fails its
upper-bound check. A fresh destination succeeds on retry with the same
complete bytes and descriptor. Reading the copied image separately recovers
current revision 4 and retained revision 2 with their original values.

Saved reopen faults preserve the original bytes and leave the image reusable.
A healthy retry reopens the same descriptor with read-only I/O and ships its
exact bytes. Native interruption checks remain in the replication integration
suite.

## Decision evidence

The first identity control at `8b7278d` also used a revision below the sender's
bound. Removing the identity check left rollback rejection in place, and the
mutation went undetected. The corrected control matches the admitted revision
while changing identity. An independently older image holds identity fixed.

Arm64 macOS same-object condition coverage used nightly `708c94ef0`. These
counts describe the production conditions exercised by the replication unit
suite, with the populated shipping instantiation reported at line 332.

| Source condition in `src/replication/mod.rs` | True | False |
| --- | ---: | ---: |
| `206` descriptor enrollment mismatch | 1 | 29 |
| `206` saved revision above verified head | 1 | 28 |
| `215` digest mismatch | 1 | 15 |
| `250` source file identity differs | 3 | 118 |
| `250` source revision is older | 2 | 116 |
| `332` export enrollment mismatch | 1 | 19 |
| `332` export revision above verified head | 1 | 18 |

The exact mutation controls and input pairs establish the admission and
propagation behavior. The compiler counts establish which conditions ran.

## Verification

The freeze is `729d3bab7a5d08846ef8540edb3e01dae440765a`. Rust 1.100.0 runs
26 replication unit tests and eight native integration tests per profile on
arm64 macOS and Docker Linux arm64 on ext4. The integration suite retains its
existing maintenance-only manifest-growth case. Strict Clippy passes on both
hosts and for MSVC cross-compilation. Strict-provenance Miri passes the live
source and destination admission fixture on macOS.

Both profiles on each host detect all 22 named mutations. Five distinct
baselines pass in each campaign. Each isolated mutant exits 101 and records
the intended test as its only failure, with verified process cleanup. An
earlier concurrent Linux run exhausted the Docker volume during linking.
The qualification uses the serial replay after removing generated build
caches, with the same source and toolchain. Native Windows and deployed
transport and device qualification remain open.

The review binds 19 named decisions and three overlapping OR records, all
previously open. The source denominator remains 10,602 records.

## Reproduce

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --lib replication:: -- --nocapture
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --test replication
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-snapshot-capture- \
  --output target/r1/snapshot-capture-debug
```

Repeat with `--release` and a separate output directory. Filesystem, device
and transport fault qualification remain separate from this simulator fixture.
