# Private replica progress observations

`cargo xtask replica-progress` produces version-1 JSON around the existing
`weekend-replicate describe`, `ship` and `poll` commands. It adds no core protocol,
application API or stable Rust CLI JSON contract. It parses the current text
output strictly and rejects changed/unexpected output. Use the helper with the
matching verified private CLI binary; its SHA256 is recorded in each observation.

```sh
cargo xtask replica-progress \
  --binary /private/bin/weekend-replicate --source primary.wdb \
  --stream 11111111111111111111111111111111 --peer 127.0.0.1:8400 \
  --credentials primary-credentials --pin follower-cert.der \
  --ship new-export.wdb --save new-observation.json

# After restarting/reopening the follower, reconcile through a fresh mTLS poll.
cargo xtask replica-progress \
  --binary /private/bin/weekend-replicate --source primary.wdb \
  --stream 11111111111111111111111111111111 --peer 127.0.0.1:8400 \
  --credentials primary-credentials --pin follower-cert.der \
  --poll --capture new-export.wdb --previous new-observation.json
```

Follow the [replication operations runbook](replication-operations.md) for
credential provisioning, enrollment, server restart and offline leases. Source
and export paths/configuration are trusted and must remain quiescent while the
helper uses them. `describe` is read-only, but the existing `poll` CLI opens the
primary and may stabilize recovery metadata. This helper is not a universally
read-only health probe. It does not keep a live primary writer available during
capture or remove the existing source lease requirements.

## JSON contract and interpretation

The top-level object has exactly these keys:

| Key | Meaning |
|---|---|
| `schema`, `version` | `weekend-replica-progress`, integer `1`; unknown versions/keys and duplicate JSON keys reject when loading an observation |
| `observed_at` | UTC timestamp when this observation was assembled; not a commit timestamp or remote clock |
| `action`, `cli_sha256` | `ship` or `poll`, and hash of the configured trusted executable; null hash is possible on a local configuration error |
| `enrollment` | Configured stream and locally inspected file identity; unknown file identity is null |
| `local` | `state` and `revision`; a verified readable/captured revision is not a new local durability acknowledgment |
| `capture` | `state`, `revision`, `digest`; a completed export is independently inspected with `describe` and SHA256-read in this helper |
| `follower` | `state`, `revision`, `digest`; `durable` is populated only from a fresh successful authenticated CLI exchange; `empty` is distinct from unknown/unavailable |
| `relation` | Exact comparison described below; `unknown` is not success |
| `prior` | Prior observation timestamp and comparison disposition; prior receipts never substitute for a fresh exchange |
| `error` | Null or a sanitized category; raw stderr, paths, credentials and record bytes are not included |

For a verified capture and fresh durable receipt:

- `exact_capture` requires **both equal revision and equal complete export
  digest**.
- `newer_than_capture` means a newer revision was reported; it does not prove
  that the exact old export or its old retained-root set remains selected.
- `behind_capture` reports a lower follower revision.
- `digest_conflict` rejects a same-revision/different-digest comparison.
- `no_capture` means there is a fresh durable receipt but no verified export to
  compare. Empty/unavailable/unknown follower observations have no revision or
  digest and do not imply rollback.

The existing `Primary` rejects acknowledgments ahead of its local revision;
the helper also checks comparisons against the revision verified by that CLI
operation. It rejects saved-enrollment mismatch, observed local rollback,
follower regression and same-revision receipt digest changes. A newer source
head can appear after capture; `local.revision` identifies the view actually
used, not a continuously sampled head.

Prior observations are caller-trusted local records, not signed attestations.
A prior receipt is labeled historical-only while fresh progress is unknown;
it is never presented as the result of a failed poll. A fresh newer receipt
marks the old observation superseded. Wall-clock timestamps may jump.
Revision differences include metadata commits. The sampled mode adds
monotonic timing for observations and the revisions they are waiting on.

## Sampled polling

```sh
cargo xtask replica-progress \
  --binary /private/bin/weekend-replicate --source primary.wdb \
  --stream 11111111111111111111111111111111 --peer 127.0.0.1:8400 \
  --credentials primary-credentials --pin follower-cert.der \
  --poll --samples 12 --interval-ms 1000
```

