# Exact-package deterministic operation trace

`tools/artifact-trace/run.py` supplies a finite F03/F15 semantic qualification: native Rust, C and Odin interpret one versioned binary operation trace and each transcript must equal an independent Python contract model. It never publishes, pushes, commits, or invokes remote CI. All inputs and database paths are synthetic and private.

## Verified scope

The local 2026-11-13 runs used packages identifying source commit `21e5d2638c24c6254ced35d0077c55bb30a287b2`:

| Platform and exact package | Consumers | Evidence |
| --- | --- | --- |
| macOS arm64, `target/qualification-21e5d26/mac-package` | optimized native Rust, C, Odin | `target/artifact-trace-mac-optimized-20261113a/evidence.json` |
| Linux arm64, private ext4 volume, `target/qualification-21e5d26/linux-package` | optimized native Rust, C | `target/artifact-trace-linux-ext4-optimized-20261113a/evidence.json` |

All five consumer runs matched all 59 expected transcript lines. macOS used Rust/Cargo 1.99.1, Python 3.14.8, Apple clang 21.0.0 and Odin `dev-2026-10:a22aa1984`. Linux used Rust/Cargo 1.99.1, Debian GCC 12.2.0 and Python 3.11.2 inside the already-local immutable image `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08` with `--network none`. The original checkout was unavailable inside that container.

These final runs explicitly optimize the **consumers**: native Rust uses `cargo build --release --locked --offline`; C uses `-std=c11 -O2 -UNDEBUG -Wall -Wextra -Werror`; Odin uses `-o:speed`. The installed Odin compiler's saved `build -help` output documents that flag (its default is `-o:minimal`). Runtime assertions stay enabled: C explicitly undefines `NDEBUG` and rejects compilation if it is defined, Odin has `#assert(!ODIN_DISABLE_ASSERT)`, and Rust's ordinary `assert!` remains active in release. Exact compiler/linker command vectors and `consumer_profiles` are saved in each proof.

Consumer optimization is separate from the **packaged native library** profile. C/Odin load the unchanged delivered libraries whose package manifests record `cargo rustc --locked --release --features crdt,replication` and `-C panic=unwind`. The harness does not rebuild those libraries. Native Rust separately rebuilds the packaged source using the original packaged consumer's release profile and dependency manifest, as described below. Earlier evidence directories retain their original C/Odin default-optimization scope.

Earlier Linux passes in `target/artifact-trace-linux-20261113d`, `e`, and `g` built and ran on a host bind mount. A separate host-bind rebuild in `target/artifact-trace-linux-20261113f` stopped before running the driver when rustc received SIGBUS inside LLVM archive creation. Its failure logs remain preserved; the fresh `g` run passed with unchanged tool/driver sources. The compiler failure's underlying cause was not established. The current private-ext4 profile qualifies a different storage environment; its success does not prove a cause or fix for that compiler failure. The trace and Rust driver remain unchanged; the current C/Odin drivers additionally strengthen the buffer and failure-output controls described below.

The Linux package has no Odin bindings or Rust source payload. Its Rust consumer therefore rebuilt the **same-commit Rust source payload from the Mac package**, using the packaged consumer manifest and its original lockfile. C used the Linux package's own header and `.so`. No Linux Odin, C++, native Windows, MSVC, or NTFS result is claimed. This finite trace is semantic and package-consumption evidence; it does not establish power-loss durability, long-run reliability, every value kind, or the behavior of subsequent engine changes.

## Shared trace and independent oracle

`model.py` defines the trace and its independent model without importing or invoking the engine. The runner emits `trace.bin` and `expected.txt` before compiling any consumer. Each consumer reads `trace.bin`; no expected values or transcript are linked into a consumer.

The binary format is explicitly little-endian:

* Eight-byte magic/version `WDBTRC1\n`.
* One `u32` operation count, at most 256.
* Exactly that many 16-byte cells: `(opcode, transaction_slot, key_or_retained_id, value_number)`, four `u32` fields. Slots are 0–3, keys are 0–255, and value numbers are 0–254. Extra/truncated bytes and out-of-range cells are rejected.

The opcodes are:

