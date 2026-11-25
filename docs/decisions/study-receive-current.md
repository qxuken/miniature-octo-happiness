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