`--samples` accepts 1 through 64 sequential polls. The interval defaults to
1,000 milliseconds and accepts 100 through 60,000. It starts after the
previous poll finishes. Each poll has its own `--timeout` budget and follows
the ordinary primary file lease rules. The source must be available to the
existing CLI during its local inspection and open. A live application can
use the embedded replication API with its existing database handle.

Stdout contains one version-1 `weekend-replica-monitor` JSON object per line.
`--previous` and `--save` belong to one-shot observations. In sampled mode
the monitor retains the latest accepted observation in memory for
reconciliation. A failed sample leaves it in place, and later polls still
check its enrollment, revision and digest. The command returns success when
the last poll is accepted. Earlier failures remain visible in the lines
already emitted.

| Field | Meaning |
|---|---|
| `schema`, `version` | `weekend-replica-monitor`, integer `1` |
| `sample`, `elapsed_ms`, `probe_ms` | One-based sequence, elapsed time since this monitor started, and duration of this complete observation, including local verification |
| `accepted`, `observation` | Reconciliation result and the full one-shot progress record, including any error |
| `revision_gap` | Verified local revision minus fresh accepted follower revision. Null when the follower is empty or the observation is rejected |
| `last_durable` | Last accepted durable revision, digest and observation timestamp, plus `age_ms` since it was observed. Its age resets on each accepted durable poll, including a repeated receipt |
| `waiting_for` | On an accepted sample, the first observed local revision that is still outstanding, with `observed_for_ms` since it was first seen. Null on rejection or when the follower has reached the target and the latest sampled local revision |
| `caught_up` | Target revision and `observed_after_ms` when an accepted poll first reports that target or a newer revision. If the primary advanced, this can accompany a new `waiting_for` target |

The age and catch-up fields use a monotonic clock inside the current monitor
process. They measure elapsed time between local observations. The sampled
primary revision is the one verified by the CLI call. A follower may install
an image between polls, including after a reply is lost. Commit timestamps
and deployment SLOs require separate evidence.

### Sampled poll qualification

The archived `444b0eb` build exercised the real `weekend-replicate` and
`xtask` executables on macOS arm64/APFS with Rust 1.99.1 and Docker Linux
arm64/ext4 with Rust 1.99.0. The fixture checks the exact current data and
retained revision 2 on both endpoints.

| Stage | Verified result |
|---|---|
| Fresh empty follower | Local revision 3 is outstanding. The current gap and durable receipt are unknown |
| Lost ship reply, then refused poll before restart | The relay discards 79 bytes of the TLS reply on both platforms. The failed poll supplies neither a fresh receipt nor catch-up evidence |
| First poll after restart | Authenticated revision 3 matches the saved export digest. The observer reports that revision 3 caught up |
| Primary advances to 4 | Two polls with `--samples 2 --interval-ms 100` each report a gap of one and a fresh receipt for revision 3. The second reports 563 ms of observed wait on macOS and 312 ms on Docker Linux, including the next probe's time |
| Shipment and poll at revision 4 | Gap zero and observed catch-up to revision 4. Source and selected follower data and retained roots verify |
| Follower stops again | Fresh progress is unavailable. The historical receipt stays at revision 4 and its age advances |

Ten progress controls passed on each platform. The series control rejects a
regressing receipt after a failed sample and then accepts a later valid
receipt. A failed final sample emits its record and exits nonzero. Strict
Clippy passed on both platforms and for the Rust 1.99.0 MSVC cross-target.
Native Windows reruns use the same authenticated scenario. The Docker
container and volume were removed after copyback, and the reports verify
credential cleanup.

| Evidence under `target/replica-monitor-qualification/` | macOS SHA-256 | Docker Linux SHA-256 |
|---|---|---|
| Report | `c25d957bcf42a38a62f5cb86db5a23548506eba3a695221548f890a4a66c0879` | `970994cbfdf1f415aa2896cc06a8791f4fcfb9058b2ef6e9774d25786e9e5b62` |
| Sampled CLI stdout | `34ed22894367722a108c2e287f82c03fd2404772fdc0ca6744c5086f52fff642` | `09e9bfa6219e18bc3d4d366dfbe87a44a4d27411d85581112e4e413d0138c61c` |

