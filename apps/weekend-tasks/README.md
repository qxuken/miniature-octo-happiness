# weekend-tasks

A local Rust task tracker built with `weekend-db-typed` derives. Supports tags,
notes, priorities, unique slugs, durable checkpoints, task undo/redo, explicit
schema upgrades and offline compaction. Commands reopen the same real database.

```sh
cargo run -p weekend-tasks -- demo
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb init
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb add learn "Build a Rust app"
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb tag learn rust
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb checkpoint
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb done learn
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb list
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb undo 1
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb history
cargo run -p weekend-tasks -- --db /tmp/my-tasks.wdb compact
```

Choose a new path for `init`; creation never overwrites. The default path is
`tasks.wdb` in the working directory. `help` lists all commands. Persistence and
compaction use the macOS or Linux (x86_64/aarch64) file adapter; the in-memory
demo runs on other platforms too. Build once with `cargo build --release -p weekend-tasks` and use
`target/release/weekend-tasks` directly for everyday commands.

Checkpoints retain all database roots. `undo` restores task rows and their
indexes from a selected checkpoint and saves a redo checkpoint first. It keeps
the ID counter advancing, so delete and undo never reuse task identities.
Release unwanted checkpoints using `forget ID`; at most 64 may be retained.

`upgrade` is an explicit migration from the legacy four-field task model to
the current model with notes and priority. New databases register both schema
descriptors and insert current records. The demo creates genuine legacy records,
migrates them, updates through an older model, and exercises undo/redo. Listing
projects older records; mutation through the newer model requests `upgrade`.

Compaction opens the closed file independently. On interruption, it can leave
`.wdb-compact-*.tmp` recovery files; reopen/verify the canonical database before
removing leftovers. Concurrent writers in another process receive `Busy`.
