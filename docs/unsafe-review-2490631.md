# Independent unsafe-boundary review of 2490631

Reviewed locally on 2026-11-12 at documentation head
`51e8f278ede20af56afb48ddd71b8b56c964524e`. All seven boundary files below are
unchanged from qualified runtime `2490631dab43678279f8ba2c56458c8ea908c1b4`.
No actionable unsafe-boundary defect was found for calls satisfying the
documented pointer, exclusive-transaction, release-exclusion and close-quiescence
contracts. This is a scoped independent review, not a whole-library memory-safety
proof or production-readiness declaration. No implementation edits or remote
actions were performed for this review.

The review used the ownership and threading contracts in DESIGN.md,
`docs/decisions/{memory,ffi,native-storage}.md`, the live-budget decision ledger,
and `docs/local-qualification-2490631.md`. It reviewed every production unsafe
block/implementation in the files below, all macro-generated FFI bodies, their
native declarations and relevant safe wrappers, and the test-only unsafe controls.

## Source binding

Each file was compared with both Git revisions above and with its recorded hash
in the qualified macOS C/Rust ASan manifest. All matched. All 17 evidence JSON
records in `target/local-qualification-2490631/index.json` also matched their
indexed SHA-256 values. That check verifies the records' integrity; it does not
rerun every recorded command or rehash every referenced binary/log.

| File | SHA-256 |
| --- | --- |
| `src/memory.rs` | `56b3852ee4c3fc970cccf60a98cf5a3db4145c5298960f6d7200f48755b731dd` |
| `src/ffi.rs` | `36680a4ebabcab192b5695d1e9a82a8f5e100d843467c8c910aa78ce24ff8c5d` |
| `src/storage/file.rs` | `c996ade5bca8ae3ff6c845259731eca53a6c4ad3f737547ba3d1f70d48ad6056` |
| `src/storage/macos.rs` | `04929b34939c3400c2e5693e90f3f1ffc36f1395ae71df60c51b1e7f64e268e6` |
| `src/storage/linux.rs` | `b86adc2e11b1a1b27be85d5384b26526b32a6aa3b2dc9c7b57987daa4703d230` |
| `src/storage/windows.rs` | `9e5168092d973d8c5e6dd58188c9b9e2431c38996eb2e4dc79aae4af9dd4594d` |
| `include/weekend_db.h` | `303319e5322c5c1687baae6dcbc59c56ad271d082ff299410cbfbd367ac194b1` |

## Unsafe-site and invariant checklist

Line numbers refer to the bound source above. Each row was reviewed without
finding a demonstrated violation in current production call paths.

| Sites | Invariants reviewed |
| --- | --- |
| `memory.rs:16-30` | Valid nonzero allocator layouts; same-allocation, original-layout and nonzero-new-size realloc preconditions; test null results preserve allocator ownership. |
| `memory.rs:101-102` | Both unsafe `Send`/`Sync` implementations require `T: Send + Sync`; shared access exposes only `&T`, with final destruction synchronized. |
| `memory.rs:107-115,127,143,148-164` | Aligned initialized SharedBox allocation, live source during Relaxed clone, abort threshold before practical count wrap, AcqRel release sequence before last destruction, one-time charge transfer, and payload lifetime. |
| `memory.rs:178-185` | Exact-layout deallocation precedes charge release; the allocation guard also runs after payload destruction unwinds. |
| `memory.rs:202-230` | Checked element addition and array layout; ZST capacity prevents zero-size allocation; exact old/new capacities; failed realloc retains original pointer, initialized contents and charge; successful realloc is followed only by non-unwinding metadata replacement/forget. |
| `memory.rs:233-324` | Safe slice/mutation API cannot change backing capacity implicitly; reserved-capacity assertions, partial consuming iteration and field destruction retain charge until buffer release; explicit external transfer releases accounting. |
| `memory.rs:435-460` | Nonzero raw allocation, correct global-allocator provenance/alignment, initialization, one-time Box/raw ownership transfer and destruction. `free_raw` assumes exact original T and exclusive ownership; a panicking destructor can leak its allocation, without a demonstrated subsequent invalid access. |
| `ffi.rs:20-37,46-86` | Opaque typed live handles, borrowed versus exclusive raw-pointer conversion, output initialization, bounded input slices, checked handle admission, allocation-failure rollback and owner lifetime. |
| `ffi.rs:95-118,136-155` | Required-size writes, null/capacity guards, no partial copying on insufficient capacity, source/destination nonoverlap, bounded path slices, embedded-NUL rejection and platform encoding. |
| `ffi.rs:120-365` | Every export body, including `file_factory`, `release` and `indexed_edit` expansions: output checks before publication, foreign-owner checks, extended snapshot/transaction borrows, terminal transactions, all release/close paths and panic containment. Scalar widths match header types; opaque structs require no C-visible field layout. |
| `file.rs:25-29,75,90,217-232,271` | Native declarations, valid borrowed descriptors, NUL-terminated leaf names, openat variadic argument promotion, nonnegative-descriptor ownership transfer into File exactly once, flock operations and unlinkat arguments. |
| `macos.rs:18-29,42,52,67-88` | C ABI types, fcntl command, RNG writable span, rename arguments and flags, copyfile state/flags and descriptor lifetimes. |
| `linux.rs:36-56,75,94,115,167-215` | C ABI widths/constants, RNG suffix bounds and progress checks, NUL-terminated exchange/xattr names, bounded initialized xattr buffers, size-zero query semantics and returned-size checks, borrowed descriptors and metadata calls. |
| `windows.rs:25-43,60,88-108,204` | System calling convention, HANDLE/object-pointer representation, DWORD/BOOL/NTSTATUS widths, bounded UTF-16 and RNG outputs, optional null volume outputs and live-handle duration. This was source review, not native execution. |
| Test-only unsafe sites in the reviewed files | Synthetic Shared/foreign counter restoration, output-only pointer sentinels, oversized declared lengths rejected before access, real allocated FFI handles and release balance, Linux bounded callback writes and TLS errno controls. Forged/dangling handle dereferences were not used. |

