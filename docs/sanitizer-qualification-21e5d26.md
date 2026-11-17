# Fresh sanitizer qualification of 21e5d26

New diagnostic runs on 2026-11-13 passed for frozen source
`21e5d2638c24c6254ced35d0077c55bb30a287b2`: the complete Rust workspace under
ASan and TSan on native arm64 macOS, and calibrated shared-runtime C/Rust ASan
consumers on macOS and arm64 Docker Linux. The Linux result covers the C
memory/thread/file consumers and calibration controls; it is not a Linux Rust
workspace ASan or TSan result. These finite executions do not prove general
memory safety, race freedom, hardware durability or production readiness.

No production source changed. Existing installed compilers/runtime images were
inputs; libraries and instrumented standard libraries were built into fresh
qualification output directories. Old checkpoint results retain their original
scope. No publication or deployment occurred.

## Native macOS Rust workspace

The original `tools/sanitizers.py address` and `thread` commands ran from the
clean detached 21e5d26 source at `/tmp/weekend-qualification-21e5d26`. Each invokes
`cargo +nightly test -Zbuild-std --workspace --all-features --target
aarch64-apple-darwin`, with the corresponding sanitizer flag for Rust and
rustdoc. Rebuilding std instruments standard-library operations as well as
workspace code. Each run exited zero and produced 53 successful test summaries:
**397 unit/integration tests plus 8 doctests, 405 reported tests per run**.
Zero-test binaries/summaries are retained in the logs but do not inflate that
test count. Each configuration's 48 executed test binaries were independently
rehashed after completion.

Compiler: `rustc 1.101.0-nightly (708c94ef0 2026-11-03)`, full commit
`708c94ef093ecc42fb8851e17d8383d999727abc`, LLVM 23.1.5. The compiler matched
before/after; all 568 source files matched Git21 before/after and the source
remained clean. Independent verification rechecked those files, the compiler,
terminal exit statuses, every test summary and both complete log hashes.
No unexpected ASan/TSan runtime report appeared in the retained runner logs.

| Run | Exit | Elapsed seconds | Log SHA-256 |
| --- | ---: | ---: | --- |
| Rust workspace ASan | 0 | 182.023 | `c70a106e52d06a8e2eff8e0c557c6c9437d0a0d8894ebe6209c19b1376c34560` |
| Rust workspace TSan | 0 | 571.519 | `94855cd61da8696a606b92a911b75698b333783ce8d20c50b576f5aa6a66750b` |

Local evidence is `target/rust-sanitizers-21e5d26/evidence.json`, SHA-256
`6403321e9286f7b75ae35f2d6deb9dbc5aa0109a41ee69056638ed6e0d2bebfd`.
The independent count/source/compiler/log/test-binary index is
`target/rust-sanitizers-21e5d26/independent-verification.json`, SHA-256
`eb03dc0965695ca44888acf72d515ec75aaa4c41999946b956a5efe28d4aa23c`.
Logs are adjacent `address.log` and `thread.log`.

These commands used committed 21e5d26 tests, including its original coordinator
process suite. Later test overlays, including the 1,024-receipt
native-capacity extension, are not attributed to these workspace sanitizer runs.

## Calibrated C/Rust shared-runtime ASan

The unchanged original `tools/c_sanitizers.py` ran separately on both platforms
from a new clean detached 21e5d26 worktree. It compiles a Rust calibration shared
library and a C driver, requires valid C/Rust reads to succeed, and requires
deliberate C/Rust heap overruns to be detected. It then rebuilds std and the Rust
library with `-Zsanitizer=address -Zexternal-clangrt`, features
`crdt,replication`, and links the exact external Clang shared ASan runtime used
by the C compiler. C consumers use `-fsanitize=address`, matching runtime search
paths and, on Linux, `-shared-libasan`. Diagnostic configuration differs from
the release packages and the whole-workspace Rust configuration above.

| Platform | Rust compiler / LLVM | C compiler | Actual shared runtime |
| --- | --- | --- | --- |
| macOS arm64 | 1.101.0-nightly `708c94ef0` / 23.1.5 | Homebrew Clang 23.1.5 | `libclang_rt.asan_osx_dynamic.dylib` |
| Docker Linux arm64 | 1.101.0-nightly `f9ceb4a4d` / 23.1.4 | Debian Clang 16.0.6 | `libclang_rt.asan-aarch64.so` |

The Linux image's existing nightly resolves to full compiler commit
`f9ceb4a4d45a471eed7bd8c40dddb7c38e749e21` (commit date 2026-10-31).
It is a separate compiler from the Mac nightly, not relabeled as the same run.
Its concrete compiler/runtime combination is qualified by the calibration
results below, not by assuming compatibility from version numbers alone.

Each platform completed 16 recorded commands with these exact calibration and
consumer outcomes. Independent verification required exit 86 specifically for
both invalid controls, in addition to the original tool's required diagnostic.

