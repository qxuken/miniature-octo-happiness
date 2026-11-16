# Bounded coordinator admission and dispatch

One process owns the coordinator database and serves local clients on
loopback. The server bounds connection work independently from database work.
It admits up to eight sockets and queues up to eight complete, authorized
commands. A single database worker processes queued commands in FIFO order.
Preconditions, transaction publication and durable receipt creation still run
as one command. A thread waiting for a request line or sending a reply holds
no database state.

The server starts a three-second admission deadline at accept. The deadline
covers framing the request and waiting for dispatch. The dispatcher checks the
deadline and the connection's admission receiver before starting the database
command. The connection then waits for the command to finish, with a separate
three-second socket write timeout for the reply. The client's own deadline can
expire during a storage operation. The durable receipt determines the result
when the client cannot read the reply.

Full queue and full connection admissions both reject before dispatch.
The listener attempts `error Busy` on an excess socket without waiting and
closes it. A client can see EOF or a reset if the socket closes before it
reads the response. The application queue counts commands, and the operating
system still owns the listen backlog. The profile retains the local trust
boundary and shared-secret authorization for configured clients.

## Regression and decision controls

| Case | Expected and observed outcome |
|---|---|
| One authenticated client withholds its newline | The baseline `bf246aa` fails the second writer's native regression after a two-second read timeout. The second writer now commits revision 4. The unfinished request leaves its slot empty. |
| Eight unfinished connections fill the admission limit | The ninth connection receives `error Busy`. After stopping and reopening, revision 3 and all eight empty receipt slots remain authoritative. |
| Full command queue | A complete request receives `error Busy`, and the queued commands remain untouched. A released connection slot can be admitted again. |
| Expired command and dropped admission receiver | Both are queued behind a held command and skipped before database dispatch. The two valid commands reach revisions 4 and 5. The skipped requests leave both receipt slots empty. |
| Busy after durable journal staging | Reopening recovers the exact pending intent. `journal resume` sends the same command bytes and records the confirmed revision 4 receipt. The durable state advances once. |
| Transport closes after an identified write | Bounded polling observes the previous or expected exact state without repeating the write. After the expected publication, restart and replay return the original receipt. |

The first Linux capacity run exposed an observer that assumed the order of TCP
connections also ordered complete commands. At `6849ef6` the interrupted-reply
tests poll the exact expected state before killing the process. The tests
preserve the original no-resend check on the first request and compare the
returned receipt after reopening.

## Reproduction

Run the native app suite and the two extended maintenance profiles from the
archived checkout. The maintenance cases each publish 1,152 writes and verify
the state and receipts at nine cutovers.

```sh
cargo test --locked -p write-coordinator
cargo test --locked --release -p write-coordinator
cargo test --locked --release -p write-coordinator --test writers -- --ignored --nocapture
cargo clippy --locked -p write-coordinator --all-targets -- -D warnings
```

At `6849ef6`, the macOS/APFS release suite passed 26 ordinary checks. Docker
Linux/ext4 passed the same 26 in debug and release. Each platform separately
passed both extended cases, including all 1,152 writes and nine cutovers per
case. The final server reopened at revision 1,155. The journal run ended with
two sequence-576 receipts and no pending intent. The 1,024-receipt legacy
capacity run also passed, including its eight lost-reply/restart checkpoints.

The extended journal samples measured the following client write times,
including process startup, durable staging, network exchange, server commit
and local receipt persistence.

| Platform | p50 | p95 | Maximum |
|---|---:|---:|---:|
| macOS arm64/APFS | 33.779 ms | 62.104 ms | 73.444 ms |
| Docker Linux arm64/ext4 | 1.773 ms | 2.343 ms | 4.306 ms |

These are finite observations on separate hosts. Strict Clippy passed on both
platforms and on the Windows MSVC x64 cross-target. Native macOS
ThreadSanitizer (`nightly-2026-11-12`) passed the three queue controls and the
live incomplete-request overlap control. Native Windows admission tests remain
pending.

The source, first reproduced Linux observer failure, terminal logs and
checksums are stored under `target/coordinator-admission-proof/`. The final
archive is `6849ef62f8fe258c48b03dedfa561f4312430ef5` and has SHA-256
`5f5548db07a6203417daaf3e30875cb3c55138de5e8decce1bcff3fb59dadc97`.
The bounded loopback profile retains separate release gates for service-level
latency targets, remote authorization and graceful operator shutdown.
