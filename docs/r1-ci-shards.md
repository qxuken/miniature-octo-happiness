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

For this 62-control batch, CI ran indices 0, 1 and 2 on macOS and Linux, each
in debug and release. The controls divided into 21, 21 and 20 cases per profile.
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

## Catalog shard count

At PR #12 head `a1a95a1`, a hosted macOS job with 31 controls per profile
took approximately 15 minutes. The catalog batch adds 27 controls, which
would put 40 or 41 on each of three jobs. Four shards keep each hosted Unix
job at 30 or 31 controls in each profile. The 20-minute job limit and
120-second command deadline stay in force. Windows still runs one
reconciliation job and the separate native RNG campaign.

Mise-pinned actionlint 1.7.12 accepts the matrix. The actual CLI listing in
both profiles verifies all 122 names across indices 0 through 3, with no
duplicates or omissions. The catalog's frozen local run uses its original
three-shard layout and provides evidence for that layout.

## DATA directory shard count

The longest completed [PR #12 macOS shard](https://github.com/qxuken/miniature-octo-happiness/actions/runs/44277108233/job/134707930309)
ran for about 15 minutes with 32 controls per profile. The DATA directory batch brings the total
to 151. Five hosted Unix shards select 31, 30, 30, 30 and 30 cases per
profile. The 20-minute job limit, 120-second command deadline and single
Windows reconciliation job are preserved.

## VALUE reader shard count

The VALUE batch raises the R1 registry from 151 to 186 controls. Six hosted
Unix shards run 31 controls each per profile, preserving the previously
qualified per-job count. The job and command deadlines remain 20 minutes
and 120 seconds. Windows retains one reconciliation job and the separate
native RNG campaign.

At frozen `1450e64`, actionlint 1.7.12 accepts the workflow. The actual CLI
listings select all 186 names exactly once in each profile, with 31 on each
index. The local mutation evidence uses three 62-control shards.

## Encoder append shard count

The 205 R1 controls use seven Unix shards of 30, 30, 29, 29, 29, 29 and 29
per profile. The existing 20-minute job limit and 120-second command
limit apply. Windows runs one reconciliation job and its separate native
RNG campaign.

## VALUE encoder shard count

The 234 R1 controls use eight Unix shards of 30, 30, 29, 29, 29, 29, 29
and 29 per profile. They use the 20-minute job and 120-second command limits.
Windows runs one reconciliation job and the separate native RNG campaign.

## DATA encoder shard count

The 259 R1 controls use nine Unix shards, with 29 cases for indexes 0 through
6 and 28 cases for indexes 7 and 8. Debug and release use the same selections.
The 20-minute job and 120-second command limits apply. Windows runs one
reconciliation job and the separate native RNG campaign.

## COMMIT encoder shard count

The 276 R1 controls use the nine hosted Unix shards, with 31 cases for
indexes 0 through 5 and 30 for indexes 6 through 8 in each profile.

## Hosted macOS job deadline

At PR #26 head `88e0030`, the 20-minute job deadline stopped macOS shards 2
and 4 during release. Shard 2 finished on retry in 15m 34s. Shard 4 reached
the job deadline again, just after logging its last of 31 release controls.
The cancelled job is incomplete qualification. Linux shard 4 finished both
profiles in 10m 08s.
[Shard 2 retry](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45075433603/job/137561064463) ·
[Shard 4 retry](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45075433603/job/137561064525) ·
[Linux shard 4](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45075433603/job/137371496681)

That change gave macOS a 30-minute job deadline.

## Unix deadline for the production stack

At PR #48 head `6b0d335`, all nine hosted Linux shards were cancelled at the
20-minute job deadline. Shard 0 detected all 69 debug controls and 34 of the
69 release controls before cancellation. Its source reconciliation passed.
[Linux shard 0](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45210022904/job/137816047993)

The expanded derive registry selects 872 Linux and 834 macOS R1 controls.
The nine shards selected 96 or 97 Linux controls and 92 or 93 macOS controls
in each profile at that point. Both Unix hosts received a 60-minute job deadline
to run both profiles and retain evidence.

At integration PR #78 head `922e191`, Linux shard 1 reached that deadline.
The uploaded debug and release reports each record all 111 controls caught,
but the hosted job concluded `cancelled`. The Unix job deadline is now
90 minutes to leave time for the expanded registry and evidence upload.
Individual commands retain their 120-second deadline. Both profiles, all
control selections and the Windows reconciliation limit of 20 minutes remain
in place. Hosted completion under the new deadline is pending.
[Linux shard 1](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45277738443/job/138053801390)

## Pull request and release matrices

Pull requests and pushes to `main` use the following Engine checks matrix.
Version tags matching `v*` and manual runs of Engine checks use the full
release qualification matrix on the selected source.

| Run | Linux jobs | macOS jobs | Windows jobs |
| --- | ---: | ---: | ---: |
| Pull request or push to `main` | 18 | 3 | 6 |
| Version tag or manual qualification | 16 | 16 | 6 |

The three macOS PR jobs run native durability and relocated dylib checks, the
Darwin R1 campaign, and the calibrated Darwin ThreadSanitizer workspace. Linux
runs the complete Linux R1 selection in both profiles, general engine coverage,
workspace AddressSanitizer, and the portable recovery and helper campaigns.
Windows retains its existing native, allocator, quota and exact-DLL checks.

The Darwin selection currently contains 126 controls per profile. It includes
all 12 macOS-tagged controls, plus shared controls for the Unix file adapter,
namespace publication order, native FFI path and file factories, and native
snapshot file factories. The job checks the exact registry selection against
the runner's list before executing it. It uploads both lists and both profiles
with the decision inventory. The job has a 120-minute ceiling for the combined
campaign. The individual command deadlines stay at 120 seconds.

Release qualification runs all nine macOS R1 shards and both profiles, plus
native macOS coverage, AddressSanitizer, recovery, and helper jobs. Its Unix
shards keep the 90-minute ceiling. A tag or a manual run collects CI evidence
for that candidate. Crate publication has a separate release decision.
