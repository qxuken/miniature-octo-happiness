# Windows parent and leaf open flow

The Windows adapter and the scripted file calls use the same open sequence.
The Win32 adapter passes the results from path queries, handle opens,
metadata, volume checks and file type to that sequence.

| Boundary | Failure outcome |
| --- | --- |
| Parent preparation | A failed normalization reports the path error before opening a handle. |
| Parent lookup | A refused lookup preserves the open error. A failed canonical query drops the lookup handle. |
| Canonical drive | A canonical network path is refused before opening a directory lease. |
| Directory open, metadata and admission | A refusal at any of these stages prevents volume inspection. An acquired handle is released. |
| Volume | A refused volume prevents leaf preparation and opening. |
| Leaf append | A maximum-length canonical parent refuses the append and drops the open directory handle. |
| Exclusive leaf open and metadata | A refusal drops all handles acquired by that stage. |
| Leaf admission | A non-disk leaf drops both the leaf and directory handles. |

The success fixture checks the canonical path, requested access, share
mode, disposition and flags for create, writable reopen and read-only
reopen. It verifies the lookup is closed before reopening the directory,
the directory remains open through leaf admission, and only the leaf
handle becomes the lease. The fault fixture checks error identity, the
exact prefix of executed operations, and ownership after each refusal.

The scripts qualify the source-level sequencing. The Windows path prefix
and early input decisions still require native evidence. A native run also
checks the actual Win32 calls, exclusive handles and the NTFS files.

## Qualification

Frozen source `ee67459b06f5895c41aab5dd35d0185a1d10bd43` used Rust 1.100.0 and locked
dependencies. Each profile passed 291 tests on macOS arm64 and 301 on
Docker Linux arm64/ext4. Both serialization fixtures matched their
golden bytes. The registry and strict Clippy checks passed on both platforms.
Windows x64 MSVC cross-Clippy passed and three test executables linked in
each profile. Native execution remains pending.

Both Unix platforms caught all 12 mutations in each profile. Fresh
extraction verified each expected stage error, the 41 supervised commands
and 1,044 source hashes per campaign. The audit leaves six runtime gaps
at the native path entry and 7,384 tooling gaps.
[Frozen source and evidence checksums](windows-open-flow-ee67459.sha256)

## Carried storage module records

The new module declaration relocates these source positions.

| Source | SHA-256 |
| --- | --- |
| `src/storage/mod.rs` | `1ed765a74f3e47804266317534b65a77252d55d7275fc0bb4dd865aaf33e9dae` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/storage/mod.rs:305:22– | 10786..10791 | match | persist | Length success continues; refusal poisons with Io before append. |
| I | weekend-db::weekend_db | src/storage/mod.rs:316:41– | 11156..11157 | try_propagation | persist | An admitted length rounds up within the aligned cap and cannot overflow. |
| E | weekend-db::weekend_db | src/storage/mod.rs:407:49– | 14168..14169 | try_propagation | recover_replica | The injected read error propagates without publication. |
| C | weekend-db::weekend_db | src/storage/mod.rs:410:8– | 14325..14402 | short_circuit_or | recover_replica | Nested short-circuit expression within the four-operand guard. |
| C | weekend-db::weekend_db | src/storage/mod.rs:410:8– | 14325..14442 | short_circuit_or | recover_replica | Nested short-circuit expression within the four-operand guard. |
| I | weekend-db::weekend_db | src/storage/mod.rs:137:62– | 4844..4845 | try_propagation | space | The admitted length is within a region-aligned cap, so rounding cannot fail. |
| C | weekend-db::weekend_db | src/storage/mod.rs:202:12– | 7007..7096 | short_circuit_or | verify | The identity and revision prefix is exercised by the envelope pairs. |
| C | weekend-db::weekend_db | src/storage/mod.rs:202:12– | 7007..7141 | short_circuit_or | verify | The identity, revision and end prefix is exercised by the envelope pairs. |
| E | weekend-db::weekend_db | src/storage/mod.rs:245:9– | 8494..8497 | for_iteration | create | On success write all four zero regions in order; each failure stops at its own callback. |
