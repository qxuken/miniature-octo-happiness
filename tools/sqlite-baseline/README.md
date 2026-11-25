# Private durable-history baseline

Run from the repository with the pinned Rust toolchain. The SQLite worker links
to the native SQLite library and requires its development headers on Linux
(`libsqlite3-dev` and `pkg-config` on Debian/Ubuntu). Fetch the standalone
manifest's locked dependencies once before running offline.

```sh
cargo fetch --locked --manifest-path tools/sqlite-baseline/Cargo.toml
cargo test --offline --locked --manifest-path tools/sqlite-baseline/Cargo.toml \
  --no-default-features --features sqlite-comparator --bin sqlite-comparator
mkdir -p target
cargo xtask sqlite-baseline --revision 2490631dab43678279f8ba2c56458c8ea908c1b4 \
  --output target/sqlite-baseline-2490631
```

The output path must not exist and its parent must exist. Existing files,
directories and dangling symlinks are rejected; no cleanup or overwrite option
exists. A failed run preserves its newly reserved directory for inspection. The
controller builds both release fixtures offline from a Git archive of the
requested engine revision and hashes the overlaid harness files separately.
It records sources, binary hashes, toolchain and options, the SQLite runtime
identity and linkage, compile options, and raw timing samples in
`evidence.json`. The isolated Cargo workspace has `publish = false`.

Defaults: three alternating-order rounds, each with 32 seeded keys, 64 data
transactions of four 256-byte replacements, four separately durable history pins,
and 32 reopen samples. Bounds reject unbounded parameters. No cache eviction,
concurrency, crash injection, reads-only workload, throughput claim or statistical
performance guarantee is implied. See [the protocol and results](../../docs/sqlite-baseline.md)
for timing boundaries, durability settings, oracle coverage and differences.

Pass `--sample-rss` to retain the main worker's RSS samples, sample count
and largest observed value (KiB) in the supervision report for each round.
Sampling covers startup through maintenance and exit. A zero sample count
means the worker exited before measurement. Run timing-only comparisons
separately to keep the sampling overhead out of latency results.
