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
| `src/storage/file.rs` | `a671d59cc6d2ca075ad49084901aa1d39a1c45532c4cc5a9c7c077eab3176c4f` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:146:37– | 4659..4660 | try_propagation | length | Preserve a terminal failure and allow a later read of the claimed file. |
| E | weekend-db::weekend_db | src/storage/file.rs:256:33– | 8688..8689 | try_propagation | open_mode | The initial regular-file read stops opening and drops the file on refusal. |
| E | weekend-db::weekend_db | src/storage/file.rs:259:39– | 8790..8791 | try_propagation | open_mode | Propagate either metadata refusal while trying to acquire the lease. |
| E | weekend-db::weekend_db | src/storage/file.rs:280:44– | 9635..9636 | try_propagation | check_lease | The named-file read stops before reading the held file. |
| E | weekend-db::weekend_db | src/storage/file.rs:281:43– | 9680..9681 | try_propagation | check_lease | Refusal on the held file releases the failed claim. |
| E | weekend-db::weekend_db | src/storage/file.rs:315:49– | 10787..10788 | try_propagation | matches | Refuse named-file metadata independently for the canonical and the scratch location. |
| E | weekend-db::weekend_db | src/storage/file.rs:316:43– | 10832..10833 | try_propagation | matches | Refuse held-file metadata independently for both locations. |
| E | weekend-db::weekend_db | src/storage/file.rs:497:32– | 16572..16573 | try_propagation | compact_file_impl | An unreadable source link count stops compaction before creating its scratch. |

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
