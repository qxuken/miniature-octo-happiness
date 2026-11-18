# Locked dependency review on 2026-11-18

All fourteen tracked lockfiles at
`3de695f70284ce562dcc6e03343a3f029f6b0abb` pass a fresh cargo-audit 0.22.2
scan against frozen RustSec database
[`233155b4025b854c68a4b199c0e32d7fd53291a2`](https://github.com/RustSec/advisory-db/tree/233155b4025b854c68a4b199c0e32d7fd53291a2).
The scan finishes at `2026-11-18T00:48:56.329Z`. Every command exits zero
with zero reported vulnerabilities and warnings. This is a new database scan,
including all 1,527 parsed advisories; the
[previous review](locked-dependency-review-3db5bcd.md) parsed 1,509.

Every invocation uses the explicit frozen database path, `--no-fetch`,
`--json`, `--deny warnings` and an exact copied lockfile. There are no ignored
advisories, severity thresholds, OS/architecture filters, or disabled
informational-warning categories. The raw settings retain `unmaintained`,
`unsound` and `notice`. The shared global advisory checkout is not used or
modified. Source lockfiles match the selected Git blobs before and after the
scan; their read-only copies retain those same bytes.

| Lockfile | Reported dependency count | Vulnerabilities / warnings |
|---|---:|---:|
| `Cargo.lock` | 117 | 0 / 0 |
| `crates/weekend-db-typed/tests/renamed/Cargo.lock` | 8 | 0 / 0 |
| `examples/rust_package/Cargo.lock` | 8 | 0 / 0 |
| `fuzz/Cargo.lock` | 26 | 0 / 0 |
| `probe/rust/Cargo.lock` | 1 | 0 / 0 |
| `tools/atomic-model/Cargo.lock` | 31 | 0 / 0 |
| `tools/decision-inventory/Cargo.lock` | 23 | 0 / 0 |
| `tools/incident-recovery/Cargo.lock` | 2 | 0 / 0 |
| `tools/monthly-workload/Cargo.lock` | 8 | 0 / 0 |
| `tools/replica-progress-fixture/Cargo.lock` | 60 | 0 / 0 |
| `tools/resource-envelope/Cargo.lock` | 5 | 0 / 0 |
| `tools/serialization-scale/Cargo.lock` | 2 | 0 / 0 |
| `tools/soak/Cargo.lock` | 2 | 0 / 0 |
| `tools/sqlite-baseline/Cargo.lock` | 23 | 0 / 0 |

Counts overlap across lockfiles and are not a distinct-package total. All
fourteen lockfiles are also byte-identical to the previous reviewed source
`3db5bcdd20de297b8b5424c1fbc2d1e316e2145d`. That comparison carries the old
lock identity; it does not replace this fresh scan or approve source/licenses.

## Frozen inputs and restoration

The tool is selected through Mise with `cargo:cargo-audit@0.22.2`. Original
and retained scanner binaries match SHA-256
`eed9c84fbe41ba0ba2156c2c8538ead83b0a82806aff6f0563401efae8db159c`,
also matching the scanner identity recorded in the earlier review. No project
tool configuration, dependencies, lockfiles or Cargo publication settings are
changed. All 21 manifests retain `publish = false`.

The isolated public database clone and its Git archive identify the same
commit. All 1,546 extracted files match its Git blobs and remain unchanged
after scanning. Regenerating the archive with explicit `core.autocrlf=false`
reproduces its SHA-256. The scanner reports null database Git/time fields with
this extracted database and `--no-fetch`; the separate Git/blob/archive checks
establish provenance.

| Evidence | SHA-256 |
|---|---|
| Database Git archive | `1c009d087b12b2f195936f4722731319ece13588b9257a3366be68f204a045b3` |
| `evidence.json` | `e6d7c33a2da9e9e5f587f0f2f26b34eac9f3119329eccc146ea6d5609772b322` |
| Protected proof archive | `0370e48a597609175c882f7455655b3c6325309978ba796f8b490478c60718ca` |

The proof is retained privately as
`target/archived-proof/locked-dependencies-3de695f-20261118.tar.zst`.
Fresh extraction verifies all 1,595 indexed payloads, every raw lockfile and
database file, scanner hash, report settings and result counts. All fourteen
scans are then rerun with the extracted tool, database and lockfiles: every
stdout/stderr digest matches its original report. The restore audit is under
`target/r6/restore-validation-3de695f/audit.json`. Local archive protection and
successful extraction do not establish durable off-machine custody.

## Repeat the matching step

Start with a newly owned database snapshot and verify its exact commit/archive
identity before scanning. Use one explicit `--file` for each lockfile in the
table, preserving its raw JSON, stderr and exit status.

```sh
mise exec cargo:cargo-audit@0.22.2 -- cargo audit \
  --db /absolute/frozen/advisory-db --no-fetch \
  --file Cargo.lock --json --deny warnings
```

The manifest retains complete arguments, original lock hashes, tool/database
identities, bounded process results and links to all fourteen raw reports. Each scan has a
30-second deadline and one-MiB stream limit in the retained one-off driver.

## Remaining acceptance

This is matching against the named database, not a dependency source/unsafe
review or a guarantee about undisclosed defects. Registry/yanked metadata is
not independently snapshotted. Final candidate locks and every promised
distribution's license/notice/source attribution must still be reconciled,
with dated advisory inputs and reviewed exceptions. R6 remains open for those
release/distribution actions and owner approval; R7 retains proof custody and
the eventual immutable release freeze.
