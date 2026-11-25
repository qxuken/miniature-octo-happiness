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
| `apps/weekend-replicate/src/native.rs` | `cca34437b0e7b100cd5d031d3d607bfbacd18213a1ffe3e1fd1618e20c8bd027` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:314:6– | 11858..11859 | try_propagation | announce | The closed-pipe process fails writing the capture line and never contacts the follower. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:315:19– | 11879..11880 | try_propagation | announce | A complete capture line followed by a failed flush stops delivery before TCP connect. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:352:73– | 13238..13239 | try_propagation | deliver | The delivery gate reports capture output failure before connecting. A writable first line and successful flush let delivery proceed. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:359:52– | 13643..13644 | try_propagation | deliver | A writer that holds only the capture line refuses the acknowledgment line after the follower has made the install durable. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:360:23– | 13668..13669 | try_propagation | deliver | A writer that accepts both lines and refuses the second flush returns the I/O error. Retrying the same export preserves the installed revision and manifest size. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:217:49– | 8608..8609 | try_propagation | main | Closing stdout prevents the first capture line and reports an I/O diagnostic. A normal capture prints both exact lines. |
