# Production readiness roadmap

The [execution status](roadmap-status.md) tracks the full scope including
distributed features and remaining gates. Cargo packages stay private.

The release scope includes the local embedded store, typed Rust, C/Odin,
passive snapshot replication, fixed two-actor CRDT peers and the coordinator,
as selected by the user on 2026-11-09. Each surface has its own qualification
gates; distributed features are part of the intended release. The local
profile uses one process holding the writer lease and concurrent transactions
and readers inside that process. Qualify explicit OS, architecture and
filesystem combinations for each supported distribution.

Use SQLite as the standard for evidence. For each supported profile,
document what happens on success, contention, interruption, invalid
input, and exhaustion. Test those promises against the actual artifacts
users receive. A versioned scope and reproducible evidence are more
useful than a single project-wide coverage percentage.

## What to borrow from SQLite

SQLite checks behavior, hostile inputs, crash recovery, and the compiled library running on supported platforms through separate tests. The useful benchmark for weekend-db is whether each promise can be checked independently and reproduced against a specific release. [SQLite quality management plan](https://sqlite.org/qmplan.html)

**Use separate checks for behavior, coverage, and the delivered library.** SQLite’s public-domain Tcl suite is its main development suite. The separate public SQL Logic Test compares SQL results across engines. TH3 is a proprietary C harness that exercises the compiled library through documented interfaces and can run on embedded targets. SQLite reports 100% branch coverage for the default core configuration, including the Unix virtual filesystem, with extensions such as FTS3 and RTree outside that measurement. It measures branches with an instrumented `gcov -b` build, uses explicit probes to exercise conditions such as individual bits in a mask for modified condition/decision coverage (MC/DC), and reruns the suite with delivery compiler options to check that the results agree. Its coverage build treats designated defensive conditions as constants; other test builds assert those conditions or evaluate them normally. [SQLite test harnesses](https://sqlite.org/testing.html#test_harnesses), [TH3](https://sqlite.org/th3.html), [coverage method](https://sqlite.org/testing.html#test_coverage)

For weekend-db, compare generated operation sequences against a small independent model of documented transaction and snapshot behavior. Replay representative cases through Rust and through C and Odin consumers using the exact release artifacts. Report coverage with its metric, configuration, and exclusions, and use gaps to find untested failure paths and boundaries. A workspace-wide percentage alone says little about how well recovery or the ABI is tested.

**Inject failures, then reopen and check the data.** SQLite advances a failure counter through allocation and I/O calls, testing both one-off failures and persistent failures. Its crash simulators exercise unsynced-write loss, reordering, and damage, then reopen to check integrity and that transactions either took full effect or rolled back. It also tests a second failure during recovery. [SQLite anomaly tests](https://sqlite.org/testing.html#anomaly_testing)

Apply that pattern to commit, reopen, and compaction. Sweep the failure point through the storage operations and use a fresh handle to check the complete logical state, including retained snapshots. Assert that acknowledged commits survive and that an interrupted commit recovers to a whole allowed state. Use simulated power loss to exercise the storage model. Run process-kill and reopen tests on supported filesystem and OS combinations to exercise the actual adapter and process lifecycle. Test the allocation failures the library explicitly promises to report.

**Turn hostile inputs into permanent regressions.** SQLite uses OSS-Fuzz and its own proprietary `dbsqlfuzz`, which mutates the database and SQL together. The public `fuzzcheck` harness replays selected cases in regular testing, including known bugs; malformed database tests also check for a controlled corruption error. [SQLite fuzz testing and malformed file tests](https://sqlite.org/testing.html#fuzz_testing)

Fuzz weekend-db’s file decoder and reopen path with both malformed images and valid images modified alongside operation sequences. Check for bounded work and documented errors as well as crashes. Minimize and save failures as deterministic tests so every subsequent release reruns them.

**Make compatibility and release evidence explicit.** SQLite maintains a human-reviewed release checklist. Its 3.54.0 checklist includes platform and build variants, exported-symbol checks, and tests that the candidate and historical versions can read and write one another’s files when no unsupported feature is used. SQLite documents its file format and version policy and exposes matching version and source identifiers in the C header and running library. [Release checklist](https://sqlite.org/src/ext/checklist/3540000/index), [file format](https://sqlite.org/fileformat.html), [version policy](https://sqlite.org/versionnumbers.html), [runtime identity](https://sqlite.org/c3ref/libversion.html)

For weekend-db, state separately what is stable in the Rust API, C ABI and Odin bindings, and on-disk format. Keep fixtures written by prior releases and check every supported upgrade or downgrade direction; check that unsupported images are rejected before any write. Give each candidate a short checklist and keep its results with the artifact version and source revision.

## Where the project stands

| Area | Evidence already in the repository | What still needs a release gate |
|---|---|---|
| Core engine | Copying value model, serial replay of successful schedules, allocation fault sweeps, real thread overlap, retained history, and conflict regressions | Exact acceptance and rejection oracle for generated schedules, unsafe-code review, resource envelopes, full decision inventory |
| Durable storage | v2 codec, simulated torn and reordered writes, native leases, process interruption tests, retained-root checks, quiescent verified compact copies | Repeat and compound failures, platform and device qualification, read-only inspection, verified backup and restore, format compatibility |
| Coverage | Decision pairs and targeted mutations in the [MC/DC ledger](../DESIGN.md#mc-dc-decision-ledger). [Coverage script](../tools/coverage.py) emits Rust line and region reports | Versioned scope and denominator, recorded branch outcomes and independent condition pairs, replayable mutations, delivered binary checks |
| Fuzzing | Deterministic malformed-image, truncation, and operation tests, including some checksum-repaired images | Coverage-guided targets, persistent corpora, scheduled sanitizer runs, minimization and regression workflow |
| Platforms | [CI jobs](../.github/workflows/check.yml) define macOS, Linux, and Windows test runs. The repository records historical native runs, cross-checks, and local Rust/C/Odin examples | A fresh native matrix for the same release candidate. CI currently skips Odin, and Windows uses DLL ABI smoke via Python plus Rust tests. Add compiled Windows C/C++ clients and Odin jobs for every advertised distribution target |
| Library and operations | Native Rust core, typed Rust wrapper, experimental C header, Odin examples, task CLI, and compact-copy primitive | Supported API and ABI contract, upgrade and rollback policy, stable diagnostics, administrative tools, and versioned packaging |
| Distributed features | Passive snapshot follower, fixed two-actor CRDTs, TLS peer demo, and loopback write coordinator | Separate protocol compatibility, failure and resource contracts, authenticated deployment and recovery tooling, sustained fault and convergence evidence |

The worktree at this audit includes changes on top of `13d61c6`. Treat
the ledger and prior platform results as historical evidence tied to
their inputs. The first release evidence bundle should identify one
clean source revision, toolchain, lockfile, feature set, and artifact
hash per platform.

**Observed in this audit.** `python3 tools/check.py` passed on arm64
macOS with rustc 1.99.1, including the debug and release workspace
tests and Rust, C and Odin consumers. Native Linux and Windows
qualification is a separate release-candidate requirement.

## Functional and operational requirements

**Before the first durable production release**

- **Commit, retry, and ownership contracts.** Publish the isolation and
  conflict rules for existing records, absent keys, scans, and catalog
  changes. Document transaction termination, retained view lifetimes,
  thread and process ownership, and lock contention. Specify what
  `Io`, `OutcomeUnknown`, and `RecoveryRequired` guarantee. Provide a
  recipe for atomically recording an application request ID with its
  effect when callers need safe retries after an uncertain outcome.
  Test the contract through each shipped API.
- **Inspection and diagnostics.** Add a read-only verifier for a
  quiescent file or verified export. The current writable open syncs
  and writes a witness. Expose an `info` and `check` command or API with
  format, selected revision, retained roots, verified graph and
  schema/index status where applicable, and a useful error location
  and category. Separate cheap health/space queries from a full
  integrity scan. Surface native I/O context while keeping stable
  error categories, including actionable disk-full, quota, and
  permission errors. Include build and format identity in diagnostics.
- **Backup, restore, and migration.** Turn the verified compact-copy
  primitive into a supported offline or quiescent backup workflow.
  Define what the copy includes, how an interrupted copy is handled,
  and how users verify and restore to a fresh destination. Test a
  restore in an independent process, including retained history and
  typed lookups. Set recovery-point and recovery-time objectives for
  the supported workflow. Define a separate, best-effort salvage/export
  path for damaged images that writes to a new destination and
  identifies what was recovered. Specify identity and enrollment changes on
  restore or fork. Provide versioned logical export/import for format
  transitions and an explicit migration and rollback path for each
  supported upgrade.
- **Storage lifecycle.** Ship a predictable way to detect low headroom,
  reclaim released history, recover after interrupted maintenance, and
  identify which file is authoritative. Measure append growth, space
  required for a compact copy, blocking time, and recovery time near
  the current 64 MiB image cap and 64 retained-root cap. Keep the caps
  if they fit the first workload. Raise them or change the layout only
  with new bounds and evidence. Account for headroom needed by recovery
  and interrupted replacement files. Verify the canonical image before
  removing any orphan. Windows compact copy is an existing capability.
  Qualify in-place Windows replacement independently if it becomes part
  of the release.
- **Versioned APIs and distribution.** Decide which Rust types, methods,
  feature flags, resource limits, and error behaviors are stable.
  Specify separate compatibility rules for disk, wire, Rust API, and
  C ABI. Archive prior-version files and run read/write/upgrade and
  explicit downgrade rejection tests as applicable. Keep C layout,
  symbols, status codes, ownership and buffer rules under a versioned
  contract. Add runtime ABI/build identity, an installable header and
  Odin bindings, import/link instructions, and an ABI symbol/layout
  diff. Test packaged Rust, C, C++ and Odin clients against the exact
  shipped artifacts, including optimized builds. Document MSRV,
  toolchain, target, loader dependencies and support windows. A
  private release channel can use the same checks without crates.io
  publication. [Cargo compatibility guide](https://doc.rust-lang.org/cargo/reference/semver.html)
- **Bounded behavior and secure maintenance.** Measure maximum memory,
  stack, disk amplification, and latency on adversarial inputs and
  near-limit histories. Review fallible allocations, integer
  arithmetic, panic containment and unsafe invariants, including
  builds made by downstream consumers. Measure indexed decoder traversal, DATA-cache searches and encoder cache
  searches on near-limit graphs. Define aggregate
  memory budgets for pinned snapshots and concurrent drafts. Publish
  the supported filesystem and device assumptions, path/permission/ACL
  policy, and the corruption threat model. Provide release notes, a license
  decision before distribution, a security reporting and patch
  policy, dependency review, and reproducible release provenance.
  Record how applications can avoid leaking records or secrets in
  diagnostic output.

**Additional requirements when a surface is promoted**

- **Typed Rust.** Qualify stable field IDs, unknown fields, missing
  versus explicit null versus defaults, migrations and atomic index
  updates. Add index consistency checking and an atomic reindex or
  repair workflow. Specify catalog registration, schema/index layout
  upgrades, bounded or resumable migration, and mixed-version use.
  Make raw writes and typed index invariants an explicit ownership
  decision. Test migration and restore with old retained roots.
- **Passive snapshot replication.** Ship enrollment, restart and reseed
  tooling, configured authentication, bounded transfer and retry,
  staging cleanup, manifest growth/rotation, progress and lag
  diagnostics, mixed-version rejection/upgrade, and a backup/restore
  story. Preserve the separate local commit and follower durability
  results. Define revision and digest checks for duplicate, stale,
  conflicting, and lost replies, and keep the selected image
  authoritative even when an older image is present.
- **Offline CRDT writers.** Specify lawful merge and resolution contracts
  for app records, enforce or clearly protect reserved rows, and qualify
  policy/schema evolution, actor identity and replacement, replay,
  causal history growth, resource limits, and recovery from stale
  backups. Test mixed generation, enrollment mismatch, unauthorized
  writes, independently applied resolution, and zero extra commits
  after repeat sync. Membership changes, discovery/gossip, larger
  rosters, and cross-record constraints each need their own design and
  assurance evidence if offered.
- **Write coordinator.** To ship it beyond the loopback demo, define
  authentication and authorization, request identity and durable
  receipts, admission control, timeouts and backpressure, record
  preconditions, shutdown/restart, and failure after commit before
  reply. A high-availability variant would additionally need a
  leadership/fencing and failover contract. Independent local writes
  on one file still require the native lease contract.
- **Workload-driven expansion.** Ordered cursors and range queries,
  larger images, bounded incremental or online maintenance, richer
  indexes and query APIs, multiple file-owning processes, additional
  filesystem and device classes, encryption and key management, and
  SQL are separate product decisions. Add each when a named workload
  requires it, then expand the assurance scope for it.

## MC/DC and branch inventory by module

Create a versioned inventory of decisions in every shipped profile.
For each compound decision, record the source identity, condition
pair, test and expected state. Show each feasible condition
independently changes the outcome, including any short-circuit masking.
For a single-condition branch, record both feasible outcomes. Track
unreachable or logically coupled conditions with a proof and review
owner. Keep unknown and uninstrumented decisions visible. Record
compiler-measured line and branch data separately from manual MC/DC
evidence and mutation results. Pin a coverage toolchain only after
checking its behavior with known positive and negative examples.
Record crash outcomes independently of profile collection from killed
children. For example, the metadata check for an end above the image
cap cannot decide independently on public open once image length is
capped and the preceding condition already rejects an end above that
length. Revisit the proof if bounds or check order change.

| Module | Decisions and behaviors to qualify |
|---|---|
| [Engine](../src/engine.rs) | Transaction active/terminal state, whole-root and record/collection observations, absence/scan and ABA cases, independent edits, rebasing and overlay precedence, catalog identity, ID/revision exhaustion, retained root bounds, preparation and publication failure atomicity, and thread/poison transitions |
| [Tree](../src/tree.rs) and [values](../src/value.rs) | Hash collisions and prefix divergence, split/collapse, list boundary transitions, sharing and equality, scalar representation, foreign store, kind/index/depth/size/scan limits, and atomic allocation failure |
| [Memory](../src/memory.rs) | Independent configuration bounds, checked counters, allocation budget and errors, ownership transfer, refcount ordering and single final drop, overflow/abort policy, and adversarial cross-thread schedules |
| [Storage core](../src/storage/mod.rs) | Lease acquire/release, exact short-read/write progress and impossible adapter results, physical length and append bounds, poison and error classification, sync order, verification, and compact copy |
| [Byte codec](../src/storage/codec.rs) | Header/slot/witness parsing, absent versus checksum-invalid versus structurally invalid metadata, format and representation fields, checked arithmetic and alignment, selected root and witness, graph ranges/overlap, depth/work/object bounds, cache alias and revision checks, torn rereads, current and retained graph, encode/decode consistency |
| [File adapters](../src/storage/file.rs), [macOS](../src/storage/macos.rs), [Linux](../src/storage/linux.rs), [Windows](../src/storage/windows.rs) | Path and filesystem/volume gates, identity and symlink/reparse/hard-link handling, lease identity across races, short and interrupted I/O, platform-specific sync, error mapping, failure cleanup, reopen and replacement boundaries |
| [Replacement](../src/storage/replacement.rs) and [simulator](../src/storage/simulator.rs) | Swap/flush/rename/cleanup order, verification and uncertain outcome, prior and canonical image safety, lease epochs, durability model, pending queue and trace limits, faults during recovery, writes surviving outside the first mask word, and deterministic replay |
| [C FFI](../src/ffi.rs), [header](../include/weekend_db.h), [public exports](../src/lib.rs) | Live handle ownership, thread and close quiescence, null/length/buffer boundaries, valid foreign handles, error outputs, allocation rollback, panic and poisoned-store behavior, platform and feature dispatch. Verify signatures, discriminants and exported symbols as ABI checks |
| [Schema](../src/schema.rs) and [typed runtime](../crates/weekend-db-typed/src) | Descriptor validation and evolution, field presence and defaults, value conversion bounds, unknown fields, stale models, atomic primary/index updates, conflicts, restore and migrations, and explicit corrupt index outcomes |
| [Derives](../crates/weekend-db-macros/src) | Model, Record, and Resolution attribute combinations, invalid names/IDs/versions/order, duplicate/missing fields, generated codec and policy dispatch, compile-fail diagnostics, renamed dependencies, stable schema/wire output |
| [Snapshot replication](../src/replication) | Enrollment and version, request/reply shapes, transfer length and digest, selected image and manifest gates, dropped connection and bounded retry, monotonic progress, stale/duplicate/conflicting offers, sender acknowledgment validation, and protocol compatibility |
| [CRDT core and containers](../src/crdt.rs), [container modules](../src/crdt) | Actor and policy validation, field codecs, actor-owned progress, bounded counters and clocks, add/remove and register causality, convergence/idempotence, conflict policy and durable local publication, stale actors and no-op replay |
| [Apps](../apps), [examples](../examples), [probe](../probe), [tools](../tools) | Treat task CLI, peer and coordinator as their own lifecycle and protocol profiles if shipped. Run Rust/C/Odin examples as actual artifact consumers. Validate test-only tools and simulator oracles independently so mistakes in the harness can be detected |

The existing ledger provides the starting pairs. For the next tranche,
finish metadata parsing, graph traversal/cache checks, and record and
collection validation. Then audit the allocator and FFI unsafe boundaries,
file adapters, and replacement logic. Require every applicable decision
in a shipped safety-critical scope to have evidence or a reviewed
disposition. An unexplained gap blocks that scope. Mutation checks should
be reproducible per decision and demonstrate that a relevant test fails
when the guard or result is changed. Use concurrency models and dynamic
analysis alongside those checks, particularly for unsafe ownership and
atomic ordering.

## Coverage-guided fuzzing and independent oracles

Start with a separate fuzz workspace and persistent corpora. Run raw
byte fuzzers and structure-aware stateful targets. Reuse the repository's
copying value model and serial replay tests, then extend them to check
promised successes as well as failures. Keep oracles independent of the
decision being checked. Encoder/decoder round trips are useful secondary
checks. [Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html)

| Target | Generate | Assert |
|---|---|---|
| `open_image_bytes` | Truncations, mixed regions, arbitrary bytes, old/new formats, changing length and read errors | Bounded time and memory, documented errors, no writes on invalid pre-stabilization images, no unjustified fallback |
| `open_structured_image` | Valid histories with repaired CRC mutations to slot/witness metadata, offsets, types, object graph and catalogs | Expected revision or rejection from independent fixture rules, full current and retained state, no corruption acceptance through checksum repair |
| `image_and_ops` | Mutate a valid seed image together with transactions, opens, retains and compaction | Successful opens match a logical oracle and preserve durability across further operations |
| `value_ops`, `tree_adversarial_keys` | Nested edits, long/shared keys, collisions, list radix boundaries, floating bit patterns, persistent old roots | Independent `BTreeMap`/`Vec` semantics, immutable snapshots, meaningful size and allocation limits |
| `txn_program`, `txn_schedule` | Multiple drafts, reads, missing keys, scans, dropped collections, retain/release, allocation failure, scheduler decisions | Accepted commit order is serializable, promised compatible writes succeed, conflicts preserve old state, revisions and snapshots match |
| `commit_crash`, `retention_compact_crash` | Operation streams plus I/O phase, short/torn writes, failed sync and failure during recovery, space and namespace faults | Independent persistence model predicts allowed old/new state and exact outcomes when survival is determined. Acknowledged revisions and retained roots survive. Reopen permits further commits |
| `replacement_namespace`, `backup_restore` | Replacement phases and competing leases, orphan cleanup, interrupted copy/restore, old-format fixtures | Complete canonical image and preserved metadata, backup independently verified, original untouched on failed restore, no unrelated deletion |
| `bytestore_contract` | Lease epochs, multi-fault scripts, short/zero/oversized progress, unstable rereads, pending writes beyond 64 | Independent durable-byte and lease model, predictable error classification, fail-closed publication and no infinite loops |
| `ffi_ops` | Sequences of C calls using valid allocated handles and buffers, allowed nulls, foreign-store handles and documented concurrency | Native Rust behavior agrees, statuses and outputs follow the ABI, no leaks or UB under sanitizer. Keep intentional abort cases in subprocesses |
| `schema_index_migration` | Model versions, missing/null/default/unknown fields, corrupted catalogs, updates and migration faults | Rebuild indexes independently from primary records, preserve unknown fields and retained schemas, verify all-or-nothing changes |
| `derive_tokens` | Valid and invalid Rust declarations, attributes, generics, collisions and field orders | Bounded expansion and diagnostics, compile-fail coverage, compile representative expansions, compare bytes/schema IDs against fixtures |
| `snapshot_frames`, `snapshot_install_schedule` | Arbitrary or structured requests and replies, truncation, repeated uploads, manifest/storage faults, disconnect after publish | No false durable acknowledgment, verified selected image, monotonic progress and exact revision after reopen |
| `crdt_frames`, `crdt_schedule` | Malformed fields and frames, actor/generation errors, partition/reorder/duplicate/restart histories, resolution policies | Independent actor-history model, lawful joins and convergence, no forged progress, stable repeated sync, policy results committed atomically |
| `coordinator_protocol`, `peer_transport` | If shipping the apps, malformed requests/config, connection and deadline schedules, replay or kill around acknowledgment | Bounded work, revision/precondition checks, no lost or doubled acknowledged effect, authenticated peer and correct restart |

Seed each target with versioned valid and invalid inputs, old-release
files, boundary values, compacted and retained histories, and every
minimized regression. Run short smoke jobs for changes and scheduled
sanitizer jobs with explicit per-target time and memory budgets.
Store failing input, seed, schedule and replay command. Replay minimized
findings in deterministic tests and keep parser acceptance, allocated
bytes, work and liveness as separate observations. Include a C-side
sanitizer job for the shipped ABI. Use [Miri](https://github.com/rust-lang/miri)
for suitable unsafe Rust paths and a bounded scheduler such as
[Loom](https://github.com/tokio-rs/loom) for modeled atomic interactions.
Retain real threaded and cross-process tests on native artifacts.

## Crash, platform and release qualification

| Gate | Required evidence |
|---|---|
| Fault model | State which writes, flushes, protected regions, namespace operations and leases the adapter relies on. Separate modeled power loss, actual process kill, OS interruption and device power loss |
| Fault matrix | Sweep one-off and persistent allocation and I/O failures across create/open/commit/retain/release/backup/compact/restore. Include repeated failure during recovery, failed flush that persists some or all bytes, real disk-full/quota/permission faults, alias races, and cleanup interruption |
| Platform and filesystem | Record OS version, architecture, filesystem, mount/device/cache assumptions and native evidence for each supported profile. Start from the documented macOS, Linux and fixed NTFS paths and qualify concrete combinations. Verify real restart/power-cut behavior under the declared stack for any power-loss claim. Cross-compilation checks buildability separately |
| Compatibility | Keep a fixture manifest with format, API/ABI, wire and schema versions. Exercise old-reader/new-writer and new-reader/old-writer directions only where promised, including upgrade interruption and restoring the pre-upgrade backup. Inspect unknown files without altering them |
| Binary delivery | Build from a clean tag with locked dependencies and record compiler flags, artifact hashes, and exported symbols. Run tests using the packaged artifacts on supported targets. Include release Rust/C/C++/Odin consumers, import/link/install instructions, and feature-isolated as well as combined builds. Archive provenance and test output |
| Soak and reproducibility | Run long-lived mixed read/write, retain/release, maintenance, and restart workloads under concurrency. Check retained-state and resource budgets throughout, save failing schedules, and replay failures against the candidate |
| Performance and scale | Set budgets for one real checkpoint-heavy application before release. Measure p50/p95/p99 commit and reopen, RSS and stack, bytes per edit, history growth, conflict/retry rate, compaction pause and space, backup/restore time, and follower lag if shipped. Include worst-case parser work and compare SQLite under matched durability and retained-history semantics |
| Support and release | Independently review durability assumptions and all unsafe boundaries. Resolve reproduced data-loss, UB, or resource-exhaustion findings in the shipped profile. Replay the corpus and verify support, incident response, upgrade, backup, restore and rollback instructions with a fresh consumer. Track exceptions with an owner and a bounded support claim |

Recovery must preserve each acknowledged commit under the declared
storage model and return a whole allowed revision for interrupted
commits. A readable older graph does not justify reporting it when the
selected newest graph fails. Loss of synchronized bytes, dishonest
flushes, and correlated destruction of root and witness require their
own failure containment and restore procedures. [SQLite atomic commit assumptions](https://www.sqlite.org/atomiccommit.html#hardware_assumptions)

## Order of work

The first batch checkpointed the reviewed work at `dcd2402`, added the
[metadata decision pairs](../DESIGN.md#rec-v6-validate-metadata-before-choosing-a-root),
and landed the [first three fuzz harnesses](../fuzz/README.md) with
reproducible seeds and AddressSanitizer smoke results.

The second batch adds the
[record and collection decision cases](../DESIGN.md#tx-v2-collection-and-record-observations),
[graph traversal and alias checks](../DESIGN.md#rec-v7-traversal-aliases-and-stable-object-reads),
and the `txn_schedule` fuzz target with an exact commit/conflict oracle.
The ledger records defensive bounds that cannot independently reject
under the current image and graph invariants. Parser resource limits,
the remaining allocator/FFI boundaries, and the fuzz inventory stay
tracked in the module tables above.

The allocator and FFI batch adds [MEM-V1/MEM-V2](../DESIGN.md#mem-v1-independent-configuration-bounds)
and [FFI-V1](../DESIGN.md#ffi-v1-input-output-and-ownership-boundaries).
Eight allocator configuration mutations and eight FFI mutations were
detected. The scoped tests passed in debug, release, and a combined
nightly AddressSanitizer run on arm64 macOS. Remaining native path,
panic, atomic-interleaving, abort-threshold, and platform checks are
explicitly tracked in those ledger entries.

1. **Define the release candidate scope and baseline.** Checkpoint the
   reviewed code, assign owners to the support matrix and compatibility
   policies, and turn the decision ledger into a repeatable audit.
2. **Finish the core input and durability assurance tranche.** Audit the
   remaining parser, traversal, transaction, allocator and FFI decisions.
   Land the first image, operation/schedule, and fault fuzz targets with
   deterministic replay. Start with the metadata decision table and
   `open_image_bytes`, `open_structured_image`, and `commit_crash`.
   Qualify native leases, flushes and recovery.
3. **Make data operable.** Ship read-only verification, tested
   backup/restore, explicit format migration, actionable diagnostics,
   and a maintenance story that fits the chosen size and retention
   envelope.
4. **Qualify the public release.** Stabilize the chosen Rust/API/ABI
   profiles, test packaged artifacts on the supported matrix, and run
   one real workload and a restore drill against the candidate. Keep
   the compatibility and evidence bundle alongside the release.
5. **Promote optional surfaces individually.** Typed schema/indexes,
   passive replication, CRDT peers and the coordinator each need their
   scoped functional and assurance gates before making the same
   production promise.
