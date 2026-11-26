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

## Verification

Frozen source `60b973afbc5bd493437ee0ba78b3ae1296abd4f4` passed with Rust
1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4. Each debug
and release configuration passed all 53 Study tests and 14 core CRDT tests.
Strict Clippy passed on both hosts.

All 23 mutations were detected in each of the four configurations. Fresh
extraction checked the named assertion failures, 320 process reports,
exact runner hashes and all 950 copied files per campaign. The
[archive checksums](study-cli-admission-60b973a.sha256) bind these inputs
and outputs to the frozen source.

On macOS, four unit probes and a separately instrumented CLI built with
nightly `739adecd4` exercised both outcomes at the following conditions.
Both extracted executable and profile pairs reproduced their LLVM exports
byte for byte.

| Condition in `native.rs` | True | False |
| --- | ---: | ---: |
| 61, no-push token | 8 | 10 |
| 61, pushing still enabled | 6 | 2 |
| 63, sync-only token | 8 | 4 |
| 63, sync-only still disabled | 6 | 2 |
| 76, expected receipt flag | 5 | 2 |
| 87, long help spelling | 1 | 2 |
| 87, short help spelling | 1 | 1 |
| 91, keygen dispatch | 1 | 1 |
| 94, init dispatch | 4 | 4 |
| 97, checkpoint dispatch | 1 | 3 |
| 105, stage dispatch | 3 | 3 |
| 120, verify dispatch | 2 | 5 |

Twenty named decisions, three coupled expressions and four scoped rows
account for 27 runtime records. The project now has 502 runtime and
7,514 tooling gaps. Native failure injection after initialization creates
the file, and the other startup and service-loop records, remain open.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 --case r1-study-cli- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.
