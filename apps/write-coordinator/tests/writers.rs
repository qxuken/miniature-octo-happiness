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
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
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

fn configured_service(path: &str, secret: &str) -> (Process, Process, String) {
    let server = Process::start(&["serve", path, "0", "--secret-file", secret]);
    let ready = server.receive();
    let port = ready.strip_prefix("ready 127.0.0.1:").unwrap().to_owned();
    let client = Process::start(&["client", &port, "--secret-file", secret]);
    (server, client, port)
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

fn full_service_replays_without_new_revision(client: &mut Process) {
    let state = format!(
        "value {} {}",
        DURABLE_RECEIPTS + 2,
        capacity_value(DURABLE_RECEIPTS - 1)
    );
    client.send("get");
    assert_eq!(client.receive(), state);
    client.send(&format!(
        "set-id capacity_overflow {} 999999",
        DURABLE_RECEIPTS + 2
    ));
    assert_eq!(client.receive(), "error Limit");
    client.send("set-id capacity_0000 2 10001");
    assert_eq!(client.receive(), "error Invalid");
    client.send("set-id capacity_0000 3 10000");
    assert_eq!(client.receive(), "error Invalid");
    for index in 0..DURABLE_RECEIPTS {
        client.send(&capacity_request(index));
        assert_eq!(client.receive(), format!("committed {}", index + 3));
    }
    client.send("get");
    assert_eq!(client.receive(), state);
}

#[test]
fn native_receipt_capacity_survives_restarts_backup_restore_and_compaction() {
    use std::net::{Shutdown, TcpStream};
    use weekend_db::{Limits, storage};

    let dir = Directory::new();
    let file = dir.0.join("capacity.db");
    let secret_file = dir.0.join("secret");
    let token = "c".repeat(64);
    fs::write(&secret_file, &token).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&secret_file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let path = file.to_str().unwrap();
    let secret = secret_file.to_str().unwrap();
    assert!(
        Command::new(BIN)
            .args(["init", path])
            .status()
            .unwrap()
            .success()
    );
    let (mut server, mut client, mut port) = configured_service(path, secret);
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
            let mut lost = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            lost.set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            writeln!(lost, "auth {token} {request}").unwrap();
            lost.shutdown(Shutdown::Write).unwrap();
            drop(lost);
            lost_replies += 1;
            client.send("get");
            assert_eq!(
                client.receive(),
                format!("value {} {}", index + 3, capacity_value(index))
            );
            drop(client);
            server.stop();
            (server, client, port) = configured_service(path, secret);
            restarts += 1;
            client.send(&request);
            assert_eq!(client.receive(), format!("committed {}", index + 3));
            // The very first receipt remains the original acknowledgment even
            // after later writes and every post-commit restart.
            client.send(&capacity_request(0));
            assert_eq!(client.receive(), "committed 3");
            writeln!(
                std::io::stderr(),
                "receipt capacity: committed {}/{}; restarted and replayed",
                index + 1,
                DURABLE_RECEIPTS
            )
            .unwrap();
        } else {
            client.send(&request);
            assert_eq!(client.receive(), format!("committed {}", index + 3));
        }
    }
    assert_eq!((lost_replies, restarts), (8, 8));
    writeln!(
        std::io::stderr(),
        "receipt capacity: checking full-service replay"
    )
    .unwrap();
    full_service_replays_without_new_revision(&mut client);
    drop(client);
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
        let (mut server, mut client, _) = configured_service(file.to_str().unwrap(), secret);
        full_service_replays_without_new_revision(&mut client);
        drop(client);
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
