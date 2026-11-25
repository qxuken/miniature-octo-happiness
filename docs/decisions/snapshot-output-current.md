# Snapshot CLI output and durable acknowledgment

The sender writes and flushes its capture announcement before contacting
the follower. The closed-pipe process test verifies that a saved capture
remains readable and the follower is unchanged. A separate writer accepts
that line but fails its flush. The sender again exits before TCP connect.

The loopback test accepts the local line, completes an authenticated
transfer, and refuses the follower acknowledgment line. An independent
follower reopen still finds the current and retained roots. Replaying the
same export with a writer that rejects the final flush preserves the
manifest size. A fresh authenticated poll returns the original receipt.
This verifies the durable install independently of stdout.

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native.rs` | `877644213284371ed1fafff85a12eeeeb3f8afe14feb09703b18ab9223369e1a` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:304:6– | 11552..11553 | try_propagation | announce | The closed-pipe process fails writing the capture line and never contacts the follower. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:305:19– | 11573..11574 | try_propagation | announce | A complete capture line followed by a failed flush stops delivery before TCP connect. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:342:73– | 12932..12933 | try_propagation | deliver | The delivery gate reports capture output failure before connecting. A writable first line and successful flush let delivery proceed. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:349:52– | 13337..13338 | try_propagation | deliver | A writer that holds only the capture line refuses the acknowledgment line after the follower has made the install durable. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:350:23– | 13362..13363 | try_propagation | deliver | A writer that accepts both lines and refuses the second flush returns the I/O error. Retrying the same export preserves the installed revision and manifest size. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:207:49– | 8302..8303 | try_propagation | main | Closing stdout prevents the first capture line and reports an I/O diagnostic. A normal capture prints both exact lines. |
