# Verification failure context

`storage::with_storage_diagnostics` captures optional native I/O and verification
context for one synchronous operation. Existing `inspect_on`, `inspect_file`,
`Database::open_on`, backup and restore return types remain unchanged. Wrap them
when the caller needs more than the engine's compact error code:

```rust
use weekend_db::{Limits, storage};

fn check(path: &std::path::Path) -> Result<storage::ImageInfo, storage::StorageFailure> {
    storage::with_storage_diagnostics(|| storage::inspect_file(path, Limits::default()))
}
```

`StorageFailure` contains the authoritative `error`, optional `native` context and
optional `verification: VerificationDiagnostic`. The latter contains `stage` and
`offset: Option<u64>`. Error remains the category: Corrupt, UnsupportedFormat, Io,
Limit and OutOfMemory are not collapsed into a generic verification error.
Custom byte stores can report Io with verification context and no native OS error.
Lock/path/preparation failures outside verification can have no verification field.

The old `with_native_diagnostics` name remains an alias of the same capture
function. Existing native diagnostic behavior is preserved: the first terminal
native failure observed in the capture wins, including cleanup. Native and
verification context are independent; neither replaces the other.

## Stages and offsets

| Stage | Verified boundary | Offset meaning |
|---|---|---|
| ImageLength | Backend length and allowed image extent | None |
| Header | Header read, version, checksum and identity | 0 |
| RootSlot | One root metadata read and validation | That slot region's start |
| Witness | One witness metadata read and validation | That witness region's start |
| RootSelection | Available root selection, witness agreement, adjacency | None: relation spans metadata records |
| CommitDescriptor | Selected commit object and its retained catalog | Selected descriptor start |
| Directory | Current/retained data directory and its entries | Directory object or attempted reference start |
| Value | Value object, payload, parent/depth/cache constraints | Value object or attempted reference start |

Offsets identify a rejected structure or attempted reference, **not necessarily
the exact bad byte**. An invalid reference may itself be outside the file. A
resource failure at a value stage does not imply the value bytes are corrupt.
Context must not be used as an automatic repair instruction.

Result-returning wrappers record only actual errors, after the operation returns.
Successful reads do not update a last-read marker. Direct nested recovery
propagation keeps the deepest failure: a bad child value reports its own offset,
not its parent directory or the later successfully read commit descriptor.
A new recovery attempt clears prior verification context. Blank or checksum-torn
metadata that the existing protocol treats as absent returns Ok(None), so it does
not create a diagnostic. All root selection, witness, corruption rejection and
no-fallback rules are unchanged.

## Scope and handled failures

Capture is thread-local and synchronous. Nested captures save/restore their
caller's context, including during unwinding. Capture on another thread neither
reads nor changes the current thread's state. Async task migration is outside
this API's contract; put the complete synchronous storage call inside capture.
The diagnostic records are Copy and contain no paths, keys or record bytes.
No heap allocation is added for successful object verification.

Context is an **observed failure within the captured operation**, not a general
causal trace. Error is Copy: a caller can swallow one failure and return an
unrelated error, and TLS cannot infer that causal distinction. Use one operation
per capture, and nested captures around separately handled operations. A later
successful recovery clears its verification marker; a successful capture returns
no StorageFailure at all. Existing native first-error semantics remain independent.

Built-in salvage deliberately handles corrupt candidate roots. It explicitly
clears verification context when discarding such a probe, so a later destination
failure does not carry a discarded source offset. If no candidate survives,
salvage's own Corrupt may have no specific verification offset. Ordinary recovery
still rejects a corrupt selected newest graph rather than silently using an older
root. Inspection continues to avoid writing/syncing its input on either outcome.

## API compatibility and CLI

The engine Error enum, persistent format, C ABI and ordinary inspection/recovery
signatures are unchanged. `StorageFailure.verification` is an additive Rust field
in this pre-1.0 API: external exhaustive struct literals must initialize it and
exhaustive patterns must name it or use `..`. Prefer inspecting the returned
fields rather than constructing failures. The two diagnostic type exports and
`with_storage_diagnostics` are new.

`weekend-admin` now wraps its operation in this capture. Display prints the
original error, existing native context when present, and verification stage plus
structure-start offset when present. It does not print source path or record
contents as part of the diagnostic.

## Regression evidence

New codec cases reuse the existing physical image fixture and checksum helpers:

- `verification_diagnostics_identify_metadata_and_selected_graph_failures` checks
  image length, unsupported/corrupt headers, checksum-valid malformed slot/witness,
  missing-root selection, and selected commit/directory/child-value corruption.
  Every corrupt image remains byte-identical after failed inspection. An intact
  older root cannot hide selected-graph corruption.
- `verification_diagnostics_preserve_io_resource_errors_and_success_has_no_marker`
  checks custom Io without invented native context, Limit and injected OutOfMemory
  at the actual descriptor allocation stage, and tolerated torn metadata. A
  successful verification after a handled failure leaves no stale marker.
- `discarded_salvage_probe_does_not_label_destination_failure` verifies the built-in
  discard boundary using a valid candidate, a later corrupt candidate and failed
  destination I/O; source bytes remain unchanged and the destination failure has
  no stale source verification offset.
- Diagnostic scope tests check deepest-error precedence, restoration of an outer
  populated context after nested captures/panics, reset between operations,
  concurrent thread isolation and independent native context.

On 2026-11-09, all-feature runs passed 33 storage unit tests and 28 integration
tests (14 durability, six retention, five inspection, three salvage) in both debug
and release. Strict library/admin Clippy passed. A release admin smoke check on a
truncated image printed `Corrupt (verification ImageLength)`, left the input
unchanged, and omitted the input path from its diagnostic. Verification used
`CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/verification-diagnostics` to isolate
artifacts. These are diagnostic/recovery regression tests, not new compiler MC/DC
claims or a cross-platform native qualification result.
