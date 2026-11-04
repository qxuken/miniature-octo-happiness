# Positioning research: useful beyond learning?

Research date: 2026-11-04. Scope: the target in [DESIGN.md](../../DESIGN.md), compared with primary documentation and source; no implementation or market-demand evidence is assumed.

## Assessment

**Inference:** yes, there is a credible specialist niche: a portable C application-state store with direct nested persistent values, explicitly retained historical roots that survive reopening, and atomic record-scoped optimistic transactions. The opportunity is this combination and its integration experience. Neither “embedded NoSQL” nor “immutable database” alone distinguishes it. This review establishes differences among close alternatives, not universal competitor absence or proof that developers want another engine.

“Direct nested values” matters: changing one leaf should share unchanged stored subtrees without decoding and rewriting the entire containing document. “Historical” means an application-retained version remains addressable after shutdown and compaction, not merely a read transaction observing a fixed current view. Both are explicit targets, still requiring implementation evidence. [Design §§2, 7](../../DESIGN.md)

## Closest alternatives

| Alternative | Existing overlap | Consequence for positioning |
| --- | --- | --- |
| xitdb | Embedded, nested persistent collections, file-stored history, cheap undo | Closest abstraction; C packaging and transaction contract must earn their keep |
| Vedis | Portable embedded C, structured commands, transactions, amalgamation | C plus hashes/lists is already available |
| UnQLite | Portable C amalgamation, transactional KV and document storage | Differentiate persistent value graphs from encoded documents |
| LMDB | C, copy-on-write, concurrent snapshot readers | A live MVCC view is not an explicit durable history catalog |
| redb | Embedded Rust, persistent rollback savepoints | Durable checkpoints already exist; distinguish historical reading from restoration |

### xitdb: the decisive comparison

