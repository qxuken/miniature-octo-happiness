# Private durable-history baseline

Run from the repository with installed Mise pins:

```sh
mise x python@3.14.8 -- python3 tools/sqlite-baseline/test_runner.py
mise x python@3.14.8 -- python3 -O tools/sqlite-baseline/test_runner.py
mise x python@3.14.8 -- python3 tools/sqlite-baseline/run.py \
  target/sqlite-baseline-2490631 --revision 2490631dab43678279f8ba2c56458c8ea908c1b4
```

The output path must not exist and its parent must exist. Existing files,
directories and dangling symlinks are rejected; no cleanup or overwrite option
exists. Failures retain only the newly reserved directory for inspection. No
publication, remote execution or CI is involved. Cargo builds offline, release,
locked, through `mise x rust@1.99.1` from a Git archive of the resolved explicit
revision. The current harness is copied into that archive and separately hashed;
it is not attributed to the old engine commit. Sources, archive, binary,
toolchain/options, SQLite compile options and raw timing samples are bound in
`evidence.json`. The isolated Cargo workspace has `publish = false`.
Production checks use explicit failures and remain enabled under Python `-O`.

Defaults: three alternating-order rounds, each with 32 seeded keys, 64 data
transactions of four 256-byte replacements, four separately durable history pins,
and 32 reopen samples. Bounds reject unbounded parameters. No cache eviction,
concurrency, crash injection, reads-only workload, throughput claim or statistical
performance guarantee is implied. See [the protocol and results](../../docs/sqlite-baseline.md)
for timing boundaries, durability settings, oracle coverage and differences.
