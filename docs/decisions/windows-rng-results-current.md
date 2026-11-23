# Windows file identity result handling

The Win32 adapter obtains a complete 16-byte draw from BCryptGenRandom.
A shared result handler evaluates the status and bytes on each attempt.
The handler records the raw Random diagnostic for any nonzero status
and returns Io immediately. A zero status permits checking the bytes.
The first nonzero identity succeeds. Three complete zero draws exhaust
the attempt limit and return Io without a native error.

| Result | Controlled input | Expected outcome |
| --- | --- | --- |
| Successful draw | Zero status and nonzero output on draw one, two or three | Return that exact identity and stop drawing |
| Terminal status | Positive or negative status with nonzero output, on draw one or three | Return Io and the raw Random diagnostic before admitting the bytes |
| Attempt limit | Three zero-status draws containing only zeros | Return Io after exactly three attempts |

These cases exercise the portable result handler used by the actual
Win32 adapter. The native RNG callback, DLL execution and intended
Windows environment retain their native qualification requirements.
The Windows runner retains both original mutations and exact failure
assertions. Its fixture now targets the shared result handler.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/windows_rng.rs` | `80fe6d4649e03e6c54ca8e2049800a7733b87cbdcb560b4412a23cdeb8e883d6` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/windows_rng.rs:7:5– | 254..257 | for_iteration | identity_with | The third draw is admitted. Three consecutive zero draws exhaust the budget. |
| P | weekend-db::weekend_db | src/storage/windows_rng.rs:10:9– | 344..346 | if | identity_with | A nonzero status independently refuses an otherwise nonzero identity and preserves its raw code. |
| P | weekend-db::weekend_db | src/storage/windows_rng.rs:17:9– | 560..562 | if | identity_with | After a zero status, a zero identity retries while a nonzero identity returns. |

## Verification

Frozen source `a1e36bfacf18c5087bfd33db237557095b455871` used Rust 1.100.0.
The selected engine and storage suites passed 292 tests per profile on
macOS arm64 and 303 per profile on Docker Linux aarch64/ext4. Both
golden images matched their fixtures in each run. Strict Clippy and the
18 mutation-runner contract tests passed on both hosts. Cross-target
Clippy on `x86_64-pc-windows-msvc` passed for the library and tests.

All four portable controls were detected in debug and release on both
hosts. The exact-source audit records two predicate dispositions and
one bounded loop disposition. It rebinds 58 storage-module rows and
records 9,408 open decisions (2,039 runtime, 26 driver and 7,343 tooling).
The Win32 adapter retains 19 open records. The native Windows runner
pins the shared source at `983bf98`. Native execution remains pending.

On macOS or Linux, run the portable cases with:

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --lib \
  storage::windows_rng::tests
cargo +1.100.0 xtask mutations --toolchain 1.100.0 \
  --case r1-windows-rng- --output target/windows-rng-debug
```

Repeat with `--release` and a separate output directory. On native
Windows MSVC, use `cargo +1.100.0 xtask mutations --toolchain 1.100.0
--case windows-rng- --output target/windows-rng-mutations` for both profiles.
