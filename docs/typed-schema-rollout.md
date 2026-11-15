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

## Reproducing the Windows NTFS profile

On native x64 Windows with MSVC and a fixed NTFS volume, use a checkout with
both commits available. The hosted gate fetches the frozen commits explicitly
when checkout is shallow. Run with Rust 1.99.1 and a fresh output directory.

```powershell
cargo fetch --locked
cargo xtask compatibility --profile typed --historical 1ca5e5ab63a2772f679c7186e2d84fba3ac0a534 --current 6cc8ef3e60dcf406a272e3f176e83d3c5c83190a --output target/windows-typed-schema
```

The runner disables Git checkout line conversion for the archives and compares
each extracted lockfile's Git object ID against the frozen commit. Cargo builds
offline with the verified lockfiles. Native jobs own each child tree and record
the exit code, captured streams and confirmed cleanup. The proof includes both
consumers, both identity executables, their archives and lockfiles, and the final
database files.

### Windows result on 2026-11-15

All 12 phases passed on hosted native x64 Windows with fixed NTFS and Rust
1.99.1 MSVC. The controller ran from PR head `5818ba5` at test merge
`a7db539e3ed204453f61c0631e1f77182130a673`. The downloaded evidence matched
all 75 listed artifact hashes and all 32 command records had verified
child-tree cleanup. Both source archives matched the macOS rerun byte for byte.
Both lockfiles matched their original Git blobs and all four executables
matched their recorded hashes and native x64 PE headers.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43730992567/job/132863152882)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43730992567/artifacts/14167899717).

The staged migration exited 70. Registration, interruption and the fresh
reopen produced the same database hash. The resumed migration preserved the
legacy edit, the score and opaque field 99. The two v2 rows and retained
checkpoint 1 reopened with consistent indexes. The rollback file matched the
pre-upgrade backup before the historical binary committed and reopened a new
v1 insert. The upgraded file and backup hashes held throughout rollback.

| Windows evidence | SHA-256 |
|---|---|
| Downloaded ZIP | `721091af06ca71615d2d6b83ffc9f3e62c07593dc0fd47a62f1d7327916d7f7e` |
| `evidence.json` | `4aa2fa1dd5f1f483aac1937e358665efb71b7649d526465a052e2d12bb3760ec` |
| Historical lockfile | `353ab69da0e2a7ecc9094126773d277a7be1ef64d2f829d9d7be0b9488113d28` |
| Successor lockfile | `62c1797a7e32f6a17f213edcf2590d748dca07e763caddfbe3c3f662e8ccd8c0` |
| After registration, interruption and fresh reopen | `c821233ecd777c198c7daa891ff6c7fe439dc9e51b8d5b1b9b1b2cc64541aafb` |

The first Windows run passed the 12 behavioral phases. Its artifact audit
found CRLF conversion inside the Git archives. The verified result above ran
after disabling that conversion and adding a lockfile blob check. The macOS
rerun passed with `core.autocrlf=true` in the environment as a regression
control. Its `evidence.json` is
`e86c4f18aba130508e820417f94b7a39041c7f8be775fbfc56432e5eda0a7ef2`.
