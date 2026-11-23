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
| `src/storage/file.rs` | `f981e5d525eee133ba9e5554491c724440e8bf4b57df2768b552af98894a7206` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/file.rs:67:37– | 2295..2296 | try_propagation | open_read_only | Read-only open propagates an empty name as Invalid. |
| E | weekend-db::weekend_db | src/storage/file.rs:74:51– | 2518..2519 | try_propagation | open_in | Create and writable reopen propagate allocator refusal before leaf open. |
| P | weekend-db::weekend_db | src/storage/file.rs:169:9– | 5517..5519 | if | new_in | A path without zero is admitted and an embedded zero rejects before allocation. |
| P | weekend-db::weekend_db | src/storage/file.rs:172:9– | 5599..5601 | if | new_in | Accept the exact 1 MiB cap and reject one byte more before allocation. |
| I | weekend-db::weekend_db | src/storage/file.rs:175:77– | 5754..5755 | try_propagation | new_in | The split always yields a last segment, including the empty segment on empty input. |
| P | weekend-db::weekend_db | src/storage/file.rs:176:9– | 5765..5767 | if | new_in | An ordinary leaf passes. Empty, dot and dot-dot independently reject. |
| C | weekend-db::weekend_db | src/storage/file.rs:176:12– | 5768..5799 | short_circuit_or | new_in | Shares the empty and dot predicates of the final-component decision. |
| C | weekend-db::weekend_db | src/storage/file.rs:176:12– | 5768..5816 | short_circuit_or | new_in | Same predicates and source bounds as the final-component decision. |
| E | weekend-db::weekend_db | src/storage/file.rs:184:97– | 6130..6131 | try_propagation | new_in | Refuse the exact terminated parent size first and return OutOfMemory. |
| E | weekend-db::weekend_db | src/storage/file.rs:189:17– | 6425..6427 | if | new_in | A real successful parent open and native ENOENT and ENOTDIR outcomes. Only a valid descriptor is owned. |
| E | weekend-db::weekend_db | src/storage/file.rs:195:15– | 6672..6673 | try_propagation | new_in | A parent open refusal preserves its native status and stops before the leaf. |
| E | weekend-db::weekend_db | src/storage/file.rs:197:58– | 6742..6743 | try_propagation | new_in | Refuse leaf storage after opening the parent and before creating or opening a target file. |
| E | weekend-db::weekend_db | src/storage/file.rs:204:60– | 6973..6974 | try_propagation | terminated_path | Both terminated buffer calls propagate allocation refusal. Successful buffers include the terminating zero. |
| E | weekend-db::weekend_db | src/storage/file.rs:215:45– | 7306..7307 | try_propagation | copy_path | The selected allocator grants or refuses the exact path capacity. |
| E | weekend-db::weekend_db | src/storage/file.rs:225:80– | 7719..7720 | try_propagation | duplicate | Duplicate the path using the same provider. A refused copy preserves the previously held location. |
| P | weekend-db::weekend_db | src/storage/file.rs:231:9– | 7971..7973 | if | open_mode | Opening a directory read-only reaches metadata and rejects a nonregular file. A regular file is admitted. |
| P | weekend-db::weekend_db | src/storage/file.rs:265:13– | 9365..9367 | if | open_flags | Failed create and ordinary open record the correct native operation independently. |
| E | weekend-db::weekend_db | src/storage/file.rs:279:17– | 9839..9841 | if | open_flags | Use real negative descriptors from missing or occupied leafs and a successful native open. |
| E | weekend-db::weekend_db | src/storage/file.rs:285:10– | 9997..9998 | try_propagation | open_flags | Propagate the original leaf open error and its exact diagnostic. |

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
