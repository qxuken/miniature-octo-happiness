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
| `value_ops` | Bounded map/list edits compared with the existing copying `BTreeMap`/`Vec` model. Includes nested values, common-prefix and empty keys, float bit patterns, list radix boundaries, invalid indices, and immutable histories surviving database drop. |
| `image_and_ops` | Generated edits, retains, releases, empty commits, reopens and compact copies around a controlled image mutation. An independent model tracks full current and retained rows, publication/data revisions, and retained IDs. |
| `retention_compact_crash` | Generated retained history followed by a fault at a compact-copy I/O. Acknowledged copies reopen completely. Any recoverable interrupted copy matches the full independent model, and the unchanged source remains writable with preserved retained-ID allocation. |
| `ffi_ops` | Exported C calls through opaque valid handles. Generated create/clone/release/map edits check native Rust byte values, foreign-owner errors, buffer sizing and sentinels, and busy-close/final-close behavior. |
| `crdt_frames` | Arbitrary or structured counter frames through receiver merge and sender acknowledgment validation. Invalid frames preserve state, valid merges match independent component totals, and repeated offers add no commit. |
| `crdt_schedule` | Two durable counter/set records with offline edits, delayed offers, lost replies and reopen. Independent integer components and membership bitsets predict every state and final convergence with no extra repeat-sync commits. |

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
for target in open_image_bytes open_structured_image commit_crash txn_schedule \
  value_ops image_and_ops retention_compact_crash ffi_ops; do
  mkdir "$run_dir/$target"
  case "$target" in
    open_image_bytes) cap=262144; runs=10000 ;;
    open_structured_image) cap=64; runs=10000 ;;
    commit_crash) cap=32; runs=5000 ;;
    txn_schedule) cap=96; runs=10000 ;;
    value_ops|image_and_ops|retention_compact_crash) cap=192; runs=3000 ;;
    ffi_ops) cap=192; runs=10000 ;;
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

The value input has up to 64 three-byte commands. The low three bits of
byte one select map set/remove/get or list push/insert/set/remove/get.
The remaining bits select scalar, bytes, or a nested map payload. Byte
two selects a key or list index, with its high bit producing a 128-byte
common key prefix. Byte three supplies payload bits. Lists start with
33 integers to exercise edits around the first radix boundary. Every
prior root is checked again after dropping the database. Forced full
hash collisions remain covered by the internal tree tests because the
public interface computes its own hashes.

The history inputs use three-byte commands for put, delete, retain,
release, empty commit, reopen, and compact copy, selected modulo seven
by the first byte. The second byte selects a key modulo eight and the
third supplies the value or retained-ID selector. The model records
retained rows and revisions independently when each retain occurs.
`image_and_ops` prefixes the program with a mode selecting a clean image,
an abandoned tail, a missing latest witness, truncation, or corruption
of the selected commit object. It executes up to 12 commands before and
12 after the image mutation. These mutations have exact accept/reject
oracles. Arbitrary checksum-repaired image mutation is covered separately
by `open_structured_image`.

`retention_compact_crash` starts with I/O position, fault mode, and crash
policy bytes, then up to 12 history commands. It selects the I/O from a
successful copy trace and injects an error, one-byte short I/O, torn
write, or failed sync. Crash policy selects dropping or persisting the
pending writes. A successful copy must reopen. A failed copy may reject
or reopen the entire generated state, and every accepted copy must
preserve all retained roots. The source's bytes and logical state must
remain intact, and a subsequent retain must use the next modeled ID
and survive reopening. Namespace replacement faults and persistent or
compound copy faults remain separate coverage.

The FFI input has up to 64 three-byte commands. Byte one's high bit
selects one of two database owners and the byte modulo five selects
create, clone, byte-copy, map-set/get, or release. Byte two selects one
of eight value handles and the length for creation. Byte three supplies
contents, destination slot, buffer capacity, or map key. All non-null
pointers address valid buffers or allocated opaque handles. This target
calls symbols linked from the Rust library. Compiled C/C++ clients,
invalid-pointer caller-contract violations, transactions, scans, and
threaded FFI schedules require their own checks.

