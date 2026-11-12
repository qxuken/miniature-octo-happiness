# Native storage source decision audit

This audit covers 180 production AST records: `file.rs` 96, `macos.rs` 6,
`linux.rs` 40 and `windows.rs` 38. All spans are classified below. It reuses the
namespace publication ledger and native fault/process tests without treating
hook failures as actual kernel failures. Source records, overlapping expressions
and platform alternatives are not a compiler MC/DC denominator.

P = directed predicate/arm controls; C = overlapping syntax; E = executed calling
path/delegated propagation; B = mixed tested and coupled/uninjected predicates;
D = native/platform outcome not directly demonstrated here; I = invariant.
Windows rows are D in this host report even when new executable tests are supplied:
cross-compilation is not execution. No production code changed.

## N1: error mapping and retry

Each platform has new `decision_tests::native_error_mapping_preserves_raw_code_and_operation`.
These construct io::Error values with each mapped errno/Win32 code plus an unmapped
code and an error without a raw code. They verify Error, operation and preserved
native code. This tests conversion, not a claim that each syscall produced it.
macOS/Linux tests ran natively; Windows awaits native CI.

`file::decision_tests::interrupted_retry_can_finish_successfully_without_diagnostics`
retries two Interrupted results before returning success. Existing diagnostics
`retried_interruptions_never_replace_the_terminal_native_error` retries Interrupted
then reports PermissionDenied. All match arms/guard outcomes are controlled;
interruptions do not overwrite the terminal native diagnostic. The retry loop has
no artificial attempt cap; persistent EINTR is delegated to OS behavior.

## N2: pathname, lease and claimed state

New `file::decision_tests::pathname_and_claimed_read_only_controls` rejects empty,
dot, dot-dot, trailing slash and embedded NUL names. Exactly 1 MiB is accepted by
Location construction (without opening that oversized leaf); one byte more is
Limit. It creates a real file, rejects I/O before claim, accepts one claim,
rejects double claim Busy, writes and syncs, then rejects I/O after unlock.
Read-only reopening reads exact bytes and rejects writes with a real kernel error.
Removing the pathname makes reacquisition Busy and leaves the adapter unclaimed.

New `namespace_rechecks_distinguish_aliases_replacement_and_removal` compares a
matching inode, a same-inode hard-link alias, removed pathname and a newly created
replacement inode. Ordinary lease checking allows the hard-link alias while the
replacement check rejects nlink>1. Missing path is Busy for lease checking and
NotFound for replacement checking. Both reject a different inode. A failed exchange
with a missing name and failed unlink of that name return NotFound and leave the
source inode intact; successful exchange/removal comes from compaction tests.
No fake file descriptor or handle is passed to a native primitive.

Reuse `tests/files.rs::{creation_is_exclusive_and_aliases_cannot_bypass_locking,
nonregular_leaves_and_parents_are_rejected_without_blocking,
native_byte_names_and_a_renamed_parent_preserve_the_held_file,
unlocked_adapter_cannot_reacquire_a_replaced_inode}` and Linux's private
`opener_paused_before_flock_rejects_the_replaced_inode` race control.
Missing/nonregular/symlink leaves, exclusive creation, renamed parents and lock
races are covered. The multi-term dev/inode/type checks are B: a pinned directory
normally keeps the device fixed, and a different object type implies a different
inode. These are not independently toggled four-term MC/DC pairs. The source
iterator rsplit always yields at least one segment, so its None error is I.

## N3: barriers, compaction and observations

Reuse `tests/compaction.rs::files::offline_file_compaction_preserves_metadata_and_enforces_quiescence`
and `file_errors_at_every_observation_preserve_a_whole_canonical_store`.
They preserve modes/ACL/xattrs, reject hard links and active leases, and sweep all
before/after hook points. Replacement phase failure maps to OutcomeUnknown;
pre-exchange preparation failures preserve a whole source. Every callback step
is execution/propagation evidence, not evidence of a matching native errno.
Source/target observation choices and present/absent hooks are exercised. The
existing `replacement.rs` ledger owns the six ordered namespace barriers.

