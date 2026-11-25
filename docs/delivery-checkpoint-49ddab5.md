# Private delivery checkpoint at 49ddab5

The four delivery lanes qualified the same Git commit,
`49ddab548fa60fced67f04d666d429cd49746492`. This is the test merge for
PR #81. Its file tree is identical to branch head `5f49a59`. All lanes used
Rust 1.100.0 and exercised private packages with combined features, base
features, replication only and CRDT only.

| Platform | Evidence origin | Files checked against controller inventory | Combined library SHA-256 |
| --- | --- | ---: | --- |
| macOS arm64 | Native development host | 4,019 | `99ade49d74699a7b161617dd2d5579d7bef029b40d229089fd6def44d8f06a6d` |
| Linux arm64 | Docker, ext4 runtime and database | 4,022 | `1c063932434d61283af39788ba21e4e41e68ab398c0d02fef83d00b3cb28c01a` |
| Linux x64 | [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45590823125/artifacts/15010842410) | 4,014 | `1f6666d3c2a710c45c7ecdae206e30c55c2aa24b7a6ce4f92eed66683116dce7` |
| Windows x64 | [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45590823125/artifacts/15011733343) | 2,277 | `f316ee26e5c5023847aeeda9948c14b7a4ae1e3f5b218111e9488af4690391f1` |

The verifier read fresh extractions of all four archives, checked files and
lockfiles against the Git source, and verified each package manifest and
consumer trace. Three macOS benchmark runs also passed same-source replay
with compaction enabled. The delivery and benchmark JSON report has SHA-256
`66083c2240fcdeeb59f2fe6980b2fa389f37dfa29983c3765c69a1b6dcfd1b4e`.

The Linux arm64 harness first rejected overlay-backed temporary storage. A
second setup reached the native consumer trace but its isolated Cargo process
could not resolve cached packages. The passing run put temporary files on
ext4 and made the offline registry available at Cargo's default home in the
container. Both failed setup reports remain on the development volume.

The hosted 22-target fuzz job passed on this commit. Inspection of its ZIP
found that artifact upload omitted empty per-target failure directories.
The verifier requires those directories when reviewing an extracted campaign.
The updated workflow includes a tar of the original evidence tree alongside
the existing loose logs. [Hosted fuzz archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45590823137/artifacts/15011241702)

A fresh macOS AddressSanitizer run at the same Git commit completed 21,354
executions across all 22 targets with seed 1, a 1,000-execution limit and
a 20-second target limit. The GNU tar create-and-compare steps used by the
updated workflow preserved the evidence. After extracting that archive,
`xtask verify-delivery --require-isolated` accepted all four delivery lanes
together with the benchmark and complete fuzz evidence. The consolidated
report SHA-256 is
`08a752efc538e07282965a71ee6f8cb93fbab5ddbaca19b50e119dcdd9827c57`.

Reproduce with the checked-out commit, the four delivery directories named
`macos-arm64`, `linux-arm64`, `linux-x64` and `windows-x64`, the extracted
benchmark and the fuzz directory restored from tar.

```sh
cargo +1.100.0 xtask verify-delivery \
  --source 49ddab548fa60fced67f04d666d429cd49746492 \
  --matrix "$MATRIX" --benchmark "$BENCHMARK" --fuzz "$FUZZ" --require-isolated
```

The development host holds the source bundle, platform archives, fuzz and
benchmark inputs, and the combined JSON report. Off-machine restore,
qualification on deployment devices and release sign-off remain open.
