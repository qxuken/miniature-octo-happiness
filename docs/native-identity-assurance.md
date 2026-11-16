# Native identity admission and creation order

Base `c56993ccc997f855a9fc1ba240c2497a2a569c31` gains private callback cores in
the three native adapters and a private file-creation body in the engine.
Public APIs, C ABI, CSPRNG selection, error categories and operation order remain
unchanged. These are implementation extractions for deterministic testing;
binary/code-generation identity is not promised. No dependency or public fault
injection interface is added.

## Production boundary

| Platform | Unchanged native operation | Private admission behavior |
|---|---|---|
| macOS | `arc4random_buf` with an exclusive 16-byte buffer | Up to three complete attempts; return the first nonzero identity, otherwise `Io`. The native function has no status return to simulate. |
| Linux | `getrandom` with flags 0 and exactly the remaining writable suffix | Capture errno immediately after a signed-negative result, then use the existing interruption retry/error mapping. Reject zero or oversized progress before advancing the offset. Apply the same three-complete-zero-attempt rule. |
| Windows | `BCryptGenRandom(null, buffer, 16, 2)` | Reject every nonzero signed status before accepting output; preserve its exact raw status with operation `Random`. Never consult `GetLastError`. Apply the same zero-attempt rule. |

The Linux supported-architecture gate still precedes the core/native call.
No weaker random source or fallback is introduced. Linux's existing persistent
EINTR retry and possible kernel entropy wait remain unbounded OS behavior;
finite scripts do not establish a creation-time deadline or entropy quality.

`Database::create_file` forwards to the private `create_file_using` body. That
body preserves limits validation, identity acquisition, pathname conversion and
file creation in their original order. A failed identity returns before invoking
`FileStore::create`.
The provider is an ordinary private parameter, with no global replacement,
environment selector, application callback API or new database state.

## Directed controls and mutation sensitivity

| Scope | Controls and independent observations |
|---|---|
| macOS core | First and third success with only byte 15 nonzero, exact returned bytes/call counts, and exactly three zero attempts returning `Io` without fabricated native context. Debug/release mutants for two/four attempts, unconditional admission and prefix-only admission fail the intended assertions. |
| Linux core | Exact 5+11-byte assembly and suffix lengths; interruption retries the same suffix without leaving terminal context. Initial/partial terminal errors preserve error category, operation and raw errno. Initial/partial zero and oversized counts reject without invalid writes or consulting errno. First/third final-byte success and three-zero exhaustion retain exact call counts. Eight isolated mutations fail in both profiles: lost offset advance, omitted zero/oversize guards, premature errno consultation, wrong operation, too few attempts, category remapping and a missed signed-negative result. |
| Windows core | Status 0, positive 1 and negative `0xc0000001` share one callback callsite/type, nonzero terminal bytes and zero/two preceding zero attempts. Exact success bytes or `Io`/`Random`/raw status distinguish admission outcomes. Separate controls cover first/third nonzero success and three-zero exhaustion. Native baseline execution passes below; runtime mutant detection remains pending. |
| Engine creation body | One `ProbePath` and function-pointer provider instantiation tests invalid limits before a forbidden entropy callback, then `Io` before a forbidden pathname conversion. A fixed nonzero identity creates an exclusively owned real file and survives reopen. Three isolated mutations fail in both profiles: late validation, ignored provider error and substituted identity. Every command leaves its owned temporary directory empty. |

The signed-negative Linux mutation matters independently of the final `Io`:
casting missed `-1` to `usize` still reaches the oversized-progress rejection,
but loses the required native context. The context assertion detects that
masking rather than treating any `Io` as sufficient.

Only bounded local callback behavior is qualified. Callback types differ between
some test callsites and the native production wrappers; loop cases within a
callsite reuse its concrete type. The engine controls use one explicit private
body instantiation. No actual OS entropy failure in the public factory, every
generic instantiation, kernel/CSP fault occurrence or compiler MC/DC is claimed.
After a complete zero attempt the buffer is already zero, so these controls do
not independently distinguish moving its initialization outside that loop.

## Executed evidence

macOS arm64 Rust 1.99.1 passes the two focused native-core tests and the engine
creation-body test in debug/release, strict test-target Clippy and fourteen
intended red controls across those profiles (eight native-core, six creation
body). Sources and targets are isolated for mutations; compilation failures,
empty selections and supervision failures do not count as detection.

