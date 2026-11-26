# Coordinator credential and client I/O

The CLI checks the configured path before opening it, then validates the
metadata on the opened handle and limits its read. The native connection
still sets its send timeout, writes the request and reads the reply in that
order. The tests supply failures at those operations on the same code path.

| Boundary | Successful observation | Failure and retained state |
| --- | --- | --- |
| Open | A configured regular file opens and its contents decode. | PermissionDenied after path admission retains its diagnostic and skips handle metadata. |
| Handle metadata | Metadata for an opened regular file permits the bounded read. | NotFound retains its diagnostic without reading. Opened metadata that reports a directory produces the regular-file error without reading. |
| Bounded read | The 64-character credential with or without one trailing newline decodes. | A failure after two bytes preserves UnexpectedEof and its message. |
| Request configuration | Both forms connect through loopback and send the expected bytes. | An unsupported write timeout leaves the writer untouched and skips the reply. |
| Configured and plain writes | The peer sees the complete expected line. | A ConnectionReset from either writer is reported after one attempted write, before reply collection. |
| Client output | The CLI returns the coordinator reply through stdout. | Closing the stdout reader preserves the original pipe diagnostic. A later command already buffered on stdin does not reach the coordinator. The exact image survives and reopens at revision 2. |

These filesystem and transport callbacks inject distinct failures. The
normal test paths still exercise real filesystem metadata, local TCP
and the production stdout pipe. Native socket fault qualification on
the declared deployment remains open.

## Qualification

Frozen source `bce6c3b342fea5c8fe7ad98ec6e8450e3f863c22` used Rust 1.100.0.
Each debug and release profile on arm64 macOS and Docker Linux/ext4
passed 56 coordinator unit tests, 22 coordinator integration tests
and 55 standalone fuzz-library tests. Two macOS and three Linux
workloads remained opt-in. Both Cargo targets passed strict Clippy.
The coordinator unit and integration binaries and the fuzz-library
test binary linked for Windows MSVC x64 in both profiles. Native
Windows execution remains pending for this source.

Each profile detected all 57 named mutations. This includes eight new
I/O controls, 48 prior CLI controls and the readiness-output control
that exercises the shared test watchdog. Fresh extraction verified the
named assertions, 187 supervised commands and 1,030 restored source
hashes per campaign. The audit closes all eight remaining native CLI
source records. The repository retains 99 runtime and 7,441 tooling
gaps, with 5,070 blind spots.
[Frozen source and evidence checksums](coordinator-client-io-bce6c3b.sha256)

## Hosted Windows evidence for earlier batches

The Windows MSVC minimum-Rust job passed on PR #82 head
`908f3c528728afe835ce98c75330ca00f5f68446`. The archived debug
and release workspace commands each passed the nine lifecycle tests
and five legacy protocol tests selected by their named controls.
The tested merge `2993a148f02f5e65ed234fb74bc2fa4b25e664d3` has the
same tree as the PR head. The evidence ZIP SHA-256 is
`3fa61aa506d65560bfc681a91d56528b977ae3a295b97bfd0a23a581666968f2`.
[Windows minimum-Rust job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45744882703/job/139658414497)
and [verified evidence ZIP](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45744882703/artifacts/15083352925).

## Coordinator source records

| Source | SHA-256 |
| --- | --- |
| `apps/write-coordinator/src/native.rs` | `152771a84da70bea736f1cfb33bcd6d295315fd269ca9ed454bad5fd5f772815` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:42:5– | 1512..1517 | match | run | Help, both initializers, both listener profiles, both client forms, journal and usage have separate process results. |
| E | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:165:5– | 5943..5946 | for_iteration | client | EOF exits, blank input continues, quit stops, and a regular line reaches the request path. |
| C | write-coordinator::write-coordinator | apps/write-coordinator/src/native.rs:199:8– | 6913..6916 | pattern_test | exchange | The pattern selects the same configured or plain form as the enclosing conditional. A loopback peer observes both forms. |
