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

| Source | SHA-256 |
| --- | --- |
| `src/storage/diagnostics.rs` | `1387cc634a599b19426b4fd2c83ac6b014f3fd25018cb72f37b4b74b637a89eb` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/diagnostics.rs:70:38– | 1727..1728 | try_propagation | fmt | D1 |
| C | weekend-db::weekend_db | src/storage/diagnostics.rs:71:9– | 1738..1740 | if | fmt | D1 |
| P | weekend-db::weekend_db | src/storage/diagnostics.rs:71:12– | 1741..1744 | pattern_test | fmt | D1 |
| E | weekend-db::weekend_db | src/storage/diagnostics.rs:76:14– | 1941..1942 | try_propagation | fmt | D1 |
| C | weekend-db::weekend_db | src/storage/diagnostics.rs:78:9– | 1962..1964 | if | fmt | D1 |
| P | weekend-db::weekend_db | src/storage/diagnostics.rs:78:12– | 1965..1968 | pattern_test | fmt | D1 |
| E | weekend-db::weekend_db | src/storage/diagnostics.rs:79:67– | 2076..2077 | try_propagation | fmt | D1 |
| C | weekend-db::weekend_db | src/storage/diagnostics.rs:80:13– | 2091..2093 | if | fmt | D1 |
| P | weekend-db::weekend_db | src/storage/diagnostics.rs:80:16– | 2094..2097 | pattern_test | fmt | D1 |
| E | weekend-db::weekend_db | src/storage/diagnostics.rs:81:59– | 2193..2194 | try_propagation | fmt | D1 |
| E | weekend-db::weekend_db | src/storage/diagnostics.rs:83:29– | 2238..2239 | try_propagation | fmt | D1 |
| C | weekend-db::weekend_db | src/storage/diagnostics.rs:125:9– | 3938..3940 | if | record | D2 |
| C | weekend-db::weekend_db | src/storage/diagnostics.rs:125:12– | 3941..3944 | pattern_test | record | D2 |
| B | weekend-db::weekend_db | src/storage/diagnostics.rs:125:12– | 3941..4016 | short_circuit_and | record | D2 |
| P | weekend-db::weekend_db | src/storage/diagnostics.rs:128:28– | 4054..4059 | match | record | D2 |
| C | weekend-db::weekend_db | src/storage/diagnostics.rs:148:9– | 4832..4834 | if | clear_verification | D2 |
| P | weekend-db::weekend_db | src/storage/diagnostics.rs:148:12– | 4835..4838 | pattern_test | clear_verification | D2 |
| C | weekend-db::weekend_db | src/storage/diagnostics.rs:162:13– | 5184..5186 | if | verify | D2 |
| C | weekend-db::weekend_db | src/storage/diagnostics.rs:162:16– | 5187..5190 | pattern_test | verify | D2 |
| B | weekend-db::weekend_db | src/storage/diagnostics.rs:162:16– | 5187..5272 | short_circuit_and | verify | D2 |

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
