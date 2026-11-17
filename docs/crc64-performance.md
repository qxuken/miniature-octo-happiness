# CRC-64 throughput and storage compatibility

The [follower phase profile](follower-installation-profile.md) measured graph
verification separately from SHA-256 readback and manifest selection. Disk
regions and the logical format use CRC-64/ECMA-182. The previous implementation
advanced the CRC one bit at a time. At `e1fd1db`, `update_crc` processes full
eight-byte words with eight compile-time tables and handles the final zero to
seven bytes with the first table. The tables occupy 16 KiB of static data.

The generator derives the first row from the ECMA polynomial and each later
row advances a byte remainder through another zero byte. Each update mixes
the prior remainder with a word in stream byte order, then XORs the eight
positional remainders. The checksum's zero initial state, non-reflected input,
zeroed checksum field and stored byte order follow the original format.
Authenticated snapshot digests use the exact captured image bytes and SHA-256.

## Equivalence and recovery controls

The new reference test computes the polynomial recurrence directly, compares
all 256 byte values at each of the eight word positions, and checks every split
of unaligned slices with zero and nonzero starting remainders. Existing known
vectors, padding corruption tests and golden disk and logical fixtures also pass.

The baseline `47fa976` and candidate `e1fd1db` use the same saved primary at
revision 93 with 31 retained roots. All paired exports are 36,851,712 bytes and
match the original capture, including its SHA-256 digest
`d0da420bcf352c15b550ac00089e6c0f65bb3b114ab8ee33d6b678b03412c040`.
Both platforms independently compared the files byte for byte. The input file
SHA-256 stayed `f7200254198776b99e778279160fc5fcc92cb132f39a08cb38b1323ac289dc27`.

## Timing

An optimized macOS kernel probe runs 16 CRC calls over an 8,388,613-byte
buffer with incrementally varied seeds. Across three runs the bit loop takes
284.3–286.5 ms, a byte lookup table takes 284.6–286.8 ms, and the eight-byte
implementation takes 118.8–119.3 ms. The probe checks each chain's final
remainder against the original recurrence.

The archived builds then run in baseline, candidate, candidate, baseline
order on macOS arm64/APFS and Docker Linux arm64/ext4. Each measured process
performs 16 read-only inspections or 16 verified live exports. The export
run includes filesystem writes, sync and image hashing. Timings include probe
overhead and host load and describe this fixture.

| Platform and phase | Baseline elapsed range | Candidate elapsed range |
|---|---:|---:|
| macOS, 16 inspections | 1.569–1.583 s | 0.529–0.540 s |
| macOS, 16 exports | 5.904–8.177 s | 3.426–4.340 s |
| Docker Linux, 16 inspections | 1.409–1.924 s | 0.551–0.638 s |
| Docker Linux, 16 exports | 4.590–6.258 s | 2.730–2.890 s |

### Authenticated follower replay

The same [Linux timing overlay](follower-installation-profile.md) on `e1fd1db`
completed 31 installs over TLS from the same released macOS sender binary.
Both endpoints reopened revision 93 with all 31 retained roots. The final
selected image matched the export. The two real process interruptions also
reopened revisions 93 and 94, respectively. A fresh authenticated poll and
the duplicate ship recovered the same revision-94 receipt and digest as the
previous drill.

| Follower daily phase p95 | Previous run | CRC candidate |
|---|---:|---:|
| Receive and write | 100.496 ms | 107.562 ms |
| Staged image sync | 9.549 ms | 57.300 ms |
| SHA-256 readback | 80.764 ms | 91.285 ms |
| Graph decode and verification | 81.922 ms | 31.854 ms |
| Manifest selection | 0.925 ms | 4.180 ms |

The graph stage was shorter in this replay. Sync and manifest time increased
under a separate host-load sample. These runs establish exact-state behavior
and phase observations for this schedule. Deployment latency remains open.

### Platform and dynamic checks

The candidate passed 663 macOS workspace tests with six registered ignores
and strict workspace Clippy. Linux passed 146 library tests and 60 selected
integration tests with one maintenance ignore. Strict MSVC x64 cross-Clippy
passed. The native Windows segmented-staging and golden results are tied to
the earlier [staging checkpoint](fresh-image-staging.md#native-windows-staging).

The CRC candidate also passed the native Windows x64/fixed-NTFS minimum
compiler job at PR head `bf246aa`. The clean merge was
`8c4473b060c19ae405385af1b5808420e8028f6a`. The bit-recurrence control,
both golden histories and the live-copy budget case passed in debug and
release, and the delivered codec source matched the branch. The ZIP digest
is `37339c2a2b1e4a8ac9b5026e7c5d59c3ad0f48c91e740e81887cb5c2de910d8c`.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43781592295/job/133019787388)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43781592295/artifacts/14188589920).

AddressSanitizer fuzz smoke at `nightly-2026-11-12` and cargo-fuzz 0.13.2
completed 24,000 executions with the archived seeds and seed value 1.

| Target | Executions | Maximum input bytes |
|---|---:|---:|
| `open_image_bytes` | 5,000 | 262,144 |
| `open_structured_image` | 10,000 | 64 |
| `commit_crash` | 3,000 | 32 |
| `logical_import_export` | 3,000 | 256 |
| `snapshot_install_schedule` | 3,000 | 96 |

The fuzzer logs and updated scratch corpora are under the proof directory.
The checked-in seed corpora serve as replay inputs.

## Reproduce

The proof directory `target/crc64-qualification/` contains both exact source
archives, the kernel probe and the phase logs. Copy the saved
`baseline/examples/live_copy_phase.rs` into each archived checkout's `examples/`
directory to run the [fixed-input phase probe](fresh-image-staging.md#reproduce-and-inspect)
on `target/live-copy-phase/month/primary.wdb`. Keep each destination directory
fresh, and compare the final export to that run's `exports/snapshot-31.wdb`.

```sh
cargo build --locked --release --features replication --example live_copy_phase
target/release/examples/live_copy_phase inspect "$source" "$staging" 16 9223372036854775807
target/release/examples/live_copy_phase copy "$source" "$staging" 16 9223372036854775807
cargo test --locked -p weekend-db --all-features --lib \
  storage::codec::tests::crc_words_and_incremental_tails_match_the_bit_recurrence -- --exact
```

| Proof file | SHA-256 |
|---|---|
| `baseline.tar` | `a2757559b69bfc2b26a31d853ffd5cb95bb5839d8a60ed675fda36ab46ff87b7` |
| `candidate.tar` | `07bc423440197756f4f08e1bebd5bcbf62e5eb76e72c7d888103974bb67e0207` |
| `kernel_probe.rs` | `a99287f0f3e499a5f36e5306dc44934addab71070a6fabd0577d20c3fc5ffd09` |
| `authenticated/run/follower-phases.csv` | `e606ec29f4d72bbed8bf09b8abce0bbd06fb643350f52008590e6a4f820e67f3` |
| `authenticated/run/interruptions.log` | `02b19d3e3b5fb58f9010a8f96d48b8250a4b92fffcd8dfe8cc742dc35c894e6f` |