The expanded core batch completed 3,000 AddressSanitizer executions each
for `value_ops`, `image_and_ops`, and `retention_compact_crash`, and
10,000 for `ffi_ops`, all with seed 1 and a 192-byte input cap. No
findings occurred. The same arm64 macOS nightly toolchain was used.
All four stable replay tests and warnings-denied Clippy passed. The
four new targets add 35 deterministic seeds generated by
`examples/seed_corpus.rs`. These bounded smoke runs leave long-running
campaigns and the unimplemented target inventory open.

## Snapshot replication targets

`snapshot_frames` accepts up to 4 KiB of arbitrary reply bytes and drives
`Primary::poll`. Its independent oracle decodes the documented status and
acknowledgment layout, checks stream and verified-head bounds, and predicts
monotonic progress relative to a previously accepted acknowledgment. After
failure, replay of the prior acknowledgment must still succeed.

`snapshot_install_schedule` uses public native `Follower` APIs in a unique
temporary directory and processes up to six two-byte commands. The low bit
selects an export at revision 3 or 4. The next three bits select successful
upload, truncated upload, payload corruption, dropped Ready reply, dropped
terminal acknowledgment, poll, cleanup or malformed request. The second
byte parameterizes truncation, corruption and malformed-frame selection.
Every command is followed by close and reopen. An independent state model
checks the exact durable acknowledgment, current value and retained value.
The target exercises transport failures and native lifecycle behavior.
Simulated filesystem faults are covered by the follower unit fault suite.

Generate the 29 snapshot seeds with
`cargo run --locked --manifest-path fuzz/Cargo.toml --example snapshot_seeds`.
Stable replay is included in the fuzz library tests. On arm64 macOS,
AddressSanitizer completed 1,000 snapshot frame executions and 587 native
install schedule executions before the 30-second time budget ended, with
no findings. Evidence is under
`target/fuzz-evidence/tmp6nhpcqqd`. The native target requires a writable
supported local filesystem and is slower because it performs real durable
writes. Authentication remains the embedding application's responsibility.

## Typed schema/index migration and derive targets

`schema_index_migration` executes up to 48 four-byte commands through public
`weekend-db-typed` APIs. It starts with version-one records, registers version
two, then migrates selected IDs, inserts, updates unique keys, deletes, rebuilds
indexes, reopens the durable memory image, or submits duplicate migration IDs.
A separate `BTreeMap` predicts primary rows, versions, defaulted values and
unique ownership. After every command, all rows and every candidate unique key
are compared against that model. Callback errors, missing IDs and unique
collisions have explicit expected errors and leave the model unchanged.
`check_indexes` is an additional assertion, not the source of expected owners.
This target does not inject storage faults or model arbitrary schema shapes.

`derive_tokens` drives the exact internal `Model`, `Record` and `Resolution`
expansion functions used by the proc-macro entrypoints. Model expansion lives
in a private module; the fuzz library includes the three implementation files
by path, with no copied parser or new public macro API. Inputs up to 8 KiB are
parsed as derive input, and a second bounded Model fixture varies stable IDs
and versions from the first two bytes. Both successful expansions and emitted
diagnostics must parse as Rust file syntax. This oracle detects panics and
malformed emitted syntax, not every type-checking or semantic error. Stable
replay includes valid and invalid examples for all three derives.

Eight deterministic seeds are checked into `fuzz/seeds` for these targets.
Actual downstream compiler rejection is tested separately by
`cargo test --offline --locked -p weekend-db-macros`: its integration test
builds twelve malformed attribute fixtures with Cargo/rustc, verifies failure
and the intended diagnostic, and uses a private temporary target directory.
The cases cover zero/duplicate/reserved IDs, unknown attributes, unsupported
generics, repeated/unknown/omitted wire fields and invalid resolution placement.

