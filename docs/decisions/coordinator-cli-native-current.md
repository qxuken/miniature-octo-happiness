# Coordinator command line and native startup

| Boundary | Observed outcome |
| --- | --- |
| Command dispatch | Both help spellings print usage and succeed. Unknown command shapes and incorrect configuration flags print usage and fail. Positive integration cases exercise both initializers, both listener profiles, both client forms and the journal. |
| Port and configuration order | With a missing configuration file, an invalid port returns the numeric parse error in each of the five listener and client forms. A valid port reaches the missing-file error in each configuration-file form. |
| Native initialization | Both initializers create a fresh image. Repeating either invocation reports `AlreadyExists` and preserves exact bytes. |
| Startup precedence | With a port already bound, a missing database and each incompatible profile report their own error before bind. A valid database for either profile reaches the original socket error. No readiness line is emitted on failure. |
| Configuration file admission | Missing metadata preserves `NotFound`. A directory or FIFO rejects before opening. Unix owner-only permissions pass and broader permissions reject. The correct width and hex alphabet are checked independently, with one trailing newline allowed. |
| Command-line input and reply | EOF exits successfully. A blank line is skipped and trimmed `quit` stops before a later command. Invalid UTF-8 and the native connection error are propagated. A truncated reply returns `InvalidData` and the original diagnostic. |
| Request framing | The loopback fixture observes the plain request line and the configured form with the expected prefix and bytes. |
| Reply deadline | An expired deadline preserves the queued line. A live deadline reads that same line. |

The forwarders preserve error text as well as process status. The socket
fixture derives the expected native bind and connection errors on the same
host. Guard pairs keep the command spelling fixed when varying the flag and
keep the flag fixed when varying the spelling.

The separate inventory rows for faults within initializer publication,
file I/O after metadata admission, socket setters and writes, worker launch,
and stdout still require evidence. The delivered binary tests use both input
and socket paths, with the source decisions tied to the exact target below.

## Qualification

Frozen source `2b204c98c39441bc13b9ea934a7b24bc600a73c8` ran with Rust
1.100.0 and locked dependencies. Each debug and release profile on macOS
arm64 and Docker Linux arm64/ext4 passed 45 unit and 19 integration tests.
Two macOS and three Linux workloads remained opt-in. Both hosts passed
strict Clippy and detected all 44 mutation controls per profile. The
freshly extracted archive verifies the expected test and assertion for
each failure, the supervised command reports and copied source hashes.

The coordinator unit and integration binaries cross-linked for Windows x64
in both profiles. Native execution remains pending for this source.
Reconciliation covers 43 CLI runtime and five service startup records,
leaving 144 runtime and 7,458 tooling gaps.
[Frozen source and evidence checksums](coordinator-cli-native-2b204c9.sha256)

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native.rs` | `1891fbd97bff686d9486424a69be983ad9c5d92bec2aee161fbc77f073f667f0` |
| `apps/write-coordinator/src/native/service.rs` | `a5c5dd34ac9de1685f58631f3ccde36e15771baca3a5e3871e86d8869a2bbaf6` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:42:5– | 1512..1517 | match | run | Help, legacy and sequenced initialization, both service profiles, both client forms, journal and usage have observable process results. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:149:5– | 5512..5515 | for_iteration | client | EOF exits, blank input continues, quit stops and regular input reaches the request path. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:167:8– | 6091..6094 | pattern_test | request | Presence is the same decision as the enclosing if. The peer observes the plain and configured wire forms. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native/service.rs:72:65– | 2094..2099 | match | serve | Both valid profiles reach bind. Swapping the database and profile yields the appropriate storage rejection. |
