# Simulator durability across consecutive failed syncs

The starting durable image is `----`. Writing `ab` at offset zero followed
by `cd` at offset one makes the volatile image `acd-`. A failed sync persists
one byte of the second write, leaving `-c--`. With no intervening write, a
second sync fails after persisting one of the following subsets.

| Second sync mask | Durable bytes after the second error |
| --- | --- |
| 0 | `-c--` |
| 1 | `ab--` |
| 2 | `-cd-` |
| 3 | `abd-` |

The masked policy applies selected writes in reverse order; with both selected,
the first write wins at their overlapping byte. All four schedules still read
`acd-` through the live lease. A successful retry makes that image durable.
Writing `!` at offset three and crashing with mask one must then reopen
`acd!`: bit zero now selects the first write since the successful sync.

The [original simulator audit](simulator.md) defines S1–S4 and the coupled
lease and mask operands. Its public lease, trace, range, pending-capacity,
tear and epoch tests apply to the source below. The [allocator audit](simulator-allocator-current.md)
covers selected-provider refusal. Range checks on the prevalidated pending
writes and pre-reserved write buffers retain their stated invariants. The
32-bit offset-conversion case and physical-device durability retain their
separate platform requirements.

| Source | SHA-256 |
| --- | --- |
| `src/storage/simulator.rs` | `f52d26ee5547b04e900282b05095eee6d9d01e3faf9fe5955f22bc466206ab50` |

| Status | Target | Position | Bytes | Kind | Owner | Evidence |
|---|---|---|---|---|---|---|
| P | weekend-db::weekend_db | src/storage/simulator.rs:91:9– | 2512..2514 | if | from_image_with_allocator | S2 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:134:70– | 4061..4062 | try_propagation | crash | S1 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:135:56– | 4119..4120 | try_propagation | crash | S3 |
| C | weekend-db::weekend_db | src/storage/simulator.rs:148:9– | 4484..4486 | if | check | S1 |
| I | weekend-db::weekend_db | src/storage/simulator.rs:148:12– | 4487..4532 | short_circuit_and | check | S1 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:155:25– | 4718..4719 | try_propagation | event | S1 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:156:9– | 4729..4731 | if | event | S1 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:157:13– | 4758..4760 | if | event | S1 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:164:71– | 5057..5058 | try_propagation | event | S1 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:165:9– | 5068..5070 | if | event | S1 |
| D | weekend-db::weekend_db | src/storage/simulator.rs:173:66– | 5351..5352 | try_propagation | range | S2: 64-bit invariant; 32-bit conditional control |
| E | weekend-db::weekend_db | src/storage/simulator.rs:177:29– | 5475..5476 | try_propagation | range | S2 |
| I | weekend-db::weekend_db | src/storage/simulator.rs:187:53– | 5863..5864 | try_propagation | apply | S2: validated pending range / pre-reserved write capacity |
| P | weekend-db::weekend_db | src/storage/simulator.rs:188:5– | 5870..5872 | if | apply | S2 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:189:55– | 5947..5948 | try_propagation | apply | S2 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:197:5– | 6174..6179 | match | survive | S3 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:200:13– | 6261..6264 | for_iteration | survive | S3 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:201:71– | 6362..6363 | try_propagation | survive | S3 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:205:13– | 6432..6435 | for_iteration | survive | S3 |
| C | weekend-db::weekend_db | src/storage/simulator.rs:206:17– | 6512..6514 | if | survive | S3 |
| I | weekend-db::weekend_db | src/storage/simulator.rs:206:20– | 6515..6553 | short_circuit_and | survive | S3 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:207:75– | 6630..6631 | try_propagation | survive | S3 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:212:13– | 6730..6732 | if | survive | S3 |
| C | weekend-db::weekend_db | src/storage/simulator.rs:212:16– | 6733..6807 | short_circuit_and | survive | Overlaps torn-prefix presence and length guard (S3). |
| C | weekend-db::weekend_db | src/storage/simulator.rs:213:20– | 6764..6767 | pattern_test | survive | Pattern belongs to the enclosing torn-prefix guard (S3). |
| E | weekend-db::weekend_db | src/storage/simulator.rs:220:18– | 7026..7027 | try_propagation | survive | S3 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:229:9– | 7207..7209 | if | lock | S1 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:238:9– | 7444..7446 | if | unlock | S1 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:245:9– | 7656..7658 | if | length | S1 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:253:14– | 7870..7871 | try_propagation | length | S1 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:269:10– | 8309..8310 | try_propagation | read | S4 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:270:25– | 8336..8341 | match | read | S4 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:275:50– | 8536..8537 | try_propagation | read | S2 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:277:9– | 8621..8623 | if | read | S4 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:291:10– | 9051..9052 | try_propagation | write | S4 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:292:34– | 9087..9092 | match | write | S4 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:298:44– | 9412..9413 | try_propagation | write | S2 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:299:9– | 9423..9425 | if | write | S4 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:300:13– | 9447..9449 | if | write | S4 |
| P | weekend-db::weekend_db | src/storage/simulator.rs:308:13– | 9915..9917 | if | write | S4 |
| I | weekend-db::weekend_db | src/storage/simulator.rs:312:63– | 10142..10143 | try_propagation | write | S2: validated pending range / pre-reserved write capacity |
| P | weekend-db::weekend_db | src/storage/simulator.rs:313:13– | 10157..10159 | if | write | S4 |
| I | weekend-db::weekend_db | src/storage/simulator.rs:314:66– | 10235..10236 | try_propagation | write | S2: validated pending range / pre-reserved write capacity |
| P | weekend-db::weekend_db | src/storage/simulator.rs:321:9– | 10386..10388 | if | write | S4 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:332:10– | 10711..10712 | try_propagation | sync | S4 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:333:9– | 10722..10727 | match | sync | S4 |
| E | weekend-db::weekend_db | src/storage/simulator.rs:341:65– | 11049..11050 | try_propagation | sync | S3 |
| D | weekend-db::weekend_db | src/storage/simulator.rs:355:40– | 11452..11453 | try_propagation | new | Constructor allocation boundary |
