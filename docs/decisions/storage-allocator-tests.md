# Cold-open and Linux metadata allocation tests

The cold-open test calibrates one backend allocation and 34 decode
allocations for the unwitnessed revision 4 fixture. It then separately
refuses the VALUE index, VALUE header, DATA index and DATA header
requests through the selected allocator.

Each refusal reports OutOfMemory, releases the lease and restores the
original live-byte charge. The simulator trace contains the complete
decode with no stabilization I/O. Durable and visible bytes match the
input. The success control performs sync, witness write and sync. It
reopens revision 4 and retained revision 2.

Linux replacement now uses the selected allocator for bounded extended
attribute name and value buffers. These short-lived buffers remain
outside the live-byte cap. The callback test verifies no allocation for
size zero, a single exact request at sizes 3 and 65536, and rejection at
65537 before requesting storage. A real refusal at 65536 returns
OutOfMemory after the size query. The fill callback and native metadata
checks retain their existing bounds.

## Verification at f0d5e8e

Rust 1.100.0 passed debug and release on macOS arm64 (168 library tests
per profile) and Docker Linux arm64/ext4 (175 library, eight file and
12 compaction tests per profile). The Linux cases include metadata and ACL
preservation, interrupted file writes and retained-root checks. Strict
Clippy passed on macOS, Docker Linux and for the Windows MSVC target.

All seven existing cold-open controls for catalog and DATA allocation
propagation were detected on macOS in both profiles. The two clean-source
reports verify 52 command retirements. The current inventory still
reconciles 10,590 decisions, with 2,536 open runtime records.
