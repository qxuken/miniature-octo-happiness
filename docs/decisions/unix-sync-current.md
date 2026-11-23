# Unix creation sync and I/O attribution

Both Unix adapters use the same private sequencing method for a claimed
file. The production call supplies the native file and parent sync
functions. The file callback receives the held file descriptor. The
parent callback receives the held location. The test supplies callbacks
that record those same descriptors, order and errors.

| Case | Executed barriers | Next successful sync |
| --- | --- | --- |
| New file, first file sync fails | File | File, parent, file |
| Parent sync fails | File, parent | File, parent, file |
| Final file sync fails | File, parent, file | File, parent, file |
| All creation barriers succeed | File, parent, file | File |
| Later file sync fails, including after reopen | File | File |

Every failure preserves the callback error and stops the sequence. The
creation flag clears after the final barrier succeeds. Successful native
create and reopen still run through the real adapter in the existing file
and interruption suites. The callback results qualify ordering and
propagation. Physical persistence after an interrupted flush remains a
separate device gate.

The retry controls cover an interrupted call followed by success and an
interrupted call followed by a permission failure. The callback count
proves the interruption is retried. The terminal error retains the read
operation and permission diagnostic.

The native compaction observer records length, read, write and sync as
source operations through source recovery and sizing. Candidate writing
and first verification record the corresponding target operations. The
trace checks both phases independently, before the metadata and namespace
operations. Mutating the source/target classification at any of these
four callbacks breaks that trace.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `5c00d486e37527ce3e3a0375f9eb53163338bd24f3828e439a4bcac908fea144` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:36:9– | 1166..1171 | match | retry | Return an ordinary result and retain the terminal native diagnostic after interruption. |
| P | weekend-db::weekend_db | src/storage/file.rs:37:23– | 1205..1243 | match_guard | retry | Retry Interrupted and return the succeeding result. A terminal failure also returns without retry. |
| E | weekend-db::weekend_db | src/storage/file.rs:153:30– | 4881..4882 | try_propagation | sync_with | A first file barrier failure reaches the caller without a parent sync. |
| E | weekend-db::weekend_db | src/storage/file.rs:155:40– | 4956..4957 | try_propagation | sync_with | A parent barrier failure stops before the final file sync. |
| E | weekend-db::weekend_db | src/storage/file.rs:156:34– | 4992..4993 | try_propagation | sync_with | A final file barrier failure preserves the creation flag and error. |
| P | weekend-db::weekend_db | src/storage/file.rs:413:13– | 13888..13890 | if | length | Length callbacks are labeled source during recovery/sizing and target through verification. |
| P | weekend-db::weekend_db | src/storage/file.rs:423:13– | 14193..14195 | if | read | Source recovery and target verification record distinct read steps. |
| P | weekend-db::weekend_db | src/storage/file.rs:433:13– | 14502..14504 | if | write | Writes that stabilize the source and create the candidate have distinct labels. |
| P | weekend-db::weekend_db | src/storage/file.rs:443:13– | 14785..14787 | if | sync | Source stabilization and target publication have distinct sync labels. |

## Verification at a1b2dd5

- Rust 1.100.0 passed 262 tests per profile on macOS arm64 and 269 per
  profile on Docker Linux arm64/ext4. Native file interruption tests,
  allocator routing and both golden histories passed in every run.
  The frozen source was `a1b2dd59478c84db8ddfa71cad5d9eaf9a9fbced`.
- Both macOS profiles detected all 13 selected controls. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-unix-sync- --case r1-unix-lease-sync-claim --case r1-unix-lease-pending-parent`
  and repeat with `--release`. The selection reuses the two claim and
  pending-parent controls.
- Strict Clippy passed on macOS and for Windows MSVC. Ten mutation-runner
  tests passed.
- The inventory maps nine more source records and rebinds the existing
  pending-parent pair. It reports 10,581 decisions and 9,520 open records,
  including 2,150 runtime.