| Executed control/consumer per platform | Exit | Result |
| --- | ---: | --- |
| Valid C heap read | 0 | Accepted |
| Valid Rust heap read across C/Rust boundary | 0 | Accepted |
| Intentional C heap overrun | 86 | `AddressSanitizer: heap-buffer-overflow` detected |
| Intentional Rust heap overrun | 86 | Same diagnostic detected |
| C memory consumer | 0 | Passed |
| C pthread consumer | 0 | Passed |
| C native-file consumer | 0 | Passed |

The intentional errors are isolated calibration programs, not findings in the
database. macOS uses `detect_leaks=0`; Linux uses `detect_leaks=1`. Both use
halt-on-error and exitcode 86. This records the actual settings without claiming
exhaustive leak detection or a separately calibrated TSan negative control.

### Source and runtime verification

The C/Rust tool's 55 input hashes on each platform match Git21. Independent
verification also rehashed all 568 files in the detached source worktree,
confirmed its clean status after both runs and checked the unchanged Cargo.lock
SHA-256 `384d128361f436d847264173af62063e151d93b5396e025459e2066c289f881a`.
The source tree is `1d61181f1a2a7505cf47463f69c5411b4a0c2fc2`.

The exact three consumer binaries and Rust shared library were independently
rehashed after execution, together with all 16 command logs per platform.
The Mac shared runtime was rehashed locally; the Linux shared runtime was
independently reread from the same immutable image after container removal.

| Bound binary | SHA-256 |
| --- | --- |
| Mac shared ASan runtime | `c50cdeb65055b4d52c02ddf295dbf2cad919ef5e106f349ee78f7900a58a9369` |
| Linux shared ASan runtime | `fb4188f63e8c06314f4fe9a9f6b080c859629e4d30e78fc37057966377d1fb81` |
| Mac diagnostic Rust library | `8c31eb42ee6a62d99b8c56b5b922ba3c96c59952029d884b5892d91dcffd76cb` |
| Linux diagnostic Rust library | `e2dec069caa936071e7487cdafb2aad4c88f9e2524ad67f85cfc64db2ef9600e` |

Mac used the requested `/opt/homebrew/opt/llvm/bin/clang`; its resolved runtime
was `/opt/homebrew/Cellar/llvm/23.1.5/lib/clang/23/lib/darwin/libclang_rt.asan_osx_dynamic.dylib`.
Linux used `/usr/bin/clang-16` and
`/usr/lib/llvm-16/lib/clang/16/lib/linux/libclang_rt.asan-aarch64.so`.

### Fresh local artifacts

| Local artifact | SHA-256 |
| --- | --- |
| `target/sanitizer-qualification-21e5d26/source-binding.json` | `b488e02ac67269efb6ae3c5b6181b4e2d2fef7efd5cf97a0697e7ba89ef1063f` |
| `target/sanitizer-qualification-21e5d26/mac/evidence.json` | `a64ee06df9cf2349ab2e69d5b9eb4b5be12861d06b29d2b2b3d3adbfc53f2fb5` |
| `target/sanitizer-qualification-21e5d26/linux/evidence.json` | `79732f6e353244be9c6fafdcd1d6f32c379d2dc71b6649b29ef490b8dd02a499` |
| Mac provenance | `134dc2854ac07067785ca2da3f137a2292ad00ca537467912d8e511acf4bbfa7` |
| Linux provenance | `a2ac4fa2c2bd2324e40f9238bb77933fb77c3191de358a1fa6bee4bf6a248901` |
| `target/sanitizer-qualification-21e5d26/independent-verification.json` | `34580004c0a84932a722d05a76937e117c2f2938a65131378e6d9df7c53ad34d` |

The independent index contains every command-log and consumer-binary hash;
per-command logs sit beside each platform's evidence. Mac and Linux wrapper
processes exited zero in 20.197 and 15.528 seconds respectively. Those durations
are run records, not performance comparisons or latency guarantees.

Both shared-runtime builds used offline Cargo and two build jobs. Mac used the
installed Mise-selected environment; the original tool explicitly selects its
installed nightly. Linux used local immutable image
`sha256:25d3b9a14d0b56cb21a43022b08304da91fa669e0297e30984a71f45dab33249`,
read-only exact21 source, network disabled, a fresh writable build/output path,
4 GiB memory and 256-process containment. Outer execution was bounded to 900
seconds; the original tool retains its per-command 600-second waits. Linux
evidence was copied out, its container removed, and daemon-live absence verified.
No cache was deleted. Source/output/build directories remain private local
qualification artifacts; the detached source is retained for inspection.

## Limits of this record

These are diagnostic configurations with executed finite workloads. The results
do not cover every input/interleaving/failure path or establish absence of all
undefined behavior, races or leaks. Compiler/runtime/OS behavior remains a
dependency. They do not measure compiler MC/DC or replace separate unsafe-code,
allocation, model-checking, fuzz, compatibility, package or native-device gates.
No native Windows sanitizer execution, Linux whole-workspace sanitizer run,
cross-platform transfer, physical power-loss qualification or sanitizer result
for later production/test changes is claimed. Private release-package hash
verification is a separate record and does not turn these instrumented libraries
into release artifacts.
