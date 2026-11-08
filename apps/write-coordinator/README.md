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
