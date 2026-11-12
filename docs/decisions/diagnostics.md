# Storage diagnostics source decision audit

All 20 production AST records in `src/storage/diagnostics.rs` are classified:
six ifs, six pattern tests, five `?`, two AND records and one match. P = direct
controls, C = overlapping syntax, E = propagation evidence, B = dependent
pattern/condition domain. This is not a compiler MC/DC percentage.

## D1: optional display fields and formatter errors

New `display_decisions::optional_context_and_failing_format_sinks_propagate_errors`
checks native context absent/present against verification absent, present without
an offset and present with an offset. Display includes exactly the applicable
context sections. A std::fmt::Write sink first counts every write in a successful
format, then fails each write ordinal separately; the error propagates and no later
sink call occurs. This supplies failure/success evidence for all five Display `?`
records without assuming that std formatting always succeeds. No path or record
bytes are stored in the diagnostic types.

## D2: active context, first observation and categories

Reuse `tests::scoped_diagnostics_keep_first_error_and_clear_between_operations`,
`scoped_diagnostics_restore_on_unwind_and_isolate_threads`,
`verification_inner_failure_wins_and_scopes_restore_on_unwind`, and
`verification_context_is_thread_local_and_independent_from_native_context`.
They cover fresh/repeated native and verification observations, Permission,
DiskFull, Quota, nested scope restoration on success/error/unwind and thread
isolation. New `display_decisions::inactive_capture_is_a_noop_and_other_category_is_retained`
checks native/verification recording and clearing outside capture, Other category,
a fresh later capture without stale verification, and successful capture.

The `if let Some(context) && context.field.is_none()` records are B: absent scope,
active empty field and active populated field are controlled. A context field
cannot be evaluated while its context is absent; no impossible independent pair
holds that nonexistent field fixed. Treat these as dependent sequential guards,
not a complete two-condition unique-cause MC/DC table. `clear_verification` is
controlled with no active scope and with active scopes by the reused recovery and
salvage tests.

## D3: operation integration and delegated mechanics

Reuse `retried_interruptions_never_replace_the_terminal_native_error`,
`native_factory_reports_origin_without_paths_and_resets_on_validation_error`,
and `commit_diagnostics_preserve_uncertain_and_recovery_required_results`.
Codec diagnostics tests preserve Io/Limit/OutOfMemory categories, record deepest
actual verification failure, and clear deliberately discarded salvage probes.
The unchanged Error is authoritative; diagnostics cannot infer causal identity
when arbitrary user closures handle an error then return a different copied Error.
Capture's documented one-operation/nested-scope contract remains essential.

The thread_local declaration and Display write macros are inventory blind spots;
the write effects are exercised by D1, and std TLS/Cell implementation is delegated.
Derived traits, std error formatting and unwinder internals are not enumerated as
source decisions. Panic-abort execution cannot restore scopes through unwinding;
no broader panic contract is inferred. There are no new production changes.

## Source records

### src/storage/diagnostics.rs

SHA-256: `1387cc634a599b19426b4fd2c83ac6b014f3fd25018cb72f37b4b74b637a89eb`.

| Location | Kind / owner | Status | Evidence | Token hash |
| --- | --- | --- | --- | --- |
| 70:38 | `try_propagation` / `fmt` | E | D1 | `8a8de823d5ed` |
| 71:9 | `if` / `fmt` | C | D1 | `935f68319d4f` |
| 71:12 | `pattern_test` / `fmt` | P | D1 | `5654d4106d70` |
| 76:14 | `try_propagation` / `fmt` | E | D1 | `8a8de823d5ed` |
| 78:9 | `if` / `fmt` | C | D1 | `935f68319d4f` |
| 78:12 | `pattern_test` / `fmt` | P | D1 | `5654d4106d70` |
| 79:67 | `try_propagation` / `fmt` | E | D1 | `8a8de823d5ed` |
| 80:13 | `if` / `fmt` | C | D1 | `935f68319d4f` |
| 80:16 | `pattern_test` / `fmt` | P | D1 | `5654d4106d70` |
| 81:59 | `try_propagation` / `fmt` | E | D1 | `8a8de823d5ed` |
| 83:29 | `try_propagation` / `fmt` | E | D1 | `8a8de823d5ed` |
| 125:9 | `if` / `record` | C | D2 | `935f68319d4f` |
| 125:12 | `pattern_test` / `record` | C | D2 | `5654d4106d70` |
| 125:12 | `short_circuit_and` / `record` | B | D2 | `1bb5ed3a1d4a` |
| 128:28 | `match` / `record` | P | D2 | `4945a70fa7f9` |
| 148:9 | `if` / `clear_verification` | C | D2 | `935f68319d4f` |
| 148:12 | `pattern_test` / `clear_verification` | P | D2 | `5654d4106d70` |
| 162:13 | `if` / `verify` | C | D2 | `935f68319d4f` |
| 162:16 | `pattern_test` / `verify` | C | D2 | `5654d4106d70` |
| 162:16 | `short_circuit_and` / `verify` | B | D2 | `d321ccb99078` |

Totals: B=2, C=8, E=5, P=5; **20 records**.

## Executed validation

All nine diagnostics tests passed debug and release on macOS/aarch64 and
Linux/aarch64 Docker, including the two new display/inactive-context cases.
The broad native tests also exercise diagnostic capture around real operations.
The source inventory completed with zero parse failures. No native syscall
failure is inferred merely from a synthetic ErrorKind record; native behavior
and hardware durability remain separate qualifications.

Both new diagnostics tests also passed Miri on the installed nightly. Formatting
passed for all five audited files. The final workspace Clippy run encountered
an unrelated concurrent logical-format test lint; the combined checkpoint must
rerun strict Clippy after that test is finalized. No diagnostic-code lint was
reported by that run.