New file tests call sync on initial creation and again after parent_pending clears.
Reopen/backup tests use read-only sources and preserve bytes. Existing
`tests/files.rs::interruption::{operating_system_locks_exclude_another_process_and_release_on_kill,
process_kill_at_file_io_boundaries_preserves_acknowledged_data_and_catalogs}`
and `tests/inspection.rs::native_backup_restore_and_check_run_in_independent_processes`
check actual processes. Process termination is not a power cut or storage-controller
cache-loss test. Raw fsync/F_FULLFSYNC and metadata-copy error arms remain D when
only their surrounding hooks were faulted. Explicit unlock failure remains D;
owned File destruction is the fallback lease release.

## N4: Linux metadata callback bounds

New `linux::blob_decisions::metadata_blob_bounds_exact_fill_and_short_callbacks`
accepts sizes 0, 3 and 65536 with exactly two callback calls, verifies returned
bytes, rejects query size 65537 before allocation, and distinguishes size 65536
OutOfMemory under zero claims. For queried size 3, second-call lengths 2 and 4
both reject Io; no fake slice or oversized write occurs. New
`metadata_blob_retries_interruption_and_reports_terminal_callback_error` forces
negative query/fill callbacks with errno EACCES, checks preserved native context,
and retries EINTR before an empty successful result. It also controls status(0)
versus nonzero using thread-local Linux errno. These are callback/error-conversion
controls, not kernel xattr corruption injection.

Native Linux compaction tests exercise nonempty/empty accessible xattrs and
`replacement_drops_inherited_acl_when_source_has_none`. Removal of target-only
metadata and copying source entries run against the actual filesystem. Ownership
uid/gid mismatch, malformed kernel-returned name lists, empty xattr names, and
post-copy uid/gid/mode mismatch remain D: no fabricated kernel data or global
syscall hook was added. CopyMetadata syscall failure categories stay delegated
unless captured by a real test. Native getrandom short/zero/oversized returns,
terminal failure and three all-zero attempts are not injected; their branches
remain D. Successful identities are exercised by native creation/compaction.

## N5: Windows gates

New `windows::pathname_decisions::drive_prefix_and_early_path_guards` provides
relative/disk/verbatim-disk controls versus UNC/device prefixes, rejects empty,
trailing space/dot/separators, embedded NUL, ADS/punctuation/control characters,
and rejects 32767 UTF-16 units. These tests cross-compile on this macOS host;
Windows execution is a separate gate. Existing native Windows tests cover NTFS
copy/reopen, device/ADS rejection, Busy on repeated lock, and Closed/Io after
unlock closes the handle. The FFI audit adds Windows UTF-8 rejection separately.

Drive type, volume-query failure, non-NTFS name, unsupported directory/file
attributes, file type/reparse checks, BCrypt failure/zero output and all Windows
syscall errors remain target/native qualifications; no forged handle is used to
force them. Multiple file-type predicates can overlap. This audit does not label
all of them independently covered because a path-level rejection occurred.

## Native constants and ABI checks

A C probe compiled with installed headers verifies actual values rather than
copying comments: LOCK_EX=2, LOCK_NB=4, LOCK_UN=8; off_t is 8 bytes and uid_t/gid_t
are 4; common errno values 2/17/20/21. On macOS it also statically checks
O_DIRECTORY=0x100000, O_EXLOCK=0x20, required flags 0x4|0x100|0x01000000,
create flags 0x200|0x800, F_FULLFSYNC=51, RENAME_SWAP=2, COPYFILE_METADATA=7,
EWOULDBLOCK=35 and ELOOP=62. Linux/aarch64 headers verify O_DIRECTORY=0x4000,
O_NOFOLLOW=0x8000, O_NONBLOCK=0x800, O_CLOEXEC=0x80000, create flags 0x40|0x80,
RENAME_EXCHANGE=2, EWOULDBLOCK/EAGAIN=11 and ELOOP=40. The probe contains x86_64
Linux assertions too, but this run executes only aarch64. No new Windows SDK
constant/layout check is claimed; `cargo check --tests --target x86_64-pc-windows-msvc`
checks Rust source/types, not native import ABI or hardware behavior.

