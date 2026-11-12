# Pre-1 historical-reader qualification

Run from a checkout with the historical Git object and Cargo dependencies
already available locally:

```sh
python3 tools/compatibility.py --historical a7dc343 --output target/compatibility/run-001
cargo test --offline --locked -p weekend-db-typed --all-features --test process_evolution
cargo test --offline --locked -p weekend-db-typed --all-features --release --test process_evolution
```

The output directory must not exist. The Python tool archives the resolved
historical commit into a disposable directory. Two isolated downstream projects
compile the same driver against the historical and current engine, with no
optional features, using offline Cargo builds and generated locked resolutions.
No Git checkout is changed and no network fetch is performed. The same installed
Rust compiler builds both sources; this does not qualify older Rust toolchains.

The current writer creates a format-two image with current revision4, integer
and byte-string records, and a retained revision2. Both readers check exact
current/retained row counts and values. Full image bytes must stay unchanged.
Both readers then reject an unsupported header version without changing bytes.
That negative fixture deliberately changes the version before CRC validation;
it is not a valid future encoder or format-transition test.

`evidence.json` records resolved source revisions, current dirty state and diff
hash, current core file hashes, driver hash, toolchain, Cargo, lockfile hashes,
features, commands and image hash. Logs and resulting images remain in the output
directory; disposable source/build directories are removed. Build commands in
evidence therefore reference paths that no longer exist. A failed command
returns nonzero and its log remains available. Environment compiler overrides
are inherited; run in a controlled toolchain environment for release evidence.

The typed integration test runs native subprocesses for each evolution phase.
It creates version-one rows with an unknown field and retained snapshot,
partially migrates to version two, backs up and restores, then rolls back the
restored table to retained old rows under the new catalog. It checks that newer
model updates require migration, explicitly remigrates, and verifies primary
values, exact index lookups, unknown fields and historical rows. Source and
backup remain independently readable at the original mixed versions. An
unsupported disk version is rejected with the entire file unchanged. These
are orderly process boundaries, not killed-process or device-power-loss tests.

The archived `tests/fixtures/pre1-v2.wdb` and `tests/compatibility.rs` remain the
opposite direction, current-reader/historical-writer evidence. Do not regenerate
that archived fixture. These tests qualify only the named pre-1 checkpoints and
covered values/operations; they are not a stable disk-format, Rust-API or release
compatibility promise. New schema shapes, feature encodings and format changes
need additional fixtures and reader/writer combinations.

## Passive snapshot wire profile

`--profile snapshot` separately qualifies two archived cores using their original
unchanged Cargo.lock files and the same private example overlay. The bounded
[dcd2402 ↔ 315068f qualification](../docs/snapshot-compatibility.md) passed both
wire-v1 directions, exact export digest/receipt and retained-root reopen checks.
Its negative case mutates a **poll request** version byte to 255 and verifies
rejection without follower-file changes; it does not inject upload faults.
See that report for the command, exact source/binary evidence and the distinction
between caller-authorized loopback transport and the newer CLI's TLS deployment.
