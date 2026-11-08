# Shared ownership model

Run `cargo run --offline --locked --manifest-path tools/atomic-model/Cargo.toml`.
The isolated workspace pins Loom 0.7.2 and uses its own lockfile.

The model copies the atomic operations from `Shared::clone` and
`Shared::drop` in `src/memory.rs`. A source owner keeps the payload alive
while clone increments with Relaxed ordering. Drop decrements with AcqRel
ordering and the transition from one owner triggers destruction.

Three threads start with one owner each. Each of two worker threads clones
its owner, writes a separate payload field, and drops both owners. The
parent drops its owner before joining. The model checks that whichever
thread destroys last sees both writes, that destruction occurs exactly
once, and that the final count is zero. Loom explores schedules up to two
preemptions with a branch bound of 1,000. No permutation or duration cutoff
is configured. These bounds limit the claim to this model and workload.

Loom's Arc owns the model allocation. The custom counter independently
controls the simulated destructor. Native ownership tests and Miri check
raw allocation, pointer access and deallocation separately. The model must
be reviewed when the production atomic operations change. Replacing the
modeled AcqRel decrement with Relaxed caused the visibility assertion to
fail in the recorded run.


## Allocation admission and telemetry

`cargo run --offline --locked --manifest-path tools/atomic-model/Cargo.toml --bin budget`
checks the atomic reservation protocol separately from raw allocation. One worker
admits four units while another releases its initial four-unit reservation and
tries to admit five, under an eight-unit cap. The parent samples usage twice
before joining. Every release must correspond to accounted bytes, usage stays
within the cap, sampled peaks cannot decrease, and final live usage is zero.

The model copies the production AcqRel admission CAS and release, Acquire usage
load, and Relaxed peak fetch_max operations. Loom fetch_update represents the
production try_update CAS. It uses three threads, preemption bound two, branch
bound 1,000, and no duration/permutation cutoff. It does not model allocator
failure internals, layout arithmetic, raw pointers or collection semantics.
Successful reservation followed by release also represents allocation rollback.

The baseline passed on arm64 macOS. Two deliberate controls were detected:
`--broken-admission` replaces CAS with a load/store pair and loses accounting;
`--broken-sampling` stops persisting sampled peaks and permits a decreasing report.
The former fails the release-accounting assertion and the latter the monotone
peak assertion. `python3 tools/atomic-model/check.py` replays both ownership and
budget baselines, requires both controls to fail for those reasons, and saves
model hashes and logs. Native Linux CI runs this alongside C ASan. Review the
model when production atomics change; this remains a bounded copied model.
