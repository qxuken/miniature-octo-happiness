# Study upgrade protocol source admission

The Study CLI upgrade between `fb80989` and `1f3f42a` stopped before building
either peer. The source guard compared complete protocol files. Both
files have the same production prefix. All of the changes are inside the
inline `#[cfg(test)]` module, and the remaining guarded runtime files match.
The ordered, reviewed file pair now qualifies for the existing upgrade
scenario. The guard retains the byte comparison for other file pairs.

| Input | SHA-256 |
| --- | --- |
| Historical protocol file | `3a11026d333bf97ff105748dd38e98023720752bced98b2b12af2a251b55d73e` |
| Candidate protocol file | `4517dfe5c848f7033f8b7c80c5a9c0a85b771f1fb6cd9fa28495e7d835e2518a` |
| Byte-identical prefix before the test module | `c365329f9d6eb404d2a031f96ef46c71702a94ed4340f27e0b1578a4fc8a2bfd` |

| Whole files match | Ordered reviewed pair | Admit |
| --- | --- | --- |
| true | false | true |
| false | true | true |
| false | false | false |

The unchanged-file and exact-pair cases each change the decision relative to
the rejected case. The tests also change each side of the approved pair
independently and reverse the pair. The regression failed with whole-file
matching alone and passes with the audited pair. Three named mutation
controls remove the approved transition or broaden it to accept a change on
either side.

| Source | SHA-256 |
| --- | --- |
| `xtask/src/study_upgrade.rs` | `7624ba2e3597701b188598d0619a48560c4c512c12c0ff84838cedaeb7734782` |
