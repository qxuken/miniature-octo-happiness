# Packaged capacity benchmark at 0601618

The verified private package for commit `0601618` supplied the Rust source for both runs. Each release benchmark created a 1,014,595,584-byte format-4 file on a 1 GiB capacity setting, retained a revision, edited a key and used a 64 MiB reader budget. Each observation used a fresh process. The timed operations ran in order: witnessed read-only open, a point lookup of a 1 MiB value, then explicit verification of the entire reachable image. Peak bytes are engine allocations.

| Platform and cache state | Processes | Open p50 / p95 (ms) | Point p50 / p95 (ms) | Full check p50 / p95 (ms) | Peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| macOS arm64, APFS, warm | 50 | 0.978 / 2.082 | 0.834 / 1.414 | 703.852 / 872.387 | 2,109,181 |
| Docker Linux arm64, ext4, warm | 50 | 0.838 / 1.438 | 1.074 / 1.733 | 690.197 / 873.437 | 2,109,173 |
| Docker Linux arm64, ext4, cold guest page cache | 5 | 11.731 / 16.761 | 1.167 / 1.520 | 733.280 / 971.741 | 2,109,173 |

The warm trials streamed the file before starting the child. Linux used a disposable ext4 loop filesystem. Before each cold sample the driver synced the file, issued `POSIX_FADV_DONTNEED` and queried the entire mapping with `mincore`. All five samples reported zero resident guest pages. Host and device caches could still serve reads. With five samples the reported cold p95 is the maximum. The macOS run covered warm reads only.

Run from the package's delivered `rust-source` directory. Set `TMPDIR` to the filesystem being measured.

```sh
cargo +1.100.0 test --release --locked --offline -p weekend-db --all-features \
  --test storage_capacity -- --ignored --exact capacity_benchmark::packaged_reader --nocapture
```
