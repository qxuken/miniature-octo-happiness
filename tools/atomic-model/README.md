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
