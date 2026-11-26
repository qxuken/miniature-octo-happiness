# Coordinator legacy initialization and native admission

Both CLI initializers use the same create-file boundary. The dispatch
and native test fixtures preserve the same error and file bytes when
the destination already exists. The sequenced initializer keeps its
existing separate format and three-publication qualification.

| Legacy boundary | Error and acknowledged revision | Fresh recovery |
| --- | --- | --- |
| Collection append through first sync | Io, revision 0 | Revision 0, collection absent |
| Collection publication after append sync | OutcomeUnknown, revision 0 | Revision 0 before root sync. At root sync, dropping or persisting the pending root selects revision 0 or 1. Later faults select revision 1. |
| Counter append through first sync | Io, revision 1 | Revision 1, collection empty |
| Counter publication after append sync | OutcomeUnknown, revision 1 | Revision 1 before root sync. At root sync, dropping or persisting the pending root selects revision 1 or 2. Later faults select revision 2 with exactly one unsigned counter at zero. |
| Unsigned allocation or draft limit | OutOfMemory or Limit, revision 1 | The durable bytes equal a separate collection-only commit. A fresh open sees the empty collection at revision 1. |

The simulator sweeps every observed I/O operation in both publications
and crashes with all pending writes discarded and with all persisted.
The failed call reports the typed error and retains its last acknowledged
revision. The recovered file is inspected for exact revision, collection
existence and record count. A native process refuses to serve empty or
unseeded images before bind. The same images reject repeat initialization
and keep their exact bytes and reopened revisions.

## Qualification

Frozen source `8040d89bf3754b4d670245975c0f1d5f51e62335` used Rust 1.100.0.
Each debug and release profile on arm64 macOS and Docker Linux/ext4
passed 55 coordinator unit tests, 21 coordinator integration tests,
and 55 standalone fuzz-library tests. Two macOS and three Linux
workload tests remained opt-in. Strict Clippy passed in both targets.
The coordinator unit and integration binaries and the fuzz-library
test binary linked for Windows MSVC x64 in both profiles. Native
Windows execution remains pending for this source.

Each platform and profile detected all 48 named CLI mutations. The four
new cases check propagation of collection publication, value allocation,
draft insertion and seed commit failures. The two dispatch controls
check that the shared create-file path preserves AlreadyExists through
each command. Archive verification checked each exact baseline and
mutation, 157 supervised commands and 1,028 restored source hashes
per campaign.

Reconciliation accounts for five additional runtime records and combines
the two create-file boundaries into one. It inventories 10,775 decisions
across 223 files without failures. The remaining queue is 107 runtime
and 7,441 tooling gaps, including eight runtime rows in the native CLI
module for credential and client I/O.
[Frozen source and evidence checksums](coordinator-init-8040d89.sha256)

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native.rs` | `0c0c92ddbbdf876740a19b975dba0ef8ef84d9540bbf36854299b375767e4822` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:42:5– | 1512..1517 | match | run | Help, both initializers, both listener profiles, both client forms, journal and usage have separate process results. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:153:5– | 5607..5610 | for_iteration | client | EOF exits, blank input continues, quit stops and a regular line reaches the request path. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:171:8– | 6186..6189 | pattern_test | request | Presence controls the enclosing conditional. The peer observes both the plain and configured forms. |
