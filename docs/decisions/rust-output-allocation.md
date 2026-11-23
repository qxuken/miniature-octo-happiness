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
overlay and allocator-owned scans. Existing synthetic allocation failures
elsewhere retain their own scope pending migration.

| Source | SHA-256 |
|---|---|
| `src/memory.rs` | `dd4c8754ec790f53340cae4e4d1c12372683b8ac5279a9f9f04f32d71b1001d3` |
| `src/tree.rs` | `71e23e66d95a34d8919fd29e8361a1bf30d7e11198c077cf776385f88c7514b6` |
| `src/storage/inspection.rs` | `f4c8b4b51570019de30e45a58285f28104f18c2e8b1a0c7cd9391e3b1d06fbf6` |
| `src/storage/simulator.rs` | `af1a89b919143469a2f722695fefc1d9013e332e84803de2e216f6e9eaa29416` |

| Status | Target | Source span | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/memory.rs:503:49– | 16997..16998 | try_propagation | into_external | Selected-to-Global output refusal |
| E | weekend-db::weekend_db | src/memory.rs:694:49– | 22890..22891 | try_propagation | reserve_external | Rejected Global reservation propagates |
| E | weekend-db::weekend_db | src/memory.rs:703:53– | 23179..23180 | try_propagation | copy_external | Refusal while copying a key or image |
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