The initial arm64 macOS ASan smoke completed 1,000 executions per target with
seed 1, an 8 KiB input cap and no findings. Typed schedules took 19 seconds of
fuzz execution. Evidence and exact toolchain/commands are in
`target/fuzz-evidence/typed-derive-batch/evidence.json`. All ten stable fuzz
library tests and all-target warnings-denied Clippy passed. The macro suite
passed in debug/release, all 26 typed integration tests and six doctests passed,
and all eleven CRDT integration tests passed after the expansion extraction.
StudyProgress still converged to weekly goal 90 and session limit 20 with
repeat sync leaving both peers at revision 4. These are bounded smoke results,
not an exhaustive schema or macro semantic proof.

Operational copy boundaries are exercised by `logical_import_export`,
`backup_restore`, and `bytestore_contract`. Regenerate their 66 committed seeds
with `cargo run --manifest-path fuzz/Cargo.toml --example operations_seeds`.
The logical target accepts bounded raw streams, XOR edits, truncation, or a
known valid stream, plus a checksum-repaired mutation mode (including zero next-retention-ID rejection); rejection must leave the empty destination untouched and
success must produce the identical canonical export. The fixture additionally
checks current records and retained revision 2 independently of the codec.
The backup target faults destination publication, crashes, injects another
recovery I/O error, crashes again, and reopens. An acknowledged backup must
remain complete; any image that reopens must match the source's current and
retained roots. Source bytes remain unchanged; invalid sources cannot modify
an occupied destination. `restore_file` shares the exercised `backup_on`
implementation, but native path handling is outside this harness.
The ByteStore target bounds short progress to the first 64 calls, choosing
0/1/63/4095/4096/8192 bytes, then resumes normal progress. Zero progress must
terminate with an error; positive progress must preserve the complete copy.
These targets do not establish native filesystem correctness, broad schema
coverage, exhaustive crash coverage, or full MC/DC coverage.

## Shipped application protocol boundaries

`coordinator_protocol` includes the exact private module used by the shipped
write-coordinator. It shares the bounded line reader, authentication matcher,
command execution and durable receipt implementation. The socket wrapper still
sets its original absolute request deadline before each byte read. The harness
uses a cursor and a deterministic deadline callback, independently checking
newline position, the 256-byte limit, UTF-8 and premature EOF, plus a forced
pre-read timeout that must consume no bytes. Raw authentication is compared
against the configured exact prefix; generated authorized/unauthorized requests
then drive real state changes.

Up to 32 commands run against a durable MemoryStore. An independent
revision/value/receipt map predicts get, successful/stale set, identified commit,
old acknowledgment replay, mismatched reuse and invalid IDs/numbers. Every
operation checks state and every eighth operation reopens the image. Receipts
are bounded to eight IDs here; capacity exhaustion is covered by the existing
application test at the real 1,024-receipt limit.

`peer_transport` exercises the shipped study-peer application's exact enrollment
parser and local command executor, including its existing conflict retry helper.
These functions were moved into a private module and are included by path,
without a duplicate test parser or new public API. Arbitrary command text checks
that rejection leaves revision and CRDT state unchanged. Generated show,
practice and edit commands use an independent counter/set/register oracle,
including zero increments, lesson bounds, malformed arity and numeric overflow.
Enrollment checks actor choice, exact hexadecimal width, nonzero identity and,
on Unix, invalid UTF-8 OS arguments. Inputs are capped at 2 KiB and generated
schedules at 32 commands.

The target name does not imply a TLS handshake or arbitrary network fault
campaign. It covers the application configuration/local-command boundary;
`crdt_frames` separately covers the engine's binary frame decoder. Certificate
loading/pinning and native deadlines remain exercised by existing application
TLS/process tests. The peer fuzz schedule has no concurrent writer and therefore
does not force the retry helper's conflict branches. No new MC/DC denominator or
whole-transport coverage claim is made. Neither target opens network listeners.

