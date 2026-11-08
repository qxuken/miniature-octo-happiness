# Storage fuzzing

The fuzz workspace exercises the real byte codec and publication path
through `Database::open_on` and `SimulatedDisk`. Its lockfile and
dependencies are separate from the library workspace. The checked-in
seed inputs live under `seeds/`.

| Target | Input and oracle |
|---|---|
| `open_image_bytes` | Raw images up to 256 KiB. A rejected image must return a documented error before writes or syncs. A successful open may change only the selected witness and must preserve current and retained revision metadata across power-loss replay. |
| `open_structured_image` | A real v2 fixture with roots 3 and 4 and retained revision 2. Directed cases check exact revision, row values, retained state, or error. The patch mode mutates metadata and graph regions and can repair CRCs to reach the structural checks. |
| `commit_crash` | A generated transaction on a small retained history. It injects an I/O error, short write, tear, or sync failure, then simulates power loss. An independent copying map checks both possible logical states and the retained snapshot. The barrier oracle checks an exact revision where persistence is determined. Recovered stores must accept and preserve a later commit. |
| `txn_schedule` | Up to 32 commands interleave three drafts over two collections and four keys. An independent copying model and committed write history predict every commit or conflict and check reads, scans, final rows, revisions, terminal state, and pinned/retained snapshots. |

The raw image target compares revision metadata because it accepts
arbitrary collections. The structured and crash targets compare full
row state for their known collection. The crash target models one
publication fault and can inject a second failure at the first recovery
I/O. Arbitrary partial root persistence allows the whole prior or next
revision. The simulator and these short runs are evidence for the
modeled storage contract.

Use a nightly Rust toolchain and `cargo-fuzz`. The smoke run below keeps
the mutable corpus in a temporary directory and uses AddressSanitizer.
Run from the repository root. [Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz/guide.html)

```sh
rustup toolchain install nightly
cargo install cargo-fuzz --version 0.13.2 --locked
cargo +nightly fuzz build --target-dir target/fuzz-build

run_dir=$(mktemp -d)
for target in open_image_bytes open_structured_image commit_crash txn_schedule; do
  mkdir "$run_dir/$target"
  case "$target" in
    open_image_bytes) cap=262144; runs=10000 ;;
    open_structured_image) cap=64; runs=10000 ;;
    commit_crash) cap=32; runs=5000 ;;
    txn_schedule) cap=96; runs=10000 ;;
  esac
  cargo +nightly fuzz run --target-dir target/fuzz-build "$target" \
    "$run_dir/$target" "fuzz/seeds/$target" -- -runs="$runs" -max_len="$cap" -seed=1
done
```

Replay a saved input by passing the file in place of the corpus paths.
Keep minimized findings with the target's seed corpus and record the
replay command and toolchain.

```sh
cargo +nightly fuzz run --target-dir target/fuzz-build open_structured_image \
  fuzz/seeds/open_structured_image/lost-witness-and-slot -- -runs=1
```

The seed generator and representative replay tests also run without
fuzz instrumentation.

```sh
cargo run --locked --manifest-path fuzz/Cargo.toml --example seed_corpus
cargo test --locked --manifest-path fuzz/Cargo.toml --lib
```

The structured input's first byte selects a case modulo 20. Cases 0–9
are directed. Modes 1 and 2 use the low bit of the second byte to clear
the newest witness. Cases 10–19 read up to twelve five-byte patches.
Each patch specifies a region, a little-endian offset within that
region, an XOR byte, and a checksum repair flag. The crash input starts
with five bytes for I/O position, fault mode, crash policy, a parameter,
and flags, followed by up to twelve key/value edits. The first three
I/O positions directly select the append, root, and witness syncs.

The schedule input consists of three-byte commands. Control bits 0–1
select a draft modulo three, bit 2 selects a collection, and bit 3
selects whole-database validation when beginning a draft. Bits 4–6
select begin, get, put, delete, scan, commit, drop collection, or abort.
The second byte selects a key modulo four. The third byte is a signed
integer value for put. Commands without a matching active draft are
ignored. Remaining drafts attempt commit in slot order at the end.
Six seeds cover independent edits, deletion conflicts, absent-key
observations, empty scans and collection drop, whole-database
validation, and draft overlays. This target uses in-process schedules
and keeps one fixed retained snapshot. Allocation failure, generated
retain/release sequences, and real thread scheduling remain separate
targets or tests.

The first batch passed on arm64 macOS with rustc
`1.101.0-nightly (708c94ef0 2026-11-03)`, cargo-fuzz 0.13.2, and the
locked libfuzzer-sys 0.4.13. AddressSanitizer completed 10,000 raw-image,
10,000 structured-image, and 5,000 commit-crash executions with no
findings. The single-input replay and the two representative replay
tests also passed. These are smoke results for the first corpora.

The second batch ran `txn_schedule` for 10,000 executions with
AddressSanitizer, seed 1, and a 96-byte input cap on the same nightly
toolchain and platform. It completed without findings. All three
stable replay tests and warnings-denied Clippy checks passed.
