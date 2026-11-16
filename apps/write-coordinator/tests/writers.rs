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
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "linux")]
#[path = "writers/journal_space.rs"]
mod journal_space;
#[cfg(target_os = "linux")]
#[path = "../../support/test_tmpfs.rs"]
mod test_tmpfs;

const BIN: &str = env!("CARGO_BIN_EXE_write-coordinator");
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
// Keep the capacity workload out of the journal CLI's socket deadline.
static STORAGE_WORKLOAD: Mutex<()> = Mutex::new(());

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
        let mut command = Command::new(BIN);
        command.args(args);
        Self::start_command(command)
    }

    fn start_command(mut command: Command) -> Self {
        let mut child = command
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

    fn wait_for_success(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(exit) = self.child.try_wait().unwrap() {
                assert!(exit.success(), "operator stop exited with {exit}");
                return;
            }
            assert!(Instant::now() < deadline, "operator stop did not finish");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_transition(mut observe: impl FnMut() -> String, previous: &str, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let observed = observe();
        if observed == expected {
            return;
        }
        assert_eq!(observed, previous);
        assert!(
            Instant::now() < deadline,
            "request did not publish {expected}"
        );
        std::thread::sleep(Duration::from_millis(10));
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
    wait_for_transition(
        || {
            observer.send("get");
            observer.receive()
        },
        "value 2 0",
        "value 3 10",
    );
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
fn incomplete_request_leaves_room_for_another_writer() {
    let dir = Directory::new();
    let path = dir.0.join("coordinator.wdb");
    let path = path.to_str().unwrap();
    let secret = dir.0.join("secret");
    let token = "a1".repeat(32);
    write_secret(&secret, &token);
    assert!(
        Command::new(BIN)
            .args(["init-sequenced", path])
            .status()
            .unwrap()
            .success()
    );
    let (_server, client) =
        configured_service("serve-sequenced", path, secret.to_str().unwrap(), &token);

    let mut stalled = std::net::TcpStream::connect(client.address).unwrap();
    write!(stalled, "auth {token} set-seq 0 1 3 9").unwrap();
    let writer = client.send("set-seq 1 1 3 42");
    writer
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reply = String::new();
    BufReader::new(writer).read_line(&mut reply).unwrap();
    assert_eq!(reply, "committed 4\n");
    drop(stalled);
    assert_eq!(client.request("get"), "value 4 42");
    assert_eq!(client.request("receipt 0"), "receipt 0");
}

#[test]
fn excess_connections_receive_busy_without_publishing_requests() {
    let dir = Directory::new();
    let path = dir.0.join("coordinator.wdb");
    let path = path.to_str().unwrap();
    let secret = dir.0.join("secret");
    let secret_path = secret.to_str().unwrap();
    let token = "b2".repeat(32);
    write_secret(&secret, &token);
    assert!(
        Command::new(BIN)
            .args(["init-sequenced", path])
            .status()
            .unwrap()
            .success()
    );
    let (mut server, client) = configured_service("serve-sequenced", path, secret_path, &token);

    let held: Vec<_> = (0..8)
        .map(|slot| {
            let mut socket = std::net::TcpStream::connect(client.address).unwrap();
            write!(socket, "auth {token} set-seq {slot} 1 3 9").unwrap();
            socket
        })
        .collect();
    let excess = std::net::TcpStream::connect(client.address).unwrap();
    excess
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reply = String::new();
    BufReader::new(excess).read_line(&mut reply).unwrap();
    assert_eq!(reply, "error Busy\n");
    drop(held);
    server.stop();

    let (_server, client) = configured_service("serve-sequenced", path, secret_path, &token);
    assert_eq!(client.request("get"), "value 3 0");
    for slot in 0..8 {
        assert_eq!(client.request(&format!("receipt {slot}")), "receipt 0");
    }
}

#[test]
fn operator_shutdown_releases_the_lease_and_preserves_receipts() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::{
            System::Console::GetConsoleCP, System::Threading::CREATE_NEW_CONSOLE,
        };
        // SAFETY: queries this process's console and accepts no pointers.
        if unsafe { GetConsoleCP() } == 0 {
            // A test started by a detached CI runner needs a console shared
            // with its server child to send a targeted console event.
            let marker = "WEEKEND_TEST_OPERATOR_CONSOLE";
            assert!(
                std::env::var_os(marker).is_none(),
                "test child did not acquire a console"
            );
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "operator_shutdown_releases_the_lease_and_preserves_receipts",
                    "--exact",
                ])
                .env(marker, "1")
                .creation_flags(CREATE_NEW_CONSOLE);
            Process::start_command(command).wait_for_success();
            return;
        }
    }

    #[cfg(unix)]
    for signal in ["-TERM", "-INT"] {
        exercise_operator_stop(signal);
    }
    #[cfg(windows)]
    exercise_operator_stop();
}

