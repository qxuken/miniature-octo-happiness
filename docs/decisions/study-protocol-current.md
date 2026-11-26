# Study command protocol in the standalone fuzz workspace

The application and standalone fuzz library compile the same protocol
through separate Cargo targets. Named mutations now select the latter
with its locked manifest and exact test names. The Study application
runs the same deterministic tests in its own target.

| Operation | Evidence |
| --- | --- |
| Admit a collection and actor | Check both actor IDs and the mixed-case decoded bytes. Distinguish wrong width, non-hex ASCII, zero collection, unknown actor, and invalid argument encoding in either position. |
| Read a progress row | An enrolled row reads normally. A deleted current row returns NotFound, while the pinned earlier snapshot still reads its original state. |
| Parse local commands | Each numeric field returns InvalidDigit and preserves the revision and progress. |
| Edit progress | A good edit updates all four fields together. Out-of-range lessons, zero increments and register exhaustion return their original errors and discard every change in the draft. |
| Retry a draft | Only Conflict retries. Three conflicts exhaust the existing limit. RecoveryRequired and OutcomeUnknown return immediately. One conflict followed by success publishes one increment. |

The register fixture seeds a valid maximum sequence using the public
register decoder and the local replica. It checks exhaustion of each
register separately. This is a logical boundary fixture for the draft,
with storage crash and real uncertain-outcome qualification elsewhere.

Four standalone records retain individual gaps. These are the actor
and command dispatches, the fixed retry-loop iterator record and the
post-admission integer conversion. The latter remains infeasible under
the runtime admission invariant. Runtime evidence below keeps its
original reviewed scope.

## Preserved runtime source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native/protocol.rs` | `4517dfe5c848f7033f8b7c80c5a9c0a85b771f1fb6cd9fa28495e7d835e2518a` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:34:26–34:27 | 1045..1046 | try_propagation | parse | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| P | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:35:9–35:11 | 1056..1058 | if | parse | Original Study runtime predicate scenarios retain their source binding. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| C | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:35:12–35:72 | 1059..1119 | short_circuit_or | parse | This short-circuit expression overlaps the enclosing enrollment format decision. |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:38:30–38:31 | 1239..1240 | try_propagation | parse | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| I | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:38:61–38:62 | 1270..1271 | try_propagation | parse | After exactly 32 ASCII hex digits pass admission, the u128 conversion cannot overflow or have an invalid digit. |
| E | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:53:20–53:25 | 1579..1584 | match | command | Study exercise accepts and rejects the documented case classes. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:56:51–56:52 | 1707..1708 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:67:42–67:43 | 2164..2165 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:70:15–70:16 | 2289..2290 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:74:31–74:32 | 2434..2435 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:75:32–75:33 | 2468..2469 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:76:29–76:30 | 2499..2500 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:77:30–77:31 | 2531..2532 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:80:47–80:48 | 2638..2639 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:81:65–81:66 | 2705..2706 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:82:54–82:55 | 2761..2762 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| D | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:84:15–84:16 | 2835..2836 | try_propagation | command | The pinned runtime row delegates the error boundary. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| E | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:97:5–97:8 | 3275..3278 | for_iteration | edit | Study exercise accepts and rejects the documented case classes. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| P | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:99:9–99:11 | 3343..3345 | if | edit | Original Study runtime predicate scenarios retain their source binding. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| E | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:22:21–22:26 | 638..643 | match | new | Study exercise accepts and rejects the documented case classes. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |
| P | study-peer::study-peer | apps/study-peer/src/native/protocol.rs:27:9–27:11 | 805..807 | if | new | Original Study runtime predicate scenarios retain their source binding. [Original runtime audit](distributed-apps.md#a1-study-enrollment-commands-and-reviewed-resolution) |

## Verification

Rust 1.100.0 checked frozen source
`8b33e8c54885c7c7f1e29b947b6b3c1c39015b34` on macOS arm64 and Docker
Linux arm64/ext4. Each host and profile passed 70 Study tests, 35 standalone
fuzz-library tests and 12 mutation-runner contract tests. All 17 named
mutants reached their intended assertions in all four campaigns.
Clippy passed with warnings denied for Study, the runner and the fuzz
library. The Linux driver was edited while it was active and stopped
before linting. The test and mutation reports had already completed.
Linux Clippy ran separately against the same frozen source.

The freshly extracted proof verifies 989 copied source hashes and
59 supervised command outcomes per campaign. Four Study targets and
the standalone fuzz-library target cross-linked for Windows x64 in
each profile. Native Windows execution of these tests is pending.
The workflow fetches the locked standalone dependencies in the
existing Unix R1 control step. Actionlint 1.7.12 passed.

Standalone protocol qualification closes 17 records, including one
overlapping short-circuit expression. Exact library test selection
adds one runner decision. Reconciliation leaves 410 runtime and
7,498 tooling gaps. [Campaign checksums](study-protocol-8b33e8c.sha256)
