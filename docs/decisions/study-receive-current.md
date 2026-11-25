# Study authenticated receive interruption

The test contacts a running receiver with the configured peer
credentials. The fixture waits until the authenticated session reaches
the application adapter, then closes the client without an offer.
The receiver reports one failure and zero successful sessions. In JSON
mode the nested diagnostic retains the replication phase and engine
`Io` code. In text mode it retains the original `Io` error.

| Observation | State |
| --- | --- |
| The authenticated client disconnects before the offer | The receiver stays at revision 2 with zero minutes and identical file bytes. |
| The sender commits seven minutes locally | It observes its own revision 3 before shipping. |
| The sender retries over a fresh authenticated session | The existing receiver advances to revision 3 and acknowledges seven minutes with its configured actor and policy. |
| The receiver stops and the database reopens | Revision 3 retains seven minutes. |

The two fault controls substitute the error returned by the receiving
wrapper or change the structured phase. The fixture checks both
diagnostic modes and the normal completion of the same wrapper.

## Verification

Frozen source `32a232dac748d6eae1bbac4658107b5ec29d1cb1` passed with Rust
1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4. Debug and
release each passed 66 Study tests, 14 core CRDT tests and all nine
selected mutation controls. Those controls include the two receiving
mutations and seven existing outgoing mutations. Strict Clippy passed
on both hosts.

Fresh extraction checked the expected assertion failures, the exact
runners, all 136 process reports and 968 copied files per campaign.
The macOS nightly `739adecd4` observed both outcomes of the incoming
report's JSON selector. The extracted executable and raw profiles
reproduced the LLVM export byte for byte. The Windows x64 MSVC debug
test targets cross-linked with Rust 1.100.0 and cargo-xwin 0.23.1.
[Archive checksums](study-receive-32a232d.sha256)

Reconciliation leaves 438 runtime and 7,514 tooling gaps. Study's listener
configuration and local address error paths are still open.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 \
  --case r1-study-receive- --case r1-study-peer-status- \
  --output "$OUTPUT/debug"
```

Repeat the commands with `--release` and a fresh mutation output path.

## Windows lease regression

The native minimum-Rust job for PR #82 head `f513546` failed the
byte comparison while the receiver still owned its exclusive file
lease. The archived Windows artifact records sharing violation 32
and binds the fixture to test merge `6573cd2525d785e53899e2afbc4f7bf5336b21a1`.
[Failed Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45704523143/job/139522942007)

The corrected fixture covers four combinations. In both text and JSON
modes it verifies revision 2 after the disconnect. One run stops the
receiver and compares the file bytes after lease release. The other
keeps the receiver running, accepts a fresh authenticated retry and
reopens at revision 3 with seven minutes.

Frozen source `bbe1fcd363ec5e553c112c8c8fbb71b494c8fbc9` passed
117 macOS and 118 ext4 Docker Linux tests per profile, with one
opt-in Linux case outside this run. All six selected Study
and snapshot startup controls were detected in both profiles.
Strict Clippy passed on both hosts. Fresh extraction verified 100
command reports and all 977 copied files per campaign. Nine Windows
x64 MSVC test targets cross-linked in each profile. The next hosted
run will exercise the corrected regression on Windows.
[Archive checksums](study-receive-lease-bbe1fcd.sha256)

## Native Windows confirmation

PR #82 at `06fb27c` passed the native Windows Rust 1.100.0 job at test
merge `f9d6feb54bdadb4e0a8e29a972219cad838f55c0`. The Windows artifact
contains the passing corrected disconnect/retry test and queued-listener
test in both debug and release. The packaged source matches the exact
fixture bytes in the tested merge and PR head.
[Native minimum-Rust job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45706498857/job/139529935557), [Archive checksums](windows-msrv-06fb27c.sha256)