Unsupported Linux architectures are rejected by SUPPORTED before pathname/RNG
use. This branch is not dynamically reachable on the two supported ABIs; other
ABIs require a target-specific run. Fixed-budget constructor allocations and
some handle duplication/metadata errors cannot use an instance fault budget;
calling-path E evidence does not imply every such allocation failed in a test.
Macro-generated std behavior, OS/kernel implementation, actual storage hardware
and Rust pointer/FFI validity contracts remain outside the source denominator.

## Source records

### src/storage/file.rs

SHA-256: `c996ade5bca8ae3ff6c845259731eca53a6c4ad3f737547ba3d1f70d48ad6056`.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 35:9 | `match` / `retry` | P | N1 | `4945a70fa7f9` |
| 36:23 | `match_guard` / `retry` | P | N1 | `2657665d2b41` |
| 60:37 | `try_propagation` / `open_read_only` | E | N2 | `8a8de823d5ed` |
| 63:28 | `try_propagation` / `open_impl` | E | N2 | `8a8de823d5ed` |
| 66:9 | `if` / `check` | P | N2 | `935f68319d4f` |
| 71:9 | `if` / `lock` | P | N2 | `935f68319d4f` |
| 75:13 | `if` / `lock` | D | N2 | `935f68319d4f` |
| 80:11 | `try_propagation` / `lock` | E | N2 | `8a8de823d5ed` |
| 81:9 | `if` / `lock` | C | N2 | `935f68319d4f` |
| 81:12 | `pattern_test` / `lock` | P | N2 | `5654d4106d70` |
| 90:13 | `if` / `unlock` | D | N2 | `935f68319d4f` |
| 100:21 | `try_propagation` / `length` | E | N2 | `8a8de823d5ed` |
| 101:71 | `try_propagation` / `length` | E | N2 | `8a8de823d5ed` |
| 104:21 | `try_propagation` / `read` | E | N2 | `8a8de823d5ed` |
| 108:21 | `try_propagation` / `write` | E | N2 | `8a8de823d5ed` |
| 114:21 | `try_propagation` / `sync` | E | N2 | `8a8de823d5ed` |
| 115:30 | `try_propagation` / `sync` | E | N2 | `8a8de823d5ed` |
| 116:9 | `if` / `sync` | P | N2 | `935f68319d4f` |
| 117:33 | `try_propagation` / `sync` | E | N2 | `8a8de823d5ed` |
| 118:34 | `try_propagation` / `sync` | E | N2 | `8a8de823d5ed` |
| 134:9 | `if` / `new` | D | N2 | `935f68319d4f` |
| 138:9 | `if` / `new` | P | N2 | `935f68319d4f` |
| 141:9 | `if` / `new` | P | N2 | `935f68319d4f` |
| 144:77 | `try_propagation` / `new` | I | N2: rsplit always yields a segment | `8a8de823d5ed` |
| 145:9 | `if` / `new` | C | N2 | `935f68319d4f` |
| 145:12 | `short_circuit_or` / `new` | C | N2 | `7984fc323ef7` |
| 145:12 | `short_circuit_or` / `new` | P | N2 | `98c45bed00a3` |
| 157:11 | `try_propagation` / `new` | E | N2 | `8a8de823d5ed` |
| 158:49 | `try_propagation` / `new` | E | N2 | `8a8de823d5ed` |
| 159:47 | `try_propagation` / `new` | E | N2 | `8a8de823d5ed` |
| 160:43 | `try_propagation` / `new` | E | N2 | `8a8de823d5ed` |
| 172:15 | `try_propagation` / `open_mode` | E | N2 | `8a8de823d5ed` |
| 173:49 | `try_propagation` / `open_mode` | E | N2 | `8a8de823d5ed` |
| 173:76 | `try_propagation` / `open_mode` | E | N2 | `8a8de823d5ed` |
| 175:22 | `if` / `open_mode` | P | N2 | `935f68319d4f` |
| 176:58 | `if` / `open_mode` | P | N2 | `935f68319d4f` |
| 176:95 | `try_propagation` / `open_mode` | E | N2 | `8a8de823d5ed` |
| 177:9 | `if` / `open_mode` | P | N2 | `935f68319d4f` |
| 177:67 | `try_propagation` / `open_mode` | E | N2 | `8a8de823d5ed` |
| 180:39 | `try_propagation` / `open_mode` | E | N2 | `8a8de823d5ed` |
| 189:37 | `try_propagation` / `acquire_open_lease` | E | N2 | `8a8de823d5ed` |
| 197:22 | `match` / `check_lease` | P | N2 | `4945a70fa7f9` |
| 199:27 | `try_propagation` / `check_lease` | E | N2 | `8a8de823d5ed` |
| 201:78 | `try_propagation` / `check_lease` | E | N2 | `8a8de823d5ed` |
| 202:78 | `try_propagation` / `check_lease` | E | N2 | `8a8de823d5ed` |
| 203:9 | `if` / `check_lease` | C | N2 | `935f68319d4f` |
| 203:12 | `short_circuit_and` / `check_lease` | C | N2 | `4eeffbec39d2` |
| 203:12 | `short_circuit_and` / `check_lease` | B | N2 | `06e4152e1c2f` |
| 211:13 | `if` / `open_flags` | P | N2 | `935f68319d4f` |
| 225:17 | `if` / `open_flags` | P | N2 | `935f68319d4f` |
| 231:10 | `try_propagation` / `open_flags` | E | N2 | `8a8de823d5ed` |
| 235:45 | `try_propagation` / `matches` | E | N2 | `8a8de823d5ed` |
| 236:83 | `try_propagation` / `matches` | E | N2 | `8a8de823d5ed` |
| 237:78 | `try_propagation` / `matches` | E | N2 | `8a8de823d5ed` |
| 238:9 | `if` / `matches` | C | N2 | `935f68319d4f` |
| 238:12 | `short_circuit_and` / `matches` | C | N2 | `5e4b48797a03` |
| 238:12 | `short_circuit_and` / `matches` | C | N2 | `ecbd1ce3dffe` |
| 238:12 | `short_circuit_and` / `matches` | B | N2 | `224acd7414e7` |
| 249:59 | `try_propagation` / `scratch` | E | N3 | `8a8de823d5ed` |
| 250:44 | `try_propagation` / `scratch` | E | N3 | `8a8de823d5ed` |
| 251:9 | `for_iteration` / `scratch` | E | N3 | `10c22bcf4c76` |
| 251:31 | `try_propagation` / `scratch` | E | N3 | `8a8de823d5ed` |
| 252:13 | `for_iteration` / `scratch` | E | N3 | `10c22bcf4c76` |
| 260:15 | `try_propagation` / `scratch` | E | N3 | `8a8de823d5ed` |
| 271:9 | `if` / `remove` | P | N3 | `935f68319d4f` |
| 319:9 | `if` / `event` | C | N3 | `935f68319d4f` |
| 319:12 | `pattern_test` / `event` | P | N3 | `5654d4106d70` |
| 320:57 | `try_propagation` / `event` | E | N3 | `8a8de823d5ed` |
| 326:32 | `try_propagation` / `call` | E | N3 | `8a8de823d5ed` |
| 328:31 | `try_propagation` / `call` | E | N3 | `8a8de823d5ed` |
| 346:13 | `if` / `length` | P | N3 | `935f68319d4f` |
| 356:13 | `if` / `read` | P | N3 | `935f68319d4f` |
| 366:13 | `if` / `write` | P | N3 | `935f68319d4f` |
| 376:13 | `if` / `sync` | P | N3 | `935f68319d4f` |
| 394:34 | `try_propagation` / `compact_file_with_hook` | E | N3 | `8a8de823d5ed` |
| 395:37 | `try_propagation` / `compact_file_with_hook` | E | N3 | `8a8de823d5ed` |
| 397:59 | `try_propagation` / `compact_file_with_hook` | E | N3 | `8a8de823d5ed` |
| 402:34 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 403:39 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 404:38 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 407:7 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 409:5 | `if` / `compact_file_impl` | P | N3 | `935f68319d4f` |
| 409:66 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 419:6 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 420:41 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 421:36 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 424:7 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 438:13 | `if` / `compact_file_impl` | P | N3 | `935f68319d4f` |
| 443:11 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 446:7 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 447:73 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 450:29 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 452:36 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 454:7 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |
| 458:40 | `match` / `compact_file_impl` | P | N3 | `4945a70fa7f9` |
| 472:40 | `try_propagation` / `compact_file_impl` | E | N3 | `8a8de823d5ed` |

