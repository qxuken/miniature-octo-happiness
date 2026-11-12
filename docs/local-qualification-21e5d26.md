# Local candidate qualification: 21e5d26

Exact source: `21e5d2638c24c6254ced35d0077c55bb30a287b2`. This records local
execution against clean detached sources, not hosted CI or publication. All
checks below completed successfully. No source fixes were needed.

## Executed checks

| Environment | Check | Result |
|---|---|---|
| macOS arm64 | Complete `tools/check.py` | Passed: formatting, strict workspace/all-feature/all-target Clippy, debug/release workspace tests and Rust examples, renamed Rust consumer, shared-library ABI smoke, C memory/file/thread consumers, Odin memory/file consumers with static wgpu-native |
| Linux aarch64 | Complete `tools/check.py --skip-odin` | Passed: same applicable Rust/C/ABI checks in debug and release; Odin explicitly omitted |
| macOS arm64 | Private release package, `crdt,replication`, Odin and Rust source | Verified and candidate-eligible; C/C++, Odin, relocation and ABI consumers passed |
| Isolated Linux container | Copied Rust-source package consumer | Passed typed Model/index/update and CRDT Record/Resolution committed sync, repeat-sync and wire roundtrip without the original checkout mounted |
| Linux aarch64 | Private release package, `crdt,replication` | Verified and candidate-eligible; C/C++, relocation and ABI consumers passed |

The macOS worktree was `/tmp/weekend-qualification-21e5d26`, detached at the exact
commit. The host launcher used Mise Python 3.14.8 and Rust 1.99.1
(`58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM 23.1.1).
Odin was `/Users/qxuken/projects/libs/Odin/odin`,
`dev-2026-10:a22aa1984`. `CARGO_INCREMENTAL=0` and a separate target directory
isolated these builds. Full macOS checks took about 172 seconds; private package
qualification took about 18 seconds. These durations are operational observations,
not performance comparisons.

Linux used immutable local image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
Rust 1.99.1/LLVM 23.1.1 and its installed Python 3.11.2. The runner cloned the
repository, checked out the exact commit, and placed source, build, temporary
files and test databases on a private Docker volume. `findmnt` confirmed ext4
(`/dev/vda1`); these were not host-bind filesystem tests. The full check/package
container had networking disabled. Combined execution took about 126 seconds.
The separate copied-source Rust isolation check uses the existing package tool's
registry access: dependencies are not vendored. No crate was published.

## Commands and evidence

Host commands ran from the detached worktree with the pinned Mise environment:

```sh
python tools/check.py
python tools/release.py --output NEW_PRIVATE_PACKAGE \
  --features crdt,replication --odin --rust-source --rust-source-isolated \
  --rust-source-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
```

Linux ran `python3 tools/check.py --skip-odin` followed by
`python3 tools/release.py --output NEW_PRIVATE_PACKAGE --features crdt,replication`.
No `--allow-dirty` override was used. Both source trees remained clean.

Local evidence lives under `target/qualification-21e5d26/`: `mac-run.json`,
`linux-run.json`, `mac-check.log`, `linux-check.log`, orchestration/package logs,
`source-sha256.json`, and `mac-package/manifest.json` /
`linux-package/manifest.json`. These are local artifacts, not a permanent storage
or release promise. The source inventory covers 568 tracked files; it matched
both package manifests, and the macOS source files were independently rehashed
after execution. All 652 macOS and 47 Linux packaged artifact hashes were also
independently rechecked against their manifests.

| Artifact | SHA-256 |
|---|---|
| macOS package manifest | `55186dc7c5114650c7510d4660d3f62a31d554b257caf9e413cd981e1c78fa7d` |
| macOS shared library | `200c17d94ab533112e6de2a6f87336ce728ff486128d2ed4f8e260e0b60402e0` |
| Linux package manifest | `d0a1bbce2fc5b742fe843ca5e28b68b6dd7a128aca5eb994389a256b7ba990c2` |
| Linux shared library | `16a927abb639ed68de1fdcc375bc52d41270aa1ac06552b1132fd3942088ac64` |

Both manifests bind the exact commit, requested features, compiler, exported
symbols, dependency information, relocation checks and consumer commands.
Attribution inputs were complete with no outstanding review items reported by
the packaging tool. Candidate eligibility is that tool's local qualification
result; it is not a claim that the full production roadmap is complete.

## Limits

No native Windows execution or Windows cross-link was performed in this batch.
There was no new sanitizer, fuzz, MC/DC, long-soak, physical power-loss or
hardware/filesystem matrix campaign. Existing evidence for those checks retains
its original source and platform scope. Passing functional tests is not a
compiler MC/DC percentage. Local commit acknowledgment and follower durable
acknowledgment remain separate; this qualification changes neither contract.
The repository and packages remain private, with no remote CI, push or publication
performed by this qualification.
