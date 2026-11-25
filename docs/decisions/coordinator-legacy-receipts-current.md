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
