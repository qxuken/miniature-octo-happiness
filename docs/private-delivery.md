# Same-source private delivery qualification

The macOS arm64/APFS and Docker Linux arm64/ext4 lanes ran from clean
`384ca0a225b46da863094192701d32a26409e73d` with Rust and Cargo 1.99.0.
The new macOS command uses the same checks and output structure as
`linux-msrv` and `windows-msrv`. It records the native arm64 architecture
and APFS runtime volume. A different compiler or a temporary directory
inside the checkout rejects before packaging.

| Evidence | macOS arm64/APFS | Docker Linux arm64/ext4 |
|---|---:|---:|
| Workspace passes per debug and release profile | 686 | 722 |
| Workspace skips per profile | 6 | 7 |
| Bounded controller commands | 51 | 52 |
| Controller payload hashes | 3,354 | 3,357 |
| Locked inputs | 13 | 13 |

Both lanes passed strict workspace Clippy and the debug and release
examples, compiled C11, C++17 and Odin package consumers, and built the
Rust consumer from the shipped source. The independent audit matched all
719 source-file hashes against the same Git archive. It rehashed controller,
package and trace payloads, checked process cleanup, and matched both
relocated copies of each library and manifest. Each optimized Rust, C
and Odin transcript matched the same 134-cell oracle byte for byte,
with SHA-256 `0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3`.

| Combined package and trace SHA-256 | macOS arm64 | Linux arm64 |
|---|---|---|
| Controller | `db9f75b5c6b09ce5dd42b1863d8dc19bfb02bd3db4424cc9ff78858f860b9a80` | `64cd74ec574afdfb036687095a6724e7a3548fdb44a61a77fb98b0adb08a28f8` |
| Package manifest | `e66ea9049fcb5957b68e6c48dbf1cc39d64645d98d65ad76e4d1adf87c96cdc9` | `330de5475ec971fa56b81b84341aa9c8c0cd180b5a84275facf00b2e86cf986e` |
| Shared library | `8568bda3fe7be4c23e2cbb7b470319540492de0620f40e28ac1d77436698717e` | `3a34774b2fc77563aaa8fd4992dacc09bdf8d66d862d2318dcd03ead1ab89a2a` |
| Trace report | `ece6e188adfe69c78bb37baa80dd1867a197dab55236cc2d5b214e2b23377031` | `0c5515dcaff010f023594b35c0bc68e812efbb79c1a5f0789b202439b778c4ed` |

## macOS feature flags and failure recovery

The base, replication-only and CRDT-only packages pass C11 and C++17
smoke and relocation. Cargo and the exact runtime agree on masks 0, 1
and 2 respectively. The combined package reports mask 3 and carries the
Rust source and Odin checks above.

| Isolated macOS package | Manifest SHA-256 |
|---|---|
| Base | `ca360100e14a9be78eb6528d84e647c4184ff616be54f05f454a1db142d3dcb1` |
| Replication | `591eee22f37778faecff72be669adda1202873decdc5822101db6837b950acf4` |
| CRDT | `3cb645d94325b7887a21a1b89e6bf7f715ab67b5c2fdbc24156be603c5baf060` |

C and Odin each pass five injected publication and recovery cases against
the combined dylib directly above. C also receives real kernel `ENOSPC`
on an owned APFS image, with no injection hits. A fresh process reopens
revision 3 with retained revision 2 while still full. The retry reaches
revision 4 with retained revision 2. The owned volume and process cleanup
checks passed. The C and Odin evidence hashes are
`c41a515666088ce6aaadd9990e2623fe620e99bbff7944d495b32c390a22c5fe` and
`f8bd9861354888db9d73257b8f1ed6ed9acbff6bb7cd2f6de0da8c98ac08ee32`.

## Reproduction

Use the exact clean checkout and select the declared Rust compiler with
rustfmt and Clippy. Cache the workspace, renamed typed consumer and
private Rust consumer locks before the offline run. Supply the native Odin
compiler on `PATH`. Choose fresh output directories. On Mac, keep the
temporary directory on APFS and outside the checkout.

```sh
cargo +1.99.0 xtask macos-msrv --odin --output target/macos-msrv
cargo +1.99.0 xtask package --output target/macos-base
cargo +1.99.0 xtask package --features replication --output target/macos-replication
cargo +1.99.0 xtask package --features crdt --output target/macos-crdt
cargo +1.99.0 xtask faults delivered --macos --apfs-full \
  --package target/macos-msrv/package --output target/macos-c-faults
cargo +1.99.0 xtask faults delivered --macos --odin "$(command -v odin)" \
  --package target/macos-msrv/package --output target/macos-odin-faults
```

The Linux command was `cargo xtask linux-msrv --odin --output /tmp/evidence`
inside the isolated arm64 image
`sha256:d276cf4b0967c0337be30eb9c302bf3b77794a7ade6b667968ebe5cbc037317c`.
The checkout and temporary directory were on a private ext4 volume.
Mount the locked registry cache at the configured Cargo home and the
account's default Cargo home. The trace deliberately drops the configured
`CARGO_HOME`, and a run with only that mount failed offline metadata
resolution. The rerun used the same cached registry at both paths.
The Linux Odin executable SHA-256 is
`73b9e13f4239aa695de32b8de3513b7b1150d821659920b2f1b837cdb7c2e454`.
The controller records the Mac Odin executable hash in its own manifest.

The local archive `target/archived-proof/private-delivery-384ca0a.tar.zst`
contains the exact Git archive, packages, traces, fault results and
independent audit. Its SHA-256 is
`5f3c308e8624c306477c587c75d39cea687c4589548ce541ac8e7a51d73d76b4`.

These two lanes establish one shared source identity for their packages
and tests. Native x86-64 Linux and Windows runs on other commits retain
their recorded scope. The full candidate also needs the final platform
matrix, feature profiles, sanitizer and fuzz evidence, deployment workload
and recovery evidence, and owner approval.
