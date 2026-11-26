# Coordinator legacy receipt controls

The coordinator binary and standalone fuzz library share the same
receipt logic. The existing receipt tests moved to the shared module
so each compilation target can run its own exact tests and mutations.

| Decision | Passing case | Independent rejection or different outcome |
| --- | --- | --- |
| ID admission | An ASCII ID of one to 32 allowed bytes reaches the write path. | Empty, 33-byte and disallowed-byte IDs each fail independently. |
| Receipt lookup | A stored receipt returns its recorded revision. | A fresh ID follows the revision comparison and publishes no receipt when stale. |
| Receipt value type | A byte record decodes. | An integer value gets Corrupt. |
| Length or version | A 25-byte version-1 record decodes. | Short or long version-1 records and a 25-byte version-2 record each get Corrupt. |
| Receipt revision | Expected 2 and acknowledged 3 replay at local head 3. | A nonconsecutive revision, a consecutive receipt ahead of head and integer overflow each fail independently of the valid case. |
| Request binding | The precise original expectation and value replay the original acknowledgment after a newer commit. | Changing either field causes Invalid. |
| Fresh request freshness | A fresh ID at the current head commits. | A fresh ID at a stale revision returns the current revision and value. |
| Capacity | Insertion reaches exactly 1,024 permanent receipts. | The next ID gets Limit and the oldest receipt still replays. |

The malformed cases independently compare the counter, revision and
receipt contents after the call. Synthetic receipt records isolate
format and revision fields in the in-memory transaction path. Native
filesystem interruption and journal recovery use their own campaigns.

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native/protocol.rs` | `e92395560eeb7e168c993059b15771d8a93c358d942136e31cbb14ed30ef73a3` |
## Verification

Rust 1.100.0 ran the frozen source at
`de72035887f0749fc91239c14210debbd6c5caf3` in debug and release on
macOS arm64 and Docker Linux arm64/ext4. Each profile passed 43
coordinator tests and 37 standalone fuzz-library tests. Two macOS and
three Linux coordinator workload tests remained opt-in. Clippy passed
with warnings denied for both compilation targets.

Each campaign caught all 13 controls for the coordinator binary and
all 13 for the standalone fuzz library at the expected assertions.
Fresh extraction verified 991 source hashes and 46 supervised command
outcomes in every campaign. The existing 32-case Windows coordinator
oracle matched its refreshed source and exact test selection. Two
coordinator targets and one standalone fuzz target cross-linked in
each Windows x64 profile. Native execution of these controls is pending.

The earlier source at `b71584d` failed strict Clippy on an unused test
import after the tests moved. That import was removed and both full
campaigns reran at `de72035`. The current inventory closes 12 runtime
and 12 tooling records, leaving 398 runtime and 7,486 tooling gaps.
[Campaign checksums](coordinator-receipt-de72035.sha256)