| Opcode | Operation |
| --- | --- |
| 1 | Begin record-validating transaction in an empty slot |
| 2 | Put the numbered nested value at the one-byte binary key |
| 3 | Get from a transaction, including its write overlay |
| 4 | Scan a transaction, including its write overlay |
| 5 | Commit; a repeated call checks the terminal outcome |
| 6 | Delete, including an absent row |
| 7 | Release a transaction slot |
| 8 / 9 | Acquire / release the one live snapshot |
| 10 | Durably retain the live snapshot |
| 11 | Open the retained ID in field 3 |
| 12 | Close and reopen the file; requires no live handles |
| 13 | Release the durable retained ID in field 3 |

Value number `n` constructs the full value `{"n": unsigned(n), "v": [bytes(0,n,255), unsigned(n+1)]}`. All consumers recursively inspect the actual stored kind, every map entry, every list item and every byte. Row keys include NUL and `0xff`; maps and rows use lexicographic byte ordering. Canonical values use hex byte strings, unsigned decimal integers, `m{key:value,...}` and `l[value,...]`. Rows use `[key=value;...]`.

Each step emits one canonical line:

```text
step opcode status current_revision [all_current_rows] H=revision[all_held_rows]|- R=id,...|- X=result|-
```

`X` is the actual get value, transaction scan, successful commit revision, or retained ID. Missing gets normalize native Rust `Ok(None)` and C/Odin `WDB_NOT_FOUND` to status 3. Status 10 is conflict, status 11 is closed. The model computes these statuses, publication revisions, current rows, held historical rows, and retained IDs independently.

The model follows the observation/transaction and durable-retention contracts in [DESIGN.md](../DESIGN.md), and the ownership/buffer/terminal-commit contracts in [weekend_db.h](../include/weekend_db.h). Point observations of existing records validate their record versions; absence and scans validate the collection version; writes merge over the current root after successful validation. A successful transaction publishes a new revision, including a delete of an absent key. A failed commit is terminal and does not publish. Retain/release publish metadata revisions without changing collection versions. Retained IDs preserve historical roots through close/reopen, and releasing a durable ID does not invalidate an already-open historical snapshot.

The trace covers these cases:

* Two transactions modifying independent existing records both commit.
* Stale point reads, scans, and missing-row reads each conflict; a second commit and read on a conflicted transaction return closed.
* A missing delete succeeds, and reads/scans see an in-transaction deletion.
* A live old snapshot stays unchanged while current roots advance.
* A retained historical root reopens with its original revision and full values; durable release preserves an existing historical reader and stays released after another reopen.

## Exact artifacts, isolation and bounds

The runner verifies every manifest-listed file, rejects symlinks, extra files and dirty source identities, copies the native and source packages into fresh staging directories, renames those copies, and re-verifies their complete manifests. The copied package inventories remain unchanged. C and Odin compile against the relocated header/binding/library. Their runtime loader diagnostics must confirm that the relocated library was loaded, and their ABI, file format and embedded source ID must match.

C/Odin assert the actual zero-capacity and undersized-buffer statuses. Each undersized byte/key request checks **all 32 bytes** remain `0xa5`. Required-length outputs are reseeded to 99 before both undersized and full calls, and each must equal the original queried length. For get/retained-open calls, the output starts with a valid live owned value/snapshot handle; the original probe is retained separately and safely released after the call. Errors must clear the output to null; successful calls must return a distinct owned result. Commit revision outputs start at nonzero `0xdeadbeefcafebabe` and must clear to zero on errors. No fabricated pointer is used. Scans outlive temporary snapshots, and all valid values, scans, transactions, snapshots and databases are released.

Native Rust uses the Rust API directly. The runner creates a separate copy of packaged `rust-source`, replaces only `examples/rust_package/src/main.rs` with the saved driver, and builds through the original packaged consumer manifest and original lockfile using `--locked --offline`. Cargo metadata must show that all local dependencies remain inside that copied source and declare `publish=false`; dependency files must not reference the original checkout's production source. The driver has no standalone manifest intentionally. Native library features follow the shipped package; Rust retains the packaged consumer's existing `crdt`/typed dependency profile.

For Linux, `cargo vendor --locked --offline` copies the original locked registry dependency closure from the host cache. A frozen seed containing the captured tools, packages, dependency tree, trace, expected transcript and context is mounted at `/seed` **read-only**. `docker_worker.py` verifies and copies it into a fresh UUID Docker volume mounted at `/private-volume`. Rust build sources/targets, C build outputs, native libraries, executables, databases, logs and `TMPDIR` all reside on that volume. An explicit source-replacement config is added to the disposable Rust overlay. The original packages and their lockfiles are not modified.

