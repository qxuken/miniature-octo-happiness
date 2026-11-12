# Finite private resource envelope

Requires a new output directory whose parent already exists. Uses the existing
SQLite-baseline runner's tested no-overwrite/source-command helpers and public
engine APIs; no core code or configured limits are expanded.

```sh
mise x python@3.14.8 -- python3 tools/resource-envelope/run.py \
  target/resource-envelope-new --revision 96e105f80c189a2f9de12530518c4d834f2796ea
mise x python@3.14.8 -- python3 tools/resource-envelope/run.py \
  target/resource-envelope-linux-new --revision 96e105f80c189a2f9de12530518c4d834f2796ea \
  --docker-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
RESOURCE_ENVELOPE_EVIDENCE=target/resource-envelope-new/evidence.json \
  mise x python@3.14.8 -- python3 -O tools/resource-envelope/test_runner.py
```

Fixed workload: 64 depth-64 roots, real draft/owned-value pressure against a
64 MiB requested-core budget, a separately labeled dense-zero-tail exact image
boundary, and distinct 896 KiB reachable payloads until natural image admission
fails under a separate 256 MiB core budget. A dedicated subprocess uses a worker
thread with requested 1 MiB stack for depth-64 value and logical-parser work.
The engine's 64 MiB physical image cap is unchanged throughout.

Each driver has a 180-second deadline; Docker has a 300-second outer deadline,
2 GiB memory and 256-process limits, no network, immutable image ID, native
writable-layer data files, bounded cleanup and explicit removal verification.
Native timeout cleanup kills the child process group, including the stack worker.
macOS RSS probes have a five-second timeout; setup probes/builds default to 120
seconds. A stuck probe fails qualification and records terminal cleanup metadata.
Actual sources, toolchains, phase timings/core/RSS/file bytes, raw RSS sample count
and independent Python byte-oracle checks are recorded. Core requested layouts,
sampled RSS, file lengths and requested worker stack are separate quantities.
See [the qualification report](../../docs/resource-envelope.md) for actual results
and exclusions. No publication or remote service is used.