## One-shot failures, retry and observation files

Syntax and argument-admission errors use CLI diagnostics and a nonzero
exit, without JSON. After argument admission, operational failures produce a
nonzero exit with a JSON error observation: failed CLI execution, timeout,
unexpected CLI output, incompatible capture or invalid prior state.
A failed ship remains **unconfirmed**: the follower may already have durably
installed the image. Local capture can be reported verified when completion
was observed and the export can still be independently checked. If the overall
budget expires before that check, capture remains unknown; inspect/reconcile it
on a later invocation. Restart the follower when its core requires recovery,
then poll over a new authenticated connection. Never infer rollback from the
helper's failure exit status.

`--attempts` defaults to one and permits at most three, using the CLI's existing
same-export retry behavior. The helper does not retry an old saved file as a new
`Export` after a process restart. `--timeout` defaults to 110 seconds (range
1–600); all child CLI invocations share that deadline. On timeout the helper
retires its child process group on POSIX or its owned job on Windows. The supervisor drains output
incrementally under an 8,192-byte per-stream ceiling. Exceeding it rejects
promptly and reaps the child. A timed-out ship can interrupt
local or remote work and must be reconciled. Local filesystem/kernel stalls
are outside the subprocess deadline. No helper operation kills the independently
running follower server.

`--save` uses create-new mode and rejects an existing destination before
invoking replication. On POSIX it uses mode 0600. On Windows choose an
ACL-protected parent directory for saved observations and a private temporary
directory for captured child output. A write interrupted during observation
saving can leave an incomplete JSON file; strict loading rejects it. File sync
is performed, but the observation file is not a durable database receipt or a
namespace-persistence guarantee. Store paths/directories and executable/config
files must remain trusted; this is not hostile-path race protection. Saved JSON
contains stream/file identities and digests, so protect it as operational
metadata even though credentials and record contents are absent.

Windows hashes regular disk files, rejects reparse-point files, and checks
that the held handle and reopened path still identify the same file, length
and last-write time. The fixed text grammar is private and must be
requalified if CLI output changes.

## Rust controller qualification

Run the normal controls with `cargo test --offline --locked -p xtask --bin
xtask replica_progress`. To execute the separate authenticated scenario, use
the exact native `weekend-replicate` and `replica-progress-fixture` release
executables built from the same archived revision and original locks.

```sh
REPLICA_PROGRESS_BIN=/absolute/path/to/weekend-replicate \
REPLICA_PROGRESS_FIXTURE=/absolute/path/to/replica-progress-fixture \
REPLICA_PROGRESS_XTASK=/absolute/path/to/xtask \
REPLICA_PROGRESS_EVIDENCE=/absolute/path/to/fresh-result-directory \
cargo test --offline --locked -p xtask --bin xtask \
  authenticated_observations_restarts_dropped_reply_and_stale_capture -- --ignored
```

The observer and evidence paths are optional on Unix. Supplying them
exercises the sampled CLI executable and preserves the report after the
temporary stores and private keys are removed.

