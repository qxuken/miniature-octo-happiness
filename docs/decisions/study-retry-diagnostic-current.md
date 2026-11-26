# Study retry failure diagnostics

The retry test now reports the attempt, diagnostic mode, application
bytes observed, EOF, last transport error, storage context and follower
revision when a served exchange fails. It still requires the initial
EOF, unchanged bytes before retry, two acknowledgments and one committed
revision advance, followed by a successful reopen.

A directed macOS test expired the application channel after TLS
admission. The failure reported a timed-out read, zero transferred bytes
and follower revision 2 without a native storage error. All 25 local
focused repeats passed with ordinary deadlines. The operation that failed
in the original Windows MSRV job remains unresolved.
[Original Windows evidence checksums](windows-msrv-retry-c44c26d.sha256)

Frozen source `1ec259cc47fa41502fb98c75211e8e6c645553fc` used Rust 1.100.0.
macOS and Docker Linux arm64/ext4 passed 85 Study tests in each profile.
Both hosts caught all six existing mutations that use the retry test. The
restored reports identify each intended failure and 23 supervised commands
per campaign. Strict Clippy passed on both hosts and for Windows MSVC x64,
which linked four executables in both profiles. Native Windows follow-up
remains pending. The interrupted first Linux attempt is archived with the
verified serial runs.
[Frozen source and evidence checksums](study-retry-diagnostic-1ec259c.sha256)
