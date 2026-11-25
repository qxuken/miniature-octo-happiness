# Unix compaction file clone failures

The Unix adapter shares one descriptor duplication wrapper for directory
and file clones. The wrapper keeps interruption retry and the
DuplicateHandle diagnostic. The test uses the same 64-descriptor child
limit as the descriptor-exhaustion suite. A unit-test checkpoint fills
the remaining descriptor slots just before the selected real clone
syscall. The filler handles close when that attempt returns.

| File whose descriptor is cloned | Prior duplication attempts | Result of refusal | Scratch image |
| --- | ---: | --- | --- |
| Canonical source | 1 | Io with DuplicateHandle and errno 24 | Absent |
| Scratch target | 4 | Io with DuplicateHandle and errno 24 | Empty |

The fixture checks the returned descriptor count, canonical inode and
raw image, then reopens the exact current and retained revisions and
values. A same-process retry produces the reference simulator bytes.
The empty scratch from the target refusal stays empty and available for
inspection. With the earlier directory cases, this qualifies refusal
at both file clone and all three directory duplication attempts in the
native compaction sequence.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `0584ba7e5eb42f5d949ceb6408f17c04059c53403d10ee9534efb53e4f212390` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:492:48– | 16337..16338 | try_propagation | compact_file_impl | EMFILE on the canonical file clone stops before creating scratch. |
| E | weekend-db::weekend_db | src/storage/file.rs:508:48– | 16927..16928 | try_propagation | compact_file_impl | EMFILE on the target file clone preserves an empty scratch file for inspection. |

## Verification

Frozen source `309ceae226365cd3253abaa351e81423d7427d37` used Rust 1.100.0.
Both kernel failures passed in debug and release on macOS arm64 and
Docker Linux aarch64/ext4. Linux repeated both results as UID and GID
1000 using the same test executables. The full selected storage suite
passed 282 tests per macOS profile and 293 per Linux profile, including
both golden images. Strict Clippy passed on both hosts, and the Windows
MSVC compilation check passed.

Each profile on each host detected all three new controls and all six
descriptor-exhaustion controls, including the two rebound directory
controls. The exact-source audit rebinds 90 earlier rows and adds two
error propagation dispositions. The queue contains 9,429 open records
(2,060 runtime, 26 driver and 7,343 tooling).

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --lib \
  storage::file::clone_decisions::file_clone_exhaustion_preserves_roots_and_retry \
  -- --exact --nocapture
cargo +1.100.0 xtask mutations --toolchain 1.100.0 \
  --case r1-unix-clone- --case r1-unix-descriptor- \
  --output target/unix-clone-debug
```

Repeat both commands with `--release` and use a distinct mutation output
directory for the optimized run.
