# Shared ownership model

Run `cargo run --offline --locked --manifest-path tools/atomic-model/Cargo.toml`.
The isolated workspace pins Loom 0.7.2 and uses its own lockfile.

The model copies `retain_ref` and `release_ref` from `src/memory.rs`.
Both `Shared` and `AllocatorHandle` use those operations. A source owner
keeps the payload alive while clone increments with Relaxed ordering. Drop
decrements with AcqRel ordering and the transition from one owner triggers
destruction.

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
be reviewed when the production atomic operations change. `cargo xtask
atomic-model` runs `--broken-shared-release`, which replaces the modeled
AcqRel decrement with Relaxed and must fail the last owner's visibility
assertion.


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
peak assertion. `cargo xtask atomic-model` replays both ownership and
budget baselines, requires both controls to fail for those reasons, and saves
model hashes and logs. Native Linux CI runs this alongside C ASan. Review the
model when production atomics change; this remains a bounded copied model.

## C handle accounting

`cargo run --offline --locked --manifest-path tools/atomic-model/Cargo.toml -- --ffi-handles`
models the count used by `make_handle`, `Handle::drop` and `wdb_close` in
`src/ffi.rs`. Two workers contend for reservations. One simulates an
allocation refusal and rolls back. The other keeps its reservation until
after the parent has joined both workers. The model covers a live source
handle and an initial count just below exhaustion.

After joining, an Acquire load must count exactly the prior owners and any
successful acquisition. The model also checks that releasing the successful
reservation restores the initial count. Its bounds are three threads,
two preemptions and 1,000 branches, with no duration or permutation cutoff.
It assumes the C contract's external close quiescence. Rust tests check
raw handle allocation, outputs and destruction through the exported functions.

`cargo xtask atomic-model` runs this baseline and checks that
`--broken-ffi-reservation` and `--broken-ffi-rollback` fail the post-join
accounting assertion. Review the copied operations when the FFI count changes.