xitdb already exposes arbitrarily nested maps/lists, structural sharing, memory/file backends and incremental access. Its documented pattern stores successive “moments” in a top-level list; undo appends a reference to an earlier moment. This is stored history, not only live-reader isolation. Writes are serialized; readers can run concurrently. [Official README](https://github.com/xit-vcs/xitdb#readme)

The inspected local implementation loads its root from disk, syncs appended data before publishing the root, and defaults file synchronization on. Its compactor treats a root list as history and copies only its latest moment into a new history. These source facts support normal reopen persistence and explain history loss under that compaction policy; they are not an independent crash-safety certification. [Local source, `init`, `commitRoot`, `compact`](../../../libs/xitdb/src/lib.zig)

The first-party organization also has [Java](https://github.com/xit-vcs/xitdb-java), [TypeScript](https://github.com/xit-vcs/xitdb-ts) and [Go](https://github.com/xit-vcs/xitdb-go) implementations. **Inference:** “usable outside Zig” is insufficient differentiation. A C source distribution with Rust bindings, strict immutable private values, selective retention through compaction and compatible independent-record commits is a more defensible comparison. The last three are weekend-db targets, not benchmarked advantages. [Design §§2–7](../../DESIGN.md)

### Vedis: packaging and structured storage already overlap

Vedis is an embedded C datastore with disk and memory modes and a cross-platform file format. Its command surface includes hashes, sets, lists, and BEGIN/COMMIT/ROLLBACK. It ships as `vedis.c` plus a header. [Overview](https://vedis.symisc.net/), [commands](https://vedis.symisc.net/commands.html), [amalgamation source](https://github.com/symisc/vedis/blob/master/vedis.c)

Its hash command implementation stores entry data in byte blobs and returns strings; transaction rollback invokes the pager's current-transaction rollback. The reviewed API does not expose named retained historical roots or xitdb-style arbitrarily nested immutable collection handles. That is a bounded finding about this interface, not proof history could never be built above it. [Source: `vedis_cmd_hmget`, `vedis_rollback`](https://github.com/symisc/vedis/blob/master/vedis.c)

### UnQLite: nested documents differ from persistent subtrees

UnQLite already provides in-process transactional C storage, a portable file, an amalgamation, raw binary KV and a Jx9 document layer. [Official repository](https://github.com/symisc/unqlite)

The distinction is visible in code: `CollectionStore` and `CollectionUpdate` call `FastJsonEncode`, then replace a KV record; fetching reads the record and calls `FastJsonDecode`. Nested JSON therefore does not itself supply a public persistent map/list graph whose unchanged subtrees are shared across application versions. The reviewed public transaction interface is begin/commit/rollback, not a retained-snapshot catalog. [Document storage implementation](https://github.com/symisc/unqlite/blob/master/unqlite.c), [public header](https://github.com/symisc/unqlite/blob/master/unqlite.h)

### LMDB and redb: be precise about snapshots

LMDB offers C transactions, copy-on-write pages, concurrent readers and one active writer. Its API documents that long-lived readers prevent page reuse and that freed pages are otherwise recycled. Its normal read transactions pin views during their lifetime; the reviewed API does not provide an application catalog of arbitrary retained roots to reopen later. Separate database copies can preserve versions, with different costs and management. [Official API source](https://github.com/LMDB/lmdb/blob/mdb.master/libraries/liblmdb/lmdb.h)

redb explicitly has `persistent_savepoint`, deletion/listing and `restore_savepoint`; these require appropriate durability and retain otherwise reusable pages. Persistent savepoints survive reopening, as its tests demonstrate. However, `Savepoint` is a restoration token, with no direct historical table-reading API exposed on it. Restoration happens in a write transaction and invalidates later savepoints. This overlaps checkpoint/rollback use cases, without being the same historical-read interface. [WriteTransaction](https://docs.rs/redb/latest/redb/struct.WriteTransaction.html), [Savepoint](https://docs.rs/redb/latest/redb/struct.Savepoint.html), [tests: `persistent_savepoint`](https://github.com/cberner/redb/blob/master/tests/integration_tests.rs)

## What the current design can honestly serve

**Recommendations, not evidence of demand:** start with an application that owns one writer process and needs historical inspection of structured state:

- A desktop configuration/workspace store: inspect the settings and metadata associated with named checkpoints after reopening.
- A local build or analysis tool: retain selected dependency graphs or analysis results and read an earlier run without loading complete serialized graphs.
- A game/tooling state inspector: checkpoint stable entities and compare versions while ordinary work continues.

These examples fit keyed records and selective checkpoints. They do not require SQL, replication or collaborative text merging. They still need retention budgets, useful inspection and a concrete consumer.

Do not advertise automatic undo, branching or an editor backend yet. Transactions currently begin at the current head; retained snapshots are readable, but arbitrary historical writes, old-root restoration and a branching API are absent. An application could deliberately create compensating assignments, but that requires its own restoration semantics. Middle list insertion/removal may cost O(n), and two edits to different fields or list positions inside one record conflict. Even unrelated new inserts can conflict through collection-level absence tracking. These are material constraints for large editable documents. [Design §§2, 4, 5, 7](../../DESIGN.md)

Record-level OCC is conventional concurrency control, not a fundamental invention: private work followed by read/write validation and publication has long precedent. **Inference:** its value here is a clear conflict contract and avoiding unnecessary retries for independent records; measure whether that matters before making it the headline. [Kung and Robinson](https://www.eecs.harvard.edu/~htk/publication/1981-tods-kung-robinson.pdf)

## Validation before expanding scope

**Recommendation:** validate one consumer against xitdb and an encoded-document baseline with matching durability and retention policies. Measure integration effort, allocations, bytes written for a leaf update, retained-history growth, reopen latency, durable commit latency and conflict/retry rates. A result is useful only if the consumer values the difference enough to adopt the API.

Require crash/reopen tests preserving acknowledged commits and retained roots; compaction must preserve selected history and reclaim released history. Exercise same-record conflicts, independent existing records, absence/scan dependencies and allocation failures. Explicitly demonstrate historical inspection; do not imply restoration from it. [Design §§7, 10](../../DESIGN.md)

The present design also lacks a stable disk format, proven recovery, inspection/export/repair tools, established bindings, ordered range queries and automatic index/schema enforcement. **Inference:** these delivery costs may outweigh any algorithmic advantage for an early adopter. A small trustworthy state store with one convincing workload would establish usefulness beyond learning; expanding toward a general database before that would not establish demand. [Design §§3, 8–11](../../DESIGN.md)
