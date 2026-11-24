# FFI handle accounting and allocation refusal

The C exports share a checked handle count. A successful reservation
increments the count before allocating. If allocation refuses, dropping
the temporary handle restores the count and the output stays null. A
successful allocation remains counted through explicit release.
`wdb_close` requires external quiescence and returns Busy while any
handle remains live. The counter supports concurrent acquisition and
release under that contract.

| Case | Status and output | Ownership observation |
| --- | --- | --- |
| Count already at `usize::MAX` | Limit and null clone | Count and source stay intact. After restoring a reachable count the source can be cloned, read and released. |
| Actual allocation refuses after reservation | OutOfMemory and null output | Temporary handle rolls back its count and the selected allocator returns to its baseline. |
| Two callers cloning a shared value with one allocation allowed | One success with an owned clone and one OutOfMemory with null | Joining leaves exactly the source and one clone. Close returns Busy. Releasing both permits close and returns selected-allocator allocations to zero. |

The bounded Loom model copies the checked relaxed reservation, AcqRel
release and post-join Acquire observation from production. A refusal
worker rolls back when it can reserve, and a successful worker keeps its
reservation until the post-join check. One run starts with one owner. A
second starts just below saturation. Each uses three threads, preemption
bound two, branch bound 1,000 and no duration or permutation cutoff. The
two negative controls replace CAS with a load/store pair or omit the
allocation rollback. Native and Miri tests exercise the real pointers and
allocator independently of the model's integer count.

## Exact source dispositions

| Source | SHA-256 |
| --- | --- |
| `src/ffi.rs` | `4e9a21c5ff377463642c3f52156cad82f190469daefe8250bdd8b7db2ec0294d` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/ffi.rs:82:35– | 2939..2940 | try_propagation | make_handle | A full count returns Limit without creating a handle. A free reservation reaches allocation. |
| E | weekend-db::weekend_db | src/ffi.rs:84:53– | 3065..3066 | try_propagation | make_handle | Allocator refusal returns OutOfMemory and restores the count. A successful allocation transfers one live handle. |

## Verification

Frozen source `be140f58eb22c134b96d68a740aab38d393061ac` used Rust 1.100.0.
The library, allocation, allocator-routing and concurrency suites passed
234 tests per profile on arm64 macOS and 244 on aarch64 Docker Linux/ext4.
All three named source mutations failed their intended tests in both
profiles on both hosts. The Loom baseline and both fail-required
controls passed the campaign on each host. Strict Clippy and the 18
mutation-runner contract tests passed on both hosts. macOS Miri
`708c94ef0` passed the new real-allocator test with strict provenance.
Windows MSVC cross-target Clippy passed. Native Windows and delivered
C/C++/Odin artifact qualification remain open.

The C consumers `examples/memory.c` and `examples/threads.c` passed
against each of the following libraries. Loader output identifies the
linked library on Linux, and `otool -L` and byte comparison identify the
Mach-O image on macOS.

| Host | Profile | Loaded library SHA-256 |
| --- | --- | --- |
| macOS arm64 | Debug | `cded21f450856e1f4b7e442329dd3144372815443bbed71291d741878e9b8b46` |
| macOS arm64 | Release | `a1697c0aa3cc6876d1d619792e30cf7a0196ebae7d62ddf3177b4ac46fe31c8e` |
| Linux aarch64 | Debug | `a6951ac5b66a044e0f225af6896f5b7e97687efc24647634e39f1f9c137e7fb0` |
| Linux aarch64 | Release | `302b83f2a37848bf70aebc29e183ac04da95ccf827f1b2b89b4c1af1cf332b67` |

The audit records two runtime propagation controls. The eight new model
and dispatch source records retain tooling gaps. The open inventory is
9,413 records (2,036 runtime, 26 driver and 7,351 tooling). The checked
count and allocator rollback are covered here. Generated export bodies
and the remainder of batch 6 still require review.

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features \
  --lib --test allocation --test allocator_routing --test concurrency
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-ffi-handles- \
  --output target/ffi-handles-debug
cargo +1.100.0 xtask atomic-model --toolchain 1.100.0
```

Repeat the first two commands with `--release` and a separate mutation
output directory.
