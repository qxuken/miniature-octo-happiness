# Write coordinator

One process owns a database file. Client processes send requests over
loopback TCP. The demo uses one ordinary unsigned record and the core
transaction API. It binds to `127.0.0.1` for use by local clients you trust.

Initialize a fresh file and start the coordinator in one terminal.

```sh
cargo run -p write-coordinator -- init counter.db
cargo run -p write-coordinator -- serve counter.db 8400
```

Start a client in each of two other terminals.

```sh
cargo run -p write-coordinator -- client 8400
```

1. Enter `get` in both clients. Each sees `value 2 0`, meaning revision 2
   and value 0.
2. Enter `set 2 20` in A and `set 2 15` in B. If A reaches the coordinator
   first, A gets `committed 3` and B gets `conflict 3 20`. If B reaches it
   first, B commits and A gets `conflict 3 15`.
3. The losing client can add its intended increment to the returned
   value and enter `set 3 35`. Both clients now see `value 4 35` on
   `get`. Use `quit` to leave a client.

The coordinator reports a commit revision after the file commit succeeds.
A conflict reports the current revision and value and leaves the file
revision alone. If a connection drops before the client receives its
reply, the client may be missing an acknowledgment for a committed
write. The demo leaves the next decision to the caller.

Restart with `serve counter.db 8400` to reopen the existing file. On
Windows, use a database path on local fixed NTFS.

```sh
cargo test -p write-coordinator
```

Use `set-id <request-id> <expected-revision> <value>` when a write must be
safe to retry after a lost reply. For example, `set-id edit_42 2 20` commits
the counter and a durable receipt together, then replies `committed 3`.
Repeating that exact request returns `committed 3`, including after a
process restart or later writes. The duplicate creates no new commit.
Reusing `edit_42` with a different expected revision or value returns
`error Invalid`.

Request IDs are 1–32 ASCII letters, digits, underscores or hyphens. The
server accepts newline-terminated messages up to 256 bytes including the
authentication prefix. It retains
at most 1,024 successful identified requests, with no expiry or eviction.
At capacity, new IDs return `error Limit`, while existing receipts remain
replayable. A stale precondition returns the existing `conflict` response
and stores no receipt. Callers may recompute a rejected request. Successful
request IDs must never be repurposed. Legacy `set` continues to operate
and provides its original response contract.

Receipts occupy reserved `receipt/` keys in collection 1 alongside the
`counter` record. The version-1 receipt has one format byte followed by
three little-endian u64 values for the expected revision, replacement
value and committed revision. One core transaction writes the counter
and receipt. The coordinator owns these keys and requires exclusive
application ownership of this database layout. Existing coordinator
files require no catalog migration and keep their current revisions
until the first new write.

If a commit returns `OutcomeUnknown`, close and reopen the coordinator,
then retry the same identified request. Recovery either finds its receipt
and returns the original acknowledgment, or permits a new conditional
attempt if the expected revision still matches. Legacy `set` requires
application reconciliation after an uncertain result. Restoring an older
backup also restores its older receipt history, so request IDs whose
receipts postdate that backup require external reconciliation before
replay.

The process tests cover a request sent without reading its reply,
process interruption and reopening, original acknowledgments after later
writes, two distinct requests racing on the same revision, and two
payloads racing with the same ID. A real-process configured-service test also
reaches all 1,024 durable receipt slots and verifies replay across maintenance
(details below). The service
continues to bind only to loopback and trusts local clients. A remote authenticated transport remains a separate requirement for a
broader deployment profile.

For authenticated localhost clients, provision a secret file containing
64 hexadecimal characters generated from 32 random bytes, optionally
followed by one newline. On Unix, set mode 0600 and keep its parent
directory private. Files granting group or other access are rejected.
The secret must be a regular file; special files such as FIFOs are rejected
before opening, and the opened handle is checked again. Keep the configuration
namespace trusted: these checks do not protect against a hostile process racing
to replace path components or redirect symlinks.
On Windows, restrict the file's ACL to the intended user accounts before
starting the service. The server and client read the secret once at
startup. Rotate it by restarting both with a new file.

```sh
umask 077
openssl rand -hex 32 > coordinator.secret
cargo run -p write-coordinator -- serve counter.db 8400 --secret-file coordinator.secret
cargo run -p write-coordinator -- client 8400 --secret-file coordinator.secret
```

The authenticated client prefixes every request with `auth TOKEN `.
The server validates the token before accessing records or receipts.
Missing and incorrect tokens receive `error Unauthorized` and cannot
publish a revision. Credentials are never printed in replies or server
error diagnostics. The file path appears in process arguments, while the
secret contents do not. Requests have a total three-second read deadline,
a 256-byte line bound, and a three-second reply write timeout.

Omitting `--secret-file` retains the original demo protocol and the server
prints an explicit unauthenticated-demo notice to stderr. Both modes bind
only to `127.0.0.1`. This is a bearer credential over localhost TCP. Host
administrators or processes able to inspect local traffic or process
memory may obtain it. All clients with the shared secret have equal
access. Remote transport, per-client authorization, connection-rate
limits and credential revocation without restart require separate work.