fn exercise_operator_stop(#[cfg(unix)] signal: &str) {
    let dir = Directory::new();
    let path = dir.0.join("coordinator.wdb");
    let path = path.to_str().unwrap();
    let secret = dir.0.join("secret");
    let secret_path = secret.to_str().unwrap();
    let token = "c3".repeat(32);
    write_secret(&secret, &token);
    assert!(
        Command::new(BIN)
            .args(["init-sequenced", path])
            .status()
            .unwrap()
            .success()
    );
    let mut command = Command::new(BIN);
    command.args(["serve-sequenced", path, "0", "--secret-file", secret_path]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    let mut server = Process::start_command(command);
    let ready = server.receive();
    let client = AuthenticatedClient {
        address: ready.strip_prefix("ready ").unwrap().parse().unwrap(),
        token: token.clone(),
    };
    assert_eq!(client.request("set-seq 0 1 3 42"), "committed 4");
    let mut stalled = std::net::TcpStream::connect(client.address).unwrap();
    write!(stalled, "auth {token} set-seq 1 1 4 99").unwrap();
    assert_eq!(client.request("get"), "value 4 42");

    assert!(server.child.try_wait().unwrap().is_none());
    #[cfg(unix)]
    assert!(
        Command::new("/bin/kill")
            .args([signal, &server.child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent};
        // SAFETY: the owned, still-unreaped server was created as a process
        // group in this console. CTRL_BREAK targets that group only.
        assert_ne!(
            unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, server.child.id()) },
            0
        );
    }
    let closed_by = Instant::now() + Duration::from_secs(2);
    while std::net::TcpStream::connect_timeout(&client.address, Duration::from_millis(100)).is_ok()
    {
        assert!(
            Instant::now() < closed_by,
            "listener still accepts connections during operator stop"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    server.wait_for_success();
    drop(stalled);

    let (_server, client) = configured_service("serve-sequenced", path, secret_path, &token);
    assert_eq!(client.request("get"), "value 4 42");
    assert_eq!(client.request("set-seq 0 1 3 42"), "committed 4");
    assert_eq!(client.request("receipt 1"), "receipt 0");
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
    wait_for_transition(
        || client.request("receipt 0"),
        "receipt 0",
        "receipt 1 3 10 4",
    );
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

fn accept_journal_request(
    listener: &std::net::TcpListener,
    client: &AuthenticatedClient,
    verb: &str,
) -> (std::net::TcpStream, String) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "journal did not send {verb}");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accepting {verb}: {error}"),
        }
    };
    socket.set_nonblocking(false).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut line = String::new();
    BufReader::new((&mut socket).take(257))
        .read_line(&mut line)
        .unwrap();
    let prefix = format!("auth {} ", client.token);
    let command = line
        .strip_prefix(&prefix)
        .and_then(|line| line.strip_suffix('\n'))
        .expect("journal sent a complete authenticated request");
    assert_eq!(command.split(' ').next(), Some(verb));
    (socket, command.to_owned())
}

