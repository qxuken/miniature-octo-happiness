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
the new debug CLI passes these tests. Optimized, platform and hosted execution
results retain their actual source and are recorded after completion.
