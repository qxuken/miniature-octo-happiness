# Custom ByteStore conformance

The [shared lifecycle test](../tests/support/bytestore_contract.rs) uses the
public `ByteStore`, `Database::create_on`, `Database::open_on`, `inspect_on`,
`backup_on`, and `compact_to` APIs. To run it against an adapter, supply a
factory for empty stores and a factory for fresh handles to those same
stores. Use a scratch namespace with three unused names. The test runs
against both the native file adapter and an independent test adapter that
transfers at most 251 bytes per operation. The independent adapter has
separate visible and durable images and uses no engine simulator code.

The lifecycle requires two consecutive `Busy` results from competing
handles, followed by a successful commit through the original lease. It
checks current and retained rows, identity, and storage counters after
closing and reopening the source, backup, and compacted copy. The rows
include a 6,200-byte value spanning storage regions. The independent adapter
also checks that write requests stay within a single 4,096-byte region.

## Recovery after sync failures

The [independent adapter test](../tests/bytestore.rs) stages revision 4 after
acknowledging revision 3 and retaining revision 2. Recovery starts with new
handles loaded from durable bytes alone. Failed commits preserve revision 3
in process, block retry with `RecoveryRequired`, and retain revision 2.
Read-only inspection preserves the durable image. Writable reopen
stabilizes the chosen root and makes its witness durable.

| Sync that reports an error | Its pending bytes reached durability | Commit result | Revision seen before writable reopen | Witness already durable |
|---|---|---|---:|---|
| Append | No | `Io` | 3 | Yes |
| Append | Yes | `Io` | 3 | Yes |
| Root slot | No | `OutcomeUnknown` | 3 | Yes |
| Root slot | Yes | `OutcomeUnknown` | 4 | No |
| Witness | No | `OutcomeUnknown` | 4 | No |
| Witness | Yes | `OutcomeUnknown` | 4 | Yes |
| All three return success | Yes | `Ok(4)` | 4 | Yes |

## Verification

Rust 1.100.0 tested source tree `5a9a2acb15276bad859054c8a8629523f4e3c2ba`
before the documentation and ledger-pin updates. Each profile passed 85 tests
on macOS arm64/APFS and 86 on Docker Linux arm64/ext4. The totals include the
three new integration tests and four low-level transfer tests. The Linux
build and scratch databases ran on loop-backed ext4 with networking
disabled. macOS Clippy passed with warnings denied. Both affected integration
targets cross-checked for `x86_64-pc-windows-msvc`. Native Windows execution
for the new test is pending.

```sh
for profile in '' --release; do
    cargo +1.100.0 test --offline --locked --all-features $profile \
        --test bytestore --test files --test durability --test storage_decisions \
        --test inspection --test compaction --test retention
    cargo +1.100.0 test --offline --locked --all-features $profile \
        --lib storage::transfer_tests
done
```

Two temporary changes to the independent adapter failed their intended tests
on macOS. Unlocking a lease owned by another handle let the second contender
past `Busy`. Returning success without persisting the final witness reopened
revision 4 with `witnessed == false`, where the acknowledged case requires
`true`.

`bytestore-contract-5a9a2ac.tar.gz` retains the source, commands, results,
and reconciliation report. Fresh extraction verified all 25 indexed files.
Its SHA-256 is
`0835e5dcd312fb595708901928f85897297c09f757636e15d8a35052e407b197`.

Adapters still need to establish lease exclusion for their actual resource
and establish their flush and torn-write guarantees on the deployed device.
The barrier matrix models all or none of the pending bytes reaching
durability at a failed flush. The existing simulator and fuzz harness cover
partial writes and other transfer faults.