Totals: B=2, C=9, D=3, E=57, I=1, P=24; **96 records**.

### src/storage/macos.rs

SHA-256: `04929b34939c3400c2e5693e90f3f1ffc36f1395ae71df60c51b1e7f64e268e6`.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 32:5 | `match` / `os_error` | P | N1 | `4945a70fa7f9` |
| 42:9 | `if` / `full_sync` | D | N4 RNG / N3 native syscall | `935f68319d4f` |
| 50:5 | `for_iteration` / `identity` | D | N4 RNG / N3 native syscall | `10c22bcf4c76` |
| 55:9 | `if` / `identity` | D | N4 RNG / N3 native syscall | `935f68319d4f` |
| 67:5 | `if` / `exchange` | P | N4 RNG / N3 native syscall | `935f68319d4f` |
| 88:9 | `if` / `copy_metadata` | D | N4 RNG / N3 native syscall | `935f68319d4f` |

Totals: D=4, P=2; **6 records**.

### src/storage/linux.rs

SHA-256: `b86adc2e11b1a1b27be85d5384b26526b32a6aa3b2dc9c7b57987daa4703d230`.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 60:5 | `match` / `os_error` | P | N1 | `4945a70fa7f9` |
| 75:9 | `if` / `acquire_open_lock` | P | N2 | `935f68319d4f` |
| 83:5 | `if` / `identity` | D | N4 | `935f68319d4f` |
| 86:5 | `for_iteration` / `identity` | D | N4 | `10c22bcf4c76` |
| 89:9 | `while` / `identity` | D | N4 | `07a875073882` |
| 95:17 | `if` / `identity` | D | N4 | `935f68319d4f` |
| 100:15 | `try_propagation` / `identity` | D | N4 | `8a8de823d5ed` |
| 101:13 | `if` / `identity` | C | N4 | `935f68319d4f` |
| 101:16 | `short_circuit_or` / `identity` | D | N4 | `0ac12b6659d9` |
| 106:9 | `if` / `identity` | D | N4 | `935f68319d4f` |
| 115:5 | `if` / `exchange` | P | N2/N3 missing-name exchange control | `935f68319d4f` |
| 134:5 | `if` / `status` | P | N4 | `935f68319d4f` |
| 144:13 | `if` / `blob` | P | N4 | `935f68319d4f` |
| 151:45 | `try_propagation` / `blob` | E | N4 | `8a8de823d5ed` |
| 154:5 | `if` / `blob` | P | N4 | `935f68319d4f` |
| 158:43 | `try_propagation` / `blob` | E | N4 | `8a8de823d5ed` |
| 160:50 | `try_propagation` / `blob` | E | N4 | `8a8de823d5ed` |
| 161:5 | `if` / `blob` | P | N4 | `935f68319d4f` |
| 169:7 | `try_propagation` / `names` | E | N4 | `8a8de823d5ed` |
| 170:5 | `if` / `names` | C | N4 | `935f68319d4f` |
| 170:8 | `short_circuit_and` / `names` | D | N4 | `b37362e80b47` |
| 176:78 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 177:74 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 178:5 | `if` / `copy_metadata` | C | N4 | `935f68319d4f` |
| 178:8 | `short_circuit_or` / `copy_metadata` | D | N4 | `ea4f1cfcdf15` |
| 181:11 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 187:7 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 188:40 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 189:38 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 190:5 | `for_iteration` / `copy_metadata` | E | N4 | `10c22bcf4c76` |
| 191:9 | `if` / `copy_metadata` | P | N4 | `935f68319d4f` |
| 197:15 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 200:5 | `for_iteration` / `copy_metadata` | E | N4 | `10c22bcf4c76` |
| 201:9 | `if` / `copy_metadata` | D | N4 | `935f68319d4f` |
| 206:11 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 217:11 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 219:74 | `try_propagation` / `copy_metadata` | E | N4 | `8a8de823d5ed` |
| 220:5 | `if` / `copy_metadata` | C | N4 | `935f68319d4f` |
| 220:8 | `short_circuit_or` / `copy_metadata` | C | N4 | `968a0d4a81da` |
| 220:8 | `short_circuit_or` / `copy_metadata` | D | N4 | `301bd5da1699` |

