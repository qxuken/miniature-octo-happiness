# FFI source decision audit

`src/ffi.rs` contains 20 parsed production AST records (12 ifs, five `?`, one
match, one &&, one ||). **This is not the FFI denominator:** the inventory also
records 50 unexpanded macro sites, including export/template definitions and
invocations containing executable decisions. Both lists are classified below.
The macro appendix maps every site to obligations and evidence without pretending
to expand macros or report compiler MC/DC. P = direct predicate/arm controls;
C = overlapping syntax; E = execution/propagation, not every callee error;
D = delegated platform or unmeasured generated behavior.

## F1: status and handle ownership

`ffi::tests::exported_commit_contains_adapter_panic_and_requires_recovery`
controls successful calls, ordinary errors and caught adapter panic. Panic maps
to Panic and forces recovery; the separate native adapter test exercises the Rust
boundary. Panic-abort builds cannot catch aborts and remain outside that pair.
`handle_count_exhaustion_rejects_clone_without_losing_source` and
`foreign_handle_allocation_failure_rolls_back_live_count` control checked handle
count increment and allocation rollback. `shared_foreign_handles_count_concurrent_acquisition_and_release`
checks real concurrent handles. `invalid_commit_output_cannot_publish_and_live_handles_block_close`
checks busy/closed admission and that a missing commit output cannot publish.

`retained_id_buffers_and_foreign_snapshots_preserve_outputs` supplies same versus
foreign owner controls using two actual live databases. Neither foreign-owner
checks nor live-handle counts are a registry capable of validating arbitrary
pointers. New `decision_tests::null_inputs_and_scalar_outputs_do_not_publish_or_acquire_handles`
checks null database/transaction inputs and output resets, then valid snapshot,
scan and collection controls with balanced releases.

## F2: inputs and buffers

`input_length_kind_and_output_guards_preserve_handle_ownership` controls bounded
input, zero/nonzero length, null/non-null bytes, scalar kind guards and invalid
kinds. Length limit is checked before constructing a slice. Oversized declared
length tests use the documented early rejection path, never dereference that
span. `byte_buffers_distinguish_null_capacity_and_required_length` uses independent
pairs for A=buffer null and B=capacity nonzero: (F,T)/(T,T) and (T,F)/(T,T), plus
empty/nonempty and insufficient/exact capacity. Required-size and output sentinels
are checked. The retained-ID test repeats these controls for the u64 destination.
`scan_bounds_and_invalid_transaction_modes_reset_outputs` controls valid/invalid
indices and mode arms, and terminal transaction status.

## F3: scalar outputs and macro-local decisions

New `decision_tests::scalar_arms_and_output_failures_preserve_valid_handles`
executes every kind (None, bool, signed, unsigned, float, bytes, map, list), checks
all five scalar-success arms and collection Type failures, preserves float bits,
and checks null outputs and reset sentinels for kind/scalar/length. Null inputs
return Invalid after resetting a valid output; handles remain releasable.
The scalar conversion errors inside an arm selected by the same immutable kind
are delegated invariants, not separately injectable Type errors.

New `compaction_output_guards_are_independent_before_path_validation` supplies
(F,F), (T,F), (F,T) for before/after nullness. With both outputs valid, deliberately
invalid path input resets both to zero; either null output rejects before changing
the other sentinel. Thus controls observe the actual guard decision even when
both later paths return Invalid. It also checks scan-key missing required output.
Existing C `examples/memory.c` covers nested map/list edits, lookup absence,
transaction conflicts, historical snapshots and foreign values across the ABI;
`examples/file.c` covers native retained roots/reopen/compaction. These are execution
controls, not all exported call-site error combinations.

## F4: native path and platform boundaries

`file_path_null_length_and_nul_guards_reset_output` gives independent path-null
and zero-length OR pairs and embedded-NUL rejection. New
`decision_tests::native_path_length_boundary_and_encoding_contract` accepts exactly
1 MiB and rejects one byte more before access. Unix accepts an invalid UTF-8 byte
as an OS string; the Windows-only counterpart rejects it. The latter is compiled
only on Windows and is not claimed executed on this macOS host. Unsupported-platform
factory/compaction dispatch is cfg-selected, not a runtime Boolean pair on one host.
Native filesystem operation failures delegate to adapter tests and diagnostics.

## Contract exclusions and residual scope

Non-null handles must be valid/aligned/live, input spans valid for accessed lengths,
outputs nonaliasing and correctly sized, and close/release externally quiescent.
Dangling/forged handles, double release, aliasing output buffers and concurrent
release with access are caller-invalid UB, not testable rejection promises. No
such pointer is dereferenced by these tests. Output-only sentinels may be invalid
addresses because callees overwrite them rather than reading their pointees.
C ASan/UBSan, Rust ASan, Miri and modeled abort/refcount controls provide scoped
prior evidence; see [C sanitizers](../c-sanitizers.md), MEM-V3/FFI-V1/V2 and the
[budget ledger](../memory-budget-decisions.md). They do not establish ABI caller
correctness or whole-module MC/DC. All export/template sites below are mapped,
but repeated wrapper propagation paths are E, not fabricated independent pairs.

