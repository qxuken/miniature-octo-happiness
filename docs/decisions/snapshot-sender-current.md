# Snapshot sender reply and capture boundaries

The sender updates follower progress after accepting a durable receipt from the
configured stream. It preserves the last accepted receipt across transport
errors. The caller can use a fresh channel to reconcile an uncertain terminal
failure.

## Reply outcomes

| Phase | Reply and fault coverage | Bytes sent on failure |
| --- | --- | --- |
| Poll | Empty before progress, Installed, unexpected Ready, Empty after progress, EOF, truncated Installed, wrong enrollment status, unknown status | Poll request |
| Initial ship reply | Immediate exact or newer Installed, Ready, Empty, EOF, truncated Installed, wrong enrollment status, unknown status | Offer header |
| Terminal ship reply | Exact Installed, unexpected Ready or Empty, EOF, truncated Installed, wrong enrollment status, unknown status | Offer header and complete image |

EOF and truncated receipts yield `Io`. A foreign enrollment status yields
`ForeignStore`, and an unknown status yields `UnsupportedFormat`. A decoded
reply with the wrong role for its phase yields `Conflict`. The tests check the
bytes consumed and sent, keep the prior progress intact and accept the exact
new receipt on a fresh channel. The fixture models sender behavior. Actual
follower publication after a lost reply is verified separately by the
[follower acknowledgment ledger](snapshot-ack-current.md).

## Source, destination and image boundaries

| Decision | Control |
| --- | --- |
| Primary construction and source sampling | Durable source passes. A volatile source returns `Invalid` before use. Live polling rejects an invalid source without writing a request, and propagates stale-progress `Conflict` after a healthy source sample. |
| Enrollment and offer bounds | Zero stream and zero file identity each reject independently. Offer endpoints pass, with one byte below and one byte above rejected. |
| Observed progress and revision gap | Missing progress yields no gap. A receipt ahead of the sampled local revision also yields no gap. Valid equal and lagging revisions report exact gaps. |
| Quiescent capture | Foreign and volatile sources leave the destination untouched. An occupied destination preserves its bytes and releases its lease. Retry advances the verified sender revision. |
| Native create and reopen | Foreign source leaves the capture name absent. A pre-existing destination returns `AlreadyExists` and preserves its sentinel. Missing saved capture returns `NotFound`. A healthy read-only reopen preserves the descriptor and exact bytes. |
| Visit, upload and digest | Failed length lookup returns `Io` before a callback. Empty image visits zero chunks. A healthy image advances through chunks, and a partial upload failure prevents terminal reply reads. A failed visit remains an error when hashing. |
| Final length validation | Direct helper control supplies a locked empty destination. It returns `Limit`, releases its lease and preserves the verified bound and prior receipt. Public capture generates its length through successful encoding and verification. |

## Dispatch records

| File | SHA-256 |
| --- | --- |
| `src/replication/mod.rs` | `69f34f20a61afbf73ace1f3fb442a06a7644f726a6357546b4cfad375bdf45cf` |

| Status | Target | Location | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/replication/mod.rs:302:9–302:14 | 10442..10447 | match | poll | Installed, Empty, and unexpected Ready exercised. |
| E | weekend-db::weekend_db | src/replication/mod.rs:337:9–337:14 | 11915..11920 | match | ship | Immediate Installed, Ready with upload, and unexpected Empty exercised. |
| E | weekend-db::weekend_db | src/replication/mod.rs:346:17–346:22 | 12383..12388 | match | ship | Exact Installed and unexpected Empty and Ready exercised after upload. |

## Verification

Rust 1.100.0 at `d5e06b2f293165a2bbee902d1e116ad3a66296be` passes 29
replication unit tests and nine native integration tests in each profile on
arm64 macOS and arm64 Docker Linux on ext4. The existing maintenance-only
manifest-growth integration case remains assigned to its own suite. Strict
Clippy passes on both hosts and for the MSVC cross-target. The reply fixture
passes macOS strict-provenance Miri.

Each host detects all 23 named mutations in both profiles, with ten distinct
passing baselines in each campaign. Each mutant executes one named test,
exits 101 and reports verified process cleanup. Native admission mutations
run the exact public integration fixture.

Same-object macOS condition instrumentation with nightly `708c94ef0` reports
the stream-zero operand true 2 times and false 72, and the file-identity-zero
operand true 1 time and false 71. It reports the offer rejection predicate true
6 times and false 1,887, and the prior-progress rejection predicate true 24
times and false 78. The image-digest visitor takes the loop body 15,512 times
and exits 1,629 times. The input pairs and named controls provide decision
evidence for the corresponding predicates and error paths.

The current inventory accounts for all 26 previously open records in
`src/replication/mod.rs` with 22 named controls, one compound overlap and
three verified dispatch records. All 10,602 source records still reconcile.
Native Windows, application transport and deployed device faults remain
separate qualifications.

## Reproduce

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --lib replication::
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --test replication
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-snapshot-sender- \
  --output target/r1/snapshot-sender-debug
```

Repeat with `--release` and a separate mutation output directory.
