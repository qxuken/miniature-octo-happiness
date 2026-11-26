# Extended ASan qualification at 1f3f42a

All 22 targets passed on native macOS arm64 at frozen commit
`1f3f42afad0d759b6f692824f750528315b4878a`, the same source used by the
[arm64 packages](delivery-checkpoint-1f3f42a.md) and the
[five compatibility profiles](compatibility-1f3f42a.md). The campaign
completed **1,425,865 executions**. Ten targets reached 100,000 executions,
and twelve reached the per-target time limit first. All 253 seed files,
the fuzz lockfile and the scoped source inputs matched before and after.
All 26 supervised commands verified cleanup. Each failure-artifact
directory was empty.

The run used cargo-fuzz 0.13.2, `nightly-2026-11-12`, seed 3, a 2,048 MiB
RSS limit and 262,144-byte maximum input. Each target had its own writable
corpus. From a clean checkout of the frozen source, the command is

```sh
CARGO_NET_OFFLINE=true cargo +1.100.0 xtask fuzz \
  --toolchain nightly-2026-11-12 --runs 100000 --seconds 120 --seed 3 \
  --output ../extended-asan
```

| Target | Completed executions | Reported seconds |
| --- | ---: | ---: |
| `open_image_bytes` | 100000 | 12 |
| `open_structured_image` | 100000 | 35 |
| `commit_crash` | 44075 | 121 |
| `txn_schedule` | 100000 | 8 |
| `value_ops` | 100000 | 28 |
| `image_and_ops` | 49608 | 121 |
| `retention_compact_crash` | 40006 | 121 |
| `ffi_ops` | 100000 | 6 |
| `crdt_frames` | 100000 | 6 |
| `crdt_schedule` | 43086 | 121 |
| `snapshot_frames` | 100000 | 29 |
| `snapshot_install_schedule` | 2094 | 121 |
| `schema_index_migration` | 6001 | 121 |
| `derive_tokens` | 65825 | 121 |
| `logical_import_export` | 100000 | 72 |
| `backup_restore` | 87342 | 121 |
| `bytestore_contract` | 100000 | 72 |
| `coordinator_protocol` | 18409 | 121 |
| `peer_transport` | 100000 | 15 |
| `replacement_namespace` | 34122 | 121 |
| `tree_adversarial_keys` | 6456 | 121 |
| `register_causality` | 28841 | 121 |

The local archive `target/archived-proof/extended-asan-1f3f42a.tar.zst`
preserves the original command and terminal logs, corpora, empty failure
directories, Git bundle and all 22 executed binaries. Each retained binary
matched the actual path recorded by Cargo and the original build output
byte for byte. Fresh extraction checked all 7,976 indexed files.
`verify-fuzz` reproduced the source-bound report and completion counts
from the extracted tree.

Stable Rust 1.100.0 also passed all 55 deterministic replay tests in debug
and release on macOS and Docker Linux/ext4. Those runs preserve the same
253 input files and all four runs list the same test names.
[Archive and replay checksums](decisions/extended-asan-1f3f42a.sha256)

The extended result applies to this frozen commit. The final release
source and off-machine proof restoration remain R7 gates.
