# C file factories with selected allocators

The C11 and C++17 package consumers run the file fixture in a fresh
private directory before and after package relocation. They use the
same allocator callbacks as the [in-memory fixture](ffi-generated-exports-current.md).
Each consumer rejects invalid input before invoking the allocator,
creates a durable database and reopens its current and retained roots
with the supplied limits and allocator.

## Allocation refusal and on-disk outcome

| Stage | Handle and allocator | Path and data |
| --- | --- | --- |
| Refusal before native creation | OutOfMemory and null output. One context release and zero live allocations. | No file. |
| Refusal after exclusive open and before initialization | Same status and cleanup. | An empty file remains. Ordinary open returns Corrupt, establishing that the failing call released its lease. |
| Refusal allocating the final C database handle | Same status and cleanup. | A complete database reopens at revision 0 with an empty retained catalog. |
| Refusal at any observed reopen allocation | Same status and cleanup. | Bytes remain identical. Fresh open sees current revision 4 with value 23 and retained revision 2 with value 17. |

The fixture visits every allocation observed in successful create and
reopen calls. It checks context release after the last free, layout
agreement, the null output on refusal, exact image bytes and both
roots. It also verifies that a fresh creation can use the private trial
pathname after removing any file created there by the fixture.

## Compiled factory controls

The Rust export test checks both factory expansions. It observes
Invalid for a malformed path or invalid limits, NotFound before
creation, AlreadyExists on a second create, and the configured limits
through both live handles. Repeat creation preserves the original bytes.
The four controls change the path status, change the options status,
discard the forwarded options, or change the native adapter status.

| Source site | Byte span | SHA-256 prefix |
| --- | --- | --- |
| `file_factory_with_options` | 7107..7736 | `c5645886452b` |
| `wdb_file_create_with_options` | 7737..7819 | `9b32d127cb3d` |
| `wdb_file_open_with_options` | 7821..7899 | `06fe8d93e6f9` |

## Parsed source binding

| Source | SHA-256 |
| --- | --- |
| `src/ffi.rs` | `b3fb7b8e86ea91e4f9bccfeb825183a618220274cc499a28c4c1b0f62fe67588` |

| Status | Target | Position | Byte span | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/ffi.rs:42:5– | 1604..1609 | match | boundary | The engine suite reruns the success, ordinary errors and adapter unwind status cases. |

## Verification

Frozen source `c4c0bd4c66b1342a031f731b35c452aa60363a9f` used Rust 1.100.0.
The library, allocation, allocator-routing, file, compaction, durability
and retention suites passed 284 tests per profile on arm64 macOS and
295 per profile on aarch64 Docker Linux/ext4. All four mutations failed
the intended test in debug and release on both hosts. Clippy passed on
both hosts and for the Windows MSVC cross target.

The full xtask suite passed 245 tests on macOS and 274 on Linux, with
three and one existing platform-specific ignores respectively. The CLI
shard test fixture now filters registrations by host before checking
CLI listing and shard completeness.

Each of four C and C++ executions per package observed 21 create
refusals (3 absent, 17 empty, 1 published) and 43 reopen refusals.
The harness checked the exact loaded package and captured the same
results before and after relocation.

| Host | Delivered shared library SHA-256 | Package manifest SHA-256 |
| --- | --- | --- |
| macOS arm64 | `966d4284d3e2efb7bbd356d2aac95c9cc2dc3806456c0560c4fbcce7843204f9` | `3befdb798e8636458c9efe6176ae3d9f34643b8943f3d43bd3c622607423a533` |
| Linux aarch64 | `a06811301ca47c69617e2ad5ff3dddd5507ae35490a755d644def1ba39de758e` | `e8a5d70983ebc23e8687e7c1a557bfab364995d35d295fde6618e8009e0fbb34` |

Native Windows still needs the packaged C and C++ run. The compiled
factory checks bind three supplemental macro sites. The scratch-directory
runner adds five parsed tooling decisions. The open source inventory is
9,396 records (2,014 runtime, 26 driver and 7,356 tooling).

```sh
cargo +1.100.0 test --offline --locked -p weekend-db --all-features --lib \
  ffi::tests::file_factories_forward_options_and_preserve_stage_errors -- --exact
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-c-file-factories- \
  --output target/c-file-factories-debug
cargo +1.100.0 xtask package --features crdt,replication --odin \
  --output target/c-file-factories-package
```

Repeat the mutation command with `--release` and a separate output
directory.
