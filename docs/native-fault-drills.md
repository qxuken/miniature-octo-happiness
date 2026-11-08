# Native filesystem error drill

Run `python3 tools/native_faults.py` with Docker available. The runner creates a
fresh Linux container from `rust:1-bookworm`, copies the read-only checkout
excluding build artifacts, and builds the locked `native_faults` example. It
prints the kernel, Rust compiler, filesystem type, capacity, and mount flags.
The container is removed even on timeout. No registry publication occurs.

The runner mounts a private **8 MiB tmpfs** at `/faults`. The example fills only
a controlled ballast file there, with a hard 16 MiB attempt ceiling. Do not run
the `diskfull` example directly against another filesystem. The runner checks
the mount type; the example additionally requires its runner environment flag.
Memory is capped at 2 GiB, processes at 256, and total execution at 300 seconds.
No host filesystem is filled.

The two checks are:

- A child with UID/GID 10001 tries creating a database inside a root-owned mode
  0700 directory. The failure must report native `Permission`; the parent
  confirms that no database file exists.
- After acknowledging revision 3 and retaining revision 2, fill the mount and
  attempt a larger commit. The native append fails with `DiskFull` and `Io`,
  leaving the visible revision 3 and requiring recovery. Close the handle,
  remove only the ballast file, reopen revision 3, and verify the old row,
  absent candidate payload, and retained row. A subsequent commit acknowledges
  revision 4, survives another close/open, and preserves the retained row.

Observed on Linux aarch64, Docker's tmpfs, Rust 1.99.1: permission denial is
`OpenDirectory`, errno 13; full storage is `Write`, errno 28. Both preserve the
existing stable engine error codes. The example checks each classification and
state; the script exits nonzero if an assertion fails.

This checks real OS errors and process-level reopening, **not power-loss
persistence**: tmpfs has no durable device. It exercises exhaustion during
append before root publication, not root/witness barrier errors or delayed
writeback. Quotas, physical device failures, ext4/XFS/APFS/NTFS exhaustion,
network filesystems, and real power interruption remain separate qualification
work. Simulator fault tests continue to cover uncertain publication outcomes.
