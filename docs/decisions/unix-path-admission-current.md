# Unix pathname admission and selected allocation

The adapter validates the native path bytes before allocating either path
buffer. The tests cover a bare leaf, an embedded zero, an empty leaf, dot,
dot-dot, and the exact 1 MiB cap and next byte. They independently vary
the three final-component predicates. A bare leaf resolves against the
current directory, verified by that directory's device and inode. The
byte splitter yields a final segment even for empty input, which then
fails the empty-leaf guard.

## Allocation and filesystem outcomes

The selected allocator sees one request for the terminated parent and one
for the terminated leaf. The fixture refuses each request in turn during
both create and reopen. An allocator error carries no native diagnostic.
A failed create leaves the directory empty. A failed reopen preserves the
existing file, and normal acquisition remains possible. A successful
operation keeps only the leaf buffer in addition to the allocator's
control block. Closing the adapter restores the original live bytes.

Duplicating the held location allocates one copy of the terminated leaf
through the same provider. Refusal releases that allocation attempt,
preserves the original path and bytes, and allows retry. The success case
checks the exact terminated name and live allocation size.

| Input or refusal | Status | Native diagnostic |
| --- | --- | --- |
| Invalid bytes or final component | Invalid | None |
| Path one byte over the cap | Limit | None |
| Parent or leaf allocation refused | OutOfMemory | None |
| Missing parent | NotFound | OpenDirectory, ENOENT |
| Regular file used as parent | Invalid | OpenDirectory, ENOTDIR |
| Missing leaf in a valid directory | NotFound | Open, ENOENT |
| Exclusive create at an existing leaf | AlreadyExists | Create, EEXIST |
| Read-only open of a directory | Invalid | None. The metadata type check rejects the opened directory. |

The native diagnostic controls check the actual syscall failures and their
raw codes in the test directory. They check normal directory and regular
file acceptance on the tested macOS and Linux platforms. Native descriptor
duplication failures and metadata syscall failures retain their own gates.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/storage/file.rs` | `5c00d486e37527ce3e3a0375f9eb53163338bd24f3828e439a4bcac908fea144` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:67:37– | 2295..2296 | try_propagation | open_read_only | Read-only open propagates an empty name as Invalid. |
| E | weekend-db::weekend_db | src/storage/file.rs:74:51– | 2518..2519 | try_propagation | open_in | Create and writable reopen propagate allocator refusal before leaf open. |
| P | weekend-db::weekend_db | src/storage/file.rs:179:9– | 5774..5776 | if | new_in | A path without zero is admitted and an embedded zero rejects before allocation. |
| P | weekend-db::weekend_db | src/storage/file.rs:182:9– | 5856..5858 | if | new_in | Accept the exact 1 MiB cap and reject one byte more before allocation. |
| I | weekend-db::weekend_db | src/storage/file.rs:185:77– | 6011..6012 | try_propagation | new_in | The split always yields a last segment, including the empty segment on empty input. |
| P | weekend-db::weekend_db | src/storage/file.rs:186:9– | 6022..6024 | if | new_in | An ordinary leaf passes. Empty, dot and dot-dot independently reject. |
| C | weekend-db::weekend_db | src/storage/file.rs:186:12– | 6025..6056 | short_circuit_or | new_in | Shares the empty and dot predicates of the final-component decision. |
| C | weekend-db::weekend_db | src/storage/file.rs:186:12– | 6025..6073 | short_circuit_or | new_in | Same predicates and source bounds as the final-component decision. |
| E | weekend-db::weekend_db | src/storage/file.rs:194:97– | 6387..6388 | try_propagation | new_in | Refuse the exact terminated parent size first and return OutOfMemory. |
| E | weekend-db::weekend_db | src/storage/file.rs:199:17– | 6682..6684 | if | new_in | A real successful parent open and native ENOENT and ENOTDIR outcomes. Only a valid descriptor is owned. |
| E | weekend-db::weekend_db | src/storage/file.rs:205:15– | 6929..6930 | try_propagation | new_in | A parent open refusal preserves its native status and stops before the leaf. |
| E | weekend-db::weekend_db | src/storage/file.rs:207:58– | 6999..7000 | try_propagation | new_in | Refuse leaf storage after opening the parent and before creating or opening a target file. |
| E | weekend-db::weekend_db | src/storage/file.rs:214:60– | 7230..7231 | try_propagation | terminated_path | Both terminated buffer calls propagate allocation refusal. Successful buffers include the terminating zero. |
| E | weekend-db::weekend_db | src/storage/file.rs:225:45– | 7563..7564 | try_propagation | copy_path | The selected allocator grants or refuses the exact path capacity. |
| E | weekend-db::weekend_db | src/storage/file.rs:235:80– | 7976..7977 | try_propagation | duplicate | Duplicate the path using the same provider. A refused copy preserves the previously held location. |
| P | weekend-db::weekend_db | src/storage/file.rs:241:9– | 8228..8230 | if | open_mode | Opening a directory read-only reaches metadata and rejects a nonregular file. A regular file is admitted. |
| P | weekend-db::weekend_db | src/storage/file.rs:275:13– | 9622..9624 | if | open_flags | Failed create and ordinary open record the correct native operation independently. |
| E | weekend-db::weekend_db | src/storage/file.rs:289:17– | 10096..10098 | if | open_flags | Use real negative descriptors from missing or occupied leafs and a successful native open. |
| E | weekend-db::weekend_db | src/storage/file.rs:295:10– | 10254..10255 | try_propagation | open_flags | Propagate the original leaf open error and its exact diagnostic. |

## Verification at c28cad3

- Rust 1.100.0 passed 261 tests per profile on macOS arm64 and 268 per
  profile on Docker Linux arm64/ext4. Both golden histories and all
  allocator-routing tests passed in every run. The frozen source was
  `c28cad3a84f2fbb22bfea6ddfcec8356729ce7e3`.
- Both macOS profiles detected all 20 controls. Reproduce with
  `cargo xtask mutations --toolchain 1.100.0 --case r1-unix-path-` and
  repeat with `--release`.
- Strict Clippy passed on macOS and for Windows MSVC. Ten mutation-runner
  tests passed.
- The inventory maps 19 additional source records. It reports 10,581
  decisions and 9,529 open records, including 2,159 runtime.
