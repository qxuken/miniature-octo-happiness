# Study roster pair seeding and publication

Staging creates the two destination databases, then delegates enrollment and
seeding to the same helper used by the simulator tests. The fixed roster
supplies actor IDs directly for the two known slots. Both enrollments finish
before actor A publishes the visible source state. Actor B starts with the
default record and receives the seeded state in the usual exchange.

The seed check compares the reconstructed visible projection with the source
projection. An unchanged projection passes. Adding one completed lesson
returns `Corrupt`. Separate seeder cases start from typed records with an
exhausted plan, weekly goal or session limit sequence. The same input
succeeds from a fresh record and returns `Exhausted` at the affected
field with the preexisting field value and sequence intact.

The simulator calibrates the first I/O after enrollment and the final sync
of a successful seed. Each trial starts from two fresh stores. Reopening
checks the durable revision, every seeded field or default record, and
the absence of retained roots.

| Injected event | Helper result | A reopens | B reopens |
| --- | --- | --- | --- |
| First I/O in A enrollment | `Io` | Revision 0, no replica | Revision 0, no replica |
| First I/O in B enrollment | `Io` | Revision 2, default record | Revision 0, no replica |
| First I/O in A seed publication | `Io` | Revision 2, default record | Revision 2, default record |
| Final seed sync returns an error after persisting all writes | `OutcomeUnknown` | Revision 3, seeded record | Revision 2, default record |

The native staging path computes and emits its receipt after the helper
returns a verified seed. The earlier destination and report-writer
regressions continue to check partial creation and receipt output.

## Scoped source decisions

| Source | SHA-256 |
| --- | --- |
| `apps/study-peer/src/native/roster.rs` | `2e832210576c1b352b20613ec3fadf7a6f9a018a7fb552c97dd198ac4ed2417e` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:20:36– | 592..593 | try_propagation | parse_id | Every accepted input has exactly 32 ASCII hexadecimal digits. Its numerical value is at most `2^128 - 1`. The standard parser handles both cases of hexadecimal letters. |
| E | study-peer::study-peer | apps/study-peer/src/native/roster.rs:24:17– | 709..714 | match | enrollment | The policy fixture checks assignment of the first actor for "a" and the second for "b". An empty selector returns the actor diagnostic. |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:73:31– | 2114..2115 | try_propagation | read | The add-wins iterator ranges over exactly 64 IDs. Every yielded ID fits the GrowSet bit index. |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:102:28– | 3045..3046 | try_propagation | verify_seed | Projection reads the fixed StudyProgress representation. The planned-ID conversion is bounded by the 64-element domain. |
| E | study-peer::study-peer | apps/study-peer/src/native/roster.rs:148:9– | 4385..4388 | for_iteration | line | The independent receipt preimage fixture includes both ordered actor IDs and checks the complete hash. |
| E | study-peer::study-peer | apps/study-peer/src/native/roster.rs:151:9– | 4464..4467 | for_iteration | line | The independent preimage includes both different store IDs. Checkpoint mode includes both revisions, and stage mode omits revisions. |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:158:35– | 4767..4768 | try_propagation | line | The fixed StudyProgress buffer has the sum of the five field widths. Its generated encoder splits every field to its codec size. These concrete encoders return errors only for a mismatched slice length. |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:201:68– | 5932..5933 | try_propagation | checkpoint | Both sources opened through the native byte-backed file adapter. Their persistent identities are available without I/O, and the fixed-width encoder admits the typed state. |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:229:39– | 6860..6861 | try_propagation | stage | The old pair supplies a decoded StudyProgress. Projection only inserts IDs in the bounded planned set. |
| D | study-peer::study-peer | apps/study-peer/src/native/roster.rs:239:57– | 7277..7278 | try_propagation | stage | The native stage uses the same helper after creating both files. The helper tests enrollment and publication errors against simulated storage and verifies each reopened state. Native file creation and reopen are covered separately. |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:242:21– | 7398..7399 | try_propagation | stage | The optional staged receipt uses the two newly created byte-backed identities and fixed-width encoder. The guarded and unguarded native cases cover both option states. |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:266:38– | 8245..8246 | try_propagation | seed_pair | After a successful update, the fresh private A store holds a locally encoded row in the expected collection. Reading the published snapshot uses cached state and the same codec. The final-flush error exits before this read. |
| I | study-peer::study-peer | apps/study-peer/src/native/roster.rs:285:39– | 8849..8850 | try_propagation | verify | Agreed sources supply a decoded StudyProgress. The fixed-domain planned set guarantees the same projection invariant. |

## Verification

Frozen source `cacdacbe69246a90037c7ffea2e1c105b79e52fb` passed with Rust
1.100.0 on arm64 macOS and offline arm64 Docker Linux on ext4. Each debug
and release configuration passed all 49 Study tests and the 14 core CRDT
tests. Strict Clippy passed on both hosts.

All 72 roster mutations were detected in each configuration. Fresh
extraction verified the exact tests and process outcomes across 940
command reports, including the seven new controls. Each isolated source
copy matched all 948 files in the frozen Git tree. The saved macOS
compiler profile recorded one true and 15 false evaluations of the
visible-projection mismatch guard. LLVM export reproduced byte for byte
with the extracted executable and profile. The
[archive checksums](study-seed-pair-cacdacb.sha256) bind the source,
outputs, tool runners and replay inputs.

The refactor removed the two fallible literal slot lookups and added
one delegated seed result. The roster inventory contains 79 records with
seven new named controls and 13 new scoped dispositions. Reconciliation
leaves 529 runtime gaps across the project and 7,514 tooling gaps.
The late-sync schedules qualify simulated storage. Native late-sync
injection remains open.

```sh
export RUSTUP_TOOLCHAIN=1.100.0
cargo test --offline --locked --all-features -p study-peer
cargo test --offline --locked --all-features -p weekend-db --test crdt
cargo xtask mutations --toolchain 1.100.0 \
  --case r1-study-roster- --case r1-study-receipt- --case r1-study-pair-io- \
  --case r1-study-seeding- --case r1-study-output- --case r1-study-seed-pair- \
  --output "$OUTPUT/debug"
```

Repeat all three commands with `--release` and a fresh mutation output path.
