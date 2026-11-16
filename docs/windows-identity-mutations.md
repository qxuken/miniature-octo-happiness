# Native Windows identity mutation runner

This tooling-only follow-up to `721cbdb` provides the maintained native execution
path for the two Windows identity mutations in
[the admission report](native-identity-assurance.md). It changes no engine,
replication, publication or byte/wire-format code. Local commit acknowledgment
and the follower's durable acknowledgment keep their existing contracts.

## Admission and evidence

`cargo xtask mutations` keeps its existing Unix inventory and selects a separate
two-case inventory on Windows. The Windows fixture pins the complete reviewed
`src/storage/windows.rs` file at `721cbdb01424e67c339c0fc8fb306050f0ca929f`.
Source checks accept LF or equivalent CRLF, preserve test bytes and reject drift
before output creation and again on the copied source. Mixed endings reject.

| Mutation | Required native test result |
|---|---|
| Accept positive status by changing `status != 0` to `status < 0` | The exact status test fails when a positive status incorrectly returns its nonzero buffer. |
| Omit `Random` native diagnostics | The same test fails when the rejected status has no native context. |

Each case runs a baseline and mutant in both debug and release, using eight
fresh Cargo targets. Builds are offline/locked, incrementality is disabled,
inherited instrumentation overrides are removed and the host must be Windows
MSVC. The existing bounded process boundary owns the suspended child in its
Job Object before resuming it; each command has a 120-second deadline and a
4 MiB per-stream output cap. Original inputs remain untouched.

A qualifying baseline runs exactly the selected test successfully. A mutant
requires exit 101, that test's failed result, one consistent complete libtest
summary and the complete expected panic line. Empty selection, wrong tests,
compiler errors, malformed or contradictory summaries, timeout, output overflow,
cancellation and uncertain process retirement cannot count as detection.
Cargo's JSON artifact record must identify exactly one real, owned libtest
executable. The proof retains that executable, source originals/mutations,
source hashes, compiler/controller identity, Git provenance and raw logs/reports.
Restoration and final original/copied input equality are qualification gates.

Windows CI runs this command and uploads evidence even on failure, then enforces
the result. Disposable Cargo caches and the complete source copy are excluded
from the upload. The [command guide](mutations.md#native-windows-identity-controls)
gives the invocation. Unix's 56-case fixture, command builder, baseline/failure
classifiers and campaign body remain unchanged; Windows reuses the existing
process owner rather than introducing a second supervisor.

## Local verification and remaining runtime gate

macOS arm64 Rust 1.99.1 passes eleven mutation-controller tests in each profile,
two portable process controls, strict native Clippy and strict Windows-target
all-test-target Clippy. These exercise source pins/line endings, production-only
replacement, result rejection, artifact containment, source-copy boundaries and
restoration on errors and unwinding. Windows-only process flags are compiled
locally; their execution belongs to native Windows.

Independent review caught a loose classifier that accepted malformed summaries
and bare panic tokens. The final parser requires the complete observations
above, with explicit negative controls for malformed, duplicate, contradictory
and compiler-fabricated results. Review also required rechecking the actual
copied source against its full-file pin. Both findings are fixed before the
tooling checkpoint. Native Windows execution of the two production mutations
remains pending CI; cross-compilation and portable classifier tests do not close
that gate. No compiler MC/DC, real CSP fault or entropy-quality claim follows.

One derived loose-classifier control exits 101 at the intended malformed-footer
assertion. It is reconstructed from final inputs with an explicit negative
test, rather than presented as the original pre-review executable. A separate
compile-only macOS probe retains Cargo's actual `[rlib, cdylib]` libtest artifact
record; that probe does not execute Windows code. Earlier test/style-check
iterations remain historical in the proof, including the missing original
pre-review full-tree/binary capture.

Root independently restores all 113 indexed payloads (75,656,067 bytes), checks
the exact base/overlays/current source hashes, raw process outcomes, final test
counts and four retained executables. The protected local archive is
`target/archived-proof/windows-rng-runner-721cbdb.tar.zst`, SHA-256
`a82e84ac54bcc449e8b25fbc626fa31f9e72d8869050f3d32a4b1ac01d6821fa`.
Independent source and documentation reviews have no remaining findings.

Crates remain unpublished.
