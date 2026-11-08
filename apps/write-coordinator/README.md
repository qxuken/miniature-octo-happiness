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
payloads racing with the same ID. The in-process test reaches receipt
capacity and verifies that old acknowledgments still replay. The service
continues to bind only to loopback and trusts local clients. A remote authenticated transport remains a separate requirement for a
broader deployment profile.

For authenticated localhost clients, provision a secret file containing
64 hexadecimal characters generated from 32 random bytes, optionally
followed by one newline. On Unix, set mode 0600 and keep its parent
directory private. Files granting group or other access are rejected.
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
