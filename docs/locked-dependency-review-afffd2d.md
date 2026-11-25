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

## Standard-library and Odin notices

The four Rust 1.100.0 compiler archives for the delivery checkpoint matched
the SHA-256 entries in the upstream channel manifest. Each contained the
same 1,512,515-byte `COPYRIGHT-library.html`. All 16 delivered copies matched
it byte for byte (SHA-256 `626556344c12679d7cab2c1aa2ce80de01937b62c2c88c62f0650a469be18799`).
[Upstream Rust 1.100.0 channel manifest](https://static.rust-lang.org/dist/channel-rust-1.100.0.toml)

For Linux arm64, Linux x64 and Windows x64, each Odin release archive matched
GitHub's release-asset digest. Its compiler executable matched the compiler
hash recorded in all four package manifests for that platform. Its license
matched the four delivered copies. The Windows archive uses CRLF line endings
and the Windows packages preserved them. [Odin dev-2026-10 release](https://github.com/odin-lang/Odin/releases/tag/dev-2026-10)

The macOS lane used a locally built Odin compiler. Its executable hash matched
all four recorded compiler hashes, and the four delivered notices matched the
license from its recorded source commit. Rebuilding that compiler from source
remains outside this evidence. [macOS Odin source license](https://github.com/odin-lang/Odin/blob/a22aa1984f008545f0bd8bf9ecce5066717563ef/LICENSE)

The local archive `toolchain-notices-49ddab5.tar` has SHA-256
`541bfc0564a357d8d2735c7f8939efeb120316578699e9dc8c98943effe0e16a`.
Rereading it verified seven upstream archive digests, the local compiler,
and the notice hashes across all sixteen package profiles. Legal approval and
rechecking the final release's toolchains remain open.
