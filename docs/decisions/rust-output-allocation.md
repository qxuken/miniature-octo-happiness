# Allocation failures in public Rust outputs

The standalone `allocator_routing` integration binary uses a thread-local
Global allocator observer. It accepts a prefix of allocations, then returns
null for later requests on the calling thread. The observer is disarmed before
assertions and result comparison. Each sweep requires a real refusal, the
public `OutOfMemory` result, and an eventual successful retry.

| Boundary | Evidence |
|---|---|
| Map entries and snapshot/draft scans | A selected System allocator backs core state. The sweeps refuse Global key and output buffers, then return binary-ordered rows. Current and retained state and charged bytes are checked after dropping the results and draft. |
| Retained IDs | The selected allocator prepares IDs. The final Global buffer can be refused and retried with the same ID. |
| Simulator trace and durable image | Empty outputs succeed with Global disabled. Nonempty output allocation failure preserves bytes, the trace and the writer lease. The selected allocator handles separate simulator storage. |
| Image inspection and logical export | The read-only sweeps cover their fallible Global allocation paths on a retained image. Successful inspection returns the retained ID and revision. Successful export matches the baseline bytes. The operation trace contains only length queries and reads, and the source bytes and lease remain usable. |

The allocation suite separately uses the selected allocator for observation,
overlay and allocator-owned scans. External reservations delegate capacity
reuse and grow refusal to Vec. The engine checks count overflow separately.

| Source | SHA-256 |
|---|---|
| `src/memory.rs` | `e6ebf06ad09de067f4ad341c7f517532cd9d8ca0c33fb41cc4575b092c8062cf` |
| `src/tree.rs` | `e869d415667661e56dc63766af54e70b77dfd0f14627d1d310e26312d0183f6e` |
| `src/storage/inspection.rs` | `f4c8b4b51570019de30e45a58285f28104f18c2e8b1a0c7cd9391e3b1d06fbf6` |
| `src/storage/simulator.rs` | `f52d26ee5547b04e900282b05095eee6d9d01e3faf9fe5955f22bc466206ab50` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/memory.rs:500:49– | 16859..16860 | try_propagation | into_external | Selected-to-Global output refusal |
| E | weekend-db::weekend_db | src/memory.rs:663:53– | 21790..21791 | try_propagation | copy_external | Refusal while copying a key or image |
| E | weekend-db::weekend_db | src/tree.rs:100:74– | 3210..3211 | try_propagation | materialize | Refusal while gathering ordered keys |
| E | weekend-db::weekend_db | src/storage/inspection.rs:33:75– | 1060..1061 | try_propagation | information | Refusal of retained metadata output |
| E | weekend-db::weekend_db | src/storage/simulator.rs:125:63– | 3691..3692 | try_propagation | trace | Refusal of trace output |

## Executed checks

On Rust 1.100.0, macOS arm64 passed 421 core and typed tests with two explicit
ignores per profile. Docker Linux arm64/ext4 passed 256 selected core and
storage tests per profile. Debug and release both ran the storage golden
histories. The scoped macOS default-feature run passed 43 tests.
The Rust and simulator output sweeps passed under Miri. Strict Clippy passed
on macOS and for the Windows MSVC target.

The inventory reconciled all six propagation rows against 10,582 decisions.
The remaining 9,898 source reconciliation gaps include 2,538 runtime records.
Native Windows execution retains its hosted gate.
