# StudyProgress peers

Each process owns one file and accepts edits on standard input. The two
processes use the same StudyProgress record as the in-memory example.
Their sync workers exchange full record state over mutual TLS. Each actor
is configured with the other actor's certificate and address.

From the workspace root, initialize both actors in fresh files. Use the
same nonzero collection ID on both sides of a pair.

```sh
cargo build -p study-peer
./target/debug/study-peer keygen a-identity
./target/debug/study-peer keygen b-identity
./target/debug/study-peer init a.db a 0123456789abcdef0123456789abcdef
./target/debug/study-peer init b.db b 0123456789abcdef0123456789abcdef
```

Start each process in a separate terminal. On Windows, use the `.exe` binary
and database paths on local fixed NTFS. For separate machines, copy only
the peer's `cert.der` over a trusted channel and keep each `key.der` private.

```sh
# Terminal A
./target/debug/study-peer run a.db a 0123456789abcdef0123456789abcdef 127.0.0.1:8301 127.0.0.1:8302 a-identity b-identity/cert.der --no-push
# Terminal B
./target/debug/study-peer run b.db b 0123456789abcdef0123456789abcdef 127.0.0.1:8302 127.0.0.1:8301 b-identity a-identity/cert.der --no-push
```

Enter `edit 1 20 60 20` on A and `edit 2 15 90 30` on B. Each prints
`local <revision>` as soon as its own file has committed. Enter `push on` on
both. Once they have exchanged state, `show` on each displays
`state <revision> 35 [90] [20] [1, 2]`. Those fields are the local revision, total
practice minutes, committed weekly goal values, committed session limit
values, and completed lessons.

`peer` shows the most recent outgoing result and the number of successful
exchanges since startup. For example, `peer 3 ack 35` means the third
acknowledgment covered the offered state and reported 35 practice minutes
on the remote peer. `push off` pauses future outgoing attempts while the
listener stays available. Use `quit` to stop and repeat `run` to reopen the
existing file. By default, outgoing sync starts on launch.

## TLS admission and coordinated upgrades

Upgrade both study peers together: the hardened CLI requires `study-progress/1`
ALPN, so older binaries without ALPN are rejected. Snapshot uses the distinct
`weekend-snapshot/1` protocol. Enrollment, on-disk encodings, merge rules and
application wire messages are unchanged; no CRDT migration is required.

Both apps compile one private TLS implementation with fixed profiles. Study
retains SAN `study-peer` and a three-second absolute exchange budget, now including
TCP connect. Before application admission, the handshake must finish, pending
writes must drain, and the exact configured leaf pin and ALPN must match. Disk
work is outside the socket deadline; local and remote acknowledgments stay separate.

Credentials are trusted local configuration: prevent concurrent untrusted edits.
Certificates/pins must be regular, nonempty and at most 64 KiB; PKCS#8 keys at
most 16 KiB. Opened Unix keys must have no group/other permission bits. Keygen
creates a 0700 directory and 0600 key. Windows keygen fails closed: provision
cert.der/key.der externally with a private ACL. The CLI does not verify Windows
ACLs or provide race-proof hostile-directory traversal. Credentials are validated
before database open/listen. See [shared TLS evidence](../../docs/shared-app-tls.md).

The [persistent credential cutover guide](../../docs/study-credential-cutover.md)
tests stopped-endpoint leaf/pin replacement on Mac/Linux, preserved offline
commits and retained roots, stale-credential rejection, and full-record
convergence without extra repeat-sync commits. Actor identities stay fixed.
