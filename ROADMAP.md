# weekend-db learning roadmap

Scope: [PLAN.md](PLAN.md). Budget roughly 6–8 hours per weekend. Each milestone
has a usable stopping point; split it when its acceptance cases do not fit.
Durability and concurrency validation may require several weekends each. The
user writes the implementation; this is a learning plan, not a release date or
production-readiness claim.

The integrated target architecture is in [DESIGN.md](DESIGN.md). Its concrete
data-structure and persistence sketches are candidates for the experiments here,
not a frozen implementation or file-format promise.

## Keep the core small

Core responsibilities are immutable values/collections, snapshot and transaction
handles, conservative validation of stable record assignments, storage encoding,
and durable publication/recovery. Keep typed validation, migrations, and index
maintenance in an application/wrapper layer. Source packaging is one generated
implementation header, developed from small files, with separate C tests.

Both maps and lists are early value types. A list nested inside a record belongs
entirely to that record's conflict unit. List positional transformation, operation
history, distributed replication, multi-process writers, and automatic retries
of application callbacks are outside this roadmap. Generated Rust derives,
advanced indexes, and format converters are later options.

## 1. Establish the C interface and a Rust consumer

Write a minimal C/Rust example with opaque database, snapshot, and transaction
handles, explicit-length byte values, and explicit error results. Decide the C
baseline, allocation contract, cleanup rules, and which handles/slices remain
valid after writes, abort, handle release, and database close. Keep these rules
small enough to explain on one page.

Develop as small source files and generate the public implementation header.
Do not freeze the complete database interface yet.

Done when Rust can call the minimal C implementation, two separate C translation
units can include the distribution without duplicate symbols, the development
and amalgamated builds behave alike, and ownership/cleanup tests pass. This is
a packaging proof, not a functional database.

## 2. Build a simple reference model

Use straightforward in-memory keyed records, nested maps/lists, and copied
snapshots. Select a fixed first scalar set and define absence, bounds, key
identity, and cloning semantics. Use deliberately small, deterministic fixtures.
Add the slow model's logical operations before optimizing representation.

Done when nested map/list updates, independent clones, and historical reads
have clear expected outcomes. This model becomes the oracle for persistent
collections and transaction schedules.

## 3. Implement persistent collections

Replace whole-state copying with structural sharing and path copying behind the
collection interface, retaining both maps and lists. Choose one simple tree
layout, write down its invariants, and defer tuning. Preserve full map key
identity even if routing uses hashes. Shared immutable storage must not become
shared mutable object identity between records.

Done when random operation sequences agree with the reference model, old
snapshots never change, clones diverge independently, and allocation failures
leave reachable committed state intact. Test routing collisions and shared-value
lifetimes. Record the chosen layout and its tradeoffs.

## 4. Build a whole-root transaction baseline

Implement a base snapshot, private final put/delete overlay, read-your-writes,
and atomic commit/abort. Give commits monotonic revisions. At publication check
that the current revision still equals the base revision; otherwise conflict.
Initially interleave two handles on one thread.

Done when a conflict/abort publishes none of its writes, only one of two drafts
at one base can commit, and intervening equal-value/ABA writes are detected.
This intentionally rejects unrelated concurrent edits: it is a correctness
baseline, not the completed merge feature. Do not replay old callbacks on retry.

## 5. Refine to conservative record-scoped merging

Track observed present-record generations and write-target generations. A
nested access tracks the containing record. Use a containing collection
version for missing-key reads and any supported scans; bump it on every
relevant write. This avoids relying on equality or an indefinite missing-key
history, at the cost of false conflicts.

Under one commit gate validate dependencies, apply only final assignments to the
current root, and publish. Preserve other transactions' independent changes.
Cover namespace/ancestor replacement consistently; do not use physical node
addresses as conflict identities. Add observations to the interface only with
an accompanying validation rule.

Done when two transactions updating different existing records commit in both
orders; edits within one record, including its lists, conflict; stale/missing
reads and supported scans invalidate safely; and failures never publish partial
changes. Explicitly test unrelated-insert false conflicts. Compare short
transaction schedules with a simple sequential oracle and minimize failures.
Do not claim serializability before covering every supported observation.

## 6. Specify the byte format and simulated durability

