# Follower maintenance output and receipt recovery

The follower CLI rejects a closed output pipe with a structured I/O error.
A new process can open a follower initialized before the output error.
Status leaves the manifest and both image paths as they were. Cleanup
still removes the inactive file and preserves the current image. Manifest
compaction on macOS and Linux survives the output error. The next status
shows the same durable acknowledgment and the verified retained root
remains readable.

The cleanup test also places a directory at the inactive path. The
operation rejects it with Invalid, reports the maintenance phase and
preserves both the current image and the directory.

| Source | SHA-256 |
| --- | --- |
| `apps/weekend-replicate/src/native.rs` | `cca34437b0e7b100cd5d031d3d607bfbacd18213a1ffe3e1fd1618e20c8bd027` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:60:5– | 2555..2560 | match | ack_line | The initialized follower reports empty; after installation, each observer prints the same durable revision and digest. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:73:35– | 2993..2994 | try_propagation | print_report | Closed-pipe process cases return a local I/O diagnostic. Working stdout reports the expected text. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:136:15– | 5130..5131 | try_propagation | main | An absent parent is rejected. Even with closed stdout, a valid initialization can be opened by status. |
| E | weekend-replicate::weekend-replicate | apps/weekend-replicate/src/native.rs:159:92– | 6193..6194 | try_propagation | main | An inactive directory produces Invalid during maintenance. A regular inactive file is removed. |
