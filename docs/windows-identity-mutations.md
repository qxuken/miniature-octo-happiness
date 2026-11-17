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
tooling checkpoint. At that checkpoint, native execution remained pending;
portable checks could not close that gate. The native result below closes it.
No compiler MC/DC, real CSP fault or entropy-quality claim follows.

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

## Recorded native Windows result

Windows x64 Rust 1.99.1 tests head `4dc6fd8` through merge
`4807a494969fb63749c1a71c2a7fe3519870464b`, with the exact head tree
`60b89d26a758135185d931330db490f1a8648f20`. All eight commands qualify:
four one-test baselines and both production mutations failing their intended
assertions in debug and release. All seventeen metadata/test Job reports have
expected exits, verified cleanup and no timeout, overflow or stop. Original and
copied sources restore unchanged.

Artifact `14267427327` retains 47 payloads and eight actual x64 PE test binaries.
Original ZIP SHA-256 is
`7cbe5139d6207d61937c3f322c98219add12868a62460f13d40fb802bf5cce00`.
All 751 input hashes reconstruct from the exact Git tree: 503 LF-to-CRLF
checkout forms and 248 exact blobs. The Windows pin, production-only mutations
and preserved test bytes match. Shallow checkout leaves the raw parent field
empty; retained Git API metadata supplies the actual base/head parents. The
runner digest is recorded, while its executable is excluded from the upload.

Root independently restores 65 proof payloads (38,445,817 bytes), extracts the
original ZIP and checks raw reports, source binding, mutations and PE identities.
Separate review checks complete Git/checkout reconstruction. Protected archive:
`target/archived-proof/native-windows-rng-4dc6fd8.tar.zst`, SHA-256
`5c5e24f478134280dfaf895113b24c7277a28a44e82fb3fa24e8c6f126130938`.
This closes the two named native mutation gates. Other native job/workflow
checks were still running at capture; their success is not inferred.

Crates remain unpublished.
