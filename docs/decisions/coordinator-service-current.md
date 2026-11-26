# Coordinator dispatch, receipts and journal recovery

| Decision | Passing case and independent rejecting cases |
| --- | --- |
| Request admission | The matching 64-byte credential admits the command. Missing prefix, missing separator, a short credential and wrong bytes of the correct length reject. A 255-byte line followed by a newline passes. A 256-byte line, missing newline, invalid UTF-8 and read errors reject. |
| Dispatcher | An admitted request commits. With shutdown, expiry, or loss of the admission receiver, the relevant queued request does not dispatch. The expiry and disconnect test observes the following live request commit and checks that both rejected slots remain empty. The shutdown test reopens the admitted revision and verifies the queued slot is empty. |
| Sequenced receipt header | A 33-byte version-one receipt decodes. The same receipt with a trailing byte rejects independently of the version. With the correct length, an unknown version rejects independently of the length. |
| Receipt history | A current canonical receipt passes. Holding the other predicates false, a zero sequence, inconsistent expected revision, sequence ahead of its committed prefix, and a revision newer than the selected root each reject. An overflowing expected revision also rejects. |
| Slot, store and predecessor admission | Slot 1023 is usable and 1024 rejects. A well-formed different store identity returns `ForeignStore`. A predecessor whose sequence is out of order or whose revision is ahead of the request's expected revision returns `Invalid`. A well-formed predecessor at the correct sequence with a different value returns `Conflict`. |
| Replay and fresh writes | Exact replay returns the original receipt. A changed expected revision and a changed value each reject on their own. The previous sequence reports retirement, skipped sequences reject and a fresh sequence from a stale revision reports the current revision and value. |
| Journal header and optional fields | Valid full and empty journal states round-trip exactly. Length, magic, reserved flags, zero identity and slot capacity each reject independently. An absent receipt or intent must have zero payload bytes. A malformed present receipt propagates `Corrupt`. |
| Pending intent | An intent on the receipt floor decodes. Changing only the expected revision to below the floor or to `u64::MAX` rejects. The enclosing exhaustion check stays open in the source ledger. Canonical receipt decoding bounds its stored sequence before journal decoding can use it. |
| Durable confirmation | Busy and malformed or stale replies preserve the intent for exact retry. The exact commit reply or a well-formed strictly newer conflict clears it. The simulator sweeps interruption during retirement and during local acknowledgment after a remote commit. Reopen exposes either the complete prior state or the complete durable successor. |

The new reopen case pins revision 4 with value 12 and receipt `1 3 12 4`,
then commits revision 6 with value 30 and receipt `2 5 30 6`. After reopening,
replay, retirement, conflicts and rejected requests leave both roots and the
durable image byte-for-byte unchanged.

The decision manifest binds the named source pairs. Additional controls cover
the credential comparison, exact predecessor equality and the stale-conflict
boundary. The delivered-artifact and operator checks keep their earlier source
and deployment scope. Local commit and follower durability remain separate.
Deployed operator acceptance and the remaining
R1 source and compiler gaps are still open for this batch.

## Verification

At `b0db5d6c03f110766351293482de7163a6e5c7e1`, Rust 1.100.0 passes
22 coordinator unit tests, 13 integration tests and 19 mutation-controller
tests in both profiles on arm64 macOS and arm64 Docker Linux on ext4. Two
macOS and three Linux integration helper entries are intentionally ignored by
the direct test harness. Strict coordinator Clippy passes on both hosts.
The Windows MSVC target compiles all coordinator targets.

All 32 named controls are detected in each of the four campaigns. Each
campaign verifies seven exact passing baselines and detects each mutant
through the designated test's failure. The audit checks 107 command reports
and 854 copied source hashes per campaign against the frozen checkout.

The reconciliation binds 19 direct controls and five enclosed decisions.
R1 retains 9,262 open records (1,877 runtime / 26 driver / 7,359 tooling).
The exact-test support for a single binary adds two unresolved tooling
records to the inventory. These counts describe source records.

## Hosted Windows and packaged recovery

The native Windows x64 job ran Rust 1.100.0 on the GitHub test merge
`215646848618dc2861fb12e5232e5bcd7b36c596`. Its Git tree
`922c9d34edccb6e8ac83c63fd43427debb2787cc` matches PR head `e40e0d9`.
Debug and release each passed 22 coordinator unit and 12 integration tests.
The two extended maintenance tests were ignored in both runs.
[Native job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45585317834/job/139105302370)

For hosted Linux x64 and Windows x64/fixed NTFS, the private coordinator
package ran the four pending-restore schedules in debug and release and
repeated them after artifact download. Both forwarding outcomes reopened
revision 5, value 42, receipt sequence 1 for prefix 0 and revision 6, value
99, receipt sequence 2 for prefix 1. All 32 schedule records confirmed the
second lost reply, stable local replay, two retained roots and preserved
originals. The Windows runner used its hosted administrator account.
Ordinary-account deployment remains open.

| Platform | Original package campaign | Download and rerun | Listed output hashes rechecked per archive |
| --- | --- | --- | ---: |
| Linux x64 | [Original](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45585317834/artifacts/15007518335) | [Rerun](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45585317834/artifacts/15008671441) | 34 |
| Windows x64 | [Original](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45585317834/artifacts/15009545236) | [Rerun](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45585317834/artifacts/15009605255) | 24 |

Independent review reproduced GitHub's four ZIP digests, checked the debug
and release recovery logs, and matched the package manifest digest between
each original campaign and its rerun. Each run retired the Cargo build paths
before invoking the saved CLI and driver.

The native Windows x64 campaign later detected all 32 coordinator controls on
the PR #82 test merge `a5d2ea15d88b71520223b676e6ad35a96ddbe10e` (same tree
as `5efa36d`). Debug and release each executed 32 passing baselines and 32
expected failures. Independent archive review matched the 859 copied source
hashes to the Git tree, verified every modified source and all 128 retained
test executables, and checked each command's actual arguments, exit, cleanup,
named test result and expected diagnostic. The ZIP SHA-256 is
`84a6692aaa48a0abac7bc5942fcf124f2c990678345eacde3d8b247f62999544`.
[Windows campaign and evidence](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45599897332/artifacts/15016951779)

## Reproduce

```sh
cargo +1.100.0 test --offline --locked -p write-coordinator
cargo +1.100.0 test --offline --locked -p xtask --bin xtask mutations::
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-coordinator- \
  --output target/r1/coordinator-debug
```

Repeat each test or campaign with `--release` and a separate mutation output
directory.
