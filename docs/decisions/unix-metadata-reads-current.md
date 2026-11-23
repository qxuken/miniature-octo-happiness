# Unix file metadata reads

The shared Unix adapter uses one metadata query for file length, open,
lease checks, the source link count and the final compaction name checks.
The query keeps the existing retry on interruption and the Metadata
diagnostic. Its test build can supply one I/O result at a chosen attempt.
Successful attempts read real file metadata.

| Check | Controlled results | Outcome |
| --- | --- | --- |
| Claimed length | EINTR once, or terminal EIO | Retry and return the length, or report Metadata and errno 5. The claim remains usable. |
| Factory open | Refuse the regular-file read, the named-file read or the held-file read in turn | Stop at that read and release the lease |
| Claim | Refuse the named-file or held-file read | Disable I/O on the failed adapter and release the lease while the adapter still lives |
| Source link count | Refuse the read following three successful open checks | Return Io before scratch is created |
| Final name checks | Refuse each read of the source and target name/held-file pairs | Return Io before exchange and preserve the complete scratch image |

The compaction cases verify the unchanged canonical inode and image after
failure, reopen the exact current and retained revisions, and retry to the
reference bytes. The final checks exercise the separate source and target
calls. The kernel's metadata-error behavior remains a deployment gate.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `cfcc5d7b40eadd689b34a717825cd59cdd62171a09c8397671c63e43f61c2756` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:143:37– | 4538..4539 | try_propagation | length | Preserve a terminal failure and allow a later read of the claimed file. |
| E | weekend-db::weekend_db | src/storage/file.rs:253:33– | 8567..8568 | try_propagation | open_mode | The initial regular-file read stops opening and drops the file on refusal. |
| E | weekend-db::weekend_db | src/storage/file.rs:256:39– | 8669..8670 | try_propagation | open_mode | Propagate either metadata refusal while trying to acquire the lease. |
| E | weekend-db::weekend_db | src/storage/file.rs:277:44– | 9514..9515 | try_propagation | check_lease | The named-file read stops before reading the held file. |
| E | weekend-db::weekend_db | src/storage/file.rs:278:43– | 9559..9560 | try_propagation | check_lease | Refusal on the held file releases the failed claim. |
| E | weekend-db::weekend_db | src/storage/file.rs:312:49– | 10666..10667 | try_propagation | matches | Refuse named-file metadata independently for the canonical and the scratch location. |
| E | weekend-db::weekend_db | src/storage/file.rs:313:43– | 10711..10712 | try_propagation | matches | Refuse held-file metadata independently for both locations. |
| E | weekend-db::weekend_db | src/storage/file.rs:484:32– | 16002..16003 | try_propagation | compact_file_impl | An unreadable source link count stops compaction before creating its scratch. |

## Verification

Frozen source `0e224132914bbe24b1d74f2822960ed6d2068bcd` passed 281 engine
and storage tests per profile on macOS arm64 and 292 per profile on Docker
Linux aarch64/ext4, using Rust 1.100.0. Both golden images matched their
fixtures in each run. Strict Clippy and the 18 mutation-runner contract
tests passed on both hosts. The Windows MSVC target passed compilation
checks for the library and tests.

Both profiles on both hosts detected the nine new controls and the two
rebound controls for hard links and regular-file admission. The audit
rebinds 82 earlier rows and adds eight propagation dispositions. It
reports 9,431 open records (2,062 runtime, 26 driver and 7,343 tooling).

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features \
  --lib storage::file::metadata_decisions::
cargo +1.100.0 xtask mutations --toolchain 1.100.0 \
  --case r1-unix-metadata-read- --case r1-native-replace-hard-link \
  --case r1-unix-path-regular --output target/unix-metadata-read-debug
```

Repeat both commands with `--release` and use a distinct mutation output
directory for the optimized run.
