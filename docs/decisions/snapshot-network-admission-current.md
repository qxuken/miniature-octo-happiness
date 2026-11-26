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

## Windows fixture correction

The Windows MSVC minimum-Rust debug run on PR #82 head `4f6359c`
reached the admission test and failed while creating its credentials. The
test called the Unix-only `keygen` implementation. The failure is in
`debug-1.stdout` from artifact `15075158211`, with log SHA-256
`fea42ed933f4c2e62defd3c860739b0a0af2a7bcfa676802ddb832983f96eff1`.
[Hosted Windows MSVC job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45727730738/job/139601722079)

At `718f8f53950becf4c8867e7840a28fcac3ae2858`, the test generates
and writes both credentials using the portable fixture helper. Both debug
and release passed on macOS arm64 and Docker Linux arm64/ext4. The CLI
suites passed 66 and 67 tests per profile, with one Linux test ignored.
Both platforms passed 11 core replication tests with one ignored. All 25
admission controls failed their intended assertion on each run and Clippy
with warnings denied passed. Five Windows x64 test executables cross-linked
per profile. Native Windows execution of the corrected fixture is pending.
[Frozen fixture checksums](snapshot-credential-fixture-718f8f5.sha256)