Totals: C=5, D=11, E=16, P=8; **40 records**.

### src/storage/windows.rs

SHA-256: `9e5168092d973d8c5e6dd58188c9b9e2431c38996eb2e4dc79aae4af9dd4594d`.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 47:5 | `match` / `os_error` | D | N5: tests cross-compiled; native CI required | `4945a70fa7f9` |
| 57:5 | `for_iteration` / `identity` | D | N5: tests cross-compiled; native CI required | `10c22bcf4c76` |
| 61:9 | `if` / `identity` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 68:9 | `if` / `identity` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 76:5 | `match` / `drive` | D | N5: tests cross-compiled; native CI required | `4945a70fa7f9` |
| 77:44 | `match` / `drive` | D | N5: tests cross-compiled; native CI required | `4945a70fa7f9` |
| 88:5 | `if` / `check_volume` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 94:5 | `if` / `check_volume` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 112:5 | `if` / `check_volume` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 140:20 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 142:9 | `if` / `open_impl` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 145:9 | `if` / `open_impl` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 145:12 | `short_circuit_or` / `open_impl` | D | N5: tests cross-compiled; native CI required | `18034fdecac2` |
| 150:58 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 151:9 | `if` / `open_impl` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 152:49 | `short_circuit_or` / `open_impl` | D | N5: tests cross-compiled; native CI required | `3f7e9b0c6e3c` |
| 162:80 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 163:36 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 163:71 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 169:80 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 172:75 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 173:9 | `if` / `open_impl` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 176:9 | `if` / `open_impl` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 179:41 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 192:21 | `if` / `open_impl` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 199:15 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 202:75 | `try_propagation` / `open_impl` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 204:9 | `if` / `open_impl` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 204:12 | `short_circuit_or` / `open_impl` | D | N5: tests cross-compiled; native CI required | `5e3817a40278` |
| 204:12 | `short_circuit_or` / `open_impl` | D | N5: tests cross-compiled; native CI required | `4d6e673f2ceb` |
| 210:9 | `if` / `open_impl` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 226:9 | `if` / `lock` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 229:9 | `if` / `lock` | D | N5: tests cross-compiled; native CI required | `935f68319d4f` |
| 243:20 | `try_propagation` / `length` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 245:75 | `try_propagation` / `length` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 250:20 | `try_propagation` / `read` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 256:20 | `try_propagation` / `write` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |
| 262:20 | `try_propagation` / `sync` | D | N5: tests cross-compiled; native CI required | `8a8de823d5ed` |

