# Exact-package transaction validation modes

This gate extends the deterministic artifact trace to compare `Validation::Records` with `Validation::WholeDatabase` through the native Rust API and packaged C/Odin APIs. It changes only the trace, independent model, consumer drivers and controls. The runner and engine are unchanged.

## Versioned compatibility

The default trace now has **134 cells** and magic `WDBTRC2\n`. Its header and fixed-width little-endian cells keep the previous layout: eight-byte magic, a `u32` count (at most 256), then four `u32` fields per cell. A v2 BEGIN cell is `(1, transaction_slot, unused_key, validation_mode)`, where mode 0 means Records and mode 1 means WholeDatabase. Other mode values are rejected before database creation. PUT still uses the fourth field as its numbered nested value. Other operations and canonical transcript columns are unchanged.

All three consumers also accept `WDBTRC1\n`. V1 BEGIN retains its original Records-only behavior, including ignoring the fourth field rather than treating a legacy value of 1 as WholeDatabase. Python decoding retains the version on the decoded cell list, and re-encoding that list preserves its version unless the caller explicitly selects another version. Unknown magic is rejected.

`TRACE_V1` preserves the original 59 cells byte for byte. Both previous hashes remain unchanged:

| V1 artifact | SHA-256 |
| --- | --- |
| Trace | `78b916121e57e2cb0ce3d010dd126e46ef4d8047ec7ec9beb330f6dd3df33101` |
| Independent expected transcript | `5997dbaa08eef5bc3c98e6bdcea55f991da09d8d8d2693e5b73f5c5045e295ff` |

The first 59 transcript lines of v2 also equal that v1 transcript. Earlier artifacts and their evidence directories have not been rewritten. Their scope remains documented in [the original artifact trace report](artifact-trace.md).

## Independent contract and added cases

The Python model imports no engine/API code and invokes no consumer. It records each transaction's selected mode and base publication revision. WholeDatabase additionally requires the current publication revision to equal that base revision at commit. Records continues to validate its existing-record observations and collection observations; it does not reject an unrelated publication solely because the root revision changed. Successful empty transactions publish a new revision. Commit failures remain terminal.

The 75 appended cells exercise:

| Cells, zero-based | Behavior and exact publication result |
| --- | --- |
| 59–68 | Records transactions modify independent existing rows; both commits succeed at revisions 11 and 12. An overlay get and scan inspect full nested values. |
| 69–79 | WholeDatabase transactions modify those independent targets; the first succeeds at revision 13, the second conflicts with status 10 at cell 75. Its get and repeated commit return closed, status 11. |
| 80–88 | WholeDatabase reads, writes and scans its overlay with an unchanged published root, then commits at revision 14. A repeated commit and read are closed. |
| 89–96 | An empty Records commit publishes revision 15. A simultaneously active empty WholeDatabase transaction conflicts, then its read/recommit are closed. |
| 97–99 | An empty WholeDatabase transaction with an unchanged root succeeds at revision 16. |
| 100–105 | Two empty Records transactions both succeed at revisions 17 and 18 despite the intermediate publication. |
| 106–122 | A snapshot pins revision 18. Retain publishes ID 2 at revision 19: the WholeDatabase writer conflicts, while the Records transaction's point/scan observations and overlay remain valid and commit at revision 20. Closing/reopening preserves the retained full historical state. |
| 123–133 | Releasing durable ID 2 publishes revision 21: WholeDatabase conflicts while a Records point reader commits at revision 22. The already-open historical view remains valid. After another reopen, opening released ID 2 returns not found, status 3. |

At every cell the consumers emit actual status, current publication revision, every current row and recursively inspected value, the held snapshot's revision/full rows, durable retained IDs and operation result. Values remain nested maps/lists with NUL and `0xff` binary bytes and binary row keys. The final current rows are key `00` with value number 14 and key `07` with value number 16 at revision 22. The retained revision-18 view contains value numbers 14 and 11, so stale candidate writes and historical/current values are distinguished.

These are representative data, empty-transaction and retention-metadata publications. They do not enumerate every root-changing API, every transaction interleaving or uncertain-I/O outcome.

## Controls

Fourteen unit controls pass. New controls include hand-computed complete transcripts for an empty publication invalidating WholeDatabase, unchanged-root WholeDatabase success, and retention publication invalidating WholeDatabase while a Records reader succeeds. They also preserve the original v1 hashes, distinguish the same BEGIN field under v1/v2, verify version-preserving decode/encode, reject invalid v2 modes and unknown versions, and check the finite v2 conflict/publication matrix. Existing package, output-path, cleanup and toolchain-selector controls remain present.

