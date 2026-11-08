# C and Rust AddressSanitizer profile

`python3 tools/c_sanitizers.py --output NEW_DIRECTORY` builds a diagnostic
shared library with nightly Rust, rebuilt instrumented standard library, and
features `crdt,replication`. It compiles the existing C memory, threaded, and
file consumers with Clang ASan, then runs them against that exact library.
The file consumer includes retained-root reopen, release and compaction.
This is a diagnostic build; private release-artifact qualification is separate.

The profile explicitly links the shared ASan runtime selected by Clang into
both sides. It records compiler identities, runtime/library/consumer hashes,
commands, logs, input source hashes and sanitizer options. Inherited Rust flags,
compiler wrappers and suppression settings are replaced. A source change during
execution rejects the evidence. Linux enables leak detection; macOS does not
claim LeakSanitizer coverage. No invalid/released library handles are fabricated.
The memory consumer now allocates the full oversized input before testing Limit,
so even rejected input obeys the C API's live-buffer contract.

Before compiling the engine, a separate calibration library and C caller run
four controls: valid C and Rust reads must succeed, and deliberate out-of-bounds
heap reads in each language must fail with the ASan heap-buffer-overflow report.
These defects exist only in calibration subprocesses. An absent, incompatible
or ineffective runtime fails the profile rather than producing a green result.

On arm64 macOS 27.0.1, Apple Clang 21's runtime failed to link Rust's required
ASan version symbol. LLVM Clang 23.1.5 with rustc 1.101.0-nightly
(708c94ef0, LLVM 23.1.5) passed all four controls and all three consumers:

```sh
python3 tools/c_sanitizers.py --cc /opt/homebrew/opt/llvm@23/bin/clang \
  --output target/c-sanitizer-qualification
python3 tools/test_c_sanitizers.py
```

Recorded evidence is `target/c-sanitizer-6266359/evidence.json`: the engine
is archived checkpoint 6266359, overlaid only with this runner and the corrected
C memory consumer. The new CI job runs native Linux with the same fail-closed
calibration. Its first result remains pending. This is bounded dynamic evidence,
not proof of all unsafe paths, arbitrary C caller misuse, or race freedom.
