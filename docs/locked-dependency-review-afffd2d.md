# Dependency and package-notice review, 2026-11-25

All fourteen tracked Cargo lockfiles at `afffd2db429136873cdff990e4def5af55cae74a`
passed cargo-audit 0.22.2 with 1,568 parsed advisories and zero reported
vulnerabilities or warnings. The scanner used the explicit frozen database,
`--no-fetch --json --deny warnings`, no ignored advisories, and no OS,
architecture or severity filter. The restored scanner reproduced the stdout
and stderr of all fourteen invocations. All 1,587 restored advisory files
matched the database's Git blob IDs. [Frozen RustSec database](https://github.com/RustSec/advisory-db/tree/b9b8a3c2cb1f2a3c76ab17a551193568c9f8b7a4)

The lockfiles reference 135 distinct crates.io names and 142 distinct
name/version pairs. Every version existed in the frozen registry index.
All 142 had `yanked: false`. For every occurrence, the lockfile checksum,
registry checksum and SHA-256 of the cached crate archive agreed.
The extracted proof reproduced each index-file and crate-file digest.
[Frozen crates.io index](https://github.com/rust-lang/crates.io-index/tree/e03e62c9b9a2d90b6ea03b7be4721904b550aa47)

The lockfile contents also match the [four-platform delivery checkpoint](delivery-checkpoint-49ddab5.md).
Across its 16 packages the review checked 132 compiler-artifact package
entries and compared 272 delivered notice files byte for byte with their
crate archive or Git source. These counts include repeated dependencies
across packages and platforms. Each Cargo manifest hash and declared license
matched, and all 16 inventories recorded complete attribution inputs with
zero outstanding inventory items. The comparison covers the Cargo package
entries. Standard-library and Odin materials have separate inventories.
Legal approval, support commitments and review of the eventual release
source remain open.

The local archive `dependencies-afffd2d.tar.gz` contains the lockfiles,
scanner, advisory snapshot and Git tree list, registry records, crate
archives, raw scan reports and package-notice review summary. Its SHA-256
is `e7d2774886a087118c8071886d389c4e0e0fa351296e96e10e9e216918815c10`.