Six additional cases were run against **each actual optimized consumer binary**, for 30 driver executions across five language/platform targets:

* Original v1 59-cell trace: exact original oracle transcript.
* V1 BEGIN field 1: remains Records and commits successfully across another empty publication; this expected transcript is hand-written.
* V2 WholeDatabase empty conflict, closed read and closed recommit: hand-written expected transcript.
* V2 unchanged-root WholeDatabase empty success: hand-written expected transcript.
* V2 invalid mode 2: nonzero process exit and no database created.
* Unknown version 3: nonzero process exit and no database created.

Assertions remain enabled. C/Odin retain the complete 32-byte undersized-buffer sentinel checks, reseeded required-length checks and valid owned failure-output probes from the preceding gate. Invalid-mode/unknown-version failures are trace-input rejection controls; they are not claims about the ABI's invalid-argument return paths. Expected aborts disable core dumps.

## Exact packages and results

Qualification used the **existing** private packages identifying commit `21e5d2638c24c6254ced35d0077c55bb30a287b2`. C/Odin loaded their unchanged delivered release libraries. Native Rust rebuilt the packaged same-commit Rust source with the explicit consumer-driver overlay, original packaged consumer manifest and original locked dependencies. Compiler dependency checks bind Rust to that relocated packaged source; delivered C/Odin package inventories remain unchanged. The checked-out assurance baseline was `365daf6c8a6b7c85fb049fb5a0370b401dde2424`.

| Platform | Optimized consumers | Full 134-cell proof | Additional controls |
| --- | --- | --- | --- |
| macOS arm64 | Rust `--release`, C `-O2 -UNDEBUG`, Odin `-o:speed` | `target/artifact-trace-validation-365daf6/mac-final/evidence.json` | `mac-controls/evidence.json` under the same gate root |
| Linux arm64, ext4 | Rust `--release`, C `-O2 -UNDEBUG` | `target/artifact-trace-validation-365daf6/linux-final/evidence.json` | `linux-controls-final/evidence.json` under the same gate root |

All five full transcripts exactly match the independent model. Full-trace hashes are:

| V2 artifact | SHA-256 |
| --- | --- |
| Trace | `4ad2fe8534dc73bd3032f1dcc41220f5bf919482c1f7df0a0a8716c4c40812c8` |
| Expected transcript | `0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3` |
| Model source | `e13efebc7dc59683df91407e85da452ee0fd58619fb474da6274f2c3d22c3e48` |

Native tools used Mise Rust/Cargo 1.99.1 and Python 3.14.8, with the installed Odin `dev-2026-10:a22aa1984`. The unchanged runner preserves the explicit Rustup/Mise selector while removing build overrides. Linux used immutable local image `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`, `--pull never`, `--network none`, UUID private volumes and bounded child waits. Full-run `findmnt` records report ext4 for both build and runtime data. The separate Linux driver-control run also verified ext4. Read-only frozen host seeds were copied into the volumes; selected proofs were copied back before removing only the named container/volume and verifying their absence.

The package manifests were verified before and after, including relocation and copyback. Native library SHA-256 remains Mac `200c17d94ab533112e6de2a6f87336ce728ff486128d2ed4f8e260e0b60402e0` and Linux `16a927abb639ed68de1fdcc375bc52d41270aa1ac06552b1132fd3942088ac64`. The original Mac/Linux manifest hashes remain `55186dc7c5114650c7510d4660d3f62a31d554b257caf9e413cd981e1c78fa7d` / `d0a1bbce2fc5b742fe843ca5e28b68b6dd7a128aca5eb994389a256b7ba990c2`.

Saved tool-source maps before and after each full run equal the current files. Consumer hashes in the separate control proofs match the binaries from those full runs. Source/trace/expected/proof hashes, command vectors, tool versions, full stdout/stderr, loader diagnostics, input snapshots, binary/library hashes and cleanup records are retained under the gate root. `run.py` was not edited; its qualified SHA-256 is `c4f3a28c34023f20561571abd5f561abf0267481327596a2a114f56b2457a782`.

This is a deterministic delivered-API validation-mode gate. It adds no native Windows, Linux Odin, C++, power-loss, uncertain-I/O or full-API qualification claim. All packages/crates remain private; no commit, push, publication or remote CI was performed.
