# Coordinator operator stop and restart

The bounded loopback coordinator accepts operator stop through `SIGTERM` and
`SIGINT` on macOS and Linux. An attached Windows console uses Ctrl+C or
Ctrl+Break. The signal handler sets a stop flag. The listener uses a 50 ms
timeout for each idle readiness wait and checks the flag when the wait
returns. It also rechecks before admitting a socket. Incoming
connections can wake the readiness wait immediately.
[Microsoft console control handler](https://learn.microsoft.com/windows/console/setconsolectrlhandler)
and [Winsock readiness](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-wsapoll).

After the listener closes, the database worker finishes a command it already
dispatched. It checks the flag before starting the next command and drops the
queued requests on stop. A connection without a complete line can still use
its remaining read deadline. The server joins its workers and releases the
native lease before successful exit. A database worker unwind also requests
stop and surfaces a failed exit. A running storage operation can take longer
than any socket deadline. The acknowledgment still follows the durable
receipt, and the usual identified retry applies to an unconfirmed result.

## Controls

| Stop point | Verified result |
|---|---|
| `SIGTERM` at the baseline `e1065dd` | The native control reproduced exit by signal 15. |
| `SIGTERM` or `SIGINT` with an unfinished connection on the candidate | The listener closes and the server exits successfully. Reopening the same path reports revision 4, replays its receipt and finds the incomplete request's slot empty. |
| Stop after one command is dispatched with another waiting | The first handler finishes and acknowledges revision 4. The second reply channel closes before dispatch. A simulated crash and reopen recover revision 4 and no receipt for the queued write. |
| Database dispatcher unwinds | The thread sets the stop flag, and the failed join reports the panic to the owner. |

### Native ext4 append boundary

The release test overlay pauses the revision-4 write after its append sync
and before root-slot publication. A separate read-only inspection of the
staged image still selects revision 3. A fully authenticated second write is
queued at that point. The test delivers real `SIGTERM` and observes the
listener close with the database lease still held.

Releasing the append pause produces the first client's `committed 4` reply.
The queued client sees EOF. The server exits successfully, and a fresh
process reopens revision 4 with the first receipt and no second receipt.
Retrying the first request returns `committed 4` again. The overlay provides
timing markers and the pause. Publication and recovery use the normal ext4
adapter and storage calls.

## Reproduction and scope

From the archived checkout at `56b8f2e`, run the application tests and both
extended lifecycle controls with the workspace lockfile.

```sh
cargo test --locked -p write-coordinator
cargo test --locked --release -p write-coordinator
cargo test --locked --release -p write-coordinator --test writers -- --ignored --nocapture
```

For the ext4 process test, extract the clean `source.tar` under
`target/coordinator-shutdown-proof/`, apply `native-barrier.patch` with
`patch -p1`, and place the test's `TMPDIR` on the ext4 volume.

```sh
cargo test --locked --release -p write-coordinator --test writers \
  stop_at_native_append_sync_finishes_one_write -- --exact --nocapture
```

| Proof input under `target/coordinator-shutdown-proof/` | SHA-256 |
|---|---|
| `source.tar` | `685722b9d77e23a2a48547f01e2d86e4eb608cf7d02b1cd49fc918d586ced7d2` |
| `native-barrier.patch` | `aecf948ea19238e7fea89376f4b329f8af20f6b9148b635652e2779b55ad4fcd` |

Native macOS/APFS and Docker Linux arm64/ext4 each passed 29 ordinary checks
in debug and release and both extended 1,152-write maintenance runs. Strict
Clippy passed on both platforms and on the Windows MSVC x64 cross-target.
On macOS, ThreadSanitizer with `nightly-2026-11-12` and a rebuilt standard
library passed five service controls and the native operator-stop test.
The archived Linux log contains both the clean source suite and the distinct
overlay run at the real append barrier.

The stop contract covers the loopback coordinator. Windows console event
execution, service-manager shutdown and physical-device behavior need their
own qualification evidence.