Linux arm64 Rust 1.99.0 passes all five controls in debug/release, all sixteen
intended red outcomes and strict Clippy. The final owned Docker run uses ext4,
two CPUs, a 4 GiB cap, no network and the actual Cargo registry/source mounts
read-only. An earlier run used the image's registry despite mounting the host
cache elsewhere; it remains historical, and final qualification uses a fresh
target with the actual cache mounted read-only. All six owned containers and
both volumes are independently absent afterward.

The combined four-file overlay passes the full core library/integration suites
on macOS: **330 passes and two existing ignores per profile**, across thirty
test executables, plus strict all-feature/all-test-target Clippy. All 747 tracked
inputs stay unchanged during those runs. A later Windows-only test refinement
adds the status-zero control above; no compiled Mac source changes. Fresh real
Windows cfg-selected checks of the final combined source pass. The two Windows
guard variants separately compile in their recorded isolated source slices.
These are compilation results, not native execution or runtime mutant detection.

Subsequent native Windows CI for head `721cbdb01424e67c339c0fc8fb306050f0ca929f`
and tested merge `f386b4b0ed7eafff9fc3984f6a51cee009796de3` executes all three
new identity controls and the creation-body test successfully in both debug
and release (146 library tests per profile). The merge tree equals the head
tree. This closes the baseline execution gate for that source; the
[maintained Windows mutation runner](windows-identity-mutations.md) still needs
native execution of both production mutations. Other CI jobs were still running
when this native result was recorded.

Root independently restores/rechecks all 384 indexed payloads of the three
component proofs, plus 34 Windows-refinement payloads. They retain actual
executables, exact base/overlays/mutations, native process reports and raw logs.
Separate review finds no actionable source or proof issue. Original proofs are
frozen; the Windows-only refinement has its own supplement.

| Protected local archive | SHA-256 |
|---|---|
| `target/archived-proof/native-identity-macwin-c56993c.tar.zst` | `523b8f5abad9a600f56e87697592d5a5e8daa7cabe433fbb648d818ad8b88ee6` |
| `target/archived-proof/native-identity-factory-c56993c.tar.zst` | `5c57a3ffe9e46cc4285bd26500358972243ce93801cd794672661e528a5e6fed` |
| `target/archived-proof/linux-native-identity-c56993c.tar.zst` | `4982fbac04046f1b812587edddae2c499d4fdbb42a64a6915df358b944e70baa` |
| `target/archived-proof/native-identity-windows-pair-c56993c.tar.zst` | `92e8d56a3fcded242945dbaa7391f36457b3a5f40d132de7bd954e3e4632f5c4` |
| `target/archived-proof/native-identity-root-c56993c.tar.zst` | `1bb298f0b3ed7a03800a75c703aad32ed77f24d4948ca1fe45aa0e66a3dbc924` |

The combined root archive independently restores all 146 indexed payloads
(196,419,996 bytes), including sixty actual test executables, six integration
helpers, the inventory tool and the bounded supervisor. It preserves the first
Mac run inputs separately from the final Windows-only test refinement, exact
source snapshots, compiler identity, source inventories and raw process reports.

## Source inventory and residual gates

The default source-union inventory has 8,047 records in 131 files and 3,383
macro/attribute blind spots, with no parse failures, both before and after this
extraction. Only the four source-file hashes change. Their 228 records include
sixteen in the identity/creation-body scope; equal totals do not automatically
close old dispositions. The historical ledgers retain their hashes/locations.

| Current source boundary | Evidence or remaining disposition |
|---|---|
| Creation-body limits and identity residuals | Directed ordering/error controls and three mutations for the stated private instantiation. |
| Creation-body `FileStore::create` residual | Existing native exclusive-create/path/lease tests, rerun with the core suites; not every native errno at this callsite. |
| Linux supported-architecture guard | Unchanged gate reviewed before the native call; no unsupported-ABI execution added. |
| Linux retry/negative/progress/nonzero/loop records | Directed slice/status/diagnostic/call-count controls and eight mutations. For the progress OR, zero cannot also exceed a positive remaining length; both terms have accepted-progress controls. |
| macOS loop/nonzero records | Directed output/call-count controls and four mutations; no invented native status-error path. |
| Windows loop/status/nonzero records | Deterministic tests, real target compilation and native debug/release baseline execution at `721cbdb`. The maintained bounded Windows runner supplies the execution path; native runtime detection of both guard mutants remains pending. |

Physical device behavior, other native failures, entropy quality, broader
profiles and support/workload acceptance keep their separate gates. The
`7ec2cb5` artifact qualification remains bound to its original source and is not
relabelled as these extracted implementations. Packages remain unpublished.
