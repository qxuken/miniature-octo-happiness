# Windows file and volume admission

The Win32 adapter calls shared policy functions for parent type, local
volume and target file checks. The adapter supplies observed file-type
codes and metadata, the drive type, and a callback for the filesystem name
query. The query retains its native error mapping. An unsupported drive
returns before querying the filesystem.

| Check | Independent cases | Outcome |
| --- | --- | --- |
| Drive type | Fixed or another reported drive type | Query only a fixed drive |
| Filesystem query | Zero or nonzero result | Report the supplied native error on zero. Inspect the name on success. |
| Filesystem name | Terminated UTF-16 NTFS or a different name | Admit only NTFS |
| Parent | Directory or another kind, with or without restricted attributes | Invalid for another kind. UnsupportedPlatform for a directory with sparse, compressed, or encrypted attributes. |
| Leaf | Disk file baseline, wrong file type alone, wrong metadata kind alone, reparse point alone | Invalid for each of the three refusals |
| Leaf attributes | Ordinary file, sparse, compressed, or encrypted | UnsupportedPlatform for each restricted bit on an otherwise admitted file |
| Access and disposition | Both writable outcomes for both create outcomes | Read access is always present. Write access follows the caller. Create selects CREATE_NEW and Create diagnostics, and open selects OPEN_EXISTING and Open diagnostics. |

These tests execute the shared decision code with controlled query results
and attribute bit patterns. The Win32 entry points, real handle outcomes,
filesystem enforcement and intended NTFS account remain native evidence
requirements. The adapter uses the same exclusive share mode, write-through
and reparse-point flags for the leaf open.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/windows_admission.rs` | `6313b98e035f9b32d5f3c2c7dd9ef7dd657e47cb49123d23a65d931ff34052a7` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:17:5– | 571..573 | if | check_volume | Admit fixed drives and reject other types before querying the volume. |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:21:5– | 696..698 | if | check_volume | A zero query preserves its supplied error. Nonzero query results use the name. |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:24:5– | 765..767 | if | check_volume | The first five UTF-16 units must be NTFS followed by NUL. |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:31:5– | 975..977 | if | check_directory | Only a directory proceeds to attribute checks. |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:34:5– | 1034..1036 | if | check_directory | Each restricted attribute rejects after the directory check succeeds. |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:41:5– | 1241..1243 | if | check_leaf | Each leaf predicate independently changes the Invalid outcome. |
| C | weekend-db::weekend_db | src/storage/windows_admission.rs:41:8– | 1244..1270 | short_circuit_or | check_leaf | Both prefix predicates have independent controls in the enclosing if. |
| C | weekend-db::weekend_db | src/storage/windows_admission.rs:41:8– | 1244..1305 | short_circuit_or | check_leaf | Shares all three conditions and the baseline with the enclosing if. |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:44:5– | 1354..1356 | if | check_leaf | A restricted bit on an otherwise valid file changes the result. |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:57:36– | 1709..1711 | if | leaf_open | Create controls the disposition and operation label together. |
| P | weekend-db::weekend_db | src/storage/windows_admission.rs:63:32– | 1884..1886 | if | leaf_open | Writable independently adds write access. |

## Verification

Frozen source `438b6cbdc833a3cdbb75700dbf387d2a014151f4` used Rust 1.100.0.
macOS arm64 passed 286 storage and engine tests per profile. Docker
Linux aarch64/ext4 passed 297 per profile. Both golden images matched
their fixtures. Strict Clippy and the 18 mutation-runner contract tests
passed on both hosts. Windows MSVC `x86_64-pc-windows-msvc` cross-target
Clippy passed for the library and tests.

Both profiles on both hosts detected all 14 named admission controls.
The audit records nine Boolean decisions and two overlapping
short-circuit records in the shared code. It rebinds 58 previous
storage-module rows. The source inventory has 10,589 decisions and
9,419 open records, including 2,050 runtime, 26 driver and 7,343 tooling.
The Win32 adapter still has 30 open records. The existing two native RNG
controls use the updated adapter pin from `180296d`, with their native
execution still queued.

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --lib \
  storage::windows_admission::tests
cargo +1.100.0 xtask mutations --toolchain 1.100.0 \
  --case r1-windows-admission- --output target/windows-admission-debug
```

Repeat the test and mutation commands with `--release` and use a new
output directory for the optimized mutation run.
