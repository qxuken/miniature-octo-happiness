#![cfg(any(
    target_os = "macos",
    windows,
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
))]

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BIN: &str = env!("CARGO_BIN_EXE_write-coordinator");
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "write-coordinator-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Process {
    child: Child,
    input: ChildStdin,
    output: Receiver<String>,
}

impl Process {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(BIN)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            output,
        }
    }

    fn send(&mut self, command: &str) {
        writeln!(self.input, "{command}").unwrap();
    }

    fn receive(&self) -> String {
        self.output
            .recv_timeout(Duration::from_secs(15))
            .expect("process exited or timed out before replying")
    }

    fn stop(&mut self) {
        self.child.kill().unwrap();
        assert!(!self.child.wait().unwrap().success());
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn two_processes_edit_one_record_and_the_stale_writer_conflicts() {
    let dir = Directory::new();
    let path = dir.0.join("counter.db");
    let path = path.to_str().unwrap();
    assert!(
        Command::new(BIN)
            .args(["init", path])
            .status()
            .unwrap()
            .success()
    );
    let mut server = Process::start(&["serve", path, "0"]);
    let ready = server.receive();
    let port = ready.strip_prefix("ready 127.0.0.1:").unwrap();
    let mut a = Process::start(&["client", port]);
    let mut b = Process::start(&["client", port]);
    a.send("get");
    b.send("get");
    assert_eq!(a.receive(), "value 2 0");
    assert_eq!(b.receive(), "value 2 0");

    // Both clients choose an increment from the same observed value.
    // Submit both before waiting for either result.
    a.send("set 2 20");
    b.send("set 2 15");
    let (reply_a, reply_b) = (a.receive(), b.receive());
    let (loser, conflict, increment) = match (reply_a.as_str(), reply_b.as_str()) {
        ("committed 3", conflict @ "conflict 3 20") => (&mut b, conflict, 15),
        (conflict @ "conflict 3 15", "committed 3") => (&mut a, conflict, 20),
        _ => panic!("expected one commit and one conflict, got {reply_a:?} and {reply_b:?}"),
    };
    // The loser uses the value returned with the conflict to recompute its
    // increment. Reusing the original replacement would drop the winner's edit.
    let (_, state) = conflict.split_once(' ').unwrap();
    let (revision, current) = state.split_once(' ').unwrap();
    let value = current.parse::<u64>().unwrap() + increment;
    loser.send(&format!("set {revision} {value}"));
    assert_eq!(loser.receive(), "committed 4");
    a.send("get");
    b.send("get");
    assert_eq!(a.receive(), "value 4 35");
    assert_eq!(b.receive(), "value 4 35");

    server.stop();
    let restarted = Process::start(&["serve", path, "0"]);
    let ready = restarted.receive();
    let port = ready.strip_prefix("ready 127.0.0.1:").unwrap();
    let mut reader = Process::start(&["client", port]);
    reader.send("set 2 20");
    assert_eq!(reader.receive(), "conflict 4 35");
    reader.send("get");
    assert_eq!(reader.receive(), "value 4 35");
}

#[test]
fn durable_request_receipts_survive_lost_reply_restart_and_competing_requests() {
    use std::net::{Shutdown, TcpStream};
    let dir = Directory::new();
    let file = dir.0.join("receipts.db");
    let path = file.to_str().unwrap();
    assert!(
        Command::new(BIN)
            .args(["init", path])
            .status()
            .unwrap()
            .success()
    );
    let mut server = Process::start(&["serve", path, "0"]);
    let ready = server.receive();
    let port = ready.strip_prefix("ready 127.0.0.1:").unwrap();
    // Drop the connection after sending a complete request and before reading
    // any reply. A separate reader observes its durable effect before kill.
    let mut lost = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    writeln!(lost, "set-id lost 2 10").unwrap();
    lost.shutdown(Shutdown::Write).unwrap();
    drop(lost);
    let mut observer = Process::start(&["client", port]);
    observer.send("get");
    assert_eq!(observer.receive(), "value 3 10");
    server.stop();
    let mut server = Process::start(&["serve", path, "0"]);
    let ready = server.receive();
    let port = ready.strip_prefix("ready 127.0.0.1:").unwrap();
    let mut a = Process::start(&["client", port]);
    let mut b = Process::start(&["client", port]);
    a.send("set-id lost 2 10");
    assert_eq!(a.receive(), "committed 3");
    a.send("set-id a 3 20");
    b.send("set-id b 3 30");
    let replies = (a.receive(), b.receive());
    assert!(
        matches!(
            (replies.0.as_str(), replies.1.as_str()),
            ("committed 4", "conflict 4 20") | ("conflict 4 30", "committed 4")
        ),
        "{replies:?}"
    );
    // One ID with different payloads can never commit twice.
    a.send("set-id same 4 40");
    b.send("set-id same 4 50");
    let replies = (a.receive(), b.receive());
    assert!(
        matches!(
            (replies.0.as_str(), replies.1.as_str()),
            ("committed 5", "error Invalid") | ("error Invalid", "committed 5")
        ),
        "{replies:?}"
    );
    a.send("set-id lost 2 10");
    assert_eq!(a.receive(), "committed 3");
    a.send("set-id lost 2 99");
    assert_eq!(a.receive(), "error Invalid");
    server.stop();
    let restarted = Process::start(&["serve", path, "0"]);
    let ready = restarted.receive();
    let port = ready.strip_prefix("ready 127.0.0.1:").unwrap();
    let mut reader = Process::start(&["client", port]);
    reader.send("set-id lost 2 10");
    assert_eq!(reader.receive(), "committed 3");
    reader.send("get");
    assert!(matches!(
        reader.receive().as_str(),
        "value 5 40" | "value 5 50"
    ));
}

// Independently fixed by the public application contract, not the service's
// private constant or receipt encoder/decoder.
const DURABLE_RECEIPTS: usize = 1024;

fn capacity_value(index: usize) -> u64 {
    10_000 + index as u64 * 7
}

fn capacity_request(index: usize) -> String {
    format!(
        "set-id capacity_{index:04} {} {}",
        index as u64 + 2,
        capacity_value(index)
    )
}

fn configured_service(
    command: &str,
    path: &str,
    secret: &str,
    token: &str,
) -> (Process, AuthenticatedClient) {
    let server = Process::start(&[command, path, "0", "--secret-file", secret]);
    let ready = server.receive();
    let address = ready.strip_prefix("ready ").unwrap().parse().unwrap();
    (
        server,
        AuthenticatedClient {
            address,
            token: token.to_owned(),
        },
    )
}

struct AuthenticatedClient {
    address: std::net::SocketAddr,
    token: String,
}
impl AuthenticatedClient {
    fn send(&self, command: &str) -> std::net::TcpStream {
        let bounds = Duration::from_secs(5);
        let mut socket = std::net::TcpStream::connect_timeout(&self.address, bounds).unwrap();
        socket.set_write_timeout(Some(bounds)).unwrap();
        writeln!(socket, "auth {} {command}", self.token).unwrap();
        socket
    }

    fn request(&self, command: &str) -> String {
        // The capacity oracle tests durability, even when one hosted disk
        // flush runs past the app client's ordinary three-second deadline.
        let mut socket = self.send(command);
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut reply = Vec::with_capacity(64);
        for _ in 0..128 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "capacity reply deadline: {command}");
            socket.set_read_timeout(Some(remaining)).unwrap();
            let mut byte = [0];
            socket
                .read_exact(&mut byte)
                .unwrap_or_else(|e| panic!("capacity reply for {command}: {e}"));
            if byte[0] == b'\n' {
                return String::from_utf8(reply).unwrap();
            }
            reply.push(byte[0]);
        }
        panic!("capacity response exceeds 128 bytes: {command}");
    }
}

