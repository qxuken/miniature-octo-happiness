# Transaction model: a smaller core

Research date: 2026-11-04. This review informed the record-scoped compromise
accepted in [ADR 0007](../adr/0007-merge-stable-record-assignments.md), replacing
the earlier list-rebasing direction for the initial roadmap. Product facts cite
primary sources. Contracts, examples, and recommendations are design inferences;
the discussion below records the alternatives evaluated during that review.

## What the precedents establish

redb 4.4.0 permits multiple readers concurrently with a writer, but only one
write transaction at a time. Its API blocks a second writer until the first
finishes. This is a useful embedded-database precedent for retaining snapshots
without implementing overlapping writers. [redb Database API](https://docs.rs/redb/latest/redb/struct.Database.html#method.begin_write)

SQLite likewise serializes writers. WAL readers keep an unchanged snapshot;
attempting to promote a stale reader to a writer fails with
`SQLITE_BUSY_SNAPSHOT`, requiring a new transaction. [SQLite isolation](https://www.sqlite.org/isolation.html)

Optimistic concurrency control separates private work, validation, and public
writes; validation failure restarts the transaction. This is a concurrency
protocol, not just a tree-diff algorithm. [Kung and Robinson, 1981](https://www.eecs.harvard.edu/~htk/publication/1981-tods-kung-robinson.pdf)

PostgreSQL's Serializable implementation monitors dependencies and uses predicate
locks to detect writes affecting previous query results; it is Serializable
Snapshot Isolation, not a prescription for this project's simpler OCC.
Applications must retry serialization failures. [PostgreSQL transaction isolation](https://www.postgresql.org/docs/18/transaction-iso.html#XACT-SERIALIZABLE)

## Five alternatives

The tradeoffs below are design analysis.

| Model | Benefit | Cost and limitation |
| --- | --- | --- |
| **1. Single active writer; snapshot readers** | Small contract; mutations use ordinary private copy-on-write state; no merge log or read-set validation | Writer holds its turn throughout the transaction; long edits delay other writers |
| **2. Private branches; whole-root revision check** | Overlapping local drafts; tiny validation rule; no positional rebasing | Any intervening commit rejects a draft, including unrelated changes; retries discard work |
| **3. Conservative logical-domain OCC** | Unrelated collections/records can commit independently; lists remain ordinary values | Must track every observation and correctly propagate generations; conflicts within a domain are intentionally coarse |
| **4. Key/field OCC plus list-operation transform** | Most permissive automatic merging among these choices | Adds operation capture, target/gap rules, read dependencies, history retention, and nested-operation interactions; largest correctness surface |
| **5. Explicit merge outside commit** | Application chooses meaning and resolves conflicts; core remains small | Merge can be stale before publication; application owns decisions and retries |

For **1**, acquire writer ownership before reading the current writable root.
A snapshot reader cannot later publish edits without starting a fresh writer.
Retained old snapshots remain useful independently of write concurrency.

For **2**, save a monotonic database revision at draft creation; under the commit
gate, publish only if it still equals the current revision. Check and publication
must be indivisible. Use a revision, not root-value equality or a reusable pointer.
This rejects ABA (`A → B → A`) even when content returns to its starting value.
It deliberately implements “no intervening commit,” which is stricter than
“observed values still equal.” Re-execute application logic on retry; replaying
its old writes can preserve stale decisions.

For **3**, a useful compromise is stable outer record keys mapping to immutable
values; nested maps/lists belong to that outer record's conflict domain. Track
successful point reads, write generations, and a final `put/delete` overlay.
Under one commit lock validate dependencies and write generations, then apply
only overlay entries to the current root. Different record keys can merge;
edits to one record, including its whole list, conflict. No positional replay or
global list-operation history is required. Publishing the entire private root
would lose unrelated commits.

For absent-key reads/writes and scans, record a containing collection generation,
incremented on every relevant write. This conservatively catches absence ABA and
predicate changes without retaining missing-key tombstone history. Generations
must detect writes independently of value equality. Disjoint inserts can still
conflict. Nested reads depend on the
outer record; ancestor removal/replacement must invalidate descendants. These
are logical identities, not copy-on-write page addresses. If mutable aliases are
exposed, all paths to one logical object need the same conflict identity. A
single outer keyspace avoids much of that identity problem. This is substantially
less machinery than list transformation, but still requires complete observation
tracking and atomic validation/publication.

For **4**, keep explicit insert/remove/replace operations, transform them through
intervening committed operations, order same-gap inserts by commit order, and
reject overlapping target edits. Also account for earlier operations in the
same transaction. Budget the retained history needed by live local transactions;
on exhaustion reject/expire transactions explicitly. This avoids requiring a
distributed protocol, but does not make the local algorithm trivial.

Automerge illustrates an alternative: list operations identify elements, and
inserts reference a predecessor's identity. Its merge rules ensure agreement
between replicas, including arbitrarily resolved concurrent assignments. That is
a different contract from rejecting conflicting atomic transactions.
[Automerge merge rules](https://automerge.org/docs/reference/under-the-hood/merge-rules/),
[Automerge conflicts](https://automerge.org/docs/reference/documents/conflicts/)

For **5**, expose a separate three-way merge helper returning a candidate plus
conflicts. Publish the resolved candidate through a fresh writer or a checked
revision. An explicit merge must not silently inherit a claim that the original
transaction's reads remain valid.

## The corner cases that determine the cost

**Intent:** base `[A, A]`, current `[A]`. Those two states cannot reveal which
occurrence was deleted. A pending replacement of the second occurrence should
either follow that occurrence or conflict if it was deleted. A diff can choose
an edit script; it cannot recover the missing historical fact. Retaining roots
alone therefore cannot implement the promised list semantics.

**Read dependencies:** initially `a = b = true`. T1 reads `b`, then sets `a =
false`; T2 reads `a`, then sets `b = false`. Disjoint-write merging accepts both
and produces both false, although neither serial ordering does. Point-read
validation would reject the second commit. But absence checks, list length,
iteration, and range/predicate queries are observations too. Two transactions
that each observe “no active owner” and insert different owner keys need a
membership/predicate dependency. Coarse collection generations cover this by
rejecting any collection change; recording only returned keys misses the insert.
PostgreSQL defines such changing query-result sets as phantoms and monitors
read/write dependencies to prevent serialization anomalies.
[PostgreSQL isolation definitions](https://www.postgresql.org/docs/18/transaction-iso.html)

Therefore, do not call an OCC prototype serializable until every supported
observation—including absent reads and ancestor identity—is covered. Decide
separately whether equal-value writes count as conflicts; revision-based
validation detects history, whereas value comparison detects current equality.

## Recommendation: keep v1 small

**Design inference:** build model 2 as a correctness baseline, or model 1 if
overlapping drafts are unnecessary. Then implement the concrete model-3
compromise if preserving two disjoint transactions is a firm product goal;
measurement should determine whether to refine its conservative conflicts.
It deliberately withdraws same-list rebasing and field-level merging within a
record. Preserve snapshots and durable publication; keep model 5 optional and
defer model 4 to a separate experiment. This recommendation reflects scope and
correctness costs, not a measured throughput advantage.

Durability is independent of merge sophistication: redb's Immediate durability
promises persistence when commit returns. The file backend still needs its own
crash-safe publication protocol. [redb Durability API](https://docs.rs/redb/latest/redb/enum.Durability.html)

For the proposed schema design, descriptors are metadata; they do not create
indexes. Schema/index catalogs can be ordinary versioned keys in the same
transaction. Application-maintained secondary maps must change atomically with
primary records. Wrappers can validate/migrate records, but cannot enforce
uniqueness against bypassing raw writes: that requires engine validation or
restricting the write interface. PostgreSQL provides an explicit contrasting
contract: declaring a unique constraint creates its enforcing unique index.
[PostgreSQL unique constraints](https://www.postgresql.org/docs/18/ddl-constraints.html#DDL-CONSTRAINTS-UNIQUE-CONSTRAINTS)
