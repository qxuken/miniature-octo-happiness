# Windows handle ownership and error routing

The Windows adapter owns a handle from successful open until unlock or
drop. It uses explicit Open, Claimed and Closed states, so a claim and
its handle stay together. One dispatcher admits I/O on a claimed handle
and records the operation and raw code for each native failure.

| State | Claim | ByteStore I/O | Release |
| --- | --- | --- | --- |
| Open | Succeeds and transfers the same handle to Claimed | Io without native context | Drops the handle and becomes Closed |
| Claimed | Busy and preserves the handle | Calls the requested operation once. Preserves success or maps the native failure. | Drops the handle and becomes Closed |
| Closed | Closed | Io without native context | Remains Closed |

The unit tests count drops and exercise synthetic errors through
Metadata, Read, Write and Sync. They also check each Windows code in
the stable mapping for NotFound, Busy, AlreadyExists and Invalid.
Other codes, including access denied, disk full and quota, retain Io and
their raw diagnostic. Windows kernel error categories and native handle
sharing, I/O and closure require native results.

A Windows-only FileStore test verifies guard behavior before claim and
after release, contention during the lease, read-only write refusal and
byte preservation. Existing Windows CI includes this test executable.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/windows_io.rs` | `565c123d253d04f062cd64ec72e513b56fc4b1bb0e17b3d06a61cbbc5a179060` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/windows_io.rs:8:5– | 272..277 | match | os_error | Every mapped code, unknown code and missing code retain the diagnostic and the documented error. |
| E | weekend-db::weekend_db | src/storage/windows_io.rs:26:9– | 738..743 | match | claim | Open claims, Claimed returns Busy and Closed returns Closed. The test checks ownership at each transition. |
| E | weekend-db::weekend_db | src/storage/windows_io.rs:48:9– | 1340..1345 | match | execute | Claimed dispatches each operation and preserves its result. Open and Closed return Io before invoking the callback. |

## Verification

Frozen source `34cb30c224c33aa000488f3ff209e8f04dcaba88` used Rust 1.100.0.
The selected engine and storage suites passed 289 tests per profile on
macOS arm64 and 300 per profile on Docker Linux aarch64/ext4. Both
golden images matched their fixtures in each run. The 18 mutation-runner
contract tests and strict Clippy passed on both hosts. Cross-target
Clippy on `x86_64-pc-windows-msvc` passed with the Windows-only regression
compiled. Its native execution remains pending.

All 13 controls were detected in debug and release on both hosts.
The supervised runner completed both debug campaigns and the first
Docker Linux release shard. After release build deadlines, isolated
replays verified all 13 macOS controls and the six remaining Docker
Linux controls. Each replay rebuilt the frozen source with one exact
replacement, observed the named test fail, and restored the original
bytes. The audit checks the source, mutant and log digests, passing
baselines, exact failed test, and exit status 101. Miri passed the
isolated handle-lifetime fixture on macOS using nightly `708c94ef0`.

Three shared match decisions receive scoped execution dispositions.
The inventory has 10,584 source decisions and 9,411 open records
(2,042 runtime, 26 driver and 7,343 tooling). It rebinds 58 earlier
storage-module rows. The Win32 adapter retains 22 open records. Its
two existing native RNG controls have a refreshed source pin at
`21aaafb`, with their native execution pending.

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --lib \
  storage::windows_io::tests
cargo +1.100.0 xtask mutations --toolchain 1.100.0 \
  --case r1-windows-handle- --output target/windows-handle-debug
```

On Windows, execute the additional native fixture with:

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --test files \
  windows_claim_guards_io_and_unlock_releases_the_read_only_handle -- --exact
```

Repeat both test commands and the mutation command with `--release`.
Use a fresh output directory for the optimized mutation run. In an
isolated replay at the frozen revision, read the selected case from
`xtask/tests/mutations.json`. Run its exact test against clean source,
apply the one anchored replacement, and run the same command again:

```sh
CARGO_INCREMENTAL=0 cargo +1.100.0 test --offline --locked --release \
  -p weekend-db --all-features --lib "$test_name" -- --exact
```

The baseline must execute one passing test. The rebuilt mutant must
execute that test and report its assertion failure with exit status 101.
Restore and verify the original source bytes before the next case.
