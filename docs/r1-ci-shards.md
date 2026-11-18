# Bounded R1 mutation shards

PR #10 head `5c4aa6f58a59408e129b99d6cf83d52d1699e367` exhausted the
macOS source job's 20-minute limit. Its debug campaign finished; release was
still detecting controls when the runner cancelled the job. This is incomplete
qualification, not a passing release campaign or an infrastructure-capacity
retry. The original artifact is `14405114613`, SHA-256
`cf9cd9d50c39284fccc6ae79dbb8322d314378630bba411d44006e1a5128b73e`.

The log starts at 06:43:35 UTC on 2026-11-18. Debug starts at 06:45:14,
release at 06:51:38, and cancellation occurs at 07:03:47. No unexpected test failure is
reported. The last completed release control is `r1-metadata-different-witness`;
the six final root-selection controls did not complete in release.
The failed source retains its own scope; the frozen `795ea8a` local proofs are
not substituted for this hosted run.

## Complete selection across bounded jobs

The Unix runner now supports `--shard-index I --shard-count N`. Prefix filtering
happens first, preserving registry order. A case at position P belongs to shard
`P % N`, with zero-based indices. Defaults 0/1 preserve the complete selection.
Zero counts, invalid indices, empty prefix selections and more shards than
matching cases reject before acquiring output or launching a campaign.

Each campaign records the prefixes, shard index/count, complete matching case
names and selected names in `evidence.json`. Its results qualify only that
selected set. All expected shards must pass, with identical source/fixture and
matching-name lists, no duplicate selected names, and their union equal to the
entire filtered registry. A partial set cannot qualify the complete R1 batch.

CI runs indices 0, 1 and 2 on macOS and Linux, each in debug and release. The
62 R1 controls divide into 21, 21 and 20 cases per profile; none are omitted.
Windows keeps one inventory/reconciliation job and its separate two-control
native RNG campaign. Nondefault sharding of that native Windows campaign is
rejected explicitly. Artifact names have `-shard-I` suffixes so concurrent jobs
cannot overwrite one another. Each source job retains its original 20-minute
limit. Individual command deadlines, output caps, nonempty baselines, genuine
failed-test requirements, source restoration and verified retirement are
unchanged.

```sh
cargo +1.99.1 xtask mutations --case r1- --list
cargo +1.99.1 xtask mutations --case r1- --shard-index 0 --shard-count 3 --toolchain 1.99.1 --output target/r1-shard-0-debug-new
cargo +1.99.1 xtask mutations --case r1- --shard-index 0 --shard-count 3 --release --toolchain 1.99.1 --output target/r1-shard-0-release-new
```

Repeat with indices 1 and 2. The registry and mutation expressions are unchanged.
The change is qualification tooling; database bytes, durability barriers and
separate local/follower acknowledgment contracts are unchanged. Tooling source
and macro obligations remain unqualified in the R1 inventory. This change does
not establish compiler MC/DC or close R1–R7. All 21 manifests retain
`publish = false`; crates remain unpublished.

## Regression checks

Pure selection tests require complete, disjoint, deterministic shards with
unchanged case fields and registry order. The real CLI test compares its
unsharded list with every registered R1 name, then checks the three shards and
rejects invalid indices/counts without creating outputs. This covers argument
wiring as well as the selection helper. The former CLI rejects the shard flags;
the new CLI passes these tests in both profiles. Hosted execution results retain
their actual source and are recorded after completion.

At frozen `45381c2fe93397422f0f5a5b0e6ca8d15ab097e7`, macOS arm64/APFS
passes **242 xtask tests / two explicit ignores per profile**, strict all-target
Clippy, formatting, and the Windows MSVC test cross-check. The latter compiles
the Windows path; it does not qualify native execution. Mise-pinned
`actionlint@1.7.12` accepts the workflow.

Eight real sampled campaigns exercise unsharded selection and indices 0/1/2
in debug/release. They use `r1-codec-align-cap`,
`r1-inventory-historical-gap` and `r1-metadata-slot-gap`: a root library test,
a standalone binary target and an integration target. Each profile's default
selection catches all three; the three shards catch one each, forming the same
complete union. All **80 command retirements** verify, all baselines are
nonempty, and each mutant compiles and fails a real test with exit 101.
Independent audits compare all **780 copied/restored files** and **347 declared
inputs per campaign** to frozen Git, plus exact selection metadata and streams.
The local reports and recipe remain under `target/ci-r1-shards/`; no complete
62-control or Linux/Windows execution is inferred from this three-case sample.
The hosted results below verify the full selection at the subsequent PR head.

## Hosted results at e5ca577

PR #10 at `e5ca577d2c4273d4df14732f798866c86e020e09` passed
[all 26 engine checks](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44263429647)
and [bounded fuzzing](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44263429618).
The tested merge `9a838c67f202fb53f62a484d5b3080e6b04e184c` and the PR
head have tree `2ac24d9a23370fb1828b45787bc652287bd32560`.

The seven uploaded source artifacts verified against the published ZIP digests.
The Linux and macOS selections form disjoint 21 / 21 / 20 case unions in both
profiles. Across twelve campaigns, 940 command retirements verified and all
62 controls per platform and profile compiled and failed real tests.
Each campaign's recorded 780 copied-source hashes and 347 declared input
hashes match the tested tree. The hosted archives exclude the copied source
directories. All seven reconciliation reports match byte-for-byte, including
the Windows reconciliation job. Local audit files are in
`target/r1/hosted-e5ca577/`, with GitHub artifact IDs and digests.
