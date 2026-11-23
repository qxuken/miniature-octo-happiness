# Unix file lease ownership and pathname rechecks

Dropping a macOS FileStore used to close its descriptor without explicitly
releasing the advisory lock. A duplicate descriptor kept the open file
description and lock alive. The existing Linux regression reproduced
this on macOS. The second open returned Busy after the FileStore dropped
and while the duplicate was still open.

Both adapters now explicitly unlock on drop if their lease has not been
released. The same regression passes on macOS and Linux. It checks that
a second opener sees Busy before drop and can reopen after drop while the
duplicate remains valid.

| Claim scenario | Result |
| --- | --- |
| Factory opened the file, but the adapter is unclaimed | Length, read, write and sync return Io. Read leaves its buffer untouched. |
| First claim or reacquisition of the same inode | I/O works. A second claim on the same adapter returns Busy. |
| Explicit release | I/O is disabled. A separate description can acquire the lease. |
| Reacquisition while another description holds the lease | Busy. The current holder remains exclusive. |
| Reacquisition of the old description after its pathname was replaced | Busy. The stale adapter cannot perform I/O. Its failure releases the old inode while the stale adapter is still alive. |

The pathname checks use real files. Ordinary lease validation accepts a
hard link to the held inode. Replacement additionally requires a single
link. A missing pathname yields Busy at the lease check and NotFound at
the replacement check. A final symlink yields Invalid. A different regular
file or a directory at the pathname fails both rechecks. The fixtures check
that a read-only claim can read its file and that write permission does not
come with the lease.

These tests independently vary inode equality on the same device and link
count for the same inode. Type and device comparisons still have coupled
or platform-specific obligations. The unlock state test verifies transition
to Released after a successful syscall. The kernel unlock failure and
native barrier error evidence remain on the roadmap.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `f981e5d525eee133ba9e5554491c724440e8bf4b57df2768b552af98894a7206` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P | weekend-db::weekend_db | src/storage/file.rs:77:9– | 2594..2596 | if | check | Claimed allows I/O. Open and Released both require a claim. |
| E | weekend-db::weekend_db | src/storage/file.rs:87:9– | 2845..2847 | if | drop | Closing an open description releases its lease despite an inherited descriptor. An explicit release also closes normally. |
| P | weekend-db::weekend_db | src/storage/file.rs:94:9– | 3003..3005 | if | lock | Claim once and reject a repeated claim. |
| P | weekend-db::weekend_db | src/storage/file.rs:98:13– | 3140..3142 | if | lock | The real flock succeeds without contention and returns Busy with a competing description. |
| E | weekend-db::weekend_db | src/storage/file.rs:103:11– | 3313..3314 | try_propagation | lock | Preserve Busy after a failed reacquisition. The other owner stays exclusive. |
| P | weekend-db::weekend_db | src/storage/file.rs:105:9– | 3358..3360 | if | lock | Claim the matching name or release the stale inode and return Busy. |
| C | weekend-db::weekend_db | src/storage/file.rs:105:12– | 3361..3364 | pattern_test | lock | Same successful and failed recheck as the enclosing if-let. |
| B | weekend-db::weekend_db | src/storage/file.rs:120:22– | 3834..3836 | if | unlock | Successful unlock sets Released. The failure outcome remains a kernel obligation. |
| E | weekend-db::weekend_db | src/storage/file.rs:128:21– | 4077..4078 | try_propagation | length | Unclaimed length returns Io and a claimed length yields the exact size. |
| E | weekend-db::weekend_db | src/storage/file.rs:132:21– | 4256..4257 | try_propagation | read | An unclaimed read preserves the output byte. A claimed read returns the expected byte. |
| E | weekend-db::weekend_db | src/storage/file.rs:136:21– | 4429..4430 | try_propagation | write | Claimed write succeeds and unclaimed write returns Io without modifying the file. |
| E | weekend-db::weekend_db | src/storage/file.rs:142:21– | 4599..4600 | try_propagation | sync | Reject the unclaimed adapter and allow the claimed one to synchronize. |
| P | weekend-db::weekend_db | src/storage/file.rs:144:9– | 4642..4644 | if | sync | Creation sync clears the pending parent barrier after success. Reopened files already have no pending parent barrier. |
| P | weekend-db::weekend_db | src/storage/file.rs:229:22– | 7836..7838 | if | open_mode | Read-write and read-only adapters can both claim. Only read-write can write. |
| P | weekend-db::weekend_db | src/storage/file.rs:230:58– | 7923..7925 | if | open_mode | Create a new file or reject an existing name. Open an existing file without creation. |
| E | weekend-db::weekend_db | src/storage/file.rs:230:95– | 7960..7961 | try_propagation | open_mode | Open the requested file or preserve refusal, including AlreadyExists and Busy. |
| E | weekend-db::weekend_db | src/storage/file.rs:251:22– | 8805..8810 | match | check_lease | Missing name maps to Busy. Existing names continue and other open errors preserve their type. |
| E | weekend-db::weekend_db | src/storage/file.rs:253:27– | 8919..8920 | try_propagation | check_lease | A final symlink returns Invalid through this path. |
| B | weekend-db::weekend_db | src/storage/file.rs:257:9– | 9101..9103 | if | check_lease | Accept the held regular file and reject a different same-device inode. Distinct device/type predicates remain coupled. |
| B | weekend-db::weekend_db | src/storage/file.rs:257:12– | 9104..9154 | short_circuit_and | check_lease | Regular file versus directory is exercised. Device independence remains open. |
| C | weekend-db::weekend_db | src/storage/file.rs:257:12– | 9104..9188 | short_circuit_and | check_lease | Overlaps the lease validation if. The per-predicate limits are the same. |
| E | weekend-db::weekend_db | src/storage/file.rs:289:45– | 10146..10147 | try_propagation | matches | An absent path returns NotFound before inode comparison. |
| B | weekend-db::weekend_db | src/storage/file.rs:292:9– | 10322..10324 | if | matches | Same-device inode and same-inode link count are independent. Type and device remain coupled. |
| B | weekend-db::weekend_db | src/storage/file.rs:292:12– | 10325..10387 | short_circuit_and | matches | Regular file and directory use the native metadata path. Device independence remains open. |
| C | weekend-db::weekend_db | src/storage/file.rs:292:12– | 10325..10433 | short_circuit_and | matches | Shares the first three predicates and their limits with the replacement comparison. |
| C | weekend-db::weekend_db | src/storage/file.rs:292:12– | 10325..10468 | short_circuit_and | matches | Shares all four predicates and their limits with the replacement comparison. |

## Verification at 15b2993

- The pre-fix macOS duplicate-descriptor fixture returned Busy after the
  first adapter dropped. The fixed fixture passes in debug and release on
  macOS and Linux.
- Rust 1.100.0 passed 253 tests per profile on macOS arm64 and 260 per
  profile on Docker Linux arm64/ext4. Both golden histories passed in
  all four runs. The frozen source was
  `15b2993bac50880d86e1921d6cf957a6afa37a39`.
- Both macOS profiles detected all 22 controls. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-unix-lease-` and
  repeat with `--release`.
- Strict Clippy passed on macOS and for Windows MSVC. Ten mutation-runner
  tests passed.
- The inventory maps 26 additional source records, including mixed
  dispositions for coupled predicates and the unlock failure. It reports
  10,581 decisions and 9,548 open records, including 2,178 runtime.
