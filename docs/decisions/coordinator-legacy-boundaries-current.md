# Legacy coordinator command and receipt boundaries

The coordinator binary and standalone fuzz library compile the same legacy
protocol. The tests run there independently. This batch changes test code
only and preserves the existing request and receipt formats.

| Decision or boundary | Passing case | Independent rejection or different outcome |
| --- | --- | --- |
| Credential envelope | A prefixed request with a separator and matching token returns the command. | Missing prefix, missing separator, wrong token length and wrong same-length token each return no command. |
| Read deadline callback | A live callback permits bytes to be read. | A timed-out callback leaves all input unread and returns the original timeout. |
| Read error, EOF and newline | A newline finishes the first frame and leaves the next frame unread. A 255-byte payload fits. | A reset after a partial frame preserves its error and diagnostic. EOF after three bytes stops on poll four. A 256-byte payload and invalid UTF-8 receive their respective diagnostics. |
| Command shape | Get, set and set-id each reach their own result. | Empty, unknown and wrong-arity commands return Invalid and leave revision 2 and its only row unchanged. |
| Identifier length | A nonempty allowed identifier through 32 bytes is accepted. | Empty and 33-byte ASCII identifiers independently return Invalid. |
| Identifier byte | An ASCII alphanumeric, hyphen and underscore each independently suffice. | A slash satisfies none of the three conditions and returns Invalid before creating a receipt. |
| Receipt lookup pattern | A present receipt replays or rejects according to its stored fields. | A missing receipt follows the fresh-write path. |
| Selected allocator | A fresh identified write with the same initial image commits revision 4. | Refusal at the plain unsigned value, at the receipt-capacity count, or at the identified unsigned value preserves OutOfMemory, the live allocated bytes, the prior receipt and exact durable bytes. After retry and reopen the new receipt replays at revision 4. |
| Revision advance | A fresh identified write advances a regular head by one. | A valid logical import at the maximum revision returns Exhausted, retains the counter and the prior acknowledgment and preserves exact bytes. |

The standalone target is eligible for the two exact scoped rows below.
The rows carry the standalone target name and current file pin, and the
standalone target retains its tooling scope.

## Qualification

Frozen source `e25f2f9565464dd815a448dc2a596b02cbb0a633` used Rust 1.100.0.
Each debug and release profile on arm64 macOS and Docker Linux/ext4
passed 53 coordinator unit tests, 20 coordinator integration tests and
55 standalone fuzz-library tests. Two macOS and three Linux workload
tests remained opt-in. Strict Clippy passed for both Cargo targets.
The coordinator unit and integration binaries and the fuzz-library
test binary linked for Windows MSVC x64 in both profiles. Native
Windows execution of the five scoped coordinator tests subsequently passed in
both profiles on PR #82 head `908f3c5`. The standalone fuzz-library tests
still require native Windows execution.
[Hosted Windows evidence](coordinator-client-io-current.md#hosted-windows-evidence-for-earlier-batches)

Each profile detected 20 mutations per target, including 30 new
and ten existing controls across both targets. Fresh extraction
checked the exact test failures, successful baselines, 70 supervised
commands and 1,026 restored source hashes per campaign. Reconciliation
found no remaining source records without a disposition in the legacy
protocol. This closed 15 runtime and 17 tooling records. The repository
still has 112 runtime gaps, 7,441 tooling gaps and 5,070 blind spots.

The first macOS attempt stopped when the shell rejected an empty
optional argument array. The next run supplied the common selectors
in a nonempty array. On the first frozen source, strict Clippy found
that the legacy and sequenced tests each loaded the same allocator and
checksum modules. The final source loads both modules once from the
parent. The production code in both files stayed byte-identical.
[Frozen source and evidence checksums](coordinator-legacy-boundaries-e25f2f9.sha256)

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/protocol.rs` | `d987550248dbba747bd98d535284d3ca334a19238f9b511e7e07e8bcd39f325d` |
| `apps/write-coordinator/src/native/protocol.rs` | `e8d3019808a1ba5b676ecbf6ed4724a14e16cf02a534ab43c4f0d3d76cdae952` |

The second pin exposes the allocator fixture to the enclosing test module.
The protocol decisions and source positions are unchanged.

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:69:5– | 2087..2092 | match | handle | Get, set and set-id each return their command-specific result. Empty, unknown and wrong-arity requests return Invalid and preserve the current state. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native/protocol.rs:108:8– | 3737..3740 | pattern_test | submit_identified | The matched pattern selects the same branch as the receipt-present guard. A byte receipt returns the original acknowledgment. A missing receipt reaches a new write; replay or rejection leaves the counter unchanged. |
| E | weekend-db-fuzz::weekend_db_fuzz | apps/write-coordinator/src/native/protocol.rs:69:5– | 2087..2092 | match | handle | Get, set and set-id each return their command-specific result. Empty, unknown and wrong-arity requests return Invalid and preserve the current state. |
| C | weekend-db-fuzz::weekend_db_fuzz | apps/write-coordinator/src/native/protocol.rs:108:8– | 3737..3740 | pattern_test | submit_identified | The matched pattern selects the same branch as the receipt-present guard. A byte receipt returns the original acknowledgment. A missing receipt reaches a new write; replay or rejection leaves the counter unchanged. |