## Sequenced requests for long-running writers

Start this profile with a fresh database and a configured secret file.
Assign each logical writer its own slot in `0..=1023`, and keep that
assignment in the writer's durable journal. Clients use the same `client`
command and authentication prefix shown above.

```sh
cargo run -p write-coordinator -- init-sequenced counter-sequenced.db
cargo run -p write-coordinator -- serve-sequenced counter-sequenced.db 8400 \
  --secret-file coordinator.secret
```

| Request | Example response |
|---|---|
| `get` | `value 3 0` |
| `receipt 7` | `receipt 0` |
| `set-seq 7 1 3 20` | `committed 4` |
| `set-seq 7 1 3 20` after a lost reply | `committed 4` |
| `set-seq 7 2 4 35` after recording the prior receipt | `committed 5` |
| `set-seq 7 1 5 99` | `retired 1` |
| `receipt 7` | `receipt 2 4 35 5` |

The `set-seq` arguments are slot, sequence, expected database revision and
replacement value. A new slot starts at sequence 1. Reserve one in-flight
request per slot and durably record its exact arguments before sending. Retry
the same request after a lost reply or `OutcomeUnknown`. Restart the server
first if it requires recovery. Replaying the latest request yields its
original revision. A changed replay or a sequence gap returns `error Invalid`.
A stale precondition returns `conflict <revision> <value>` and leaves the
sequence available for a revised attempt.

Record or archive a successful reply before sending the next sequence. The
next successful write atomically commits the replacement record and replaces
that slot's receipt. `retired <sequence>` reports the last committed sequence
minus one. `receipt <slot>` returns
the latest sequence, original precondition, replacement and acknowledged
revision for reconciliation. A lost journal must be reconciled before sending
a new request in that slot. The server stores at most one receipt per slot.
All clients with the shared secret can access all slots, so the operator must
assign slots to trusted clients and coordinate slot handoffs.

The sequenced layout marker is `coordinator-sequenced-v1` at collection 1,
key `protocol`. Collection 2 contains `counter` and the receipt rows. Each
receipt key is `seq/` followed by a big-endian `u16` slot. Its value is format
byte `1` followed by four little-endian `u64` fields in response order.
The selector commands validate their layout before serving requests. For a
legacy cutover, quiesce writers, reconcile outstanding `set-id` requests and
archive the verified legacy file before routing clients to the fresh profile.
Use the last reconciled value to seed the new counter under an assigned slot.

Plan ongoing storage headroom checks and quiescent compaction or verified copy
according to the [operations guide](../../docs/operations.md). Physical history
grows between maintenance runs even though each slot holds one logical receipt.
For continued writing from a current backup, stop the old coordinator and
verify the new file's revision and receipts before routing clients. An older
backup can roll back both receipts and sequence watermarks. For a stale restore,
isolate the restored endpoint, rotate credentials, and reconcile client journals
before reopening it to requests. Retain the original file for reconciliation.

The extended native test drives 1,152 authenticated writes across two slots
in nine batches. After each batch it shuts down the server, copies the
verified database, and resumes on the compact copy. It independently checks
that the copy holds exactly one counter and two receipts. Replaying each
latest sequence and submitting retired sequences all preserve the selected
revision. macOS/APFS and Docker Linux/ext4 each finished at revision 1,155
with a 45,056-byte maintained image.

```sh
cargo test --offline --locked --release -p write-coordinator --test writers \
  native_sequenced_workload_reuses_receipts_across_maintenance -- --ignored --exact --nocapture
```

## Finite native receipt-capacity qualification

`native_receipt_capacity_survives_restarts_backup_restore_and_compaction` in
`tests/writers.rs` uses its own authenticated loopback server process,
a bounded test client and temporary native files. The test client allows 30
seconds for each reply so a slow hosted disk sync does not abort the durability
scenario. The regular CLI still uses its three-second deadline and is exercised
by the other process tests. The scenario submits exactly 1,024 unique requests;
request i expects revision i+2, stores value 10,000+7i and must retain its original
acknowledgment i+3. Every 128th request is sent completely without reading its
reply. A separate get observes the committed revision/value before the server is
killed and restarted: eight lost replies and eight post-commit restarts. These
are not mid-publication-barrier or device/power-loss cuts.

At capacity, a new ID with the correct current precondition must return Limit.
The first ID with a changed value or changed precondition must return Invalid.
All 1,024 exact duplicates must return their original acknowledgments, including
the oldest receipt after later writes, while current revision remains 1,026 and
current value remains 17,161.

With the service stopped, a raw core reader checks exactly 1,025 records: one
unsigned counter plus every reserved receipt key. It independently decodes all
25 bytes of each version-1 receipt and checks the expected precondition,
replacement and committed revision. It uses neither the coordinator's private
capacity constant nor its receipt codec. It verifies there are no retained roots
or recovery requirement. Verified backup and restore preserve that entire oracle;
on macOS/Linux, quiescent offline compaction does too. The compacted original and
restored copy each restart, replay every original receipt and reject new IDs at
full capacity. After stopping them, the full raw-core oracle runs again and each
file must be byte-identical to its image before those read/replay-only requests.
There are three full 1,024-receipt replay passes in one scenario. Existing
Process/Directory ownership handles child cleanup and temporary-file removal.

