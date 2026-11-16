# Fuzzing exact metadata and repeated recovery

At `8acf9449de0be05511add820bed206020ebd0205`,
`commit_crash` predicts the exact durable root slot and witness after
each generated publication interruption. A fault-free commit provides
the before and after bytes. The oracle mixes their prefixes according
to the requested persistence policy and compares the resulting metadata
with the simulated disk. At the generated cutoffs the fields before
the checksum are wholly old or wholly new. The checksum can be torn
at each byte. Comparing complete records distinguishes a valid new
record from a partial write that leaves the old selection intact.

The new schedule can fail up to three writable opens after publication
fails. It selects the first read, last read, stabilization sync,
witness write or witness sync. A separate medium provides the expected
I/O positions. After each failed open and crash, the model checks the
entire durable image and requires read-only inspection to report the
exact selected revision and whether its witness is present. Inspection
must issue no writes or syncs. The next successful open may change
only the selected witness. It checks current and retained contents
against the copying model and acknowledges a further commit that
survives another crash.

| Control | Expected outcome |
|---|---|
| Odd-slot seed, failed publication witness sync | Revision 5 reopens unwitnessed. A read failure and a persisted partial witness write leave it unwitnessed. A failed witness sync can persist the entire witness, and revision 5 is then witnessed even though that open returned `Io`. |
| Even-slot seed, failed publication witness sync | Revision 6 reopens unwitnessed. Failed graph read, stabilization sync and partial witness sync leave the revision intact. A successful open finishes the witness. |
| Bypass the root checksum | The deterministic replay rejects unexpected unwitnessed revision 5 where it requires witnessed revision 4. |
| Ignore witness repair errors | The deterministic replay rejects an open that returned a handle instead of `Io`. |

The normal replay checks 208 combinations of publication parity, root
or witness sync, persisted prefix and persistence policy, plus the two
three-fault seeds above. All 29 fuzz library tests pass on native macOS
arm64 with Rust 1.99.1 and on Docker Linux arm64/ext4 with Rust 1.99.0.
Strict Clippy passes on both. The two independent engine mutations
compile and fail the new replay assertion. Neither patch is applied to
the qualified source.

## AddressSanitizer campaign

The clean macOS checkout ran `commit_crash` with cargo-fuzz 0.13.2
and `nightly-2026-11-12` compiler commit
`3ef0c22cdb86ac42dc6b8f0f1ec36857daa6f2a1`. LibFuzzer completed
**96,962 executions in 241 seconds**, at the time limit. There were no
reported findings or failure artifacts. The runner confirmed stable
source and seed hashes, a stable fuzz lockfile and process cleanup.

```sh
CARGO_NET_OFFLINE=true cargo xtask fuzz --target commit_crash \
  --toolchain nightly-2026-11-12 --runs 100000 --seconds 240 \
  --seed 3649 --max-len 41 --output target/fuzz-repeated-recovery-new
cargo test --offline --locked --manifest-path fuzz/Cargo.toml --lib
```

The local archive `target/archived-proof/fuzz-repeated-recovery-8acf944.tar.zst`
contains the source tar, exact instrumented binary, mutable corpus, reports,
replay logs and mutation patches. After extraction, all 461 indexed files
and the evidence's 320 source and 244 seed hashes matched. The archive
SHA-256 is
`489f0aeb4c3ed9a5050842d45cf7ecb52f36951285f59767760e9ecf6d409ae7`.

This run covers the generated metadata cuts and one small retained
history under the simulator. Other append patterns, larger graphs,
arbitrary corruption, instrumented standard-library coverage, Linux
ASan for this campaign and physical power interruption retain their
own evidence requirements.
