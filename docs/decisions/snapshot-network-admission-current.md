# Snapshot network command admission decisions

The four network commands reject invalid identities, native address encoding
and socket syntax before opening their input. Serve and both shipping commands
also reject out-of-range option values at this point. Missing local credentials
or peer certificate report the original I/O error under credentials_load.
A present but malformed peer certificate reports its TLS error under tls_config.
After valid credentials, a missing source, follower directory or saved descriptor
reports NotFound under storage_open.

The fixture uses distinct local and peer certificates and a previously installed
revision 4 follower. Refusals create no export and preserve the exact manifest and
selected image. Fresh open checks the same durable receipt, current value and
retained revision 2. The existing process suites cover successful dispatch and
separate local capture and follower durability output.

| Command | Error propagation boundaries |
| --- | --- |
| Shared TLS setup | Credential load, pin file load |
| Serve | Enrollment, native address encoding, socket syntax, option, TLS setup, follower open |
| Ship | Stream identity, native address encoding, socket syntax, option, TLS setup, export preparation |
| Ship captured | Stream identity, native address encoding, socket syntax, option, TLS setup, saved export preparation |
| Poll | Stream identity, native address encoding, socket syntax, TLS setup, primary open |

Each named mutation substitutes a different error at one propagation site.
The exact test checks the returned error and diagnostic phase. These controls
cover the listed argument, filesystem and configuration paths.

## Qualification

Frozen source `32c695ec4f28243f9ed52b5a2198663f026cb7dd` ran with
locked dependencies and Rust 1.100.0. Docker Linux ran on ext4, including
the source, build, fixture and temporary paths.

| Platform | Debug and release, per profile |
| --- | --- |
| macOS arm64 | 58 snapshot CLI tests passed and 11 core replication tests passed with one ignored. All 25 named controls changed the expected assertion. |
| Docker Linux arm64/ext4 | 59 snapshot CLI tests passed with one ignored and 11 core replication tests passed with one ignored. All 25 named controls changed the expected assertion. |
| Windows x64/MSVC | Five CLI test binaries cross-linked. Native execution of this source remains open. |

Clippy with warnings denied passed on macOS and Linux. In each mutation
campaign the independent verifier checked 80 supervised commands, the
exact failed assertion for every control and the restoration of all
1,001 copied inputs. Reconciliation closes the 25 named propagation
records and leaves 337 runtime and 7,468 tooling gaps.
[Frozen campaign checksums](snapshot-network-admission-32c695e.sha256)