## Parsed source records

Source SHA-256: `36680a4ebabcab192b5695d1e9a82a8f5e100d843467c8c910aa78ce24ff8c5d`.
Location is line:column; hash is the first 12 source-token SHA-256 digits.

| Location | Kind / owner | Disposition | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 40:5 | `match` / `boundary` | P | F1 | `4945a70fa7f9` |
| 56:5 | `if` / `output` | P | F2 | `935f68319d4f` |
| 65:5 | `if` / `input` | P | F2 | `935f68319d4f` |
| 65:32 | `try_propagation` / `input` | E | F2 | `8a8de823d5ed` |
| 68:5 | `if` / `input` | P | F2 | `935f68319d4f` |
| 71:5 | `if` / `input` | P | F2 | `935f68319d4f` |
| 77:32 | `try_propagation` / `make_handle` | E | F1 | `8a8de823d5ed` |
| 80:35 | `try_propagation` / `make_handle` | E | F1 | `8a8de823d5ed` |
| 82:53 | `try_propagation` / `make_handle` | E | F1 | `8a8de823d5ed` |
| 89:5 | `if` / `same` | P | F1 | `935f68319d4f` |
| 101:5 | `if` / `copy_bytes` | P | F2 | `935f68319d4f` |
| 107:5 | `if` / `copy_bytes` | C | F2 | `935f68319d4f` |
| 107:8 | `short_circuit_and` / `copy_bytes` | P | F2 | `3c81d21b4ba9` |
| 110:5 | `if` / `copy_bytes` | P | F2 | `935f68319d4f` |
| 113:5 | `if` / `copy_bytes` | P | F2 | `935f68319d4f` |
| 137:5 | `if` / `path` | C | F4 | `935f68319d4f` |
| 137:8 | `short_circuit_or` / `path` | P | F4 | `4125f2b3e444` |
| 140:5 | `if` / `path` | P | F4 | `935f68319d4f` |
| 144:5 | `if` / `path` | P | F4 | `935f68319d4f` |
| 148:70 | `try_propagation` / `path` | D | F4: Windows-only UTF-8 result | `8a8de823d5ed` |

Totals: C=2, D=1, E=4, P=13; 20 records.

## Supplemental macro-site mapping

All 50 unexpanded sites are listed once. Templates recur through invocations;
rows are not added to the 20 AST records to produce a condition count. E includes
the shared `output`/`input`/`handle`/`same`/`make_handle` and delegated engine/value
error propagation where present. Every `?` preserves its callee's Error; that
structural mapping is not a claim of all callee errors at every wrapper.

