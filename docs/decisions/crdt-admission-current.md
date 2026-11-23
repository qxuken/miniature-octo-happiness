# CRDT actor and policy admission

A two-writer record belongs to one collection, generation and ordered actor
roster. A `Replica` compares that policy against its stored row and against
its peer in both sync roles. The received state may echo the local actor's
counter component up to the local value. A receipt must cover the offered
state before the sender can report it as accepted.

## Admission pairs

| Decision | Controls and expected result |
| --- | --- |
| Policy structure | Nonzero collection and generation with two nonzero ordered actors pass. Zero collection, zero generation, zero first actor and nonascending actors independently reject. The equal-actor boundary also rejects. |
| Policy equality | A different collection returns `ForeignStore`. A different generation or roster at the same collection returns `Conflict`. An exact policy advances to actor validation. |
| Actor and row | The configured actor must be in the roster. Opening an absent row gives `NotFound`. Re-enrolling an existing row gives `AlreadyExists`. The persisted actor must match the configured actor, and the peer must be the other actor. |
| Frame shape and read precedence | Exact marker and length pass. Short frame, out-of-range actor index, invalid policy and invalid payload reject. Truncated marker or body returns `Io`. A wrong marker returns `UnsupportedFormat` after exactly eight bytes, leaving even a supplied body unread. |
| Record size | A 4,096-byte payload is admitted. A 4,097-byte payload is refused before encoding or reading. |
| Counter ownership and offer coverage | A peer cannot advance the local actor's component. A receipt may echo a smaller local component at the ownership check, but its state must still cover the complete offer at the coverage check. |

## Publication and recovery

The durable simulator fixture enrolls the second actor at revision 2, retains
that revision, and commits its own counter component to 2 at revision 4. It
holds the payload fixed for collection, generation, roster and wrong-peer
actor pairs. An actor-ownership case advances the local component from 2
to 3 in the peer's proposed state. A separate sender case returns component 1
to exercise incomplete receipt coverage with the ownership check satisfied.

On rejection, the receiver sends no acknowledgment. The sender emits its
original offered row and returns the error. The current row and durable
image remain byte-identical. After power-loss simulation and reopen, the
current row is still revision 4 with components `[0, 2]`, and the retained
row is revision 2 with `[0, 0]`. A valid receipt covers the offer. Serving
the valid peer state publishes `[5, 2]` at revision 5, and a subsequent
identical offer emits identical receipt bytes at the same revision.

The trusted channel fixture exercises the core `Replica` protocol.
Authenticated application transport, certificate rotation and deployed
operator acceptance have separate qualification gates.

## Verification

At `50086ab59c93b30bb17276e7c84e41bbbc60856b`, Rust 1.100.0 passes nine
CRDT unit tests, 14 CRDT integration tests and seven migration integration
tests per profile on arm64 macOS and arm64 Docker Linux on ext4. The migration
worker remains a subprocess entry point owned by the integration tests. The
existing bounded causal-event oracle runs all 5,461 prefixes. Strict Clippy
passes on both hosts and for the MSVC cross-target, and strict-provenance
Miri passes the durable admission and retry fixture.

On both hosts, the debug and release campaigns detect all 30 named mutations.
Each campaign has three distinct passing baseline tests. Each mutant runs one
named test and exits 101 with verified process cleanup. The review adds named
source evidence for 27 historically scoped decisions and binds the full
policy OR to its enclosing decision. The open-record count remains 9,284.

The same-object macOS condition run with nightly `708c94ef0` records the
following outcomes in `src/crdt.rs`.

| Condition | True | False |
| --- | ---: | ---: |
| Zero collection | 2 | 84 |
| Zero generation | 3 | 81 |
| Zero first actor | 1 | 80 |
| Actors fail ascending order | 2 | 78 |
| Collection differs | 3 | 57 |
| Remaining policy differs | 6 | 51 |

The condition counts show execution. The input pairs and isolated mutations
establish the corresponding decision and error boundaries.

## Reproduce

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --lib crdt::
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --test crdt --test crdt_migration
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-crdt-admission- \
  --output target/r1/crdt-admission-debug
```

Repeat with `--release` and a separate mutation output directory.