Specify integer encoding, lengths/offset bounds, value tags, format version,
checksums, root metadata, and persistence ordering. Choose and document a
recovery protocol before writing a real file adapter. An append-only
copy-on-write format with independently protected root slots is a candidate to
evaluate, not a preselected implementation.

Build a simulator separating volatile and durable bytes. Inject short writes,
allocation/I/O/sync failures, permitted reordered or dropped unsynced writes,
and torn writes under a declared failure model. Include failures in recovery and
an ambiguous commit outcome. Add corruption/operation fuzzing and regression
cases here, not only after all storage work is complete.

Done when reopening after explored failures yields a complete allowed state,
preserves all acknowledged durable commits under the model, rejects unsupported
formats, and reports malformed data safely. Define whether a failed commit
requires handle recovery before additional writes.

## 7. Add the macOS file adapter and retained snapshots

Apply the tested format/protocol to real file operations. Handle creation,
publication, synchronization, errors, and reopening. Persist snapshot retain/
release metadata. Distinguish live snapshot handles from durable retention;
closing/releasing a handle must obey the ownership contract from milestone 1.

Done when Rust writes, commits, retains a prior snapshot, reopens, and reads both
versions. Aborted state stays unpublished, and process-interruption tests agree
with the contract. Keep simulator tests: process termination alone does not
model all unsynced-write loss or reordering. These tests establish evidence
within the fault model, not immunity to arbitrary hardware failure.

## 8. Add schema metadata and one typed Rust example

Store immutable versioned descriptors as ordinary data. Define stable type/field
IDs, absence/default rules, and unknown-field preservation. A handwritten Rust
wrapper can expose field setters, but commits a final record assignment; field
IDs do not imply field-level concurrent merging.

Write an app-side migration and one explicit secondary lookup map, such as email
to record ID. Update primary data, schema/catalog references, and lookup entries
atomically. Let schema metadata guide accessors; do not build a schema compiler
or a general query engine.

Done when a rename, optional field, and semantic conversion behave explicitly;
retained records retain their old schema descriptors; unknown fields survive
supported updates; and secondary lookup stays consistent under failures and
conflicts. Document that bypassing wrapper maintenance bypasses these guarantees.
File-format migration is a different future exercise.

## 9. Add real thread overlap and platform checks

Synchronize publication/shared bookkeeping and specify which handles may be
shared across threads. Keep transaction preparation private and snapshot reads
stable. Stress simultaneous readers and compatible/conflicting commits.

Done when thread tests and applicable sanitizers pass on tested configurations.
Expand operation/corruption fuzzing and report coverage. Add Linux/Windows
build/test jobs when available, including release builds and amalgamation;
label platforms accurately rather than assuming support. Establish minimized
regressions and a release checklist. Keep multi-process writers out of scope.

## 10. Reclaim space and measure

Start compaction with a documented quiescent requirement if that keeps the
contract smaller. Copy reachable values into a new store while preserving
sharing, retained roots, and schema/lookup metadata. Define safe replacement
under the platform's file semantics and test interrupted replacement. Do not
discard the only good store before the replacement is complete.

Measure point reads, nested updates, list edits, batches, conflicts, retained
history growth, and durable commit latency. Compare equal durability, workloads,
and retention policies. Set optimization targets from actual bottlenecks.

Done when compaction preserves retained state and its metadata through modeled
failures, and measurements identify the next useful experiment. Possible next
projects are layout tuning, finer key scopes, generated wrappers, engine-format
conversion, explicit three-way merges, list rebasing, or replication.

## First weekend checklist

- [ ] Read PLAN.md and ADR 0007; write the smallest desired C and Rust usage example.
- [ ] Specify ownership, error, allocation, and byte-slice lifetime rules.
- [ ] Choose the first C baseline and compiler/test commands; test their assumptions.
- [ ] Prove ordinary source builds and generated-header builds from separate C files.
- [ ] Call the C interface from Rust and exercise creation/release/error results.
- [ ] End with a one-page contract and a list of unresolved implementation questions.

Stop there if the time budget is spent. The next task is the reference model,
not an optimization or a public durability claim. Mechanical decisions remain
explicit milestone exercises; there is no hidden commitment to a particular
tree, checksum, allocator, or test framework.
