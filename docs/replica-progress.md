# Private replica progress observations

`tools/replica_progress.py` produces version-1 JSON around the existing
`weekend-replicate describe`, `ship` and `poll` commands. It adds no core protocol,
application API or stable Rust CLI JSON contract. It parses the current text
output strictly and rejects changed/unexpected output. Use the helper with the
matching verified private CLI binary; its SHA256 is recorded in each observation.

```sh
python3 tools/replica_progress.py \
  --binary /private/bin/weekend-replicate --source primary.wdb \
  --stream 11111111111111111111111111111111 --peer 127.0.0.1:8400 \
  --credentials primary-credentials --pin follower-cert.der \
  --ship new-export.wdb --save new-observation.json

# After restarting/reopening the follower, reconcile through a fresh mTLS poll.
python3 tools/replica_progress.py \
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
marks the old observation superseded. Wall-clock timestamps may jump. The tool
does not calculate elapsed replication lag, a commit-time estimate, an SLO, or
an RPO guarantee. Revision differences include metadata commits and should not
be interpreted as counts of missing application edits.

## Failures, retry and observation files

The helper returns nonzero with an error observation on failed CLI execution,
timeout, admission/parser error, incompatible capture or invalid prior state.
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
kills/reaps its child process group. CLI stdout is read incrementally through
a selector into at most 8,193 bytes; exceeding the 8,192-byte contract rejects
promptly and kills/reaps the child, rather than buffering until timeout. A timed-out ship can therefore interrupt
local or remote work and must be reconciled. Local filesystem/kernel stalls
are not bounded by Python scheduling. No helper operation kills the independently
running follower server.

`--save` uses create-new mode 0600 and rejects an existing file or dangling
symlink before invoking replication. A write interrupted during observation
saving can leave an incomplete JSON file; strict loading rejects it. File sync
is performed, but the observation file is not a durable database receipt or a
namespace-persistence guarantee. Store paths/directories and executable/config
files must remain trusted; this is not hostile-path race protection. Saved JSON
contains stream/file identities and digests, so protect it as operational
metadata even though credentials and record contents are absent.

This version qualifies POSIX subprocess behavior only. Windows use requires
separate process/ACL qualification; do not infer it from the core CLI's native
Windows support. The fixed text grammar is intentionally private and must be
requalified if CLI output changes.

## Native qualification

Exact archived engine/application source
`032dadf591b0a0ded4894898e8208283c50425fb` plus only this helper, its tests and the
private Rust fixture were built offline with original locks. The fixture creates
and independently verifies 32 KiB row contents, current revision and retained
root 1 at revision 2. No production source or shipped fault hook was added.

Twelve tests passed normally and under Python `-O` on both macOS arm64 and Linux
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

The Rust fixture passed strict Clippy. Mac used Mise Rust 1.99.1 and Python
3.14.8; Linux used immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
Rust 1.99.1 and Python 3.11.2, with network disabled and the existing registry
cache mounted read-only. Linux data files were on container overlayfs. The
initial Linux build lacked a `sha2` cache entry and stopped before test execution;
that failed attempt is retained separately. No dependency download or source
workaround was used for the passing run. All UUID containers were removed.

Final corrected evidence is in `target/replica-progress-parser/`: a fresh source
binding, Mac/Linux commands, toolchain and binary hashes, build/test logs and
hash index. Mac and Linux reused the unchanged exact-032 CLI/fixture binaries with hashes
rechecked; only the Python helper/test overlays changed for this final run. The new helper/test overlays
were frozen separately. Index SHA256:
`669962f7e50e636acd879d3002f173b1ec5a725191c7f40d4ca9464ebc674f28`.
Helper SHA256:
`96f84bb5ae7beae1007a6cbbf20654505babee574478f4ddda82236ec18fbf16`.

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
recursion exceptions; the final twelve-test runs qualify those narrow parser
fixes. No earlier result is relabeled.

These are finite native loopback tests, not cross-host transport, credential
rotation, hardware power-loss, continuous monitoring or native Windows proof.
