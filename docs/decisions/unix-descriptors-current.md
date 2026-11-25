# Unix compaction descriptor exhaustion

The fixture runs the native compaction path in a child with a soft limit of
64 open descriptors. It fills the table with read-only handles and verifies
kernel EMFILE. It then frees the specified number before compaction. Two
cases fill the remaining slots at existing compaction checkpoints. Each
child runs one case and retries in the same process.

| Free at entry | Pressure checkpoint | Operation returning EMFILE | Scratch image |
| ---: | --- | --- | --- |
| 0 | Entry | OpenDirectory for the canonical parent | Absent |
| 1 | Entry | DuplicateHandle for the source directory | Absent |
| 2 | Entry | Open for the canonical file | Absent |
| 3 | Entry | Open for the source lease recheck | Absent |
| 4 | Entry | Open for the source claim recheck | Absent |
| 16 | Before the first source length read | DuplicateHandle for the scratch location | Absent |
| 5 | Entry | DuplicateHandle for the target directory | Absent |
| 6 | Entry | Create for the scratch file | Absent |
| 7 | Entry | Open for the scratch lease recheck | Empty |
| 8 | Entry | Open for the scratch claim recheck | Empty |
| 16 | Before the final namespace checks | Open for the canonical name recheck | Complete verified image |

All eleven cases return Io with the native operation and raw errno 24,
before exchange. The test measures the available descriptor count again
while the original filler handles are still held. The count matches the
entry allowance. Fresh recovery verifies the canonical inode, bytes,
current rows and retained roots 2, 4 and 5. Once the filler handles close,
compaction succeeds in the same process and produces the exact bytes of
the simulator reference. The retry preserves the failed attempt's scratch
file where present, along with an unrelated sibling.

The table describes the tested descriptor sequence. Individual failure
paths on the later source/target file clones, metadata syscalls, system-wide
file table exhaustion, and deployed-device persistence have separate
evidence requirements.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `0584ba7e5eb42f5d949ceb6408f17c04059c53403d10ee9534efb53e4f212390` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:246:55– | 8209..8210 | try_propagation | duplicate | A real refused directory duplication is attributed to DuplicateHandle. |
| E | weekend-db::weekend_db | src/storage/file.rs:338:55– | 11526..11527 | try_propagation | scratch | Entropy and name preparation finish. A full descriptor table then refuses the scratch parent copy. |
| E | weekend-db::weekend_db | src/storage/file.rs:491:38– | 16263..16264 | try_propagation | compact_file_impl | EMFILE on the source directory copy stops before opening the canonical file. |
| E | weekend-db::weekend_db | src/storage/file.rs:506:45– | 16815..16816 | try_propagation | compact_file_impl | Propagate a scratch directory refusal before the scratch name is created. |
| E | weekend-db::weekend_db | src/storage/file.rs:507:37– | 16854..16855 | try_propagation | compact_file_impl | The scratch location is prepared. The extra directory descriptor for the target refuses. |
| E | weekend-db::weekend_db | src/storage/file.rs:507:60– | 16877..16878 | try_propagation | compact_file_impl | Kernel refusal at scratch creation or initial lease recheck exits before replacement. |

## Verification

At frozen source `49c64fd0da0686156f6bd2f177ad1fa65d59aacf`, Rust 1.100.0
passed all eleven outcomes in debug and release on macOS arm64 and on
Docker Linux aarch64/ext4. Running the exact Linux test executables as
UID and GID 1000 reproduced all eleven outcomes in both profiles. The
engine and storage regressions passed 278 tests per macOS profile and 289
per Linux profile, with both golden images unchanged. The xtask mutation
contract passed 18 tests and strict Clippy passed on both hosts.

Each host detected all six named controls in both profiles. The inventory
binds six new propagation records. The queue contains 9,439 open records
(2,070 runtime, 26 driver and 7,343 tooling).

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features \
  --test compaction files::descriptors::descriptor_exhaustion_preserves_current_and_retained_roots \
  -- --exact --nocapture
cargo +1.100.0 xtask mutations --toolchain 1.100.0 \
  --case r1-unix-descriptor- --output target/unix-descriptors-debug
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --release \
  --case r1-unix-descriptor- --output target/unix-descriptors-release
```
