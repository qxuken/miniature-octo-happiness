# Locked dependency database review at 3db5bcd

All fourteen tracked lockfiles at source
`3db5bcdd20de297b8b5424c1fbc2d1e316e2145d` pass Mise-pinned cargo-audit 0.22.2
against an independently frozen RustSec database. Each command exits zero with
zero reported vulnerabilities or warnings. Commands use `--no-fetch`, the
explicit frozen `--db` path, `--deny warnings` and the exact `--file`; there
are no ignored advisories, architecture/OS filters or severity thresholds.
Informational warnings include unmaintained, unsound and notice categories.
Later test/document changes can carry this evidence only while their lockfile
bytes match these exact retained digests.

The isolated public database clone pins
[`02e2517f457a359fc6d3440ea4560c8ea188fee8`](https://github.com/RustSec/advisory-db/tree/02e2517f457a359fc6d3440ea4560c8ea188fee8).
Its Git archive SHA-256 is
`4c0b0c2ac9494ba27801552e5f87c1f5d43214810f6cd39c61b4e8f0105d6488`.
All 1,528 extracted files are indexed and unchanged after scanning. The tool
parses 1,509 advisories: 1,489 RUSTSEC-named files and twenty CVE-named Rust
advisories. These counts differ from a filename filter that selects only
RUSTSEC names. The shared global advisory checkout is neither used nor modified.

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

Counts overlap across lockfiles; their sum is not a distinct-package total.
Exact lockfile bytes, raw JSON/stderr, command arguments and exit status are
retained. Original and copied audit binaries match SHA-256
`eed9c84fbe41ba0ba2156c2c8538ead83b0a82806aff6f0563401efae8db159c`.
With `--no-fetch`, cargo-audit reports null Git/time fields; the independently
regenerated Git archive and complete file inventory bind the database instead.

The preliminary derived summary used an underscore instead of the raw JSON's
`dependency-count` field and counted only RUSTSEC filenames. Its raw successful
audits were unaffected. The final aggregation is corrected from those reports;
the preliminary archive is retained separately and excluded from final evidence.
Registry/yanked metadata is not independently snapshotted. This is database
matching, not dependency source/unsafe review, a compiler/OS or undisclosed-defect
guarantee, or license approval. Final candidate package attribution and support
decisions remain separate acceptance steps.

The protected archive is
`target/archived-proof/locked-dependencies-3db5bcd.tar.zst`, SHA-256
`f2b1d87c289f9f807ac439160cc8d831495b2a763e4b1f5e3138c82bb5c52acc`.
Manifest `evidence.json` SHA-256 is
`38aebb3c3b840fd12cc8b4ca0ef13a862eed986a75d0d582437cdc2aee9ef2f6`.
Root fresh restoration checks all 1,604 indexed payloads, fourteen scan results
and raw lockfiles, all database files and the copied tool. Independent review
also matches database files to Git blobs, regenerates the same Git archive,
checks tool hashes and filters, and finds no discrepancy. No audits are rerun
or evidence changed by that review.