fn write_secret(file: &std::path::Path, token: &str) {
    fs::write(file, token).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn sequenced_clients_preserve_receipts_through_restart_and_restore() {
    use std::net::Shutdown;
    use weekend_db::{Limits, storage};

    let dir = Directory::new();
    let path = dir.0.join("sequenced.db");
    let secret = dir.0.join("secret");
    let token = "d".repeat(64);
    write_secret(&secret, &token);
    let secret = secret.to_str().unwrap();
    let path = path.to_str().unwrap();
    let legacy = dir.0.join("legacy.db");
    let legacy = legacy.to_str().unwrap();
    for (command, file) in [("init-sequenced", path), ("init", legacy)] {
        assert!(
            Command::new(BIN)
                .args([command, file])
                .status()
                .unwrap()
                .success()
        );
    }
    for args in [
        vec!["serve-sequenced", path, "0"],
        vec!["serve-sequenced", legacy, "0", "--secret-file", secret],
        vec!["serve", path, "0", "--secret-file", secret],
    ] {
        let mut rejected = Process::start(&args);
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(exit) = rejected.child.try_wait().unwrap() {
                assert!(!exit.success(), "{args:?}");
                break;
            }
            assert!(Instant::now() < deadline, "incompatible startup: {args:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    let (mut server, client) = configured_service("serve-sequenced", path, secret, &token);
    let unauthorized = AuthenticatedClient {
        address: client.address,
        token: "e".repeat(64),
    };
    assert_eq!(
        unauthorized.request("set-seq 0 1 3 99"),
        "error Unauthorized"
    );
    assert_eq!(client.request("get"), "value 3 0");
    assert_eq!(client.request("receipt 0"), "receipt 0");
    assert_eq!(client.request("receipt 1024"), "error Invalid");
    assert_eq!(client.request("set-seq 0 0 3 99"), "error Invalid");
    assert_eq!(client.request("set-seq 0 2 3 99"), "error Invalid");

    let lost = client.send("set-seq 0 1 3 10");
    lost.shutdown(Shutdown::Write).unwrap();
    drop(lost);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let receipt = client.request("receipt 0");
        if receipt != "receipt 0" {
            assert_eq!(receipt, "receipt 1 3 10 4");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "lost reply request didn't reach publication"
        );
    }
    server.stop();
    let (mut server, client) = configured_service("serve-sequenced", path, secret, &token);
    assert_eq!(client.request("set-seq 0 1 3 10"), "committed 4");
    assert_eq!(client.request("set-seq 0 1 3 11"), "error Invalid");

    let port = client.address.port().to_string();
    let mut a = Process::start(&["client", &port, "--secret-file", secret]);
    let mut b = Process::start(&["client", &port, "--secret-file", secret]);
    a.send("set-seq 0 2 4 20");
    b.send("set-seq 1023 1 4 30");
    let replies = (a.receive(), b.receive());
    let (a_expected, b_expected, final_value) = match (replies.0.as_str(), replies.1.as_str()) {
        ("committed 5", "conflict 5 20") => (4, 5, 30),
        ("conflict 5 30", "committed 5") => (5, 4, 20),
        _ => panic!("expected one commit and one conflict, got {replies:?}"),
    };
    let a_request = format!("set-seq 0 2 {a_expected} 20");
    let b_request = format!("set-seq 1023 1 {b_expected} 30");
    let assert_receipts = |client: &AuthenticatedClient| {
        assert_eq!(
            client.request(&a_request),
            format!("committed {}", a_expected + 1)
        );
        assert_eq!(
            client.request(&b_request),
            format!("committed {}", b_expected + 1)
        );
        assert_eq!(client.request("set-seq 0 1 6 99"), "retired 1");
        assert_eq!(
            client.request("receipt 0"),
            format!("receipt 2 {a_expected} 20 {}", a_expected + 1)
        );
        assert_eq!(
            client.request("receipt 1023"),
            format!("receipt 1 {b_expected} 30 {}", b_expected + 1)
        );
        assert_eq!(client.request("get"), format!("value 6 {final_value}"));
    };
    let conflicted = if a_expected == 5 {
        "receipt 0"
    } else {
        "receipt 1023"
    };
    let before_retry = if a_expected == 5 {
        "receipt 1 3 10 4"
    } else {
        "receipt 0"
    };
    assert_eq!(client.request(conflicted), before_retry);
    assert_receipts(&client);
    server.stop();

    let backup = dir.0.join("backup.db");
    let restored = dir.0.join("restored.db");
    storage::backup_file(path, &backup, Limits::default()).unwrap();
    storage::restore_file(&backup, &restored, Limits::default()).unwrap();
    assert_eq!(fs::read(&backup).unwrap(), fs::read(&restored).unwrap());
    let (_server, client) = configured_service(
        "serve-sequenced",
        restored.to_str().unwrap(),
        secret,
        &token,
    );
    assert_receipts(&client);
    assert_eq!(client.request("set-seq 0 3 6 40"), "committed 7");
    assert_eq!(client.request(&a_request), "retired 2");
    assert_eq!(client.request("get"), "value 7 40");
    assert_eq!(
        storage::inspect_file(path, Limits::default())
            .unwrap()
            .revision,
        6
    );
}

#[test]
#[ignore = "extended native maintenance workload"]
fn native_sequenced_workload_reuses_receipts_across_maintenance() {
    use weekend_db::{Database, Limits, storage};

    let directory = Directory::new();
    let mut path = directory.0.join("sequenced.db");
    let key = directory.0.join("secret");
    let token = "d".repeat(64);
    write_secret(&key, &token);
    let key = key.to_str().unwrap();
    assert!(
        Command::new(BIN)
            .arg("init-sequenced")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );

    let batch = 128;
    let mut writes = 0u64;
    for cycle in 1..=9 {
        let (mut server, client) =
            configured_service("serve-sequenced", path.to_str().unwrap(), key, &token);
        assert_eq!(
            client.request("get"),
            format!("value {} {writes}", writes + 3)
        );
        for n in writes + 1..=writes + batch {
            let slot = (n - 1) % 2;
            let sequence = n.div_ceil(2);
            let expected = n + 2;
            assert_eq!(
                client.request(&format!("set-seq {slot} {sequence} {expected} {n}")),
                format!("committed {}", n + 3)
            );
        }
        writes += batch;
        server.stop();

        let destination = directory.0.join(format!("copy-{cycle}.db"));
        let sizes = storage::backup_file(&path, &destination, Limits::default()).unwrap();
        assert!(sizes.after_bytes < sizes.before_bytes);
        let verified = storage::inspect_file(&path, Limits::default()).unwrap();
        assert_eq!(
            (verified.revision, verified.physical_bytes),
            (writes + 3, sizes.before_bytes)
        );
        let reopened = Database::open_file(&destination, Limits::default()).unwrap();
        let snapshot = reopened.snapshot();
        let collection = reopened.collection_id(2);
        assert_eq!(snapshot.revision(), writes + 3);
        assert_eq!(
            snapshot
                .get(collection, b"counter")
                .unwrap()
                .unwrap()
                .as_u64(),
            Ok(writes)
        );
        assert_eq!(snapshot.scan(collection).unwrap().len(), 3);
        drop(reopened);
        fs::remove_file(&path).unwrap();
        path = destination;

        let (mut server, client) =
            configured_service("serve-sequenced", path.to_str().unwrap(), key, &token);
        let sequence = writes / 2;
        for (slot, n) in [(0, writes - 1), (1, writes)] {
            let expected = n + 2;
            let revision = n + 3;
            assert_eq!(
                client.request(&format!("receipt {slot}")),
                format!("receipt {sequence} {expected} {n} {revision}")
            );
            assert_eq!(
                client.request(&format!("set-seq {slot} {sequence} {expected} {n}")),
                format!("committed {revision}")
            );
            assert_eq!(
                client.request(&format!("set-seq {slot} 1 {} 999", writes + 3)),
                format!("retired {}", sequence - 1)
            );
        }
        assert_eq!(
            client.request("get"),
            format!("value {} {writes}", writes + 3)
        );
        server.stop();
        eprintln!(
            "sequenced maintenance cycle {cycle}: {writes} commits, revision {}, {} -> {} bytes",
            writes + 3,
            sizes.before_bytes,
            sizes.after_bytes
        );
    }
}

fn verify_all_capacity_receipts(path: &std::path::Path) -> u64 {
    use weekend_db::{Database, Limits};

    let db = Database::open_file(path, Limits::default()).unwrap();
    let snapshot = db.snapshot();
    assert_eq!(snapshot.revision(), DURABLE_RECEIPTS as u64 + 2);
    assert!(db.retained_ids().unwrap().is_empty());
    assert!(!db.requires_recovery());
    let rows = snapshot.scan(db.collection_id(1)).unwrap();
    assert_eq!(rows.len(), DURABLE_RECEIPTS + 1);
    let mut seen = [false; DURABLE_RECEIPTS];
    let mut counter_seen = false;
    for (key, value) in rows {
        if key == b"counter" {
            assert!(!counter_seen);
            counter_seen = true;
            assert_eq!(
                value.as_u64().unwrap(),
                capacity_value(DURABLE_RECEIPTS - 1)
            );
            continue;
        }
        let suffix = key.strip_prefix(b"receipt/capacity_").unwrap();
        assert_eq!(suffix.len(), 4);
        assert!(suffix.iter().all(u8::is_ascii_digit));
        let index: usize = std::str::from_utf8(suffix).unwrap().parse().unwrap();
        assert!(index < DURABLE_RECEIPTS && !seen[index]);
        assert_eq!(key, format!("receipt/capacity_{index:04}").as_bytes());
        seen[index] = true;
        let bytes = value.as_bytes().unwrap();
        assert_eq!(bytes.len(), 25);
        assert_eq!(bytes[0], 1);
        // Decode the public version-1 wire layout directly through raw core.
        // No coordinator parser/receipt helper participates in this oracle.
        assert_eq!(
            u64::from_le_bytes(bytes[1..9].try_into().unwrap()),
            index as u64 + 2
        );
        assert_eq!(
            u64::from_le_bytes(bytes[9..17].try_into().unwrap()),
            capacity_value(index)
        );
        assert_eq!(
            u64::from_le_bytes(bytes[17..25].try_into().unwrap()),
            index as u64 + 3
        );
    }
    assert!(counter_seen && seen.into_iter().all(|found| found));
    fs::metadata(path).unwrap().len()
}

fn full_service_replays_without_new_revision(client: &AuthenticatedClient) {
    let state = format!(
        "value {} {}",
        DURABLE_RECEIPTS + 2,
        capacity_value(DURABLE_RECEIPTS - 1)
    );
    assert_eq!(client.request("get"), state);
    assert_eq!(
        client.request(&format!(
            "set-id capacity_overflow {} 999999",
            DURABLE_RECEIPTS + 2
        )),
        "error Limit"
    );
    assert_eq!(
        client.request("set-id capacity_0000 2 10001"),
        "error Invalid"
    );
    assert_eq!(
        client.request("set-id capacity_0000 3 10000"),
        "error Invalid"
    );
    for index in 0..DURABLE_RECEIPTS {
        assert_eq!(
            client.request(&capacity_request(index)),
            format!("committed {}", index + 3)
        );
    }
    assert_eq!(client.request("get"), state);
}

#[test]
fn native_receipt_capacity_survives_restarts_backup_restore_and_compaction() {
    use std::net::Shutdown;
    use weekend_db::{Limits, storage};

    let dir = Directory::new();
    let file = dir.0.join("capacity.db");
    let secret_file = dir.0.join("secret");
    let token = "c".repeat(64);
    write_secret(&secret_file, &token);
    let path = file.to_str().unwrap();
    let secret = secret_file.to_str().unwrap();
    assert!(
        Command::new(BIN)
            .args(["init", path])
            .status()
            .unwrap()
            .success()
    );
    let (mut server, mut client) = configured_service("serve", path, secret, &token);
    // Direct stderr bypasses libtest's per-test capture so CI interruption
    // preserves the last completed phase without reducing this full-cap test.
    writeln!(std::io::stderr(), "receipt capacity: filling 1024 receipts").unwrap();
    let mut lost_replies = 0;
    let mut restarts = 0;
    for index in 0..DURABLE_RECEIPTS {
        let request = capacity_request(index);
        if (index + 1) % 128 == 0 {
            // A complete authenticated request is sent without reading a reply.
            // Observe the committed state separately before interruption: this
            // deliberately does not claim a kill during a publication barrier.
            let lost = client.send(&request);
            lost.shutdown(Shutdown::Write).unwrap();
            drop(lost);
            lost_replies += 1;
            assert_eq!(
                client.request("get"),
                format!("value {} {}", index + 3, capacity_value(index))
            );
            server.stop();
            (server, client) = configured_service("serve", path, secret, &token);
            restarts += 1;
            assert_eq!(client.request(&request), format!("committed {}", index + 3));
            // The very first receipt remains the original acknowledgment even
            // after later writes and every post-commit restart.
            assert_eq!(client.request(&capacity_request(0)), "committed 3");
            writeln!(
                std::io::stderr(),
                "receipt capacity: committed {}/{}; restarted and replayed",
                index + 1,
                DURABLE_RECEIPTS
            )
            .unwrap();
        } else {
            assert_eq!(client.request(&request), format!("committed {}", index + 3));
        }
    }
    assert_eq!((lost_replies, restarts), (8, 8));
    writeln!(
        std::io::stderr(),
        "receipt capacity: checking full-service replay"
    )
    .unwrap();
    full_service_replays_without_new_revision(&client);
    server.stop();
    drop(server);
    let before = verify_all_capacity_receipts(&file);
    writeln!(
        std::io::stderr(),
        "receipt capacity: verifying backup and restore"
    )
    .unwrap();
    let backup = dir.0.join("capacity.backup.db");
    let restored = dir.0.join("capacity.restored.db");
    storage::backup_file(&file, &backup, Limits::default()).unwrap();
    verify_all_capacity_receipts(&backup);
    storage::restore_file(&backup, &restored, Limits::default()).unwrap();
    verify_all_capacity_receipts(&restored);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        writeln!(std::io::stderr(), "receipt capacity: verifying compaction").unwrap();
        let stats = weekend_db::Database::compact_file(&file, Limits::default()).unwrap();
        assert_eq!(stats.before_bytes, before);
        assert!(stats.after_bytes < stats.before_bytes);
        verify_all_capacity_receipts(&file);
    }
    // Both the compacted original and the restored copy remain full, preserve
    // every original acknowledgment, and perform zero new publications.
    for file in [&file, &restored] {
        writeln!(
            std::io::stderr(),
            "receipt capacity: checking maintained replay"
        )
        .unwrap();
        let image = fs::read(file).unwrap();
        let (mut server, client) =
            configured_service("serve", file.to_str().unwrap(), secret, &token);
        full_service_replays_without_new_revision(&client);
        server.stop();
        drop(server);
        verify_all_capacity_receipts(file);
        assert_eq!(fs::read(file).unwrap(), image);
    }
    writeln!(std::io::stderr(), "receipt capacity: all phases verified").unwrap();
    println!(
        "native receipt capacity: receipts=1024 revision=1026 lost_replies=8 post_commit_restarts=8 full_replay_passes=3 before_bytes={before} maintained_bytes={}",
        fs::metadata(&file).unwrap().len()
    );
}

#[test]
fn configured_secret_authenticates_before_writes_and_receipt_lookup() {
    use std::{io::Read, net::TcpStream};
    let dir = Directory::new();
    let file = dir.0.join("authenticated.db");
    let secret = dir.0.join("secret");
    let wrong = dir.0.join("wrong-secret");
    for (path, byte) in [(&secret, b'a'), (&wrong, b'b')] {
        fs::write(path, [byte; 64]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    let path = file.to_str().unwrap();
    assert!(
        Command::new(BIN)
            .args(["init", path])
            .status()
            .unwrap()
            .success()
    );
    let secret = secret.to_str().unwrap();
    let mut server = Process::start(&["serve", path, "0", "--secret-file", secret]);
    let ready = server.receive();
    let port = ready.strip_prefix("ready 127.0.0.1:").unwrap();
    // Oversized and incomplete unauthenticated requests are disconnected
    // without holding the serialized service indefinitely.
    for bytes in [vec![b'x'; 256], vec![b'a']] {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(&bytes).unwrap();
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => {}
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
            result => panic!("server must close bounded invalid request: {result:?}"),
        }
    }
    let mut anonymous = Process::start(&["client", port]);
    let mut invalid = Process::start(&["client", port, "--secret-file", wrong.to_str().unwrap()]);
    let mut valid = Process::start(&["client", port, "--secret-file", secret]);
    anonymous.send("set-id id 2 50");
    invalid.send("set-id id 2 60");
    assert_eq!(anonymous.receive(), "error Unauthorized");
    assert_eq!(invalid.receive(), "error Unauthorized");
    valid.send("get");
    assert_eq!(valid.receive(), "value 2 0");
    valid.send("set-id id 2 70");
    assert_eq!(valid.receive(), "committed 3");
    anonymous.send("set-id id 2 70");
    assert_eq!(anonymous.receive(), "error Unauthorized");
    valid.send("set-id id 2 70");
    assert_eq!(valid.receive(), "committed 3");
    server.stop();
    let restarted = Process::start(&["serve", path, "0", "--secret-file", secret]);
    let ready = restarted.receive();
    let port = ready.strip_prefix("ready 127.0.0.1:").unwrap();
    let mut valid = Process::start(&["client", port, "--secret-file", secret]);
    valid.send("set-id id 2 70");
    assert_eq!(valid.receive(), "committed 3");
    valid.send("get");
    assert_eq!(valid.receive(), "value 3 70");
}

#[test]
fn command_line_dispatch_and_clean_client_exit() {
    for args in [vec!["--help"], vec!["-h"]] {
        assert!(
            Command::new(BIN)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    for args in [
        vec![],
        vec!["wrong"],
        vec!["wrong", "0"],
        vec!["wrong", "path", "0"],
        vec!["wrong", "p", "0", "--secret-file", "s"],
        vec!["serve", "p", "0", "--bad", "s"],
        vec!["wrong", "0", "--secret-file", "s"],
        vec!["client", "0", "--bad", "s"],
    ] {
        assert!(
            !Command::new(BIN)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let mut client = Command::new(BIN)
        .args(["client", "0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    client.stdin.take().unwrap().write_all(b"\nquit\n").unwrap();
    assert!(client.wait().unwrap().success());
}

#[cfg(unix)]
#[test]
fn secret_fifo_is_rejected_before_opening_database_or_waiting_for_input() {
    let dir = Directory::new();
    let secret = dir.0.join("secret-fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&secret)
            .status()
            .unwrap()
            .success()
    );
    let file = dir.0.join("untouched.db");
    fs::write(&file, b"database must remain untouched").unwrap();
    for args in [
        vec![
            "serve",
            file.to_str().unwrap(),
            "0",
            "--secret-file",
            secret.to_str().unwrap(),
        ],
        vec!["client", "0", "--secret-file", secret.to_str().unwrap()],
    ] {
        // Process::drop kills and reaps the child even if the deadline assertion
        // fails. No FIFO writer is opened: a blocking File::open would hang.
        let mut process = Process::start(&args);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = process.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "secret FIFO blocked {args:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(!status.success());
        assert_eq!(fs::read(&file).unwrap(), b"database must remain untouched");
    }
}