The worker records actual `findmnt --json` results for both the build directory and runtime directory, and rejects any filesystem other than ext4. In the verified run both report `/dev/vda1[/docker/volumes/weekend-trace-data-e1ea6e46eb1b4b6abda77fb07b17e6a5/_data]`, filesystem `ext4`, mounted at `/private-volume`. Frozen input states before/after execution have the same SHA-256, `a2707ac439fe3c82b120dbfde114d6cd0c274d5dce71d339d460cc9e093fa643`.

Docker uses `--pull never`, a UUID container/volume pair, a 2 GiB memory limit, 2 CPUs, 256 PIDs and no network. After execution the stopped container's volume proofs are copied into the fresh host `linux-result/` directory. Packages, source/trace/expected files and transcripts are checked again after copyback. Cleanup accepts only matching `weekend-trace-<UUID>` / `weekend-trace-data-<UUID>` names, removes only that container and volume, then separately verifies both are absent. Container state, copyback and cleanup/absence logs are retained under `docker-logs/`. Native Mac proofs live directly in the output directory; Linux native proofs live in `linux-result/`, identified by `native_evidence_root`.

Every child command has a 300-second deadline; timed-out child process groups are killed. Native child files have a 64 MiB per-file cap; a completed command with either log over 16 MiB is rejected. Consumers accept at most 256 operations, 16 rows/list entries and 32-byte output buffers. The runner requires a new output directory and does not overwrite prior evidence. It rejects any existing final output entry, including a dangling symlink, at the caller's original absolute path **before resolving its parent or reading package inputs**. Interrupted or failed runs keep partial logs and `verified: false` evidence when Python can finalize it.

Evidence includes source and package manifest hashes, all copied package files, the driver overlay's original/replacement hashes, original consumer lockfile hash, trace and expected transcript hashes, compiler version logs and invoked-tool binary hashes, consumer binary/library hashes, runtime loader logs, command vectors, exit codes and durations. Tool source hashes (including the test source) are captured before and after each run and must remain identical. Every captured stdout transcript hash must equal the expected transcript hash. Linux additionally captures the complete frozen input state before/after, filesystem records and verified UUID resource cleanup. Cargo target files are retained for dependency review but excluded from the compact hash inventory.

Both verified runs used trace SHA-256 `78b916121e57e2cb0ce3d010dd126e46ef4d8047ec7ec9beb330f6dd3df33101` and expected transcript SHA-256 `5997dbaa08eef5bc3c98e6bdcea55f991da09d8d8d2693e5b73f5c5045e295ff`. The native libraries were:

| Package | Manifest SHA-256 | Library SHA-256 |
| --- | --- | --- |
| Mac | `55186dc7c5114650c7510d4660d3f62a31d554b257caf9e413cd981e1c78fa7d` | `200c17d94ab533112e6de2a6f87336ce728ff486128d2ed4f8e260e0b60402e0` |
| Linux | `d0a1bbce2fc5b742fe843ca5e28b68b6dd7a128aca5eb994389a256b7ba990c2` | `16a927abb639ed68de1fdcc375bc52d41270aa1ac06552b1132fd3942088ac64` |

## Reproduce locally

Use new output paths on every invocation:

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/artifact-trace/test_runner.py
mise exec rust@1.99.1 python@3.14.8 -- python tools/artifact-trace/run.py \
  --package target/qualification-21e5d26/mac-package \
  --output target/artifact-trace-mac-NEW \
  --odin /Users/qxuken/projects/libs/Odin/odin
mise exec rust@1.99.1 python@3.14.8 -- python tools/artifact-trace/run.py \
  --package target/qualification-21e5d26/linux-package \
  --source-package target/qualification-21e5d26/mac-package \
  --output target/artifact-trace-linux-NEW \
  --docker-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
```

Eight unit checks independently cover a hand-computed empty/missing-delete publication sequence, known conflict/terminal/history outcomes, malformed trace bounds, corrupted/extra/symlinked package rejection, manifest path escape rejection, preserving an existing output directory, rejecting a dangling final output symlink before any input/work, and exact cleanup argv restricted to matching UUID resources. The dangling-link control asserts the redirected target stays absent and the surrounding directory is unchanged. These complement the actual packaged-consumer executions; they do not replace them.
