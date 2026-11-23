# Windows file open results

`CreateFileW` returns an owned handle on success and
`INVALID_HANDLE_VALUE` on failure. The adapter uses the shared
`own_open_handle` decision immediately after that call. It reads the
native last error only on failure and keeps the original operation and raw
code. Only a successful result reaches `File::from_raw_handle`.

| Invalid handle | Last-error reads | Ownership callback | Result |
| --- | --- | --- | --- |
| No | Zero | Once with the exact pointer | The callback's result |
| Yes | One | Zero | Mapped error with the original operation and code |

The portable test checks missing directory, sharing violation, existing
file and generic I/O error outcomes at the decision used by the adapter.
The Windows integration fixture exercises the actual missing parent,
missing leaf, duplicate create and exclusive reopen results. It checks
the native operation and raw code for each, then verifies unchanged bytes
and reopens the current and retained roots after releasing the handle.

## Exact source disposition

| Source | SHA-256 |
| --- | --- |
| `src/storage/windows_io.rs` | `93f7e139d6d8e9436e8b18edefe7cf887e23a8d83316a607d6631e54bd354fc3` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P | weekend-db::weekend_db | src/storage/windows_io.rs:61:5– | 1747..1749 | if | own_open_handle | The invalid sentinel alone selects error handling. Ownership runs only for the valid handle. |

## Verification

Frozen source `9dc1e6a3121f6cf4a6fb9455bd3de5a156b39642` used Rust 1.100.0.
The selected engine and storage suites passed 293 tests per profile on
arm64 macOS and 304 per profile on aarch64 Docker Linux/ext4. Both
golden images matched in all four runs. Strict Clippy and all 18
mutation-runner contract tests passed on both hosts. Windows MSVC
cross-target Clippy passed for the library and integration tests. The
new filesystem regression still requires native Windows execution.

Both debug and release caught the two named controls on both hosts.
Changing the sentinel to null incorrectly reaches the ownership
callback. Replacing the caller's operation with Open misattributes
the missing-directory diagnostic. The baseline assertion passed in all
four runs and each mutant failed in the named test.

The audit records 9,407 open source decisions (2,038 runtime,
26 driver and 7,343 tooling). The Win32 adapter retains 18 open
records covering host-dependent transitions, including the path prefix
and native metadata and open stages.

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --lib \
  storage::windows_io::tests::open_result_checks_only_the_invalid_handle_sentinel
cargo +1.100.0 xtask mutations --toolchain 1.100.0 \
  --case r1-windows-open- --output target/windows-open-debug
```

Repeat with `--release` and a separate output directory. On native
Windows, run the `windows_open_failures_report_the_native_stage_and_preserve_roots`
test in `tests/files.rs` in both profiles.
