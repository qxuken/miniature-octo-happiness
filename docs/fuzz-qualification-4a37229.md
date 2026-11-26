# Extended ASan qualification at 4a37229

All 22 manifest targets passed at the frozen source
`4a37229e1b3be3835e79a346dc325df9357a0c3e` on native macOS arm64.
The campaign completed 1,399,569 executions. Ten targets reached 100,000
executions and twelve reached the 120-second time limit first. Each target
had its own writable corpus. All 253 seed files, the lockfile and the
scoped source inventory remained unchanged. The 26 supervised commands
verified cleanup. Each failure-artifact directory was empty.

The campaign used cargo-fuzz 0.13.2 and `nightly-2026-11-12`. From a clean
checkout of the frozen commit, the invocation was

```sh
CARGO_NET_OFFLINE=true cargo +1.100.0 xtask fuzz \
  --toolchain nightly-2026-11-12 --runs 100000 --seconds 120 --seed 3 \
  --output ../extended-asan
```

| Target | Executions | Reported seconds |
|---|---:|---:|
| `open_image_bytes` | 100,000 | 18 |
| `open_structured_image` | 100,000 | 37 |
| `commit_crash` | 42,293 | 121 |
| `txn_schedule` | 100,000 | 9 |
| `value_ops` | 100,000 | 30 |
| `image_and_ops` | 51,131 | 121 |
| `retention_compact_crash` | 35,440 | 121 |
| `ffi_ops` | 100,000 | 6 |
| `crdt_frames` | 100,000 | 6 |
| `crdt_schedule` | 42,088 | 121 |
| `snapshot_frames` | 100,000 | 37 |
| `snapshot_install_schedule` | 2,137 | 121 |
| `schema_index_migration` | 5,772 | 121 |
| `derive_tokens` | 51,798 | 121 |
| `logical_import_export` | 100,000 | 83 |
| `backup_restore` | 79,543 | 121 |
| `bytestore_contract` | 100,000 | 79 |
| `coordinator_protocol` | 16,254 | 121 |
| `peer_transport` | 100,000 | 17 |
| `replacement_namespace` | 29,001 | 121 |
| `tree_adversarial_keys` | 6,886 | 121 |
| `register_causality` | 37,226 | 121 |

Fresh extraction reproduced every target's logged count, all retained
corpora and empty failure directories. All 22 archived executable files
matched the final build output byte for byte. Rust 1.100.0 also passed all
33 deterministic replay tests in debug and release on this source.

The delivery verifier checked the restored campaign alongside the three
17-case benchmark runs and 16 packages. For a restore with directories
named as in the checkpoint, run

```sh
cargo +1.100.0 xtask verify-delivery \
  --source 4a37229e1b3be3835e79a346dc325df9357a0c3e --require-isolated \
  --matrix ../restore --benchmark ../restore/benchmark \
  --fuzz ../extended-replay/proof/extended-asan
```

A second verifier on Docker Linux arm64/ext4 with networking disabled
produced the same package, benchmark and fuzz results. Its executable
hash differed, as expected for the other platform. The [checkpoint
manifest](delivery-checkpoint-4a37229.sha256) pins the campaign, executables
and both verification reports. The campaign qualifies this source and
schedule. The final release candidate still needs its own extended run.
