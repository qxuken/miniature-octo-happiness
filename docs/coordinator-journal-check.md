# Coordinator journal check qualification

The sequenced coordinator reads its head and the assigned slot's most recent
receipt from the same snapshot. The request uses the existing authenticated
loopback connection and checks the journal's storage identity. The client
validates the slot receipt against the reported head and compares it with the
local receipt and pending intent. The check does not submit a write or record an
acknowledgment. The [restore runbook](../apps/write-coordinator/README.md#back-up-and-restore-a-coordinator-with-active-journals)
uses it on an isolated endpoint before cutover.

| Scenario | Observation and follow-up |
|---|---|
| Fresh journal and coordinator | `aligned 3 0`. Both receipts are empty. |
| Server commits 42 at revision 4, the client loses the reply, and the server restarts | `pending-committed 4 42` before and after restart. The journal stays pending until `resume` records the original receipt. A subsequent check gives `aligned 4 42`. |
| Restore the same-lineage backup at revision 3 while the journal is pending | `pending-eligible 3 0`. The restored slot remains empty. The retired branch may have committed the intent. |
| Check that backup after the local journal records revision 4 | `behind 3 0` and unsuccessful exit. The restored coordinator stays at revision 3. |
| Restore the same backup and commit value 777 in the same slot at revision 4 | `receipt-mismatch 4 777` and unsuccessful exit with and without a later pending intent. The selected primary can still acknowledge the journal's next write once at revision 5. |
| Check a separately initialized database | `ForeignStore` and unsuccessful exit. |
| Another writer advances the head while the pending slot keeps its predecessor | `pending-stale 4 14` in the simulator. The journal's intent stays pending and its remote slot stays empty. |

The native process test compares the journal's file bytes before and after
every check. The simulator compares its durable image. The parser tests reject
missing or extra fields, malformed or noncanonical numbers, a revision below
the initial head, and receipts that claim a future revision or disagree with
the head value. Corrupt receipt rows also fail on the server. The bounded fuzz
oracle checks the returned checkpoint and receipt bytes against its own slot
model after sequenced operations and across reopen, including foreign identity
rejection.

## Reproduction

The tested source is `25756e738eb977ff0e319559b045f7e3b14f7421`.
The source archive under `target/coordinator-journal-check-proof/` has SHA-256
`812a054f7c14b74eab9fb7c37ddc2604fd4b096e3e2f5db80a0bfb1af552ef9f`.

```sh
cargo test --offline --locked -p write-coordinator
cargo test --offline --locked --release -p write-coordinator
cargo test --offline --locked --release -p write-coordinator --test writers -- --ignored --nocapture
cargo clippy --offline --locked -p write-coordinator --all-targets -- -D warnings
```

macOS arm64 on APFS with Rust 1.99.1 and Docker Linux arm64 on ext4 with
Rust 1.99.0 each passed 31 checks in both debug and release, plus both extended
scenarios with 1,152 writes each. After each of nine restore cutovers, both
restored journals reported the same aligned checkpoint and replayed their
separate local receipts. The final coordinator head was revision 1155.
Strict Clippy passed on both platforms and on the Windows MSVC x64 cross-target.

Native Windows NTFS x64 with Rust 1.99.0 passed the journal-check simulator,
malformed reply cases and the process restart/restore case in both debug and
release. The artifact identifies clean test merge
`27f3e3eb7f3d7787cf0bf9b97b829dbcb83bd92a` with branch parent
`f88e737fa59610d979fd90ec436fdf7356c326fe`. The packaged journal, sequenced
protocol, entry point and integration test match that parent byte for byte.
The proof ZIP SHA-256 is
`fb4e9df72f962a46dd880d64b9eceeb4b3f8f1a37ec0efbe3432fe9985749c61`.
The nine-cutover workloads above ran on macOS and Docker Linux.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43786314888/job/133034630409),
[verified artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43786314888/artifacts/14190583584).

| Local sample | Journal write p50 / p95 | Complete journal cutover range |
|---|---|---|
| macOS APFS | 34.070 / 62.045 ms | 100.020–181.370 ms |
| Docker Linux ext4 | 7.971 / 18.283 ms | 8.867–54.497 ms |

The cutover interval includes stopping the coordinator, verifying copies,
starting the restored instance and checking both journals. These lab
measurements describe the sampled runs.

The ext4 run used the Docker image
`sha256:877e7780703eb3f4f5f85143e3cfd8ac9f57591ed45c83e2a357e7d2eb87403c`
and placed the native test files on a disposable ext4 volume.

The macOS AddressSanitizer fuzz run started from the seven checked-in
seeds and completed 20,000 executions with no findings. The source archive,
platform logs and fuzz log are indexed by `SHA256SUMS` under
`target/coordinator-journal-check-proof/`.

```sh
scratch=$(mktemp -d)
cp -R fuzz/seeds/coordinator_protocol "$scratch/corpus"
mkdir "$scratch/findings"
CARGO_NET_OFFLINE=true cargo +nightly-2026-11-12 fuzz run \
  --target-dir "$scratch/build" coordinator_protocol "$scratch/corpus" -- \
  -seed=3649 -runs=20000 -max_len=256 -timeout=5 -rss_limit_mb=1024 \
  "-artifact_prefix=$scratch/findings/"
```

Two branches can share a visible checkpoint and slot receipt. Selecting the
authority and checking external effects still require operator judgment.
