# Extended ASan fuzz qualification: 21e5d26

All 22 fuzz targets completed successfully on exact source
`21e5d2638c24c6254ced35d0077c55bb30a287b2`, with **1,431,321 actual
libFuzzer runs**. These are execution counts, including corpus processing, not
unique inputs or a coverage percentage. This report reviews completed evidence;
no fuzz campaign was rerun to produce it.

## Source and tooling

The detached `/tmp/weekend-fuzz-21e5d26` checkout remains clean. Verification
matched all 126 campaign source hashes, all 568 independent-replay archive file
hashes, and all 242 seed hashes against both checkout bytes and `git show` at the
exact commit. Campaign/replay seed inventories and both lockfiles agree. The
runner checked source, fuzz lockfile and seed stability after each target; no
change flags were recorded. The independent replay also recorded unchanged
source, locks and seeds.

ASan campaign: native `aarch64-apple-darwin`, cargo-fuzz 0.13.2,
`rustc 1.101.0-nightly` commit
`708c94ef093ecc42fb8851e17d8383d999727abc` (2026-11-03), LLVM 23.1.5.
The invocation used the `+nightly` alias; the recorded compiler identity above,
not today's alias, defines this evidence. Each recorded command used
`cargo +nightly fuzz run --sanitizer address` with:

```text
-runs=100000 -max_total_time=120 -rss_limit_mb=2048
-max_len=262144 -seed=3 -timeout=10
```

Each target received a separate writable corpus and the corresponding checked-in
seed directory. The first stopping bound wins; time-limited targets reported
121 seconds at their next completion check. Summed command elapsed time,
including builds, was 1,841.37 seconds. This was a finite accelerated campaign,
not a long-running wall-clock soak.

The fuzz lockfile SHA256 is
`0a1c5b1b5e2c3dd2ffc169f065cfea52ba40662de837b7f0cdf6078ae21c18d8`;
root lockfile SHA256 is
`384d128361f436d847264173af62063e151d93b5396e025459e2066c289f881a`.
No dependency publication or remote CI run was involved.

## Actual executions

Every target exited zero, its final `Done ... runs` count matched the recorded
metadata, and every failure-artifact directory was empty.

| Target | Actual runs | Reported fuzz seconds |
|---|---:|---:|
| `open_image_bytes` | 100,000 | 14 |
| `open_structured_image` | 100,000 | 39 |
| `commit_crash` | 55,177 | 121 |
| `txn_schedule` | 100,000 | 8 |
| `value_ops` | 100,000 | 24 |
| `image_and_ops` | 46,318 | 121 |
| `retention_compact_crash` | 32,726 | 121 |
| `ffi_ops` | 100,000 | 5 |
| `crdt_frames` | 100,000 | 5 |
| `crdt_schedule` | 37,073 | 121 |
| `snapshot_frames` | 100,000 | 37 |
| `snapshot_install_schedule` | 2,132 | 121 |
| `schema_index_migration` | 5,867 | 121 |
| `derive_tokens` | 70,437 | 121 |
| `logical_import_export` | 100,000 | 94 |
| `backup_restore` | 71,904 | 121 |
| `bytestore_contract` | 100,000 | 88 |
| `coordinator_protocol` | 43,877 | 121 |
| `peer_transport` | 100,000 | 15 |
| `replacement_namespace` | 28,172 | 121 |
| `tree_adversarial_keys` | 6,604 | 121 |
| `register_causality` | 31,034 | 121 |

## Independent deterministic replay

The separate stable-toolchain replay passed all 25 library tests, including the
checked-in corpus replays. It used Mise Rust 1.99.1, compiler commit
`58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM 23.1.1, native
`aarch64-apple-darwin`, with `cargo test --offline --locked --manifest-path
<isolated-source>/fuzz/Cargo.toml --lib`. Exit status was zero; elapsed time was
11.21 seconds. This is independent deterministic replay, not another ASan
campaign. Original evidence remains at
`target/fuzz-replay-21e5d26-independent/evidence.json`.

## Preserved evidence and limits

`target/fuzz-qualification-21e5d26/index.json` binds compact local evidence:
original campaign/replay JSON, the campaign summary, deterministic replay log,
and all 22 target logs (losslessly gzip-compressed with fixed timestamps).
The index records compressed and original SHA256 hashes, independently checked
run counts, source verification and each target's elapsed time. Index SHA256:
`0ff59e91248273bd0876f2ff4a89be21f6c584ab1381b458d641dc9c050238c8`.
Original campaign artifacts remain under `/tmp/weekend-fuzz-21e5d26-evidence`;
mutated corpora were not duplicated into the compact log bundle.

This establishes no detected crash, oracle failure or ASan report within this
specific finite campaign. It does not establish exhaustive path coverage,
MC/DC, unbounded-state convergence, absence of memory bugs, or fuzzing of every
public API. The command did not rebuild Rust's standard library with ASan;
there is no claim that prebuilt standard-library/native code was instrumented.
Protocol targets exercise their documented parsers/models, not a complete TLS
or hostile-network deployment. Simulator crash/namespace targets do not prove
native filesystem or physical power-loss behavior. Target-specific bounds and
oracles remain those in [the fuzz inventory](../fuzz/README.md). Qualification
is tied to exact 21e5d26, not subsequent test/tool overlays or the current tree.
