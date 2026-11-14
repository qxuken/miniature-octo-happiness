# Private typed/CRDT source consumer

This standalone workspace is used by `tools/release.py --rust-source`. Its core,
typed and derive dependencies resolve by relative paths within the copied source
tree. Cargo publication is disabled. The consumer checks its expected source ID,
typed Model/index/update behavior, enrolled peers' offline edits and committed
CRDT policy resolution, repeat-sync stability, and wire roundtrip. Loopback
streams are trusted only within this test fixture.

For a manual local run, set `WEEKEND_DB_SOURCE_ID=local-check` and
`CARGO_INCREMENTAL=0`, then run:

```sh
cargo run --locked --release --manifest-path examples/rust_package/Cargo.toml -- local-check
```

The packaged profile records the real checked source identity instead. External
registry dependencies remain locked build inputs; they are not vendored.
