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

## Compound workload (macOS/Linux)

```sh
mise exec rust@1.99.1 python@3.14.8 -- python tools/resource-envelope/compound.py \
  --revision c583ca7 --output target/compound-new
mise exec rust@1.99.1 python@3.14.8 -- python tools/resource-envelope/compound.py \
  --revision c583ca7 --output target/compound-linux-new \
  --docker-image sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08
mise exec python@3.14.8 -- python tools/resource-envelope/test_compound.py
mise exec python@3.14.8 -- python -O tools/resource-envelope/test_compound.py
```

The source archive plus explicit harness overlays builds the optional private
`weekend-compound-resource` binary with `compound` / engine fault injection.
The fixed profile combines 48 large values, 64 depth-64 retained roots, eight
held drafts plus live-budget saturation, eight writers/four rounds and two
readers. Natural image-limit rejection, copy budget failure/retry, a separate
fresh inspector and eight directed crash/recovery cases verify complete state.
Native and simulator RSS use separate worker processes; each observer has a
180-second deadline, bounded output admission and owned-group retirement. Docker
uses a private ext4 volume, no network, two CPUs, 2 GiB and 256 PIDs, with a
600-second outer deadline, partial evidence copy and checked cleanup. It requires
the named preinstalled image. Each output must be fresh, with an existing parent.

The [compound report](../../docs/compound-resource-envelope.md) records exact
memory/latency observations, budget domains, provenance and remaining scope.
These finite samples do not establish deployment SLOs or physical power-loss
durability. Crates remain unpublished.
