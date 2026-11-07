# C versus Rust packaging experiment

This is the milestone-1 vertical slice from [DESIGN.md](../DESIGN.md), not an
engine. Both implementations build immutable maps containing lists containing
byte values, clone a root, edit one clone, and keep the original readable.
They intentionally use copied entry arrays and shared child nodes, not the
planned hash/radix trees. There are no records, transactions, or files yet.

Run from a fresh checkout on macOS with Python 3.10+, Xcode command-line tools,
Rust/Cargo (edition 2024 support), and Odin:

```sh
python3 probe/run.py
```

The wgpu cases use Odin's installed `vendor:wgpu` bindings and their prebuilt
**static** wgpu-native archive. Install that archive as described by Odin's
vendor package if it is missing. No GPU, window, registry dependencies, or
changes to another project are needed. Without Odin, explicitly run
`python3 probe/run.py --skip-odin`; without its wgpu archive, use `--skip-wgpu`.
Skipped cases are recorded and do not establish that integration contract.

The script builds debug/release static and shared libraries for each candidate,
runs C and Rust consumers against all eight artifacts, and builds Odin consumers
both alone and with wgpu-native. It also tests C's generated single header from
two consumer translation units, allocation failures, and ASan/UBSan. Intentional
lifetime mistakes must produce an ASan use-after-free diagnostic for C and a
borrow-checker error for the Rust wrapper. No unexpected failure is accepted.
The known Rust-static/wgpu duplicate-symbol failure is a recorded experiment
result; all other linker failures stop the script.

Outputs live in ignored `probe/build/` and `probe/rust/target/`. The generated
`build/amalgamated/weekend_probe.h` is derived from the split C sources; define
`WEEKEND_PROBE_IMPLEMENTATION` in exactly one translation unit. `results.json`
records the tested toolchains, platform, archive fingerprint, and outcomes;
linker and intentional-failure diagnostics are retained beside it. Dynamic
libraries currently use local build paths; relocatable application packaging
has not been tested. Remove both output directories to clean the experiment.

## Shared ABI and ownership contract

- `include/weekend_probe.h` is the ABI for both candidates. Statuses are signed
  32-bit integers; lengths use `size_t`; tags use `int32_t`. Only bytes, maps,
  and lists are included. Missing map entries are `WP_NOT_FOUND`; empty bytes
  are a present value. Lists use zero-based bounds.
- Input slices are borrowed during a call and copied when stored. Null is valid
  only for a zero-length slice. Keys and values are binary, including zero bytes.
  Bytes are copied into caller storage; `wp_read_bytes` reports the required
  length, returning `WP_BUFFER` without a partial copy when too small.
- Each successful make/get/edit/clone returns an **owned** handle. Release each
  exactly once with `wp_release`. Children keep shared nodes alive after parent
  and original child handles are released. An edit returns a new value and
  leaves its inputs unchanged. Cross-database assignments are rejected.
- `wp_close` returns `WP_BUSY` while any value handles remain. A rejected close
  leaves the database usable. A successful close invalidates that database
  handle. `wp_release(NULL)` is harmless. Database and value handles are confined
  to one thread. No raw handle is valid after its successful close/release.
- Callers must supply valid, aligned pointers and readable/writable regions for
  their stated lengths. Output storage must not alias inputs. Handles must come
  from the same linked implementation. Raw dangling/forged/double-released
  handles are caller errors, not safely detectable status cases. The example
  Rust wrapper uses lifetimes and RAII to enforce ordinary ownership rules.
- Output handles are reset to null on status errors. The probe bounds bytes and
  keys to 1 MiB, each collection to 4,096 entries, and nesting to 64 nodes. These
  are per-value limits, not a total memory budget. Arbitrarily many live handles
  remain possible. These are experimental limits, not engine decisions.
- C uses checked allocation and reference counting, returns `WP_OOM`, and cleans
  up partially constructed results. Its allocator hook is exercised at each
  allocation site in the representative operations. Rust uses `Rc`, `Vec`, and
  `Box`; ordinary panics in status-returning operations are caught as `WP_PANIC`,
  but allocator exhaustion and reference-count overflow abort the process.
  **Recoverable allocation failure is not demonstrated for Rust.** A Rust engine
  must address fallible allocation before claiming the design's resource contract.

## Observed results, 2026-11-05

Tested on arm64 macOS 27.0.1 with Apple clang 21.0.0, rustc 1.99.1,
Cargo 1.99.1, and Odin `dev-2026-10:a22aa1984`. The installed wgpu-native reports
`0x1d000101`; archive SHA-256:
`7d8b688a6b55f0728c847e1c3b028d07ccb1009d8cea4833a6186a5775384248`.
These results do not establish Linux/Windows support or compatibility with every
wgpu-native/toolchain combination.

| Candidate distribution | C consumer | Rust consumer | Odin | Odin + static wgpu-native |
| --- | --- | --- | --- | --- |
| C static library | Pass | Pass | Pass | Pass |
| C shared library | Pass | Pass | Pass | Pass |
| Rust static library | Pass | Pass | Pass | Duplicate `_rust_eh_personality` |
| Rust shared library (`cdylib`) | Pass | Pass | Pass | Pass |

Every row was tested in debug and release. C's generated single-header form,
shared ABI checks, C allocation failures, sanitizer checks, and deliberate
lifetime diagnostics also pass. The final script records 29 cases, including
the two reproduced linker incompatibilities, as well as the individual C/Rust
consumer runs within each library case.

This reproduces the concern from the existing kui consumer without linking kui
or changing its source. The [Rust linkage reference](https://doc.rust-lang.org/reference/linkage.html#mixed-rust-and-foreign-codebases)
also explains why independently built Rust static libraries can conflict.
No duplicate-symbol suppression flags or modified vendor archives are used.

C's reference management, partial-allocation cleanup, overflow checks, and
recursive destruction are explicit. Rust expresses child sharing and cleanup
through `Rc` and owned containers; its unsafe work is concentrated at the FFI
boundary. The consumer wrapper rejects a value escaping the database at compile
time; raw C requires dynamic instrumentation to catch the demonstrated mistake.
Neither implementation prevents misuse of raw pointers by foreign callers.
This is a qualitative code/diagnostic comparison, not a measured iteration-speed
or database-performance benchmark.

**Recommendation for review:** Rust, with a native Rust library for Rust
consumers and a C header plus `cdylib` for Odin/foreign consumers. The tested
independent-Rust-static-library combination must be excluded. This gives up
C-only/single-header source distribution and requires shipping a shared library
for that Odin integration. If that distribution tradeoff or Rust's additional
fallible-allocation work is unacceptable, C has a working single-header route.
The user accepted Rust with a shared library for Odin on 2026-11-05. The engine
and its fallible allocator now live at the repository root; see DESIGN.md for
the accepted contract and progress through milestone 5. This directory preserves
the original comparison, including the probe's allocation limitations.
