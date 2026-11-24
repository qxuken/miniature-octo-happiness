# Typed unique-claim replacement

A typed update validates the old and proposed unique claims before staging
changes. It removes claims no longer in the row, inserts new claims and
stages the primary row in the same transaction. When a typed operation
returns an error, the draft closes and earlier staged changes roll back.

## Decision cases

| Condition | Accepted control | Rejected or changed comparison |
| --- | --- | --- |
| Old claim still belongs to this row | Valid old index owner | Wrong owner or non-unsigned owner returns `IndexInconsistent`. |
| Proposed key has an existing claim | An absent claim can be installed | An existing claim proceeds to owner validation. |
| Existing proposed owner is an unsigned ID | A valid owner reaches the identity check | A Boolean owner returns `IndexInconsistent`. |
| Existing owner differs from the row being updated | Same owner reaches the old-key check | Another row owns the key and the update returns `UniqueViolation(1)`. |
| Same-owner proposed key occurs among the old keys | Unchanged claim is retained | A stale claim with this row's ID for a different key returns `IndexInconsistent`. |
| Old key belongs to the new key set | Unchanged second unique field retains its claim | Replaced first field loses its old claim. |
| New key belongs to the old key set | Unchanged second unique field needs no new claim | Replaced first field installs a new claim. |

The invalid proposed-claim cases stage another row before attempting the
replacement. Each error closes the draft even when the caller handles it
inside the closure. Revision, primary rows and raw index entries stay at their
pre-operation values, including any injected corruption. The healthy retained
root continues to resolve both original claims.

The two-field control replaces a name and leaves the numeric code unchanged.
The draft resolves both current claims before commit. Commit advances one
revision. Reopening returns the same code and new name, and its retained root
resolves the old name and original row. Both roots have consistent indexes.

The existing selected-allocator sweep now compares exact primary rows,
index entries and revision after each refusal. It also checks both retained
collections on every attempt. The successful comparison verifies the updated
primary and index together. Three controls alter propagation at the index
delete, index put, and primary put. They preserve the operation and change
only the returned error when the allocator refuses.

## Source binding

| Source | SHA-256 |
| --- | --- |
| `crates/weekend-db-typed/src/store.rs` | `b1d30c5b072b6daef5639794a211b86fefd37590cea097d895797ecb0d2e87fc` |

Ten named controls bind the six executable predicates and four propagation
sites. The other expression below represents the pattern test inside the
existing-claim guard and shares its evidence. Eleven registry cases include
separate controls for removing an old key and keeping an unchanged key.

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C | weekend-db-typed::weekend_db_typed | crates/weekend-db-typed/src/store.rs:887:16– | 30718..30721 | pattern_test | put_raw | Same branches as the existing-claim guard. |

## Verification

Frozen source `29f4f540789acf374a23e25d287a512ad53f3ecb` used Rust 1.100.0.
Debug and release on macOS arm64 and Docker Linux aarch64/ext4 each passed
63 typed tests with one existing ignored doctest. The selected suites include
the allocation sweep and the existing storage crash/reopen and retained-view
checks. All eleven controls failed their intended test in both profiles on
both hosts. Clippy passed on both hosts and for the Windows MSVC cross target.
Strict-provenance Miri `708c94ef0` passed the two new tests on macOS.

For same-object condition coverage, nightly `708c94ef0` compiled `models`
and `runtime_decisions` with `-C instrument-coverage -Z coverage-options=condition`.
Each test binary ran its full suite and produced its own runtime profile.
Matching `llvm-profdata` and `llvm-cov show --show-branches=count
--show-instantiations` reported the following counts with empty stderr.

| Guard in `put_raw` | Test object | Model | True | False |
| --- | --- | --- | ---: | ---: |
| Old claim owner mismatch, 882:16 | `runtime_decisions` | `Row` | 2 | 21 |
| Proposed key already claimed, 887:20 | `models` | `OldTask` | 165 | 765 |
| Different owner, 889:20 | `models` | `OldTask` | 2 | 162 |
| Same-owner key absent from old set, 892:20 | `models` | `OldTask` | 2 | 160 |
| Delete obsolete claim, 898:16 | `models` | `ClaimPair` | 1 | 1 |
| Install new claim, 903:16 | `models` | `ClaimPair` | 3 | 1 |

These counts apply to the named monomorphizations and single-condition guards.
The inventory has 9,365 open records (1,982 runtime, 26 driver and 7,357
tooling). Other typed paths and native Windows execution remain open.

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db-typed
cargo +1.100.0 xtask mutations --case r1-typed-unique-claims- \
  --toolchain 1.100.0 --output target/r1/typed-unique-claims-debug
```

Repeat both commands with `--release` and a separate output directory.

To reproduce the compiler counts, build each integration test with the
matching nightly and `--no-run --message-format=json`. Apply the coverage
flags in `RUSTFLAGS`, and use a fresh target directory. Take the test path
from the matching Cargo `compiler-artifact` entry. Run that executable with
`LLVM_PROFILE_FILE` set to a fresh file and `--test-threads=1`. Merge its raw
profile with `llvm-profdata merge -sparse`, then use only that executable and
profile in `llvm-cov show`, restricting the source to `store.rs`.
