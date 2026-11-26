# Coordinator legacy commit and recovery

The coordinator binary and standalone fuzz library exercise the same
legacy command parser and transaction path through separate Cargo targets.
The added tests preserve typed errors and inspect the acknowledged state
before and after refusal.

| Decision | Input and outcome |
| --- | --- |
| Collection lookup, counter lookup and caller propagation | A missing collection and a missing row in an existing collection both return NotFound through get, set and set-id. The revision stays unchanged. |
| Parsed expected revision and value | Invalid integers in either position return Invalid for each write command. A matching revision publishes one value and a stale revision returns the existing revision and value. |
| Draft and value limits | A plain update with a zero transaction-byte budget, a counter dependency beyond the observation limit, an oversized receipt value and a second draft write beyond the write limit all return Limit. The counter stays at zero and the receipt remains absent. |
| Recovery guard | A definite or ambiguous I/O failure preserves the acknowledged counter and makes subsequent commands return RecoveryRequired until reopening. |

The durable case begins at revision 4 with a prior receipt from
revision 3 and a retained root at that revision. The clean trace
identifies append, root and witness syncs. Each recorded I/O fails
under both discard-all and persist-all simulator crash policies.
The writer returns Io through append sync and OutcomeUnknown later.
A fresh open sees revision 4 with the old value and absent new receipt,
or revision 5 with the new value and receipt together. The root sync
case distinguishes which pending writes survive.

Retry returns the original revision if the receipt survived and
commits once otherwise. Another crash and fresh open check revision 5,
the same retry result and the exact retained receipt bytes and value.
The simulated fault sweep adds behavioral evidence for the commit
sites and their caller. The named mutations cover source decisions
that are enumerated by the inventory.
## Verification

The frozen `29286d9babfc5885a14a2a9be0ddf26e6107701e` source passed
with Rust 1.100.0 on macOS arm64 and Docker Linux arm64/ext4. Both
profiles passed 46 coordinator tests and 40 standalone fuzz-library
tests. Two macOS and three Linux coordinator workload tests remained
opt-in. Clippy passed with warnings denied in both Cargo targets.

Every profile on each host detected all 17 coordinator and all 17
standalone mutants at the intended assertions. Fresh extraction
checked 996 source hashes and 58 supervised command outcomes per
target campaign. The 32-case Windows oracle kept its exact case
selection and the source hashes match `ba39bb0`. Two Windows x64
coordinator binaries and the standalone fuzz-library test binary
cross-linked in each profile. Native execution remains pending.
[Campaign checksums](coordinator-commit-29286d9.sha256)

### Reproduction on Unix

```sh
cargo +1.100.0 xtask mutations --case r1-legacy-coordinator-commit- --toolchain 1.100.0
cargo +1.100.0 xtask mutations --case r1-legacy-coordinator-commit- --toolchain 1.100.0 --release
```

Reconciliation binds 17 records in each target, leaving 381 runtime
and 7,468 tooling gaps.
