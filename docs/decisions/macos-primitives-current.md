# macOS native primitive outcomes

The macOS adapter uses native calls for file flush, atomic exchange, and
metadata-only copy. The fixtures run these calls with live descriptors.

| Primitive | Native success | Native refusal |
| --- | --- | --- |
| Full file sync | Full-sync a written regular file | An unsupported socket descriptor returns a Sync diagnostic and Io |
| Atomic exchange | Swap the two named inodes and their exact contents | A missing partner returns NotFound and raw ENOENT, preserving the source and sibling |
| Metadata copy | Transfer mode bits and preserve destination contents | A socket destination returns a CopyMetadata diagnostic and Io |

The socket fixtures exercise syscall error handling with valid owned
descriptors. Normal FileStore admission requires a regular file. The
native tests establish success and error propagation on the tested host.
Deployed-device persistence remains a separate qualification gate.

The errno mapping fixture checks the six mapped values separately, one
default raw code, and an error with no raw code. The default results
preserve the operation and the optional raw code. The identity callback
tests check acceptance on the first or third draw, three zero draws
before failure, and zero initialization at the start of each draw.
The identity tests control the wrapper around the platform RNG.

## Host-specific mutation selection

The Unix runner applies the optional `os` tag before prefix filtering and
sharding. The macOS native cases declare `macos`. A general R1 run still
includes the shared cases on both hosts. Unknown hosts include only
untagged cases. Qualification reports identify the host and the exact
set from which each shard was selected.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/macos.rs` | `da5c860e2e339f78e53b9d63e993c96c89aeee62f40e22c4c06c15952c6700e2` |
| `xtask/src/mutations.rs` | `c04c31e9a7b64abec124a1c16aed28c002fc113da77a9224936357ef5b028400` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/macos.rs:33:5– | 1227..1232 | match | os_error | Exercise each mapped code and preserve the diagnostic on the default arm. |
| P | weekend-db::weekend_db | src/storage/macos.rs:43:9– | 1546..1548 | if | full_sync | Full sync succeeds on the regular file and rejects the socket descriptor. |
| E | weekend-db::weekend_db | src/storage/macos.rs:57:5– | 1930..1933 | for_iteration | identity_with | Accept the third nonzero draw and reject after three zero draws. |
| P | weekend-db::weekend_db | src/storage/macos.rs:60:9– | 2007..2009 | if | identity_with | A nonzero draw returns immediately. Zero continues until exhaustion. |
| P | weekend-db::weekend_db | src/storage/macos.rs:72:5– | 2335..2337 | if | exchange | Exchange the held directory names or report the missing partner and preserve the source. |
| P | weekend-db::weekend_db | src/storage/macos.rs:93:9– | 2912..2914 | if | copy_metadata | Copy mode bits without changing bytes and report refusal on a socket destination. |
| E | xtask::xtask | xtask/src/mutations.rs:295:20– | 9619..9624 | match | select_cases | The selector test exercises Linux, macOS, and the unknown-host fallback. |
| E | xtask::xtask | xtask/src/mutations.rs:303:21– | 9848..9917 | short_circuit_or | select_cases | The selector test exercises empty, matching and nonmatching prefixes before sharding. |

## Verification at 67c91b2

- Rust 1.100.0 passed 265 tests per profile on macOS arm64 and 269 per
  profile on Docker Linux arm64/ext4, including both golden histories.
  The frozen source was `67c91b2057bf824c9a537e2d34fdde5341e59139`.
- Both macOS profiles detected all 12 native controls. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-macos-primitive-`
  and repeat with `--release`.
- The real CLI lists 632 R1 cases on macOS and 620 on Docker Linux.
  Dropping the 12 tagged cases from the macOS list yields the identical
  ordered Linux list. An explicit Linux request for the macOS prefix
  reports an empty selection.
- Eleven mutation-runner tests passed on each Unix host. The sharding
  fixture verifies disjoint unions for both hosts. Strict Clippy passed
  on macOS and for Windows MSVC.
- The inventory maps all six macOS primitive decisions, the relocated
  prefix-filter decision and the new host match. It reports 10,582
  decisions and 9,513 open records, including 2,144 runtime.
