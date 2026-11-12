# Native filesystem error drill

Run with Docker and cached Cargo inputs available:

```sh
mise exec python@3.14.8 -- python -O tools/native_faults.py \
  --source 96e105f --output /absolute/new/evidence-directory
```

The runner archives the exact selected commit, overlays only the current
`examples/native_faults.rs`, and records both source identities. It builds the
base-feature release example offline and locked inside an immutable Linux image,
with network access disabled. The example asserts its runtime source ID and
feature bits. Logs retain kernel/compiler, filesystem capacity/mount flags,
binary hash, actual errors and final completion marker. Source hashes are checked
after execution. No arbitrary untracked files are copied or registry published.

The runner mounts a private **8 MiB tmpfs** at `/faults`. The example fills only
a controlled ballast file there, with a hard 16 MiB attempt ceiling. Do not run
the `diskfull` example directly against another filesystem. The runner checks
the mount type; the example additionally requires its runner environment flag.
Memory is capped at 2 GiB, processes at 256, and total execution at 300 seconds.
No host filesystem is filled.

The checks are:

- A child with UID/GID 10001 tries creating a database inside a root-owned mode
  0700 directory. The failure must report native `Permission`; the parent
  confirms that no database file exists.
- After acknowledging revision 3 and retaining revision 2, fill the mount and
  attempt a larger commit. The native append fails with `DiskFull` and `Io`,
  leaving the visible revision 3 and requiring recovery. Close the handle,
  remove only the ballast file, reopen revision 3, and verify the old row,
  absent candidate payload, and retained row. A subsequent commit acknowledges
  revision 4, survives another close/open, and preserves the retained row.
- A valid backup is prepared before filling the mount again. Fresh backup and
  restore copies then fail with `Io`/`DiskFull`; both source images stay
  byte-identical. Restoring to the existing valid backup still rejects with
  `AlreadyExists` and does not overwrite it. In-place compaction also fails
  with `Io`/`DiskFull`, preserving the canonical current and retained roots.
  After freeing only the ballast, partial copy images reject inspection,
  fresh-target backup/restore and compaction succeed, and every resulting
  file reopens at revision 4 with current row 18 and retained revision 2/row 17.

Observed on Linux aarch64, Docker's tmpfs, Rust 1.99.1: permission denial is
`OpenDirectory`, errno 13; full storage is `Write`, errno 28. Both preserve the
existing stable engine error codes. The example checks each classification and
state; the script exits nonzero if an assertion fails.

The extended release drill passed against frozen engine
`96e105f80c189a2f9de12530518c4d834f2796ea` plus the bound example overlay, using
Rust 1.99.1, aarch64 Linux, and image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
All four append/backup/restore/compact failures were `Write`, errno 28; permission
denial remained `OpenDirectory`, errno 13. Strict example Clippy passed. An
independent reviewer rehashed all 558 extracted sources, archive, runner and
overlay, checked the nonvacuous failure/recovery oracles, and found no actionable
defect. Local evidence is `/tmp/weekend-native-maintenance-final-96e105f/evidence.json`
and its adjacent `run.log`.

Existing outputs, including dangling symlinks, reject before external work.
The default output is a fresh private temporary directory; `--output` needs an
existing parent. The runner has a 300-second container deadline and bounded
cleanup; it independently verifies that its UUID container is absent. Cleanup
failure cannot produce `verified=true`, and final failure evidence survives a
daemon error. Admission tests pass under Python optimization. Python 3.12+ is
required for safe archive extraction.

The source-bound manifest, terminal log, independent review and normal/optimized
admission-test logs are preserved locally in
`target/local-qualification-native-maintenance/index.json`. This ignored proof
bundle is not a published distribution.

This checks real OS errors and process-level reopening, **not power-loss
persistence**: tmpfs has no durable device. It exercises exhaustion during
append before root publication, not root/witness barrier errors or delayed
writeback. Quotas, physical device failures, ext4/XFS/APFS/NTFS exhaustion,
network filesystems, and real power interruption remain separate qualification
work. Simulator fault tests continue to cover uncertain publication outcomes.


## Quota capability check

A later [local baseline review](local-baseline-ffdf506.md#actual-quota-capability-remains-unqualified)
tried provisioning an isolated quota-enabled ext4 loop-file fixture. The mount
failed before quota control or engine execution. The available Linuxkit kernel
has generic quota control but lacks QFMT_V1/QFMT_V2, XFS quota and tmpfs quota
configuration. Actual EDQUOT qualification therefore remains open for a
quota-capable kernel/filesystem. Existing ENOSPC and synthetic quota-category
tests retain their original scope; neither is recorded as a native quota pass.