#[test]
fn journal_restarts_after_lost_reply_and_rejects_incompatible_history() {
    use std::net::TcpListener;
    use weekend_db::{Limits, storage};

    let _exclusive = STORAGE_WORKLOAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let dir = Directory::new();
    let file = dir.0.join("primary.db");
    let foreign = dir.0.join("foreign.db");
    let divergent = dir.0.join("divergent.db");
    let backup = dir.0.join("backup.db");
    let restored = dir.0.join("restored.db");
    let journal = dir.0.join("writer.db");
    let key = dir.0.join("secret");
    let token = "4".repeat(64);
    write_secret(&key, &token);
    let key = key.to_str().unwrap();
    let journal = journal.to_str().unwrap();
    for path in [&file, &foreign] {
        assert!(
            Command::new(BIN)
                .arg("init-sequenced")
                .arg(path)
                .status()
                .unwrap()
                .success()
        );
    }
    storage::backup_file(&file, &backup, Limits::default()).unwrap();
    for destination in [&restored, &divergent] {
        storage::restore_file(&backup, destination, Limits::default()).unwrap();
    }

    let run = |action, tail: &[&str]| {
        Command::new(BIN)
            .args(["journal", action, journal])
            .args(tail)
            .output()
            .unwrap()
    };
    let checked = |action, tail: &[&str]| {
        let output = run(action, tail);
        assert!(output.status.success(), "journal {action}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    let check = |port, expected: Result<&str, &str>| {
        let before = fs::read(journal).unwrap();
        let output = run("check", &[port, "--secret-file", key]);
        match expected {
            Ok(line) => {
                assert!(output.status.success(), "{output:?}");
                assert_eq!(output.stdout, format!("{line}\n").as_bytes());
            }
            Err(message) => {
                assert!(!output.status.success(), "{output:?}");
                assert!(output.stdout.is_empty());
                assert!(String::from_utf8(output.stderr).unwrap().contains(message));
            }
        }
        assert_eq!(fs::read(journal).unwrap(), before);
    };
    let (mut primary, client) =
        configured_service("serve-sequenced", file.to_str().unwrap(), key, &token);
    let port = client.address.port().to_string();
    let identity = client.request("identity");
    let identity = identity.strip_prefix("identity ").unwrap();
    assert_eq!(
        checked("init", &[&port, "0", "--secret-file", key]),
        format!("journal {identity} 0\nreceipt 0\n")
    );
    check(&port, Ok("aligned 3 0"));

    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let proxy_port = proxy.local_addr().unwrap().port().to_string();
    let mut interrupted = Process::start(&[
        "journal",
        "set",
        journal,
        &proxy_port,
        "3",
        "42",
        "--secret-file",
        key,
    ]);
    let (mut preflight, command) = accept_journal_request(&proxy, &client, "journal-get");
    let reply = client.request(&command);
    assert_eq!(reply, "value 3 0");
    writeln!(preflight, "{reply}").unwrap();
    drop(preflight);
    let (withheld, command) = accept_journal_request(&proxy, &client, "journal-set");
    let reply = client.request(&command);
    assert_eq!(reply, "committed 4");
    interrupted.stop();
    drop(withheld);
    let pending = format!("journal {identity} 0\nreceipt 0\npending 1 3 42\n");
    assert_eq!(checked("status", &[]), pending);
    check(&port, Ok("pending-committed 4 42"));
    assert_eq!(client.request("receipt 0"), "receipt 1 3 42 4");
    primary.stop();

    let (mut stale, stale_client) =
        configured_service("serve-sequenced", restored.to_str().unwrap(), key, &token);
    let stale_port = stale_client.address.port().to_string();
    check(&stale_port, Ok("pending-eligible 3 0"));
    assert_eq!(stale_client.request("receipt 0"), "receipt 0");

    let (mut other, other_client) =
        configured_service("serve-sequenced", foreign.to_str().unwrap(), key, &token);
    let other_port = other_client.address.port().to_string();
    check(&other_port, Err("ForeignStore"));
    assert!(
        !run("resume", &[&other_port, "--secret-file", key])
            .status
            .success()
    );
    assert_eq!(other_client.request("get"), "value 3 0");
    assert_eq!(checked("status", &[]), pending);
    other.stop();

    let (_primary, client) =
        configured_service("serve-sequenced", file.to_str().unwrap(), key, &token);
    let port = client.address.port().to_string();
    check(&port, Ok("pending-committed 4 42"));
    for _ in 0..2 {
        assert_eq!(
            checked("resume", &[&port, "--secret-file", key]),
            "committed 4\n"
        );
    }
    assert_eq!(client.request("get"), "value 4 42");
    assert_eq!(
        checked("status", &[]),
        format!("journal {identity} 0\nreceipt 1 3 42 4\n")
    );
    check(&port, Ok("aligned 4 42"));
    check(&stale_port, Err("behind 3 0"));
    assert_eq!(stale_client.request("get"), "value 3 0");
    stale.stop();

    let (_other, other_client) =
        configured_service("serve-sequenced", divergent.to_str().unwrap(), key, &token);
    let other_port = other_client.address.port().to_string();
    assert_eq!(
        other_client.request("identity"),
        format!("identity {identity}")
    );
    assert_eq!(other_client.request("set-seq 0 1 3 777"), "committed 4");
    check(&other_port, Err("receipt-mismatch 4 777"));
    assert!(
        !run("set", &[&other_port, "4", "99", "--secret-file", key])
            .status
            .success()
    );
    assert_eq!(
        checked("status", &[]),
        format!("journal {identity} 0\nreceipt 1 3 42 4\npending 2 4 99\n")
    );
    check(&other_port, Err("receipt-mismatch 4 777"));
    check(&port, Ok("pending-eligible 4 42"));
    assert_eq!(other_client.request("get"), "value 4 777");
    assert_eq!(client.request("get"), "value 4 42");
    assert_eq!(
        checked("resume", &[&port, "--secret-file", key]),
        "committed 5\n"
    );
    assert_eq!(client.request("receipt 0"), "receipt 2 4 99 5");
    assert_eq!(client.request("get"), "value 5 99");
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

#[test]
#[ignore = "extended native coordinator and journal maintenance workload"]
fn native_journal_workload_resumes_after_verified_restore() {
    use std::path::Path;
    use weekend_db::{Database, Limits, storage};

    fn journal(action: &str, path: &Path, args: &[&str]) -> String {
        let result = Command::new(BIN)
            .args(["journal", action])
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "journal {action}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    }

    let dir = Directory::new();
    let key = dir.0.join("secret");
    let token = "a".repeat(64);
    write_secret(&key, &token);
    let key = key.to_str().unwrap();
    let mut files = ["coordinator.db", "writer-0.db", "writer-1.db"].map(|name| dir.0.join(name));
    assert!(
        Command::new(BIN)
            .arg("init-sequenced")
            .arg(&files[0])
            .status()
            .unwrap()
            .success()
    );
    let (mut server, mut client) =
        configured_service("serve-sequenced", files[0].to_str().unwrap(), key, &token);
    let identity = client.request("identity");
    let identity = identity.strip_prefix("identity ").unwrap();
    let mut port = client.address.port().to_string();
    for slot in 0..2 {
        assert_eq!(
            journal(
                "init",
                &files[slot + 1],
                &[&port, &slot.to_string(), "--secret-file", key],
            ),
            format!("journal {identity} {slot}\nreceipt 0\n"),
        );
    }

    const BATCH: u64 = 128;
    const CYCLES: u64 = 9;
    let mut latency = Vec::with_capacity((BATCH * CYCLES) as usize);
    let mut writes = 0;
    for cycle in 1..=CYCLES {
        for n in writes + 1..=writes + BATCH {
            let slot = (n - 1) as usize % 2;
            let expected = (n + 2).to_string();
            let value = n.to_string();
            let start = Instant::now();
            let reply = journal(
                "set",
                &files[slot + 1],
                &[&port, &expected, &value, "--secret-file", key],
            );
            latency.push(start.elapsed().as_micros());
            assert_eq!(reply, format!("committed {}\n", n + 3));
        }
        writes += BATCH;
        let revision = writes + 3;
        assert_eq!(client.request("get"), format!("value {revision} {writes}"));
        let cutover = Instant::now();
        server.stop();

        let maintenance = Instant::now();
        let mut sizes = [(0, 0); 3];
        let mut retired = Vec::with_capacity(6);
        for (index, source) in files.iter_mut().enumerate() {
            let backup = dir.0.join(format!("backup-{cycle}-{index}.db"));
            let restored = dir.0.join(format!("restored-{cycle}-{index}.db"));
            let expected_revision = if index == 0 { revision } else { writes + 2 };
            let original = storage::inspect_file(&*source, Limits::default()).unwrap();
            assert_eq!(original.revision, expected_revision);
            let copied = storage::backup_file(&*source, &backup, Limits::default()).unwrap();
            assert_eq!(copied.before_bytes, original.physical_bytes);
            assert!(copied.after_bytes < copied.before_bytes);
            storage::restore_file(&backup, &restored, Limits::default()).unwrap();
            assert_eq!(fs::read(&backup).unwrap(), fs::read(&restored).unwrap());
            let db = Database::open_file(&restored, Limits::default()).unwrap();
            let snapshot = db.snapshot();
            assert_eq!(snapshot.revision(), expected_revision);
            assert!(db.retained_ids().unwrap().is_empty());
            let collection = db.collection_id(if index == 0 { 2 } else { 1 });
            let rows = snapshot.scan(collection).unwrap();
            assert_eq!(rows.len(), if index == 0 { 3 } else { 1 });
            if index == 0 {
                assert_eq!(
                    snapshot
                        .get(collection, b"counter")
                        .unwrap()
                        .unwrap()
                        .as_u64(),
                    Ok(writes),
                );
            }
            drop(rows);
            drop(snapshot);
            drop(db);
            retired.push(std::mem::replace(source, restored));
            retired.push(backup);
            sizes[index] = (copied.before_bytes, copied.after_bytes);
        }
        let copy_us = maintenance.elapsed().as_micros();

        (server, client) =
            configured_service("serve-sequenced", files[0].to_str().unwrap(), key, &token);
        port = client.address.port().to_string();
        assert_eq!(client.request("identity"), format!("identity {identity}"));
        for (slot, last) in [(0, writes - 1), (1, writes)] {
            let acknowledged = last + 3;
            let receipt = format!("receipt {} {} {last} {acknowledged}", writes / 2, last + 2);
            assert_eq!(client.request(&format!("receipt {slot}")), receipt);
            let journal_path = &files[slot + 1];
            assert_eq!(
                journal("status", journal_path, &[]),
                format!("journal {identity} {slot}\n{receipt}\n"),
            );
            assert_eq!(
                journal("check", journal_path, &[&port, "--secret-file", key]),
                format!("aligned {revision} {writes}\n"),
            );
            for _ in 0..2 {
                assert_eq!(
                    journal("resume", journal_path, &[&port, "--secret-file", key]),
                    format!("committed {acknowledged}\n"),
                );
            }
        }
        assert_eq!(client.request("get"), format!("value {revision} {writes}"));
        let cutover_us = cutover.elapsed().as_micros();
        for path in retired {
            fs::remove_file(path).unwrap();
        }
        eprintln!(
            "journal cycle {cycle}: {writes} writes, revision {revision}, copy_verify_us={copy_us}, cutover_us={cutover_us}, bytes={sizes:?}"
        );
    }
    server.stop();
    latency.sort_unstable();
    let percentile = |p: usize| latency[(latency.len() * p).div_ceil(100) - 1];
    eprintln!(
        "journal write_us: count={}, p50={}, p95={}, max={}",
        latency.len(),
        percentile(50),
        percentile(95),
        percentile(100),
    );
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

    let _exclusive = STORAGE_WORKLOAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

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
            wait_for_transition(
                || client.request("get"),
                &format!("value {} {}", index + 2, capacity_value(index - 1)),
                &format!("value {} {}", index + 3, capacity_value(index)),
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