Run the focused scenario or all coordinator tests locally:

```sh
mise x rust@1.99.1 -- cargo test --offline --locked -p write-coordinator \
  --test writers native_receipt_capacity_survives_restarts_backup_restore_and_compaction -- --nocapture
mise x rust@1.99.1 -- cargo test --offline --locked -p write-coordinator
mise x rust@1.99.1 -- cargo test --offline --locked --release -p write-coordinator
```

The scenario has fixed counts, not a throughput/service-latency guarantee. It
does not add eviction, receipt migration, automatic retry, remote transport,
mid-barrier crash injection, concurrent 1,024-writer load or hardware durability
qualification. The protocol and service/core implementation remain unchanged.

On 2026-11-12, frozen runtime commit
`21e5d2638c24c6254ced35d0077c55bb30a287b2` plus the reviewed test overlay passed
all coordinator tests on arm64 macOS/APFS and isolated arm64 Linux/overlayfs in
both debug and release. Each profile passed five unit tests and six process
tests, including the full native-capacity scenario. Strict all-target coordinator
Clippy passed on both platforms. Each capacity run ended at revision 1,026 with
all 1,024 receipts; its original 38,526,976-byte file compacted to 4,272,128 bytes.
Backup, restored and compacted copies all passed the complete raw receipt oracle.
These file lengths are observations for this fixture, not capacity or storage
efficiency guarantees.

The source was an isolated Git archive, with only writers.rs overlaid. Its
SHA-256 is `b58d130ce65561d164128fbcd1dede1f9bf0403818c0f9be8033bc217057dfff`;
the original Cargo.lock and all archived/overlaid files stayed unchanged.
Builds used offline/locked Mise Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), separate
targets and no incremental compilation. Linux used installed immutable image
`sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08`,
the existing registry read-only, no external networking and 2 GiB/256-process
containment. Its only TCP peers were the test's own container-loopback processes.
Container removal was verified. Successful temporary databases are removed by
the existing Directory guard; logs/source/binary identities remain in evidence.

| Local qualification artifact | SHA-256 |
| --- | --- |
| `target/coordinator-native-capacity-21e5d26/source-binding.json` | `47aa33dc1ad3fd131bb46181cfeb0c27bef853346a25188a046ec05760904c4a` |
| `target/coordinator-native-capacity-21e5d26/mac/evidence.json` | `556fac721112dc6a303cedd51efcba264dee32f14003e5735de6ba04131a91b7` |
| `target/coordinator-native-capacity-21e5d26/linux/result/evidence.json` | `02fbd6acdceb121891d5b6f47727c486be8152c3a017e2ba74510b83640ac26a` |
| `target/coordinator-native-capacity-21e5d26/linux/docker-evidence.json` | `0c3fedbd532965366bd1236d4c768dbee5504427e160ef015cfda8d69db64a98` |
| `target/coordinator-native-capacity-21e5d26/windows-cross/evidence.json` | `c80b5edd2c7088d3f99b4a9dbcb4637000979e6f5880aced424c85dcf06e9b1d` |

The Windows configuration cross-compiled and linked two MSVC test executables,
including this scenario, using pinned Rust, existing cached Windows SDK/CRT
libraries and lld-link 23.1.5. **Zero Windows tests executed.** That compile/link
check does not qualify NTFS receipt persistence, Windows process/network behavior,
backup/restore or native Windows capacity. No Windows runtime was installed.

The full four-profile matrix above remains bound to its original test-source
hash. A final `cargo fmt --all --check` exposed edition-sensitive formatting in
writers.rs; formatting that file with edition 2024 changed only layout/import
order. The final test-source SHA-256 is
`d8e14b3e865c15ae6422e38d9f34eb2a6a94cc171b057130526231984f2c8c17`.
Pinned Rust 1.99.1 workspace formatting now passes, and the formatted source
passed a fresh isolated native debug capacity check with the same 1,024 receipts,
revision 1,026, eight lost replies/restarts and three complete replay passes.
Its separate evidence is
`target/coordinator-capacity-format-followup-21e5d26/evidence.json`, SHA-256
`35b12358e2de8b66664917533d998b7669760c9eeaf817aa05eca26b27181ac7`.
Earlier Mac/Linux/Windows artifacts were not relabeled as testing this new hash.

The hosted Windows x86_64 job for PR head `af91f05` later ran the native
capacity scenario in both debug and release. The job tested PR merge
`fc31b9a3e0eafbf3c4387f37f1fde8b6cb432d4f`. Both runs filled all 1,024
receipts, performed restart/replay checks, and passed backup, restore and
maintained replay. The in-place compaction arm runs on macOS and Linux.
[Native Windows job](https://github.com/qxuken/miniature-octo-happiness/actions/runs/43706610324/job/132788972396)
