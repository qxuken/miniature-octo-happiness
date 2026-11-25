# Study local command admission

The option parser starts with pushing enabled and sync-only disabled.
For each optional flag, the matching token with a fresh option enables
the branch. An unrelated token keeps the other option's state intact.
Repeating the same token reaches the duplicate rejection with its
freshness condition false. Both orders of the two distinct flags produce
the same options. Unknown flags return usage.

| Dispatch condition | Accepted control | Rejection control |
| --- | --- | --- |
| Long or short help | Each spelling prints the same usage prefix and exits successfully | An unknown one-token command exits with an input error |
| Receipt absent or supplied | No arguments selects no receipt. The matching flag reads the selected file byte for byte, including at 128 bytes | The other flag, missing path and extra argument return usage. An absent file returns native `NotFound`. A 129-byte file exceeds the bound |
| Local startup command | `keygen` creates both local files. `init` enrolls actor A and reopens at revision 2 | An unrelated verb at each arity creates no files |
| Roster command | Checkpoint, stage and verify reach the existing process regression with a valid policy | An unrelated verb at the same arity returns usage |

The initialization checks distinguish an invalid actor, a missing
database parent and a duplicate database path. Repeating initialization
preserves the original bytes and reopened default record. An invalid
`run` option returns usage before credential loading. With valid options
and the same paths, the next operation reports the missing credential
file without opening a listener.

The roster preflight fixture supplies two valid source databases. It
changes the checkpoint policy, old stage policy, new stage policy, or
verify policy independently and observes `UnsupportedFormat`. A missing
receipt path returns the same I/O error through stage and verify. Both
staged paths remain absent and both source images retain their bytes.
The earlier simulated roster fixture supplies the definite I/O error from
collection creation in the shared enrollment helper.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native.rs` | `ac6f656dab385a3e7884710d65d6bbd00988d50e8a1c3947d2e5648adccd6462` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:60:9– | 1945..1948 | for_iteration | parse | Empty, singleton and both valid flag orders exercise traversal. Duplicate and unknown flags stop parsing. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:74:5– | 2380..2385 | match | expected_receipt | Empty operands, the selected receipt pair, wrong flag and wrong arity exercise the three arms. |
| E | study-peer::study-peer | apps/study-peer/src/native.rs:86:5– | 2706..2711 | match | main | Local dispatch and usage tests exercise the local commands. The existing native service tests enter both run modes. |
| D | study-peer::study-peer | apps/study-peer/src/native.rs:178:75– | 6454..6455 | try_propagation | initialize | Native initialization reaches the shared enrollment helper and reopens the record. The roster simulator tests the helper's collection-creation error. Native fault injection after successful file creation remains open. |

The three short-circuit records share the named source pairs for the
enclosing option and help decisions.