| Line | Site | Explicit obligation / disposition | Evidence | Source hash |
| --- | --- | --- | --- | --- |
| 120 | `export` | E: status wrapper calls boundary; no additional branch | F1 | `0dc875315ee9` |
| 128 | `wdb_open` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `be97cb03d339` |
| 156 | `file_factory` | D/E: cfg platform selection, output/path/create/open propagation | F4; C file example | `63dfb54dcb4e` |
| 168 | `wdb_file_create` | D/E: template output/path and native factory propagation | F3/F4; C file example | `a758f8c622c0` |
| 169 | `wdb_file_open` | D/E: template output/path and native factory propagation | F3/F4; C file example | `79063f92529e` |
| 170 | `wdb_file_compact` | P/E: output-null OR; cfg dispatch; path/compact propagation | F3/F4; C file example | `3a545f6b484f` |
| 181 | `wdb_snapshot_retain` | P/E: null output, foreign-owner guard, retention propagation | F1 retained/foreign test | `3f70f19dd364` |
| 186 | `wdb_retained_release` | E: shared guards and delegated operation propagation | F1; C file retained-root lifecycle | `0a4eb923a084` |
| 187 | `wdb_retained_open` | E: shared guards and delegated operation propagation | F1; C file retained-root lifecycle | `8a296fb588af` |
| 190 | `wdb_retained_ids` | P/E: null required, null-buffer AND capacity, capacity short, nonempty copy | F2 retained ID matrix | `085d486d7859` |
| 197 | `wdb_close` | P/E: null database propagation and live-handle Busy | F1 handle-close and null controls | `c2b3ac29108e` |
| 201 | `wdb_collection_create` | P/E: null output versus valid/reset; typed source operation | F3 null/scalar outputs | `60f30cd12af4` |
| 205 | `wdb_collection_drop` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `9df2bf739fe0` |
| 208 | `wdb_value_make` | P/E: bytes kind with zero bits; scalar range with zero len; eight inner kind arms; default rejection | F2/F3 input and scalar tables | `4f4dc5904061` |
| 221 | `wdb_value_clone` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `42ea166e25b9` |
| 224 | `release` | P: null release skips destruction; nonnull releases once | F1/F2 null releases; C examples | `1742751dee8e` |
| 229 | `wdb_value_release` | P: null release skips destruction; nonnull releases once | F1/F2 null releases; C examples | `07b8f9409cc0` |
| 230 | `wdb_snapshot_release` | P: null release skips destruction; nonnull releases once | F1/F2 null releases; C examples | `014a6c009cdf` |
| 231 | `wdb_tx_release` | P: null release skips destruction; nonnull releases once | F1/F2 null releases; C examples | `10cb28eefd43` |
| 232 | `wdb_scan_release` | P: null release skips destruction; nonnull releases once | F1/F2 null releases; C examples | `fe93d4f2ae6b` |
| 233 | `wdb_value_kind` | P/E: null output versus valid/reset; typed source operation | F3 null/scalar outputs | `7e6d675f1c02` |
| 237 | `wdb_value_scalar` | P/I: null output, five kind arms, default Type; matching accessor cannot fail Type | F3 scalar table | `dc63205015bd` |
| 245 | `wdb_value_len` | P/E: null output versus valid/reset; typed source operation | F3 null/scalar outputs | `a41abf0c773f` |
| 249 | `wdb_value_bytes` | P/E: null required; bytes type and copy guards | F2 byte buffer matrix | `af74b68c230d` |
| 253 | `wdb_map_set` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `7e93a58d164d` |
| 257 | `wdb_map_get` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `e7b08119516b` |
| 261 | `wdb_map_remove` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `25cd0e106fb5` |
| 265 | `wdb_map_scan` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `00420295177d` |
| 268 | `wdb_list_push` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `3146e21767f6` |
| 272 | `indexed_edit` | E: shared guards, foreign-owner check and list edit propagation | F1/F2; C memory list set/insert | `dff9d8db29f1` |
| 278 | `wdb_list_set` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `2e326c9c6185` |
| 279 | `wdb_list_insert` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `45d163b58f50` |
| 280 | `wdb_list_remove` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `58b08e5416e4` |
| 283 | `wdb_list_get` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `520c0786e851` |
| 286 | `wdb_snapshot_acquire` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `c4458a7156be` |
| 289 | `wdb_snapshot_revision` | P/E: null output versus valid/reset; typed source operation | F3 null/scalar outputs | `04e36f0a52fa` |
| 293 | `wdb_snapshot_get` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `d29870db1ed5` |
| 298 | `wdb_snapshot_scan` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `09fd9bc5ac72` |
| 302 | `wdb_tx_begin` | P/E: validation mode 0/1/default; output/creation | F2 scan/mode table | `af3e7ca5be95` |
| 307 | `wdb_tx_get` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `d770a7000bd8` |
| 312 | `wdb_tx_put` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `17d0b723af81` |
| 316 | `wdb_tx_delete` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `9c1eaaf2d973` |
| 320 | `wdb_tx_scan` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `0220e8e054e7` |
| 324 | `wdb_tx_commit` | P/E: null revision, reset, active/closed/panic propagation | F1/F3 commit tests | `bfa5a7bc73d6` |
| 328 | `wdb_tx_abort` | E: shared guards and delegated operation propagation | F1/F2; C memory example | `cf617b7eca3b` |
| 329 | `wdb_scan_len` | P/E: null output versus valid/reset; typed source operation | F3 null/scalar outputs | `23a43f8adaad` |
| 333 | `wdb_scan_key` | P/E: null required, index Option, copy guards | F2/F3 scan index and missing-output tests | `7d99f4750f87` |
| 338 | `wdb_scan_value` | E: output, index Option and handle allocation | F2 scan index test | `04a48507c20a` |
| 355 | `wdb_package_version` | E: shared copy_bytes guards; build-time identity | F2; tools/ffi_smoke.py identity assertions | `a57185ba17d4` |
| 358 | `wdb_source_id` | E: shared copy_bytes guards; build-time identity | F2; tools/ffi_smoke.py identity assertions | `01bb95f5943d` |

The three non-macro identity exports have no explicit decisions. Their returned
constants are checked separately by tools/ffi_smoke.py. Generated derive/attribute
code and platform native error branches remain outside this local source audit.

## Executed validation for this tranche

On macOS/aarch64, all-feature debug and release runs passed 14 FFI unit tests,
14 memory unit tests (including the abort subprocess), five `allocation`, five
`live_budget`, and two `live_budget_decisions` tests: 40 per profile. Four new FFI
tests also passed `cargo +nightly miri test --offline -p weekend-db --all-features
--lib ffi::decision_tests::`. Strict all-feature library/test Clippy passed.
The rebuilt debug cdylib passed `python3 tools/ffi_smoke.py
 target/verification-diagnostics/debug/libweekend_db.dylib`.

Runs used `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/verification-diagnostics`.
The inventory completed without parse failures. No production code changed;
only the appended FFI test module is new Rust source. Existing Miri, sanitizer,
Loom and mutation results cited above are prior evidence, not newly rerun claims.
This tranche does not claim Windows execution, exhaustive native failures,
new mutations, compiler MC/DC percentages or completed project assurance.
