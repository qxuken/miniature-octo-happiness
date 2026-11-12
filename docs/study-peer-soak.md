# Authenticated macOS/Linux peer qualification

The private launcher runs the actual release `study-peer` CLI on macOS and in a
Linux Docker container, with separately provisioned credentials and exact peer
pins. The Linux database and build use a native Docker volume; only the archived
source and cached Cargo registry are bind-mounted. The Linux process receives
its own private key and the macOS certificate, not the macOS private key.

```sh
mise exec rust@1.99.1 python@3.14.8 -- python -O tools/study_peer_soak.py \
  /absolute/path/to/new-output --source ed8f9f9 --rounds 30 --restart-every 5
mise exec python@3.14.8 -- python -O -m unittest discover -s tools \
  -p test_study_peer_soak.py
```

The default immutable Docker image is the locally qualified Rust 1.99.1 image;
use `--image` to identify another available tool image explicitly. The launcher
resolves its immutable image ID and records the actual toolchain. This profile
requires macOS, Docker, Python 3.12+, cached locked Cargo dependencies, and Unix
credential modes. It does not install tools or run hosted CI. Evidence includes
synthetic private test keys; keep the output private and outside distributions.

## Workload and independent observations

Both writers begin with outgoing sync disabled. In round `d`, A commits `d`
minutes and B commits `2*d` minutes into their own files while assigning distinct
weekly goals and session limits and completing separate lesson IDs. Each local
reply must advance its previous local revision by exactly one, and its outgoing
acknowledgment counter must remain unchanged. Local completion is observed
separately from a remote acknowledgment.

After enabling both outgoing workers, the launcher checks the independently
calculated sum `3*sum(1..d)`, union of completed lessons, maximum weekly goal
`200+d`, and minimum session limit `10+d%5` on **both** local states. It requires
actual peer `ack <total>` payloads to report the expected total. Merely observing
increasing exchange counters or matching local states is insufficient.

To check repeat-sync stability, both peers must complete at least two additional
covering exchanges while their revisions remain unchanged. If an exchange adds
a causal/resolution commit, the stability window restarts. Revisions need not be
equal or follow a predetermined global formula; independent offline policies
can produce extra causal events before their common state stabilizes. The CLI
does not display the planned-topic field, which this workload never edits; the
oracle covers its four displayed value groups, not every possible record field.

After `push off`, outstanding exchanges get a quiet observation window exceeding
the three-second transport budget. The next offline edit also checks that no
new reply arrived in that interval. Disk work is outside that socket budget;
the launcher has its own command/convergence deadlines. A failure to quiesce
fails qualification rather than being assumed harmless.

Every fifth round, the native process and Linux container receive SIGKILL after
the covering acknowledgments and stable state are observed. New processes open
the same files, and every displayed value **and exact local revision** must
match the pre-kill state. Docker recreation preserves the native data volume.
These are post-acknowledgment process interruptions, not mid-publication faults,
lost-reply injection, OS crashes or physical power cuts.

## Bounded execution and failure handling

The launcher accepts 1–365 rounds and a restart interval within that range.
Existing paths, including dangling symlinks, reject before external work; the
parent must exist. Source is an explicit Git archive, with no engine overlays.
It builds offline and locked with incremental compilation disabled. The runner
itself is hashed separately from that archive. Native build settings otherwise
inherit the launching environment; this is not a hermetic toolchain image.

Process commands have 15-second reply deadlines, convergence has 25 seconds,
quiet observation has 15 seconds, startup/metadata probes have 30-second limits,
and build/setup commands have 180-second limits. Completion/restart and cleanup
have their own bounded waits. Port reservation is best effort; another process
claiming a port causes failure rather than relaxing peer identity checks.

Started native peers remain tracked if Linux startup fails. Cleanup terminates
and reaps local wrappers, removes only UUID-named harness resources, then checks
the exact container and volume names are absent. Daemon failures/timeouts are
recorded as failures; they cannot leave `verified=true`. Final evidence is
written even when a cleanup command fails. Fresh output, source, logs, synthetic
credentials and closed final database copies remain for inspection.

Independent review caught and corrected three weaknesses before qualification:
ignoring acknowledged totals, losing track of A if B startup failed, and treating
failed cleanup as success. Probe timeouts were added, and the admission tests
also reject dangling symlinks before path resolution. Two oracle/safety tests
pass normally and under Python `-O`, including a negative control where local
state is correct and exchange counts grow but acknowledgment totals are stale.
No production check or side effect relies on a Python `assert`.

## Recorded result

Frozen app source `ed8f9f922bc46bd6e52b7d0776f69dad93f3184a` passed all 30 rounds
with the final launcher, under Python `-O`, on macOS 27.0.1 arm64/APFS and
Docker Linux aarch64/ext4. Both used Rust 1.99.1. The immutable container image
was `sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`.
All six paired process/container interruptions reopened the exact acknowledged
state. Final displayed state on both peers was revision 65, total 1,395 minutes,
weekly goal `[230]`, session limit `[10]`, and completed lessons 2 through 61.
Both harness Docker resources were independently confirmed absent after cleanup.

| Bound item | SHA-256 |
|---|---|
| Archived app source | `9274405db24894e4e2fe2d3ad1043337600c7d242185c140a924504764372648` |
| Final launcher | `8f413e6ac65aacf7f8b3fcd4b42f28c8c4d40a1a35dd75e01ea7f31f3cad30da` |
| Native release executable | `2a9ed3e8cc1a011c7e6f48ac41c00cb68dd15643d493f40031b62849376e6b7a` |
| Linux release executable | `00185bace5f02686d80a0281993ca3020c912f223476e31fbdd92e4fbd216fe2` |
| Evidence JSON | `cadf90328700d31cabad1029b11d1715df29c54d03e7a99f447e2b4d742a24c1` |

Full local output is `/tmp/weekend-study-soak-final-bounded-ed8f9f9`; metadata and
event copies are preserved in `target/local-qualification-study-soak/index.json`.
The subsequent handshake seam is qualified separately in
[TLS decision pairs](tls-handshake-decisions.md); this run does not retroactively
qualify arbitrary later application changes.

This accelerates 30 offline-edit/sync episodes. It is neither a wall-clock month
nor a long-duration production soak, representative large-data/resource load,
full CRDT algebra proof, transport outage/lost-reply campaign, snapshot-follower
backup test or native Windows qualification. Core crash/fuzz/sanitizer and
separate follower acknowledgment evidence retain their own recorded scopes.