The Rust test exercises the same response-loss, restart, exact receipt,
stale-capture, foreign enrollment and final retained-state checks described
below. macOS/APFS and Docker Linux/ext4 passed against `0647a51`.
The response-loss relay explicitly sets accepted sockets to blocking mode.
The first macOS run exited the relay before dropping an acknowledgment,
which exposed the platform difference. The corrected run reopened at
durable revision 3 after loss of the response, and finished at 4 with
retained revision 2. [Migration evidence](xtask-conversion.md#replica-progress-observations)

### Native Windows run

The CI scenario invokes the actual helper executable and the release
`weekend-replicate` binary. The test fixture generates external credentials
inside the runner's ACL-restricted work directory before any connections are
opened. The evidence retains both the successful JSON and the rejected
observations' JSON and exit codes. Reproduce on native x64 MSVC and fixed NTFS
after caching the locked dependencies.

```powershell
cargo build --offline --locked --release -p weekend-replicate
cargo build --offline --locked -p xtask
cargo build --offline --locked --release --manifest-path tools/replica-progress-fixture/Cargo.toml --target-dir target/replica-progress-fixture
$env:REPLICA_PROGRESS_BIN = (Resolve-Path target/release/weekend-replicate.exe).Path
$env:REPLICA_PROGRESS_XTASK = (Resolve-Path target/debug/xtask.exe).Path
$env:REPLICA_PROGRESS_FIXTURE = (Resolve-Path target/replica-progress-fixture/release/replica-progress-fixture.exe).Path
$env:REPLICA_PROGRESS_EVIDENCE = 'target/windows-replica-progress'
cargo test --offline --locked -p xtask --bin xtask authenticated_observations_restarts_dropped_reply_and_stale_capture -- --ignored --nocapture
```

### Recorded native Windows result

On 2026-11-15, PR head `da04423` passed on native Windows x64 and fixed
NTFS with Rust 1.99.1. The runner used the clean PR test merge
`0657239e92424891605a863d8b382825d2a30140`. The source, follower, and
temporary credentials used the ACL-restricted work directory.
[Hosted job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43736990618/job/132881665451)
and [evidence archive](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43736990618/artifacts/14170443630).

| Observation | Result |
|---|---|
| Before shipment | A fresh authenticated poll reported an empty follower. |
| Lost TLS response | The relay discarded 79 response bytes. The helper exited 1 with `timeout` and marked capture and follower progress unknown. |
| Restart and fresh poll | The follower reported durable revision 3 and the digest matched the saved export. Saving and reloading the observation marked the prior receipt consistent on a repeat poll. |
| Next shipment | A separate local commit and shipment advanced the follower to revision 4. Polling with the revision-3 export reported `newer_than_capture`. |
| Changed export and foreign prior | The helper exited 1 with `digest_conflict` for changed bytes at revision 4, and with `identity_mismatch` for the foreign saved observation. |
| Follower unavailable | The helper exited 1 and marked the prior receipt historical-only. The current follower revision and digest were null. |

Independent checks matched all nine JSON observations against their recorded
exit statuses, the exact binaries and the exported bytes. The final reopened
follower image matched the revision-4 export. The fixture also reopened the
primary and follower and verified the complete 32 KiB values at current
revision 4 and retained revision 2. All 62 artifact hashes and 24 process
cleanup records matched, including two intentional server stops. The archive
contained no generated private keys.

The macOS rerun passed the runner unit tests and the authenticated scenario.
The Docker Linux/ext4 run of the same source passed all eight progress controls
and the authenticated scenario. The Linux workspace and fixture locks stayed
unchanged, and the owned container and volume were confirmed absent.

| Evidence | SHA-256 |
|---|---|
| Windows ZIP | `22afad090011b6408ceabf75a7204abfa1d970e2f717393cea94193a486d44b1` |
| Windows evidence | `552427ef0f733de2e22e03cfb1a481be646c952262325ba7ee2fe09676ca00d5` |
| Windows revision-3 export | `88c7b88a1068c8f13debddc0cab44b438d858a0f54be844be3505e47a4c450d1` |
| Windows revision-4 export | `0886de961e685be0e96fb22ba339408812926bf3fee44ac56fba1b9018f928cf` |
| Docker Linux test log | `cb3a2c6b6796ebb0dc9b78c3a87422c82e23bdb35540921dc45691487af35557` |

## Historical native qualification

Exact archived engine/application source
`032dadf591b0a0ded4894898e8208283c50425fb` plus only this helper, its tests and the
private Rust fixture were built offline with original locks. The fixture creates
and independently verifies 32 KiB row contents, current revision and retained
root 1 at revision 2. No production source or shipped fault hook was added.

Fourteen tests passed normally and under Python `-O` on both macOS arm64 and Linux
aarch64. The real native fixture executes the actual TLS CLI and covers:

1. Fresh authenticated poll reports an empty follower.
2. A test TCP relay forwards the TLS handshake/upload, then discards server
   responses after upload traffic. The helper times out without claiming a
   durable receipt. After server restart, a fresh authenticated poll verifies
   revision 3 and the exact independently hashed export, proving the lost
   response followed durable installation.
3. Repeat poll reports the same receipt; source advancement and another ship
   report revision 4. Polling against the old capture labels newer progress,
   without claiming an exact digest match.
4. A verified same-revision export with a different full-file digest rejects;
   foreign saved enrollment also rejects. Final source and selected follower
   image both independently verify current revision 4, exact bytes and retained
   revision 2.
5. Pure controls cover malformed/duplicate saved JSON keys, wrong versions/types,
   unexpected CLI text, behind/ahead/different-digest comparisons, unknown fresh
   state with a prior receipt, and lost-reply local/remote separation. Existing
   saved paths reject before any replication action. A real child emits 9,000
   bytes then sleeps for 30 seconds; the wrapper must reject excess output
   before its 10-second deadline. An observe-path timeout and a real refused
   poll after stopping the TLS server preserve valid prior metadata as
   historical-only, including its timestamp, without supplying a fresh receipt.
   Selector creation failure must spawn no child; selector registration failure
   must terminate and reap an actual sleeping child and close its stdout.
   A 5,000-digit CLI revision traverses the actual observe/describe parser and
   returns sanitized `parse_error`; decimal length is bounded before integer
   conversion. A 2,401-byte nested JSON input returns `prior_invalid`, including
   Python 3.11 recursion failure, rather than escaping the observation contract.
   A table-driven schema control rejects numeric hashes/IDs/digests, zero
   enrollment IDs and malformed secondary timestamps; a valid timestamp passes.
   Zero file identity from unexpected describe output is rejected as sanitized
   `parse_error` before setting enrollment or invoking poll.

The Rust fixture passed strict Clippy. Mac used Mise Rust 1.99.1 and Python
3.14.8; Linux used immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
Rust 1.99.1 and Python 3.11.2, with network disabled and the existing registry
cache mounted read-only. Linux data files were on container overlayfs. The
initial Linux build lacked a `sha2` cache entry and stopped before test execution;
that failed attempt is retained separately. No dependency download or source
workaround was used for the passing run. All UUID containers were removed.

Final corrected evidence is in `target/replica-progress-final/`: a fresh source
binding, Mac/Linux commands, toolchain and binary hashes, build/test logs and
hash index. Mac and Linux reused the unchanged exact-032 CLI/fixture binaries with hashes
rechecked; only the Python helper/test overlays changed for this final run. The new helper/test overlays
were frozen separately. Index SHA256:
`d943746de33a9d56a4a4679a1552fd1f18f4a7f5e40d5817ff101db4f56d3f92`.
Helper SHA256:
`305b08ce9277530353408555c17eafddcf25e19e211254c663663e75da8ecf8c`.

Earlier six-test evidence remains at `target/replica-progress-qualification/`
under its original hashes. Review then found two wrapper defects: stdout was
size-checked only after buffering/child exit, and a valid prior was mislabeled
rejected on unrelated transport failure. The incremental reader and prior-state
fixes were covered by the eight-test runs retained separately at
`target/replica-progress-corrected/`. A subsequent review found that selector
creation/registration exceptions could occur outside child cleanup. The selector
is now created before spawning, with registration inside the kill/reap guard;
the ten-test runs retained at `target/replica-progress-cleanup/` qualify that
fix. Independent review then found excessive decimal conversion and JSON
recursion exceptions; the twelve-test runs retained at
`target/replica-progress-parser/` qualify those parser fixes. Final strict-schema
review required actual JSON strings for hashes/IDs/digests, nonzero enrollment
IDs and the same UTC grammar for secondary timestamps; the thirteen-test
runs remain at `target/replica-progress-schema/`. The final fourteen-test runs
also verify that a zero identity in unexpected describe output becomes a
sanitized `parse_error` observation before polling. Valid emitted records are
unchanged. No earlier result is relabeled.

These are finite native loopback tests, not cross-host transport, credential
rotation, hardware power-loss, continuous monitoring or native Windows proof.