Thirteen deterministic seeds are checked in for these two targets. Run stable
replay with `cargo test --offline --locked --manifest-path fuzz/Cargo.toml --lib`
and the existing app tests with `cargo test --offline --locked -p
write-coordinator -p study-peer`. Use a separate CARGO_TARGET_DIR when other
agents are building the workspace concurrently.

The initial arm64 macOS ASan smoke completed 1,000 executions per target with
seed 1, a 2 KiB input cap and no findings. Final evidence is under
`target/fuzz-evidence/app-protocols-final`. All 13 stable fuzz library tests
passed; the final peer replay also passed after adding actor/OS-text cases.
Both applications passed all seven existing debug and release tests, including
native TLS, process concurrency, durable receipt replay and authentication
rejection. App and fuzz all-target warnings-denied Clippy passed. These checks
used the current dirty development tree, including in-progress engine memory
accounting changes, rather than a frozen release. They are bounded smoke
results and scoped regression evidence, not a complete transport qualification.

## Namespace replacement and adversarial tree keys

`replacement_namespace` includes the exact production `publish` sequencer from
`src/storage/replacement.rs`. Its adapter model keeps separate durable,
directory-flushed and volatile namespace maps. Inputs choose failure before or
after each of six publication actions, one of three crash persistence policies,
and up to eight retries with fresh candidate names. Previously orphaned candidates
and an unrelated sentinel name must survive unrelated publication attempts. An
independent completed-action oracle predicts every name in all three layers.
Each candidate references either a verified original image or its verified
compact copy; every observable canonical image must reopen with current
revision 4, retained revision 2 and exact current/historical rows.

A simulated busy lease rejects an attempt before publication or candidate
creation. Lease acquisition itself is an adapter assumption here, not exercised
native locking code. The starting candidate is already verified and durable;
atomic exchange and the directory/file persistence model are explicit premises.
The harness does not claim to simulate a real filesystem, device cache, native
orphan cleanup or power loss. Native namespace/lease fault drills remain
separate evidence. Both existing production-sequencer unit tests also run in
the ordinary stable fuzz replay build.

`tree_adversarial_keys` stresses public immutable maps/lists and their storage
in database snapshots against independent `BTreeMap<Vec<u8>, u64>` and `Vec<u64>`
models. It starts lists at 31/32/33 or 1023/1024/1025 entries, then inserts,
removes and replaces across radix boundaries. Map keys include empty, binary,
long shared prefixes and lengths through 4 KiB. Floating payloads preserve
bit patterns for signed zero, subnormal, infinities and NaNs. Every edit checks
current values and old immutable values; retained in-memory database snapshots
are rechecked after later commits. This does not assume random input will find
a full 64-bit hash collision. The existing engine unit test
`tree::tests::full_hash_collisions_and_long_prefixes` injects those collisions
explicitly and remains the collision-path evidence.

Forty-nine deterministic seeds cover all namespace boundary/persistence choices
and six list boundary sizes plus long binary keys. The tree schedule is bounded
to 32 edits and the namespace schedule to eight attempts.

The planned `txn_program` overlaps the implemented `txn_schedule`: the existing
target already generates draft begin/read/write/remove/count/scan/commit/abort
and collection drops, both validation modes, and independent observation/conflict
oracles. A second name would add no assurance. Larger key/value spaces are
covered by these tree/value targets; arbitrary schema-aware transactional
programs or additional concurrent scheduling would require a new oracle, not
just another entrypoint.

On arm64 macOS, ASan completed 1,000 namespace executions and 1,000 final tree
executions with seed 1 and a 256-byte input cap, without findings. Namespace
execution took 7 seconds and the final tree run took 16 seconds. Evidence is in
`target/fuzz-evidence/tree-namespace` and `target/fuzz-evidence/tree-final`
respectively. The final tree oracle also rejects duplicate enumeration and
cross-checks map length and point lookups. Full stable fuzz replay passed all
17 tests; final tree replay, warnings-denied Clippy and the injected collision
unit test passed. These are bounded smoke results on the current development
source, not measured whole-tree MC/DC or native filesystem qualification.