## Preconditions that remain obligations

- C handles must have the correct opaque type and be live/aligned; each returned
  handle is released once. Input spans must contain initialized readable bytes
  within one allocation and remain stable for their borrowed use. Output storage
  must be writable, correctly sized/aligned, synchronized and nonaliasing as
  documented. The initialized/single-allocation/stable-span requirements are
  implicit in the header's validity/borrow language rather than separately listed.
- Transactions require exclusive access; synchronized transfer is permitted.
  Release excludes overlapping access to that handle. Close excludes every call
  on the database and its handles, including acquisitions and releases. Busy is
  not a concurrent-close protocol. In particular, Handle::drop decrements before
  field destruction, so count zero alone does not establish release completion.
- The unconstrained helper lifetimes in `ffi::{db,handle,handle_mut,input,path}`
  require pointees to survive their actual uses. Stored `'static` snapshot and
  transaction borrows rely on handle accounting plus external close quiescence.
- Safe internal `macos::exchange` and `linux::exchange` accept byte slices but
  require terminating NULs for C. The sole current production path supplies
  Location.name; both Location constructors establish this invariant. The helper
  signatures do not encode it.
- ChargedVec's exact-capacity and sole-buffer-owner invariants must survive future
  API changes. Original-layout reconstruction and raw-pointer transfers depend
  on them. Raw allocator wrappers and free_raw retain their allocator/exclusive
  ownership preconditions; null handling does not establish allocator correctness.

## Fresh Miri verification

All commands below exited zero, with **13 tests passed** in total. Used
`CARGO_INCREMENTAL=0`, isolated `CARGO_TARGET_DIR=target/unsafe-review-51e8f27`,
and `mise x rust@1.99.1 -- cargo +nightly miri test --offline -p weekend-db
--all-features --lib FILTER`. The explicit nightly was
`rustc 1.101.0-nightly (708c94ef0 2026-11-03)`, LLVM 23.1.5,
host `aarch64-apple-darwin`; Mise's stable pin did not replace that nightly.

| FILTER | Passed | Exercised scope |
| --- | ---: | --- |
| `memory::allocation_layout_tests::` | 2 | Over-aligned realloc, partial consuming iterator, null Shared cleanup, synthetic admission-overflow rollback. |
| `memory::live_budget_tests::` | 7 | Cap/overflow/layout decisions, concurrent admission, failed claim/null realloc rollback, Shared and iterator unwinding, usage-window telemetry, ZST and external transfer. |
| `ffi::decision_tests::` | 4 | Scalar arms/output guards, null inputs, acquisition/release balance, compaction output guards and bounded path/encoding behavior. |

These results were observed from the commands' terminal output; this review did
not create a new independently hashed Miri evidence bundle. Boundary source was
checked unchanged after the runs. Miri did not execute native storage syscalls or
the native abort subprocess, and these filters are not the entire FFI test suite.

## Reused evidence and residual platform scope

The reviewed Loom implementation copies the relevant production counter ordering.
The indexed exact-runtime record reports successful ownership/budget baselines
and detected broken-admission/broken-sampling controls. Its production-memory
hash matches the table. The model uses three threads, preemption bound two and
branch bound 1,000; no duration/permutation cutoff. Loom's Arc owns its allocation,
so this models atomic protocols rather than raw-pointer validity or Rust aliasing.
The documented Relaxed-refcount mutant is separate historical evidence, not one
of those four qualification runs.

The verified macOS/Linux shared-runtime C-ASan records report three passing
memory/thread/file consumers, passing valid C/Rust calibration controls and
detected intentional C/Rust errors. These diagnostic builds cover finite programs,
not every export or syscall failure. Earlier Miri and native constant-probe claims
remain scoped as recorded in the decision ledgers; this review did not rerun the
constant probe or silently upgrade historical evidence into exact-head execution.

Native Windows execution remains absent for runtime 2490631: cross-linking does
not qualify actual DLL/import-library C/C++ relocation, NTFS persistence/locking,
reparse/alias handling, interruption behavior or Windows-specific guards. The
local Linux native/constant campaign was aarch64, so it supplies no new native
Linux x86_64 execution. Other unexecuted platform/ABI combinations retain their
own qualification obligations. OS/kernel and global-allocator behavior, filesystem
and device guarantees, real controller/power-loss behavior and exhaustive failure
or scheduling exploration remain outside this review's proof.