Totals: D=38; **38 records**.

## Executed validation and reproduction

macOS/aarch64 debug and release: three file decision tests, one native mapping
test, nine diagnostics tests, eight `files`, ten `compaction`, five `inspection`
tests passed. Linux/aarch64 in `rust:1-bookworm` Docker: three primitive tests,
four file tests (including the open/flock race), nine diagnostics tests, eight
`files`, eleven `compaction` and five `inspection` tests passed across debug and
release runs. The final missing-name exchange/unlink assertions were rerun on
macOS debug/release and Linux debug after the earlier broad suites.

Windows all-feature tests passed compilation with
`cargo check --offline -p weekend-db --all-features --tests --target x86_64-pc-windows-msvc`.
They were not executed on Windows here. Linux's container filesystem/process
results do not qualify a deployed filesystem, controller or power-loss behavior.
All host builds use `CARGO_INCREMENTAL=0` and the reusable
`CARGO_TARGET_DIR=target/verification-diagnostics`; Docker uses a private target
inside its disposable container with source and dependency registry mounted
read-only. The source inventory has zero parse failures. No new mutation score
or whole-module MC/DC percentage is claimed.

The following probe was compiled and executed with `cc -Wall -Werror` against
installed macOS SDK headers and Docker Linux/aarch64 headers. Save as probe.c,
compile to a temporary executable and run it. Its x86_64 Linux clause is a
reproduction recipe, not evidence that this host executed x86_64 Linux.

