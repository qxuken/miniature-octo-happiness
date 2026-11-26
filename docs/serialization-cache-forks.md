# Near-cap serialization cache forks and fresh opens

This R3 checkpoint compares fresh process opens and the first durable
publication on macOS arm64/APFS and Docker Linux arm64/ext4. Both used Rust
1.100.0 and release optimization with overflow checks. The clean engine is
from `7aebd5d4ddedf1206b60c59c3bb2711cf7c662c5`. Both builds overlay only the
[qualification example](../tools/serialization-scale/examples/cache-forks.rs).
A second disposable build adds six counters at the existing cache sites.

## Workload and verification

Each seed has two collections, 65 fixed 888 KiB byte values and 12 history
rows in each of 64 distinct data revisions. Each history cell is a list
of two references to one 1 KiB bytes node. The shared shape allocates one
list and one bytes node per epoch and reuses them across its 12 rows. The
independent shape allocates one list and one bytes node per row. The two
shapes have the same keys, values and retained revisions. The seed's current
revision is 138 and the first 63 epochs are retained. The shared image is
60,796,928 bytes (90.59% of the cap) and the independent image is 66,564,096
bytes (99.19%).

Each sample starts a new process on a fresh copy of its seed. The open
interval measures the public native open, including graph verification,
index construction and stabilization. The filesystem cache is warm from
the copy and the experiment does not flush it. After open, the process stages
eight uncommitted drafts with separate 32 KiB values. Those drafts stay
live during one of the following publications:

| Publication | Change | Retained roots after reopen | Appended bytes |
| --- | --- | ---: | ---: |
| Metadata | Retain the current epoch as snapshot ID 64 | 64 | 4,096 |
| Reuse value | Add a history key with an existing list handle | 63 | 8,192 |
| New value | Add the same key with an equal, newly allocated list and bytes node | 63 | 16,384 |

The publication interval measures the retain or transaction commit call.
Draft staging, value construction, snapshot acquisition for retain and the
oracle run outside the interval. The oracle checks every key and complete
value at revision 139 and every retained revision. It checks both list
children, the catalog, the exact revision of each retained root and the
eight staged values before dropping the drafts. In the metadata case a
65th retain returns `Limit` with the same revision, catalog and image
length. A separate process then opens each resulting image and repeats the
current and retained checks.

Seven clean samples per platform, shape and publication give 84 timing
samples. The 12 instrumented samples match their clean image bytes and
core charges. The four independently created seed files have different
database identities. For each of the 12 cross-platform replays, the opposite
platform starts from a byte-for-byte copy of the original seed and reproduces
the complete destination file, including its identity. The destination
remains byte-identical after a fresh reopen on that platform.

## Index construction and forks

The instrument counts VALUE entries supplied to index construction and
DATA entries supplied to index construction. For each successful fork it
counts entries and integer index positions copied. Both platforms report the
following exact counts. The timing table below uses only clean samples.

| Shape | Open VALUE entries | Open DATA entries | New-value fork VALUE entries | New-value fork VALUE index slots |
| --- | ---: | ---: | ---: | ---: |
| Shared | 193 | 64 | 193 | 512 |
| Independent | 1,601 | 64 | 1,601 | 4,096 |

Metadata publication copies no cache entries or slots. Both writes copy 64
DATA entries and 64 sorted positions. Only the new-value write forks the
VALUE cache. These are operation counts, including all occupied and empty
slots copied in a successful fork. They do not attribute elapsed time to
the copy, index construction or native I/O in isolation.

## Observed costs

The table gives the median and range of seven clean samples in milliseconds.
The charge is the whole core heap high-water mark just after publication.
The fresh-open high-water mark is 61,065,672 bytes for shared and
62,306,600 bytes for independent on both hosts. The configured core heap
budget is 160 MiB.

| Host | Shape | Publication | Open median [range], ms | Publication median [range], ms | Peak core bytes |
| --- | --- | --- | ---: | ---: | ---: |
| macOS | Shared | Metadata | 73.09 [62.45–162.10] | 13.15 [8.41–46.32] | 61,212,680 |
| macOS | Shared | Reuse value | 76.61 [71.04–173.29] | 14.74 [8.76–36.40] | 61,224,464 |
| macOS | Shared | New value | 75.45 [70.10–149.74] | 13.85 [12.63–35.88] | 61,248,552 |
| macOS | Independent | Metadata | 98.39 [95.12–113.83] | 14.67 [9.64–16.22] | 62,467,592 |
| macOS | Independent | Reuse value | 98.42 [96.10–236.78] | 14.74 [10.07–17.93] | 62,479,376 |
| macOS | Independent | New value | 104.15 [98.63–243.15] | 14.61 [12.78–19.92] | 62,603,816 |
| Linux | Shared | Metadata | 65.13 [63.74–73.80] | 2.68 [0.23–11.06] | 61,212,680 |
| Linux | Shared | Reuse value | 63.45 [61.66–74.11] | 2.46 [0.29–4.40] | 61,224,464 |
| Linux | Shared | New value | 66.24 [61.99–153.08] | 0.99 [0.28–3.62] | 61,248,552 |
| Linux | Independent | Metadata | 68.51 [66.65–72.71] | 3.87 [0.27–31.95] | 62,467,592 |
| Linux | Independent | Reuse value | 70.22 [66.55–79.28] | 2.44 [0.87–3.43] | 62,479,376 |
| Linux | Independent | New value | 69.60 [68.06–90.49] | 1.98 [1.08–7.62] | 62,603,816 |

Whole measurement-process peak RSS, including the final oracle, ranged from
62.64 to 64.05 MiB on macOS (`time -l`) and 61.22 to 62.73 MiB on Linux
(`VmHWM`). The Mac host also ran the Linux VM during the campaign. These
samples give a bounded observation on development hardware. R3 still needs
sustained deployment schedules, end-to-end queue and lag measurements and
approved service limits on the intended hardware.

## Reproduction and evidence

Use a fresh seed path. Repeat the copy for each sample and each mode.

```sh
cargo +1.100.0 build --offline --locked --release \
  --manifest-path tools/serialization-scale/Cargo.toml --example cache-forks
probe=tools/serialization-scale/target/release/examples/cache-forks
"$probe" seed shared /tmp/cache-forks-shared.seed
cp /tmp/cache-forks-shared.seed /tmp/cache-forks-shared.metadata.wdb
"$probe" measure shared metadata /tmp/cache-forks-shared.metadata.wdb
"$probe" verify metadata /tmp/cache-forks-shared.metadata.wdb
```

Run both `shared` and `independent` with `metadata`, `reuse-value` and
`new-value`. The archive includes the exact source archive and overlay,
the two-file instrumentation patch, runner scripts, both platforms'
binaries and logs, 16 raw images stored compressed, and the reconciliation.
It records the Linux container image digest, mounting ext4 for all build
and runtime files. Fresh extraction checks the full payload index, all
96 native case records, 12 cross-platform image replays and 20 existing
engine test results. The five directed engine tests passed in debug and
release on each host. They cover cold-open allocation refusal before
stabilization and lease release, fork allocation rollback, unchanged cache
reuse, and the DATA and VALUE golden images. The clean example passes
strict release Clippy on both hosts.

The local archive is `target/archived-proof/cache-forks-7aebd5d.tar.gz`.
The target catalog and source pins account for the example's 28 new
tooling decisions. The reconciler reports 10,802 records in 225 files,
zero audit failures and 7,368 open tooling records.
[Archive and scope checksums](decisions/cache-forks-7aebd5d.sha256)
