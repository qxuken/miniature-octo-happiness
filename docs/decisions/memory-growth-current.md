# Allocation limits and charged growth

The [allocator audit](memory.md) records the eight invalid configurations
and the shared valid baseline. The scope manifest binds those pairs to
the current validation expression and its existing mutation cases.

For a four-byte allocation, asking for a fifth byte tries capacity eight.
A nine-byte live cap rejects the speculative twelve-byte peak but admits
the exact nine-byte peak. If the provider refuses that exact allocation,
the original bytes, capacity and live charge survive. With a larger live cap,
a provider refusal at capacity eight propagates immediately. The fixtures
also hold the requested count at the declared maximum and one beyond it.
The [budget tests](../memory-budget-decisions.md) and [public-output tests](rust-output-allocation.md)
cover the related arithmetic and ownership dispositions below.

| Source | SHA-256 |
| --- | --- |
| `src/memory.rs` | `e6ebf06ad09de067f4ad341c7f517532cd9d8ca0c33fb41cc4575b092c8062cf` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| E | weekend-db::weekend_db | src/memory.rs:415:75– | 13795..13796 | try_propagation | reserve | Count overflow leaves the original vector intact. |
| P | weekend-db::weekend_db | src/memory.rs:416:9– | 13806..13808 | if | reserve | An existing capacity satisfies the request without a charge. |
| E | weekend-db::weekend_db | src/memory.rs:419:74– | 13951..13952 | try_propagation | reserve | Unrepresentable layout returns Limit before provider allocation. |
| E | weekend-db::weekend_db | src/memory.rs:420:46– | 13999..14000 | try_propagation | reserve | Budget must admit the full replacement while the old charge is live. |
| P | weekend-db::weekend_db | src/memory.rs:421:9– | 14010..14012 | if | reserve | First allocation selects the provider; growth retains it. |
| E | weekend-db::weekend_db | src/memory.rs:426:45– | 14202..14203 | try_propagation | reserve | Provider refusal drops the prospective charge and preserves the old buffer. |
| E | weekend-db::weekend_db | src/memory.rs:434:75– | 14706..14707 | try_propagation | reserve_geometric | Length plus extra overflow returns Limit. |
| P | weekend-db::weekend_db | src/memory.rs:438:9– | 14795..14797 | if | reserve_geometric | Spare capacity fits without another provider request. |
| E | weekend-db::weekend_db | src/memory.rs:445:9– | 15005..15010 | match | reserve_geometric | Success, Limit and OutOfMemory take distinct tested paths. |
| P | weekend-db::weekend_db | src/memory.rs:446:34– | 15091..15109 | match_guard | reserve_geometric | Rounded Limit retries exactly; exact Limit and provider refusal propagate. |
| E | weekend-db::weekend_db | src/memory.rs:458:9– | 15496..15499 | for_iteration | extend | Reserved extension consumes the supplied elements, including ZSTs. |
| P | weekend-db::weekend_db | src/memory.rs:493:9– | 16572..16574 | if | into_external | Global transfers the original buffer; selected storage copies into public output. |
| E | weekend-db::weekend_db | src/memory.rs:494:13– | 16622..16624 | if | into_external | Empty selected storage returns an empty output; nonempty output is fallible. |
| E | weekend-db::weekend_db | src/memory.rs:568:42– | 18654..18655 | try_propagation | new | Invalid limits reject before allocating the budget state. |
| E | weekend-db::weekend_db | src/memory.rs:574:10– | 18816..18817 | try_propagation | new | Refusing the budget block propagates OutOfMemory. |
| E | weekend-db::weekend_db | src/memory.rs:584:39– | 19172..19173 | try_propagation | admit | Atomic budget admission rejects count overflow and a cap overrun. |
| E | weekend-db::weekend_db | src/memory.rs:647:70– | 21187..21188 | try_propagation | shared | Shared layout must fit the live cap before allocation. |
| E | weekend-db::weekend_db | src/memory.rs:655:57– | 21457..21458 | try_propagation | reserve_external | Count overflow returns Limit before external reservation. |
| I | weekend-db::weekend_db | src/memory.rs:659:9– | 21617..21619 | if | copy_external | A valid Rust byte slice cannot exceed isize::MAX. |
| E | weekend-db::weekend_db | src/memory.rs:680:44– | 22353..22354 | try_propagation | copy | Copied storage propagates reservation failure before extending. |
| E | weekend-db::weekend_db | src/memory.rs:690:58– | 22797..22798 | try_propagation | raw | Failed boxed ownership propagates before raw transfer. |
| C | weekend-db::weekend_db | src/memory.rs:267:12– | 9152..9206 | short_circuit_or | validate | Intermediate OR of the eight independently varied configuration bounds. |
| C | weekend-db::weekend_db | src/memory.rs:267:12– | 9152..9250 | short_circuit_or | validate | Intermediate OR of the eight independently varied configuration bounds. |
| C | weekend-db::weekend_db | src/memory.rs:267:12– | 9152..9293 | short_circuit_or | validate | Intermediate OR of the eight independently varied configuration bounds. |
| C | weekend-db::weekend_db | src/memory.rs:267:12– | 9152..9342 | short_circuit_or | validate | Intermediate OR of the eight independently varied configuration bounds. |
| C | weekend-db::weekend_db | src/memory.rs:267:12– | 9152..9394 | short_circuit_or | validate | Intermediate OR of the eight independently varied configuration bounds. |
| C | weekend-db::weekend_db | src/memory.rs:267:12– | 9152..9452 | short_circuit_or | validate | Intermediate OR of the eight independently varied configuration bounds. |
| C | weekend-db::weekend_db | src/memory.rs:267:12– | 9152..9516 | short_circuit_or | validate | Intermediate OR of the eight independently varied configuration bounds. |
