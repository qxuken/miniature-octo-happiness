# Service runtime source attribution at 3db5bcd

This bounded review binds the shared CLI diagnostic observer, its two callers,
Study incoming status, and snapshot admission to clean source commit
`3db5bcdd20de297b8b5424c1fbc2d1e316e2145d`. It compares the same source paths
with `910e4904e8f13a7bc8f0ef1401092ed2fb3b11c8`, the last
[shared-cache inventory](decision-inventory.md#shared-cache-checkpoint).
It reuses existing tests and independent reviews; it adds no runtime, test or
inventory-tool code and executes no new product qualification tests.

The purpose is current source attribution, not another branch-coverage score.
The historical [snapshot](decisions/snapshot-cli.md) and
[distributed-app](decisions/distributed-apps.md) ledgers keep their original
source/decision scope. This supplement does not extend them to every later
coordinator, roster, capture, tool or dependency implementation.

## Reproducible source scope

The unchanged Rust inventory tool ran against pristine Git archives of both
commits, using Rust/Cargo 1.99.1, offline/locked. Its source SHA-256 is
`3ec74fdc84b7b0ce6c2419e95db50771b02be6c1a099ae929c32d05d8a1926ee`.
The baseline exactly reproduces 2,604 records, 51 files and 319 blind spots.
The current full workspace has 8,041 records, 131 files and 3,379 blind spots,
with zero parse/path failures. It now includes the registered xtask and other
code outside this review; those totals are not service-only additions.
Current source-set SHA-256 is
`6a345ad805be48b13cce572c8a481beffcf722aa6e4fb6fb3bce25ddb2b2e3bb`.

This supplement selects only the following eight files and the
`study-peer::study-peer` and `weekend-replicate::weekend-replicate` targets.
The shared listener helper's separate coordinator instantiation is excluded.

| Production input | Full-file SHA-256 |
|---|---|
| `apps/support/service_diagnostics.rs` | `02a7c1dcba60626104be3f58da640459b5f185546b64a532c189d3f62ef07208` |
| `apps/support/listener_ready.rs` | `711b47bc82a36f80d390be538fe15b511cb35764292f6860efd79b1581b467a9` |
| `apps/study-peer/src/main.rs` | `784a39bdfdd83760cc707a2e6b3b87e083f37e648bc7e086988b34fec0ca0d6b` |
| `apps/study-peer/src/native.rs` | `7592846d6dff0b18e40808bf491412a832790adb521ca7675136c0e0652305d9` |
| `apps/study-peer/src/native/admission.rs` | `0d01bb5355a7b420f9cad80a7fa5fea02d09adaf7a92cce91cf6029750469dab` |
| `apps/weekend-replicate/src/main.rs` | `6abd661cdad7ac8bb8c0f87bca0ee4688bd5aa689cc70add5873820bfc650357` |
| `apps/weekend-replicate/src/native.rs` | `bf4b7a29d62011a342a9badb6dc8d74fcaab5ea51c901ff25cc19605d874679c` |
| `apps/weekend-replicate/src/native/admission.rs` | `7144c0e040b9955bf64fe857361653378d3e01fb784639da5c8eae125ad312c3` |

The selected scope has **320 source records**: Study 143 and snapshot 177.
Shared diagnostics contributes 50 records to each binary, counted separately.
Both entrypoints now delegate without an explicit AST decision; their zero
counts do not exclude their macro, cfg or runtime behavior from review.

The same selected paths had 130 records at the baseline. A multiset comparison
of target, file, owner, kind, token/condition text, alternatives and inherited
cfg finds 76 matching fingerprints, 244 added/changed and 54 removed/changed.
This ignores line movement but is a lexical comparison, not proof of semantic
equivalence or a count of newly introduced independent conditions.

Default inventory uses `test=false`; all other feature/platform cfg predicates
remain unknown. It therefore includes both Unix and Windows source branches,
without claiming either executed. There are **12 selected test exclusions**
and **85 selected macro/derive blind spots**. Unit-test bodies are excluded,
integration-test targets are not traversed, and full-file hashes still include
the excluded source bytes. Test inputs are pinned separately in the report.

## Mapping to existing behavior evidence

Every selected record has its original machine ID/range and a group mapping
in `service-runtime-delta.json`. **E** means scoped execution and source
inspection, not every record outcome; **D** retains callee/OS/core propagation
limits. The mapping has 173 E and 147 D records. No P/MC/DC or exhaustive-arm
classification is added by this supplement.

| Group | Source and owners | Records (E / D) | Existing observable controls |
|---|---|---:|---|
| S1 closed labels and renderer | Shared diagnostics: `parse`, `text`, `classify`, `tls_code`, `fields`, `operation_name`, `stage_name`, `optional_number`, `write_str`, `fmt`, `json`, `incoming` | 78 (56 / 22), across both binaries | Unknown command/error controls, poison formatter/source-chain trap, maximal numeric fields, real native/verification observations and both CLIs' canaries |
| S2 observation scope and entry | Shared diagnostics: `settings_from_args`, `at`, `storage`, `capture`, `propagate`, `entry`; inspection also includes `Restore`, `settings`, `set_last`, `take` | 22 (22 / 0), across both binaries | Leading-only opt-in, original downcasts, success/retry clearing, nested/thread/unwind restoration, enabled/disabled worker-carrier controls |
| S3 snapshot wrappers and retry | Snapshot `native.rs`: `main`, `identity`, `enrollment`, `option`, `transport`, `prepare`, `prepare_saved`, `announce`, `deliver`, `ship`, `ack_line`, `serve` | 96 (27 / 69) | Saved-descriptor open/verification errors, credential/TLS errors, successful authenticated shipment/reopen, lost terminal reply followed by refused reconnect, exact receipt recovery/retry |
| S4 Study observer and callers | Study `native.rs`: `main`, `parse`, `expected_receipt`, `enroll`, `initialize`, `run`; inspection also includes `observe_local_command` | 76 (30 / 46) | CLI-local storage observation, unchanged three-attempt Conflict budget, private-input rejection, deferred incoming report and two-writer lost-ack recovery |
| S5 Study incoming status/admission | Study admission: `report`, `record`, `receive`; inspection includes `structured`/`refuse` | 17 (15 / 2) | Stalled-session cap, serialized merge, busy accounting, cancellation/shutdown and default/JSON incoming canaries |
| S6 snapshot worker handoff/admission | Snapshot admission: `reap`, `serve`, `admit`, `finish`; snapshot profile of `listener_ready::wait_for_connection` | 31 (23 / 8) | Both admission caps, worker-failure cancellation, real verification-context carrier, per-channel clearing, panic/admission/worker precedence and real Linux ENOSPC fixture |

The five shared tests are
`opt_in_is_leading_only_and_never_serializes_argument_text`,
`unknown_errors_never_format_or_traverse_arbitrary_sources`,
`storage_capture_preserves_original_error_and_clears_between_attempts`,
`nested_captures_and_threads_restore_their_own_context`, and
`renderer_is_valid_for_maximal_numeric_fields_and_unknown_commands`.
They are compiled into each binary. The two integration canaries in each CLI
are `operational_errors_do_not_echo_private_inputs` and
`structured_errors_keep_categories_without_echoing_private_inputs`.

Specific caller controls include Study's
`cli_command_observation_preserves_storage_context_errors_and_retry_budget`
and snapshot's
`structured_connect_refusal_after_lost_reply_does_not_deny_a_durable_install`.
The worker controls are
`worker_join_preserves_real_verification_context_and_original_error`,
`nonfatal_and_successful_channels_do_not_leak_context_to_a_later_failure`, and
`admission_error_and_worker_panic_keep_their_existing_precedence`.
The existing overload/cancellation tests and
`authenticated_follower_reopens_at_the_old_receipt_when_tmpfs_is_full`
retain their separate native, ignored-test and dedicated-volume requirements.

## Reviewed invariants and evidence limits

- JSON classification uses known concrete downcasts and closed enum/numeric
  fields. It does not call arbitrary error formatting or traverse arbitrary
  source chains. Default text deliberately retains its existing behavior;
  canaries cover named inputs, not arbitrary application logging.
- Observer wrappers preserve the original boxed error and retry classification.
  A later connect refusal can follow an earlier durable installation whose
  terminal reply was lost. Shipment JSON keeps an unconfirmed outcome and
  reconciliation guidance; its phase is not a denial of remote commit.
- Scope entry clears prior context; nested capture restores its caller even
  during unwinding. Worker settings are copied explicitly. The snapshot
  authenticated exchange and immediate pure `progress()` gate share one local
  storage scope. A nonfatal error with healthy progress is discarded before
  the next channel. Local observed context does not enter protocol replies.
- Snapshot `finish` preserves worker-panic, admission-error, worker-error
  precedence. Only the selected worker error receives its safe context.
  Study keeps its original error and safe diagnostic together; its command
  releases the status lock before writing output. No new worker output,
  snapshot control thread, remote receipt or acknowledgment is synthesized.
- The two polling wrappers borrow a live listener/socket and a writable native
  descriptor for a synchronous call. Windows error lookup is on the same
  thread as failed `WSAPoll`. Kernel return behavior, cancellation timing and
  every native error cause are not proved by inspecting those wrappers.

The 85 blind spots remain explicit. `names!` emits fixed enum-to-literal
matches; `thread_local!`, formatting macros, standard derives and their
compiler/library implementations are not expanded into this denominator.
The closed-schema buffer reserves one byte for the newline and rejects writes
past 1,023 bytes; maximal-numeric controls and source review are finite evidence,
not coverage of every generated formatting path or arbitrary future fields.
Standard-library allocation, locks, thread creation/join, socket/TLS behavior,
storage operations and individual propagated failures remain delegated.

No counts are pooled across generic closure instantiations to infer independent
condition pairs. The two copies of shared diagnostics are separately compiled
binary profiles; their identical source does not imply identical execution.
The full workspace's other source files, custom application errors/records,
dependency internals and non-native unsupported-target entrypoints retain their
own scope. No new concrete functional gap was established by this attribution
review; remaining E labels alone are not a reason to invent another test batch.

## Reused executions and retained proof

The [worker-context proof](service-diagnostics.md#snapshot-follower-worker-terminal-context)
retains Mac debug/release logs with **71 passes, zero failures and zero skips
per profile**, and Linux minimum-Rust debug/release carrier and actual tmpfs
exhaustion checks. These are test counts, not source-condition counts.
This audit independently compared all eight production hashes and ten selected
test/evidence input hashes with that proof's frozen Linux source manifest;
all match. It also recounted both retained Mac logs. Independent root and
separate read-only worker reviews found no actionable issue.

That proof remains a `703948c`-based overlay qualification, not a fresh clean
`3db5bcd` execution. The initial diagnostic API's hosted native Mac/Linux/Windows
results also predate the worker handoff. This supplement does not declare the
new `3db5bcd` CI run passed or qualify native Windows worker execution, deployed
log sinks/ACLs, physical devices or a final delivered candidate.

Audit proof: `/private/tmp/weekend-service-runtime-delta-xbrrxa44/`.
The exact source archives, both full inventories, the filtered mapping, source
and compiler identities, reproduction script, review notes and reused source/
log records are retained. The current source archive SHA-256 is
`590eb6c5628e1201071d6f539209d65d3efab054e4904e6ef76556b20479d963`.
The mapping SHA-256 is
`3eefa4153ac04a372948b490acf0708397825f8287062f21f3d73b362cc308b0`;
`evidence.json` SHA-256 is
`b6c3851baf4a8c0762fb52d1001e1e6c1591e0701b62e005731e4ccc228277cd`.
The compact archive `/private/tmp/weekend-service-runtime-delta-xbrrxa44.tar.gz`
was restored and all 20 indexed payloads verified. Its SHA-256 is
`e9267b8a41dded9a9b9485f5b169aa56f87c4fdcf4c2d32c9d74253572eaffac`.
Build outputs and reconstructed source directories are excluded.

Root separately restored all 20 indexed payloads, matched both Git archives,
and checked all 320 original machine records and 674 recorded byte ranges.
Eight production hashes, ten test/evidence hashes and both reused 71-test Mac
logs match the pinned source and retained worker proof. The protected copy is
`target/archived-proof/service-runtime-delta-3db5bcd.tar.gz`, with the same
archive digest above. This adds independent attribution verification, not a
new product execution or condition-coverage result.