```c
#define _GNU_SOURCE
#include <fcntl.h>
#include <errno.h>
#include <stdio.h>
#include <sys/types.h>
#include <sys/file.h>
_Static_assert(LOCK_EX == 2 && LOCK_NB == 4 && LOCK_UN == 8, "flock constants");
_Static_assert(sizeof(off_t) == 8 && sizeof(uid_t) == 4 && sizeof(gid_t) == 4, "ABI widths");
_Static_assert(ENOENT == 2 && EEXIST == 17 && ENOTDIR == 20 && EISDIR == 21, "errno constants");
#ifdef __APPLE__
#include <copyfile.h>
_Static_assert(O_DIRECTORY == 0x100000 && O_EXLOCK == 0x20, "open directory and lease");
_Static_assert((O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC) == (0x4 | 0x100 | 0x01000000), "required flags");
_Static_assert((O_CREAT | O_EXCL) == (0x200 | 0x800), "create flags");
_Static_assert(F_FULLFSYNC == 51 && RENAME_SWAP == 2 && COPYFILE_METADATA == 7, "publication constants");
_Static_assert(EWOULDBLOCK == 35 && ELOOP == 62, "Mac errno");
#else
#include <linux/fs.h>
#if defined(__aarch64__)
_Static_assert(O_DIRECTORY == 0x4000 && O_NOFOLLOW == 0x8000, "arm64 open flags");
#elif defined(__x86_64__)
_Static_assert(O_DIRECTORY == 0x10000 && O_NOFOLLOW == 0x20000, "x64 open flags");
#endif
_Static_assert(O_NONBLOCK == 0x800 && O_CLOEXEC == 0x80000, "required flags");
_Static_assert((O_CREAT | O_EXCL) == (0x40 | 0x80), "create flags");
_Static_assert(RENAME_EXCHANGE == 2 && EWOULDBLOCK == 11 && EAGAIN == 11 && ELOOP == 40, "Linux constants");
#endif
int main(void) { puts("native constants and ABI widths verified"); }
```
