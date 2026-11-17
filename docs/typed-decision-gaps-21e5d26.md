# Typed write guard outcome qualification

This directed check uses archived commit
`21e5d2638c24c6254ced35d0077c55bb30a287b2` with exactly one test-file overlay:
`crates/weekend-db-typed/tests/runtime_decisions.rs`, SHA-256
`81adbbd3942b414f9ad080348e866ac6bc94031e2ca0ffd01528069f2f0fcba5`.
Every other tracked archived file was compared byte-for-byte with the commit.
Production `crates/weekend-db-typed/src/store.rs` is unchanged, SHA-256
`2157407964fc4ff889737efb29f21270fe6ae2985db1288aac5d2dc740c1f7fa`.

## Instrumentation and provenance

On macOS arm64, Rust `nightly-2026-10-09` (1.101.0-nightly,
`a30aa9064df6a504b6a323c6884a2f4cfbfff666`, LLVM 23.1.3) built only the
`weekend-db-typed` integration test `runtime_decisions`, with all features and
`-C instrument-coverage -Z coverage-options=condition`. The build used offline,
locked Cargo and `CARGO_INCREMENTAL=0`. All build-time profiles were removed
before executing the single resulting test binary. **16 tests passed**; exactly
one runtime raw profile was produced.

That profile was merged using the matching `llvm-profdata`, then used with only
its own executable for `llvm-cov report`, `export`, and HTML `show`. All three
commands returned zero with **empty stderr**. The normal coverage-driver filename
exclusions removed tests/examples/tools and toolchain/registry sources from the
report; no typed implementation source or selected guard was excluded.

This avoids the previously diagnosed cross-executable profile-name/hash
collisions. It does not repair or validate the rejected full-workspace aggregate.
The measured scope is this one test executable and its linked implementations;
no whole-project percentage or compiler MC/DC result is claimed.

Local evidence is under `target/typed-guards-21e5d26/`: `evidence.json` records
commands, compiler, source/overlay hashes, executable/profile hashes and empty
LLVM diagnostics; `tests.log`, `export.stdout`, `report.stdout`, and `html/`
retain the observations. These paths are local evidence, not a permanent
artifact-retention promise.

## Observed source outcomes

Counts below come from exported LLVM branch records, indexed by exact source
span and inspected per function instantiation before any source-level summary.
Unused instantiations with zero counts are retained in the export.

| Production span | Predicate/outcome | Observed counts | Directed control |
|---|---|---|---|
| `store.rs:628:12–23` | `self.failed`: true / false | **1 / 1 in the same follow-up callback instantiation** | `caught_callback_panic_closes_the_write_before_another_callback`: the same copied closure succeeds on a clean write, then is refused after a caught callback panic; it is not invoked after poisoning |
| `store.rs:696:16–64` | Existing primary record: present / absent | **1 / 30** in the `Row` insert instantiation | `duplicate_primary_insert_aborts_prior_work_without_changing_unique_claims`: duplicate primary rejected after earlier draft work, normal inserts admitted |
| `store.rs:888:13–22` | `Some(raw)` match / absent record | **4 / 1** in `remove_raw<Row>` | `deleting_an_absent_primary_leaves_existing_unique_entries_alone`, paired with deletion of present records |
| `store.rs:893:16–85` | Unique claim does not equal expected ID: true / false | **3 / 1** in `remove_raw<Row>` | `deleting_with_a_missing_wrong_or_malformed_claim_aborts_the_entire_draft`: missing claim, wrong owner and malformed value rejected; correct claim accepted |

Across all executed `checked` instantiations at line 628, the source-location
counts sum to 1 true and 46 false. The important independence evidence is the
**same-instantiation 1/1 pair**, not that sum or a union of unrelated generic
instantiations. The other three spans above each have one executed `Row`
instantiation; the export also includes unused zero-count variants.

These tests check observable rollback and state preservation: unpublished
staged records/index entries, current revision, original unique ownership,
retained reads, and fresh-write usability. They test errors through public APIs;
no production test seam or guard mutation was needed for this measurement.
Mutation testing and stable debug/release results are recorded separately in the
[typed runtime decision ledger](decisions/typed-runtime.md).

## Fresh-store and format admission follow-up

A separate clean archive of the same commit, with only the new
`crates/weekend-db-typed/tests/catalog_admission.rs` overlaid, qualified two
additional public guards. Test SHA-256:
`d309488423e6705daefc196e657419dcb51ce7497dd0858263d5d0a297ff1088`.
Production `store.rs` retains the hash above. The earlier 16-test executable,
profiles and source snapshot remain separate and unchanged.

Independent review found the two tests exercise meaningful state oracles:
`initialization_requires_revision_zero_and_preserves_nonfresh_state` admits a
fresh store with the expected three collections, rejects a populated revision-2
store, compares its entire simulated durable image, then reopens and checks the
caller's data. The byte comparison can catch a removed early guard even if a
later collection-ID check returns the same error after mutating the image.
`typed_open_requires_the_exact_format_marker_without_changing_images` covers no
catalog, no marker, unsigned 0/2, wrong-kind bytes, and exact unsigned 1. Every
case compares the complete durable image before/after admission.

Both tests passed in debug and release on isolated Linux aarch64 using the same
immutable c9 tools image identified in the local qualification report, Rust
1.99.1, offline/locked Cargo and a container-private target. Strict targeted
Clippy passed. `/tmp/catalog-admission-linux.json` binds the exact archive,
overlay, immutable image, commands and exit 0; `/tmp/catalog-admission-linux.log`
contains the results. These tests use simulated durable images on Linux; they
do not claim native filesystem interruption coverage.

The same dated nightly and single-executable procedure above ran both tests on
macOS with condition instrumentation. Exactly one runtime raw profile was
produced after removing build-time profiles; report/export/show all returned
zero with empty stderr. Evidence is separate at
`target/catalog-guards-21e5d26/evidence.json`, with commands, binary/profile hashes,
selected branch records and reports.

| Production span | Predicate | True / false counts |
|---|---|---|
| `store.rs:230:12–41` | Current revision is nonzero | **1 / 1** |
| `store.rs:247:16–251:27` | Decoded marker is not exactly `Some(1)` | **5 / 1** |

These are individual predicate outcome pairs, not a new aggregate coverage or
compiler MC/DC claim. Stable macOS and directed mutation evidence remains in
the central typed runtime decision ledger.
