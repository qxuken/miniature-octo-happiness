# Mixed-version typed schema rollout and rollback

The typed compatibility profile builds independent release executables from
two frozen Git archives. The historical application compiles only the version
1 model, with a unique slug. The successor compiles only the version 2 model,
which adds a defaulted score and keeps the same slug index. Each archive
uses its original lockfile. The proof binds both archives, application
fixtures, executed binaries and copied lockfiles.

```sh
cargo xtask compatibility --profile typed \
  --historical 1ca5e5a --current 6cc8ef3 \
  --output target/typed-schema-rollout
```

| Phase | Required outcome |
|---|---|
| Seed and back up | The historical binary inserts two records, supplies an opaque field 99, and retains checkpoint 1. The backup leaves the source bytes intact. |
| Register version 2 | Registration advances the revision once. Both existing records still have version 1, and the new reader projects score 0 from physically missing scores. An ordinary v2 edit requires migration. |
| Interrupt a staged batch | A v2 process stages an edit and its unique slug index, observes that staged row through the writer, then exits with code 70 before the transaction can commit. A new process reopens byte-identical current and retained state with no staged edit. |
| Abort and retry | An explicit batch stages the first conversion before a nonexistent ID aborts it. The revision and both indexes stay consistent. The retried batch commits one v2 row and leaves the second at v1. |
| Use the historical binary | The old binary reads both versions, rejects inserting a new v1 row, and edits the migrated slug. The successor sees the edited slug, version 2, score 8 and field 99. The replaced slug is absent from the index. |
| Complete and reopen | A batch over both records skips conversion of the row at v2 and migrates the second row to score 9. A fresh successor process reopens both v2 rows and verifies the original records, field 99 and indexes in checkpoint 1. |
| Roll back from backup | The successor restores the pre-upgrade backup to a fresh destination. The historical binary opens it, verifies the two v1 rows and checkpoint, then makes and reopens a new indexed v1 insert. The upgraded file and backup hashes stay fixed. |

The deployment pattern is to keep the pre-upgrade backup, register the additive
schema, then run explicit resumable migrations. Once the catalog has advanced,
any old client still responsible for inserts must be replaced or those inserts
must be routed to the successor. Rollback restores the pre-upgrade revision in
a fresh file. Writes made after that backup stay in the upgraded source and
require reconciliation if they are needed after cutover.

The native file lease still allows one file-owning process at a time. This
exercise covers sequential binaries, one additive default and a fixed unique
index layout. The separately documented file recovery tests cover interruptions
inside durable publication.

## Recorded qualification

Both builds passed all 12 phases on macOS/APFS and Docker Linux/ext4 with
Rust 1.99.1. An independent checksum pass verified all 93 listed artifacts
per report, and both reports bind the same 283 controller source inputs.
The Docker container and volume were confirmed absent.

| Platform | Evidence SHA-256 |
|---|---|
| macOS arm64 | `146676d85f2db08d7990f270cd5470fcc5c0d3124c297932186939aa05d7dcd7` |
| Docker Linux arm64 | `2aa75801784067991178570d18cb0aa049a783aac8c1b074565c3563564e3206` |

Private proofs are under `target/xtask-typed-compat-mac-r2` and
`target/xtask-typed-compat-linux/proof`. The proof binds the dirty controller
overlay separately from the two clean application archives.
