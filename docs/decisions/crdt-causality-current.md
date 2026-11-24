# CRDT register causality and automatic resolution

The two-actor register keeps a sequence, a live tag and a value for each
actor. An equal-sequence retirement supersedes a live entry. A newer entry
supersedes an older one. The receiver admits a new retirement only if the
other actor's sequence advances as its causal witness.

| Decision | Controls and expected result |
| --- | --- |
| Register encoding | The exact 34-byte register round-trips. A short output returns `Buffer`, and a short otherwise-valid input returns `Corrupt`. Tag 0 requires a zero value. Tag 1 requires a nonzero sequence. |
| Empty history | Two absent entries with zero sequences decode as the initial state. Nonzero history with both entries absent returns `Corrupt`. A canonical live entry with nonzero history decodes. |
| Sequence and ownership | A peer can echo the local sequence. Advancing it returns `Conflict`. For equal sequences, two differing live values conflict, while the same live value passes. |
| Retirement and witness | Older absence, higher live sequence and already-retired state pass. An unwitnessed equal or higher retirement returns `Conflict`. A higher sequence from the other actor supplies the witness and permits the retirement. |
| Resolution | No value, one value and two equal values leave the register bytes unchanged. Minimum and maximum policies select different results for the same distinct values. Advancing the local sequence to `u64::MAX` succeeds once. Further assignment or a distinct-value resolution returns `Exhausted` without changing bytes. |
| Publication | An unchanged peer offer returns the current receipt at revision 3. When a resolver changes the other actor's component, the receiver returns `Conflict` before sending a receipt. The current row, retained row and simulated durable image remain unchanged. |

The event oracle independently computes live values and their little-endian
wire representation for both replicas at each of 5,461 prefixes of local
assignments and directed joins, to depth six. The fixed two-actor model is
bounded. Application-specific `Record` implementations, deployed transports
and device interruption still require separate qualification.

## Verification

At `d5efd799bd59356285272fb014049007b23ae9b3`, Rust 1.100.0 passes
debug and release CRDT unit, integration and migration tests on arm64 macOS
and arm64 Docker Linux on ext4. The migration helper remains an ignored
test entry point owned by its parent tests. Strict Clippy passes, including a
Windows MSVC cross-target compile check.

In both profiles on both hosts, all 32 named mutation controls are detected,
including the existing resolver error control. Each campaign verifies five
passing baselines and observes only the designated failed test for each
mutant. The source inventory binds 16 cases to explicit decisions or error
propagation. The open-record count remains 9,284.

## Reproduce

```sh
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --lib crdt::
cargo +1.100.0 test --offline --locked --all-features -p weekend-db --test crdt --test crdt_migration
cargo +1.100.0 xtask mutations --toolchain 1.100.0 --case r1-crdt-causal- \
  --case r1-crdt-resolver-error --output target/r1/crdt-causality-debug
```

Repeat with `--release` and a separate mutation output directory.
