# Bounded retry of busy APFS detach

The macOS maintenance job at PR #82 head `9e660e4` reached the
cleanup of its private APFS image. The completed backup, restore and
compact cases were retained in artifact 15068993087. All five
`hdiutil detach` calls targeted the same whole disk and returned 16.
Their child processes retired, but the image remained attached and
the job failed at the cleanup gate.
[Hosted macOS job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45714156600/job/139555819673)

An isolated regression holds an open directory on a private image
through five busy detaches and releases it before the sixth. It
reproduced the old terminal error on the fifth attempt. The new
schedule permits up to eight ordinary detach attempts with at most
12.25 seconds of backoff. Each attempt rechecks exact image ownership
and the whole-disk identity. Persistent busy, unexpected statuses
and supervision failures still return errors. Qualification requires
observing the image detach, the original mount device return and the
private image file disappear.

## Verification

The frozen `087f4a94fc5390bb319d3887bab5e6aa97f94802` source
passed the native busy-image regression and ballast sync refusal
test in debug and release with Rust 1.100.0. The retry unit tests
and Clippy with warnings denied also passed.

| Native macOS qualification | Result |
| --- | --- |
| Near-cap backup, restore and compact | Kernel ENOSPC, unchanged source and backup, fresh reopen at revision 212 with 64 retained roots, and retry matching the backup. |
| Exchange at ENOSPC | Preserved the complete scratch across remount and retried to the expected image. |
| Cleanup after exchange at ENOSPC | Removed the scratch and reopened the advanced canonical image. |

All three private images detached and were removed. The verifier
binds the failed hosted artifact, frozen source, exact runner and
fixture binaries, phase evidence and local test logs. The hosted
retry on the repaired source is pending. The identity of the holder
on the hosted runner remains unknown.
[Campaign checksums](apfs-detach-087f4a9.sha256)
