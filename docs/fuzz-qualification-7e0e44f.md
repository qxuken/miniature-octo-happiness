# Extended ASan campaign at 7e0e44f

All 22 targets passed on exact source
`7e0e44f98809b5a82a223604f05425dae3c9ab2b`, totaling **1,442,896 actual
libFuzzer executions**. This count includes corpus initialization and repeated
inputs. Every target exited zero with an empty failure-artifact directory.
All 26 supervised identity/target commands verified cleanup. Source, lockfile
and seed change flags stayed false.

The native macOS arm64 campaign used cargo-fuzz 0.13.2 and pinned
`nightly-2026-11-12`: rustc 1.102.0-nightly,
`3ef0c22cdb86ac42dc6b8f0f1ec36857daa6f2a1`, LLVM 23.1.5. Each target used
ASan, a separate writable corpus, the corresponding checked-in seed directory
and these bounds:

```text
-runs=100000 -max_total_time=120 -rss_limit_mb=2048
-max_len=262144 -seed=3 -timeout=10
```

The first stopping bound wins; time-limited runs report 121 seconds at their
next completion check. Summed target-command elapsed time was 1,776.92 seconds.
The campaign verified 322 scoped source files and 244 checked-in seeds.

| Target | Actual executions | Reported fuzz seconds |
|---|---:|---:|
| `open_image_bytes` | 100,000 | 12 |
| `open_structured_image` | 100,000 | 32 |
| `commit_crash` | 50,144 | 121 |
| `txn_schedule` | 100,000 | 7 |
| `value_ops` | 100,000 | 25 |
| `image_and_ops` | 48,075 | 121 |
| `retention_compact_crash` | 40,609 | 121 |
| `ffi_ops` | 100,000 | 5 |
| `crdt_frames` | 100,000 | 5 |
| `crdt_schedule` | 45,218 | 121 |
| `snapshot_frames` | 100,000 | 29 |
| `snapshot_install_schedule` | 2,206 | 121 |
| `schema_index_migration` | 7,098 | 121 |
| `derive_tokens` | 63,458 | 121 |
| `logical_import_export` | 100,000 | 69 |
| `backup_restore` | 86,519 | 121 |
| `bytestore_contract` | 100,000 | 76 |
| `coordinator_protocol` | 18,511 | 121 |
| `peer_transport` | 100,000 | 15 |
| `replacement_namespace` | 32,615 | 121 |
| `tree_adversarial_keys` | 8,226 | 121 |
| `register_causality` | 40,217 | 121 |

The public `6eb5f1a` `verify-delivery` command binds this complete campaign,
three retained compaction microbenchmark samples and all 16 packages in the
[four-platform matrix](private-delivery.md#four-platform-checkpoint-at-7e0e44f)
to the same exact Git archive. It rechecks source/lock/seed inventories,
command and process reports, log lengths, matching combined logs, completed
counts and empty artifact directories. The integrated report SHA-256 is
`a70f827e3e846aa7de73767f7258d01db3ab250a1fb4aabf08cd0de749c77cbc`;
the campaign evidence SHA-256 is
`5b06b8a7f6add8d0f92d90505db5e99af0044b41f9ff09ce62a8c68b8f60a44e`.
Earlier public negative controls reject changed completed counts and an
incomplete target inventory before success output.

A separate Rust 1.99.1 offline, locked build passed all 29 deterministic fuzz
library tests on the same source. Its source files matched the archived tree.
That replay covers its directed cases and checked-in inputs; it does not
replay every mutated campaign input.

The [delivery checkpoint](private-delivery.md) records the retained archive
and restoration verification. It includes raw logs, mutated corpora, empty
artifact directories, process reports and exact source and verifier inputs.

This finite campaign found no crash or oracle/ASan failure in these bounded
cases. It supplies no coverage percentage, MC/DC proof, deployment latency
guarantee, unbounded convergence claim or physical power-loss evidence.
The fuzz command did not rebuild the standard library. Separate same-source
workspace ASan/TSan checks rebuild std; their instrumentation and calibration
remain distinct. Protocol targets cover the [documented harnesses](../fuzz/README.md),
which do not constitute a complete TLS or deployed-network fuzzer.
