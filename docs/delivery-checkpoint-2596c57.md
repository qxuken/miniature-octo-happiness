# Four-platform private delivery at 2596c57

All sixteen package profiles and the bounded fuzz campaign use
`2596c57465fcd5d9fb36a633bf7e9972e35c3f51`. This is PR #82's tested
merge for head `8c85281`. Git verifies that the two commits have the same
tree. All four package lanes use Rust 1.100.0 and verify the base,
replication, CRDT and combined runtime feature masks.

| Platform | Evidence origin | Files in the minimum-compiler inventory | Combined library SHA-256 |
| --- | --- | ---: | --- |
| macOS arm64 | Native APFS | 4,879 | `90fe9eb17a269a12d0e7a1034ee53dfff69be23ca711cd5a19f674e8a4c56bbb` |
| Linux arm64 | Docker with owned ext4 build and runtime volume | 4,882 | `24695266ac4ec059e5d0156fc27e26d3c0822ee20b3f725843c6588a6303babe` |
| Linux x64 | [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45773100058/artifacts/15095960352) | 4,874 | `4b2e143c10a9e8e43a2e14f7b99b54ff3ff64e61f82b16ffcdb57be040018989` |
| Windows x64 | [Hosted artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45773100058/artifacts/15095552456) | 2,707 | `48f4e4622d669ee8f68735eda0ab01f4fa33a1d6440d3d682525a75213407d7c` |

Both arm64 lanes passed the full debug and release workspace suites,
Clippy and native examples. macOS passed 1,107 tests per profile and
Linux arm64 passed 1,154 per profile, both with eight existing ignores.
All four combined packages pass their optimized 134-cell trace.
The Windows trace covers 30 cases through C, C++ and Odin.

The native x64 Odin publication evidence identifies the exact shared
library or DLL in the table. The Linux and Windows fault matrices reopen
revisions `4, 3, 4, 4, 3, 3` in case order. Each verifies retained
revision 2 and ends at revision 4. The separate real full-storage cases
return Linux errno 28 and Windows error 112. Fresh opens while the
filesystems still report zero free space find revision 3 and retained
revision 2. After space is available, both retries reach revision 4.
Independent verification checks 3,658 declared fault artifacts and the
original hosted ZIP digests. [Linux Odin evidence](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45773100058/artifacts/15095137870)
[Windows Odin evidence](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45773100058/artifacts/15095721896)

The same-source hosted ASan run passes all 22 targets with 20,628
executions and 55 stable debug replay tests. The archived source,
seed and terminal-count verification passes from the restored tar.
The macOS benchmark produces three 17-case samples with compaction
enabled. The median single compaction was 26.296 ms for the recorded
retained-history fixture, from 3,190,784 to 86,016 bytes. These small
samples measure a development-host workload.
[Hosted fuzz evidence](https://github.com/qxuken/miniature-octo-happiness/actions/runs/45773100046/artifacts/15094928886)

## Restore

The local `target/archived-proof/delivery-2596c57/` directory contains the
portable input archive, original hosted ZIPs inside that archive, source
bundle, checksums, and both restore reports. Fresh extraction verified
all 25,868 indexed files. macOS and a network-disabled Linux arm64
container then checked all sixteen packages, the four traces and source
inventories, the raw benchmark samples and the complete fuzz report.
The descriptor hashes, file counts and all verification fields match
across the two reports, with each executable's recorded verifier hash
identifying its own binary.

From the restored source bundle and payload, run

```sh
cargo +1.100.0 xtask verify-delivery \
  --source 2596c57465fcd5d9fb36a633bf7e9972e35c3f51 --require-isolated \
  --matrix "$PAYLOAD/lanes" --benchmark "$PAYLOAD/benchmark" \
  --fuzz "$PAYLOAD/restored-fuzz/ci"
```

Linux built and ran on a private, loop-backed ext4 volume inside
Docker. The initial unmount found the harness still inside the mount.
The archived build and test reports had completed. A bounded follow-up
detached the owned loop, and the fresh restore verified retirement with
the harness outside the mount.
[Archive and receipt checksums](decisions/delivery-checkpoint-2596c57.sha256)

The [extended ASan campaign](fuzz-qualification-1f3f42a.md) retains its
own earlier source. The final release freeze, the extended campaign on
that release, deployment acceptance and off-machine proof custody
remain open.
