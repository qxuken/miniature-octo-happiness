# Shared TLS credential-file admission

The same tests load temporary files through both shipping CLI binaries.
The file check preserves a missing path as `NotFound`, rejects a
directory, and returns the exact bytes of a file at the test limit.
A zero-byte file and a file one byte over the limit are rejected.

The Unix permission check allows mode `0644` for a public input. The
same file is rejected as a private key. Changing that file to `0600`
allows the private read. The tests use controlled local files and
the documented trusted-directory assumption.

| Decision | Input conditions | Accept or continue |
| --- | --- | --- |
| Path type | Existing regular file | Yes |
| Path type | Existing directory | No |
| Size | Nonempty at the limit | Yes |
| Size | Empty | No |
| Size | Nonempty one byte over the limit | No |
| Unix permission | Public input, group or other permission bits set | Yes |
| Unix permission | Private input, group and other permission bits clear | Yes |
| Unix permission | Private input, group or other permission bits set | No |

The size rows change each rejection term independently. Both terms
are observed false on the accepted boundary. The Unix permission rows
hold the mode constant when switching the input role, then hold the
private role constant when changing the mode. The same pairs cover
the source short-circuit sites. The manifest lists the explicit pairs
and the six error and guard substitutions for each binary.

This batch covers the first metadata call and the file type, size and
Unix mode guards. The later open, opened-handle metadata, handle type
check and read-failure sites remain open. The `live_month` example
compiles the shared module in a separate tooling context.

## Verification

Frozen source `3dc33ce6e4ca0dc3aa53d4e760b988b9d06b656c` passed 117
tests on arm64 macOS and 118 on arm64 Docker Linux in each profile with
Rust 1.100.0. Linux ran the source, builds and temporary files on ext4.
The opt-in Linux exhaustion test stayed outside this campaign. Strict
Clippy passed on both hosts.

Both profiles on both hosts detected all 12 credential-file controls,
15 existing interactive controls and the listener retirement control.
Fresh extraction verified the intended assertion failures, 376 command
reports, the exact runner binaries and 972 copied files per campaign.
Nine Windows x64 MSVC debug targets cross-linked.
[Archive checksums](credential-file-3dc33ce.sha256)

The first macOS archive audit detected raw compiler profiles in the
checkout used by the stable mutation run. The archived macOS results
come from a fresh checkout dedicated to stable tests and mutations.
The coverage replay uses its own checkout and writes profiles outside it.

The macOS compiler's condition counters matched between the two CLIs.
Both outcomes executed for the pre-open type guard and each term of
the Unix mode and byte-size decisions. The opened-handle type guard
observed only regular handles. Fresh extraction reproduced both LLVM
exports byte for byte from the test executables and raw profiles.

| Source condition | True | False |
| --- | ---: | ---: |
| Pre-open type check, line 310 | 1 | 7 |
| Opened-handle type check, line 315 | 0 | 7 |
| Private input, line 321 | 2 | 5 |
| Group or other bits set, line 321 | 1 | 1 |
| Empty input, line 329 | 1 | 5 |
| Oversized input, line 329 | 1 | 4 |

The final reconciliation classifies 12 additional runtime records.
It leaves 422 runtime and 7,514 tooling gaps.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer -p weekend-replicate
cargo xtask mutations --toolchain 1.100.0 --case r1-credential-file- \
  --case r1-study-queued-listener-close --case r1-study-interactive- \
  --output "$OUTPUT/debug"
```

Repeat with `--release` and a fresh mutation output path.
