# Windows C/C++ delivered-artifact trace handoff

Status: **native x64 Windows/MSVC qualification passed at harness `80eb0c2`**.
The original preparation record and commands below are retained; see the actual
source-bound [native results](#native-results-2026-11-13) at the end.
This is the next F15 qualification batch. The existing model, 134 cells,
expected transcript and common C consumer remain unchanged. No engine or public
API changes are required. This gate is not enabled as an automatic Windows CI
job until the native handoff establishes that the runner works there.

## Plan and acceptance

1. Select a verified, clean x64 MSVC package and independently pin its raw
   `manifest.json` SHA-256. Verify every payload and reject extra files,
   traversal/ADS/device names, aliases and reparse points.
2. Copy that package into fresh evidence storage. Compile the unchanged
   `consumer.c` through a Windows-only shell in **both** modes:
   `/O2 /UNDEBUG /MD /TC /std:c11` and `/O2 /UNDEBUG /MD /TP /std:c++17`.
   Link the delivered import library and use only the delivered header.
3. Execute the full 134-cell v2 trace on a local fixed NTFS data volume.
   Compare binary stdout exactly with the existing independent Python oracle.
   It includes both validation modes, full nested/binary values, held snapshots,
   durable retention/release, reopen, conflicts and terminal transactions.
4. For each language also run original v1, legacy BEGIN interpretation,
   hand-checked WholeDatabase conflict/success, invalid mode and unknown version.
   Input rejections must exit nonzero before creating a database.
5. Verify the DLL **inside the consumer process**: `GetModuleHandleW` and
   `GetModuleFileNameW` identify the imported `weekend_db.dll`; Windows CNG hashes
   that actual file before and after the trace. Its path must equal the private
   relocated copy and its hash must equal the pinned package payload. A read
   handle denies concurrent file writes/deletion during the trace. Actual
   embedded source ID, ABI, format, feature bits and PID are checked too.
6. Require deliberate wrong-hash, wrong-path and wrong-source controls to reject
   before the trace starts. A successful baseline must precede these controls.
   Save before/after attestations separately from the canonical transcript.
7. Recheck original/relocated package inventories and harness hashes, and retain
   a complete evidence directory. Mark native verification only after **all 20
   executions** (ten per language) and their expected outcomes pass.

The runner never builds the engine. The shell includes the existing C consumer
without editing it, converts UTF-8 trace paths to Windows wide CRT paths and
sets stdout to binary mode so CRLF translation cannot weaken comparison.
Compiler environment discovery is bounded; only build-related environment
values are logged. `CL`, `_CL_`, `LINK` and `_LINK_` overrides are removed.
Runtime PATH excludes the input package and build-tool directories.

Windows child processes start suspended, join an owned kill-on-close Job Object,
then resume. Timeouts/output limits terminate the entire owned job and require
zero active processes. The runner caps each stdout/stderr stream and retains
failure evidence. This is a trusted-host qualification gate, not protection
against a hostile administrator or malicious package author.

## Reference package

Use the already built and audited
[Windows package artifact](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43476038949/artifacts/14062264979).
This deliberately keeps library identity separate from the newer harness:

| Item | Pin |
|---|---|
| Workflow run | `43476038949` |
| Artifact | `14062264979`, `weekend-db-private-Windows-X64` |
| Package source / embedded source ID | `7a490e5ce24910d5d8ec96abe0fb9d218ea9e870` |
| Features | `crdt,replication` (bits 3) |
| Manifest SHA-256 | `25c668346fe270631689dfb30b59fcb7cca4ae2a54981fe96e7c923346bd6d0d` |
| DLL SHA-256 | `7d9034b54d6144044da48a8fde12346a33a7102d900042ad1a72c925d74a14f7` |
| Original artifact ZIP SHA-256 | `7d60778d68ed75c03dcc079a0eec7bea29f4660e31625a698391472719410ba4` |

Local preparation rechecked the artifact API digest and successfully admitted
the real 52-payload Windows package through the new verifier. This is package
admission evidence, not execution of its DLL on Windows.

Artifacts can expire. If this one is unavailable, create/select a different
verified Windows package and record its independently obtained manifest pin,
DLL hash and source as a **different qualification**. Preserve the reference
pins; do not relabel a rebuilt DLL as this artifact. A fresh private package can
be built from a clean checkout with `tools/release.py --features crdt,replication`
using Rust 1.99.1, but that is a separate input preparation step. No Cargo
publication is needed or allowed.

## Windows agent commands

Use native x64 Windows 10 (1709+) or Windows 11, writable local fixed NTFS storage, Visual Studio
2022-or-newer C/C++ build tools with a Windows SDK, and the compatible MSVC
runtime required by the delivered package. An x64 VS developer PowerShell is
preferred; the runner can discover VS via the existing `vswhere` helper. GitHub
CLI authentication must permit artifact download. Never put credentials in the
evidence. Use [Mise's Windows installation](https://mise.jdx.dev/installing-mise.html#windows)
and its [Python backend](https://mise.jdx.dev/lang/python.html#windows) for the
pinned Python version.

From a clean checkout of `codex/windows-file-storage`, run in PowerShell:

```powershell
git pull --ff-only
if ($LASTEXITCODE -ne 0) { throw 'Pull failed' }
git rev-parse HEAD
mise install python@3.14.8
if ($LASTEXITCODE -ne 0) { throw 'Python installation failed' }

$traceRoot = Join-Path $env:LOCALAPPDATA 'weekend-trace-handoff'
if (Test-Path $traceRoot) { throw 'Choose a fresh traceRoot; preserve old evidence' }
New-Item -ItemType Directory -Path $traceRoot | Out-Null
gh run download 43476038949 --repo qxuken/miniature-octo-happiness `
  --name weekend-db-private-Windows-X64 --dir "$traceRoot\package"
if ($LASTEXITCODE -ne 0) { throw 'Artifact download failed' }

mise exec python@3.14.8 -- python tools/artifact-trace/test_windows_trace.py
if ($LASTEXITCODE -ne 0) { throw 'Runner controls failed' }
mise exec python@3.14.8 -- python -O tools/artifact-trace/test_windows_trace.py
if ($LASTEXITCODE -ne 0) { throw 'Optimized Python controls failed' }

mise exec python@3.14.8 -- python tools/artifact-trace/windows.py `
  --package "$traceRoot\package" `
  --manifest-sha256 25c668346fe270631689dfb30b59fcb7cca4ae2a54981fe96e7c923346bd6d0d `
  --output "$traceRoot\result"
if ($LASTEXITCODE -ne 0) { throw 'Native qualification failed; preserve result evidence' }
```

Do not run the POSIX `run.py` or `tools/check_helpers.py` on Windows. The new
portable control file is the Windows entry point. Each retry needs a fresh
output directory. Keep all database files under the evidence root; no unrelated
files are removed. Long-path/8.3 alias compatibility is not promised by this
handoff; use ordinary resolved paths within Windows path limits.

## What the Windows agent should return

- Harness checkout commit and complete `result/` directory, including
  `evidence.json`, both executables, compiler logs, data-volume identity,
  before/after loader proofs and every actual/expected transcript.
- Confirmation of both `/TC` and `/TP` optimized builds with assertions enabled,
  five successful transcripts and five intentional rejections per language.
- Exact loaded DLL path/hash/source/features, with all package/harness hashes
  unchanged and all command cleanup checks verified.
- Any native fixes as a reviewable diff with regressions. Preserve failed runs;
  fix harness defects rather than changing the independent oracle/constants to
  agree with unexpected engine behavior. Rerun both languages and every case
  after a fix, and update progress with actual source-bound results.

Suggested task to paste to the Windows agent:

> Pull `codex/windows-file-storage` and follow `docs/windows-artifact-trace.md`.
> Validate the pinned delivered DLL through native optimized MSVC C and C++ using
> the unchanged 134-cell oracle. Run all twenty cases, verify process-local DLL
> path/hash/source proofs and exact transcripts, and preserve full evidence.
> Resolve native harness failures with regression controls, without weakening
> assertions or altering the oracle to fit results. Report actual native results
> and remaining limitations; update progress, commit and push verified fixes.
> Keep Cargo packages unpublished.

## Preparation results before native qualification

Twelve new controls pass locally normally and under `-O`, including manifest
tampering, Windows path aliases, wrong loader attestations, output admission and
real child timeout/output bounds. The complete helper registry has 21 suites:
42 workers, 316 passes and four registered skips (320 reported tests).
The shell compiles to x64 Windows GNU COFF objects as C11 and C++17 with Mise
Zig 0.17.0, optimization, assertions and warnings-as-errors. That cross-check
does not establish MSVC compilation, Job Object behavior or native DLL execution.

Trace and expected transcript SHA-256 remain respectively
`4ad2fe8534dc73bd3032f1dcc41220f5bf919482c1f7df0a0a8716c4c40812c8` and
`0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3`.
The original model/common consumer are untouched. At handoff, F15's Windows
full-state trace remained open pending the native evidence recorded below. This
finite gate makes no power-loss, injected-I/O, full-API or MC/DC claim.

API references: [loaded module filename](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-getmodulefilenamew),
[Windows CNG hashing](https://learn.microsoft.com/en-us/windows/win32/seccng/creating-a-hash-with-cng),
[native architecture query](https://learn.microsoft.com/en-us/windows/win32/api/wow64apiset/nf-wow64apiset-iswow64process2),
[MSVC C/C++ language selection](https://learn.microsoft.com/en-us/cpp/build/reference/tc-tp-tc-tp-specify-source-file-type).

## Native results, 2026-11-13

The clean detached checkout was `80eb0c2e6c54acf5cc525a8d48f5f96a49498465`.
All twelve controls passed normally and under Python `-O`, followed by all
twenty native cases on the first attempt. No harness, engine, model, common
consumer or oracle change was needed. These results qualify the delivered
library source below, separately from the newer harness source.

Environment: Microsoft Windows 11 Pro 10.0.26300, native AMD64 (process machine
0, native machine 0x8664); Visual Studio 18 Community, x64 MSVC toolset
14.51.36231, cl.exe file version 19.51.36247.0; Windows SDK 10.0.26100.0;
Mise Python 3.14.8, MSC v.1944, 64-bit AMD64. Data and evidence reside on fixed
local C: NTFS (drive type 3, volume serial 3189219544). Compiler environment
discovery succeeded without an existing developer shell.

| Case | C11 exit / result | C++17 exit / result |
|---|---|---|
| Full 134-cell v2 | 0 / exact oracle | 0 / exact oracle |
| Original v1 (`legacy-v1`) | 0 / exact oracle | 0 / exact oracle |
| Legacy v1 BEGIN | 0 / exact oracle | 0 / exact oracle |
| WholeDatabase conflict | 0 / exact oracle | 0 / exact oracle |
| WholeDatabase success | 0 / exact oracle | 0 / exact oracle |
| Invalid validation mode | 3 / rejected | 3 / rejected |
| Unknown trace version | 3 / rejected | 3 / rejected |
| Wrong DLL SHA-256 | 3 / rejected | 3 / rejected |
| Wrong loaded DLL path | 3 / rejected | 3 / rejected |
| Wrong embedded source ID | 3 / rejected | 3 / rejected |

Both separate builds used `/O2 /UNDEBUG /MD /W4 /WX`, with `/TC /std:c11`
and `/TP /std:c++17` respectively. Include logs identify the relocated delivered
`weekend_db.h`; linker commands identify its delivered `weekend_db.dll.lib`.
All ten successful stdout files match the independent oracle byte-for-byte.
All ten rejections have empty stdout and create no database or after proof;
loader identity rejections also create no before proof. Invalid-mode and
unknown-version cases have valid before proofs.

Every successful before/after proof identifies its actual PID, ABI 1, format 2,
features 3 (`crdt,replication`), source
`7a490e5ce24910d5d8ec96abe0fb9d218ea9e870`, and exact private DLL at
`C:\Users\21asg\AppData\Local\weekend-trace-80eb0c2-20261113\result-initial\c\weekend_db.dll`
or the corresponding `cpp` path. Process-local CNG SHA-256 is
`7d9034b54d6144044da48a8fde12346a33a7102d900042ad1a72c925d74a14f7`.
The manifest hash is
`25c668346fe270631689dfb30b59fcb7cca4ae2a54981fe96e7c923346bd6d0d`.
The downloaded original ZIP was independently hashed to
`7d60778d68ed75c03dcc079a0eec7bea29f4660e31625a698391472719410ba4`,
matching the API digest; every extracted ZIP payload matches the input package.
The DLL was never rebuilt or substituted.

Evidence root:
`C:\Users\21asg\AppData\Local\weekend-trace-80eb0c2-20261113`.
`result-initial/evidence.json` SHA-256:
`ceab27169583071ffa533a4987b6e54af8d47b6898ec7c4d148469cea46f175e`.
Its inventory contains 175 payload hashes, independently rechecked. The root
also retains the raw ZIP, original package, both control logs, native log,
independent audit scripts and `independent-audit.json` (SHA-256
`84bc38e476826a5502f456e826fa41f93b385872d0203867b713114400500f8f`).
The complete evidence root is also archived at
`C:\Users\21asg\AppData\Local\weekend-trace-80eb0c2-20261113-evidence.zip`
(1,697,183 bytes), SHA-256
`1ba2d2b2efc2f28a74fcb17bf5a67fbc0f2e7dd0f021972061681cb322d22e5b`;
the archive CRC check passes.
Both executables, compiler logs, actual/expected transcripts, databases and
loader proofs remain in the result directory. This evidence is local storage;
no hosted retention or automatic CI trace gate is established.

Original and relocated package verification passes before/after execution.
All five harness source hashes remain identical to the tested commit:

| Source | SHA-256 |
|---|---|
| `model.py` | `ef1b085fb879d47b8919a21f09b8619bdfab37e84cb83fe5f43c4b6954cd3f82` |
| `consumer.c` | `00bcd0f17c32bde3df2c0894743db83e45bbbe520a93ff6f9d8b5c0df8de3eab` |
| `windows_consumer.c` | `4ffa5f4fe0dd104641b2cf8bf8b8815ab87698b82a6ee6b58b76c97ca767e64f` |
| `windows.py` | `abb2f4a3a069ca0d5cfbc71adbeaaa27da6c0fbc9218115488f938792a9eca5c` |
| `tools/windows_consumers.py` | `079fd7fe75e3207571ebdcb213b0f6806b879da2bd3cab625c016dc5c315b295` |

Trace and full stdout hashes remain the unchanged pins in the preparation
section. All 23 command records (environment discovery, two compilations,
twenty executions) confirm cleanup: owned jobs were terminated and their active
process counts reached zero before handle closure. No owned children remain.
A supplemental post-run PID-only audit initially flagged unrelated Windows
processes reusing exited test PIDs; creation times after evidence completion
resolved this false alarm. Its initial script is preserved as
`independent-audit-initial.py`; the corrected audit retains creation-time
observations in `post-run-process-check.json`. No native failure directory was
discarded and no native retry was required.

This closes F15's named Windows x64 delivered C/C++ finite trace gate for this
exact package. It does not establish Windows 10, other SDK/toolset versions,
other architectures/filesystems/features, long paths/8.3 aliases, full API or
MC/DC coverage, injected I/O faults, physical power-loss durability, a final
release candidate or an owner support window. Crates remain unpublished.
