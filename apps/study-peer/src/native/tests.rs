use super::*;
use std::{
    io::{BufReader, Read},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const WORKER: &str = "STUDY_PEER_TEST_WORKER";
const COLLECTION: [u8; 16] = [9; 16];

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("study-peer-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Worker {
    child: Child,
    input: ChildStdin,
    lines: Receiver<String>,
}

impl Worker {
    fn start(
        dir: &Path,
        actor: &str,
        listen: SocketAddr,
        peer: SocketAddr,
        pause_reply: bool,
    ) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native::tests::two_writers_recover_a_lost_ack",
                "--nocapture",
                "--quiet",
            ])
            .env(WORKER, actor)
            .env("STUDY_PEER_TEST_DIR", dir)
            .env("STUDY_PEER_TEST_LISTEN", listen.to_string())
            .env("STUDY_PEER_TEST_PEER", peer.to_string())
            .env(
                "STUDY_PEER_TEST_PAUSE",
                if pause_reply { "yes" } else { "no" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        thread::spawn(move || {
            let mut ready = false;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let line = if ready {
                    line
                } else if let Some((_, address)) = line.split_once("ready ") {
                    ready = true;
                    format!("ready {address}")
                } else {
                    continue;
                };
                if !line.is_empty() && send.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            lines,
        }
    }

    fn line(&self) -> String {
        self.lines
            .recv_timeout(Duration::from_secs(15))
            .expect("worker stopped producing output")
    }

    fn ready(&self) -> SocketAddr {
        self.line()
            .strip_prefix("ready ")
            .expect("worker did not start")
            .parse()
            .unwrap()
    }

    fn command(&mut self, command: &str) -> String {
        writeln!(self.input, "{command}").unwrap();
        self.line()
    }

    fn wait_for(&mut self, command: &str, predicate: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let reply = self.command(command);
            if predicate(&reply) {
                return reply;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {command}: {reply}"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn kill(&mut self) {
        self.child.kill().unwrap();
        assert!(!self.child.wait().unwrap().success());
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct PauseReply<S> {
    stream: S,
    enabled: bool,
}
impl<S: Read> Read for PauseReply<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}
impl<S: Write> Write for PauseReply<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.enabled {
            println!("reply-pending");
            // The parent kills us here, after publication and before the reply.
            thread::sleep(Duration::from_secs(30));
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.stream.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn config(dir: &Path, actor: &str) -> (PathBuf, PathBuf, PathBuf, Enrollment) {
    let peer = if actor == "a" { "b" } else { "a" };
    (
        dir.join(format!("{actor}.db")),
        dir.join(actor),
        dir.join(peer).join("cert.der"),
        Enrollment::new(actor, COLLECTION).unwrap(),
    )
}

fn ack_count(line: &str, total: u64) -> u64 {
    let mut parts = line.split_whitespace();
    assert_eq!(parts.next(), Some("peer"));
    let count = parts.next().unwrap().parse().unwrap();
    if parts.next() == Some("ack") && parts.next().and_then(|n| n.parse().ok()) == Some(total) {
        count
    } else {
        0
    }
}

#[test]
fn two_writers_recover_a_lost_ack() {
    if let Ok(actor) = std::env::var(WORKER) {
        let dir = PathBuf::from(std::env::var_os("STUDY_PEER_TEST_DIR").unwrap());
        let (path, identity, peer_cert, enrollment) = config(&dir, &actor);
        let db = Database::open_file(path, Limits::default()).unwrap();
        let network = Network {
            listener: TcpListener::bind(std::env::var("STUDY_PEER_TEST_LISTEN").unwrap()).unwrap(),
            peer: std::env::var("STUDY_PEER_TEST_PEER")
                .unwrap()
                .parse()
                .unwrap(),
            tls: Tls::new(
                &Credentials::load(&identity).unwrap(),
                fs::read(peer_cert).unwrap().into(),
            )
            .unwrap(),
        };
        let enabled = std::env::var("STUDY_PEER_TEST_PAUSE").unwrap() == "yes";
        run(&db, enrollment, network, false, |stream| PauseReply {
            stream,
            enabled,
        })
        .unwrap();
        return;
    }

    let dir = Directory::new();
    for actor in ["a", "b"] {
        let (path, identity, _, enrollment) = config(&dir.0, actor);
        Credentials::create(&identity).unwrap();
        initialize(&path, enrollment).unwrap();
    }
    let dynamic = "127.0.0.1:0".parse().unwrap();
    let reserved = TcpListener::bind(dynamic).unwrap();
    let b_address = reserved.local_addr().unwrap();
    let mut a = Worker::start(&dir.0, "a", dynamic, b_address, false);
    let a_address = a.ready();
    drop(reserved);
    let mut b = Worker::start(&dir.0, "b", b_address, a_address, true);
    assert_eq!(b.ready(), b_address);

    assert_eq!(a.command("edit 1 20 60 20"), "local 3");
    assert_eq!(b.command("edit 2 15 90 30"), "local 3");
    assert_eq!(a.command("show"), "state 3 20 [60] [20] [1]");
    assert_eq!(b.command("show"), "state 3 15 [90] [30] [2]");
    assert_eq!(a.command("push on"), "push on");
    assert_eq!(b.line(), "reply-pending");
    assert_eq!(b.command("show"), "state 4 35 [90] [20] [1, 2]");
    assert_eq!(b.command("practice 5"), "local 5");
    b.kill();

    a.wait_for("peer", |line| line.starts_with("peer 0 error "));
    let mut b = Worker::start(&dir.0, "b", b_address, a_address, false);
    assert_eq!(b.ready(), b_address);
    a.wait_for("peer", |line| ack_count(line, 40) > 0);
    assert_eq!(a.command("show"), "state 3 20 [60] [20] [1]");
    assert_eq!(b.command("show"), "state 5 40 [90] [20] [1, 2]");

    assert_eq!(b.command("push on"), "push on");
    let first_b = b.wait_for("peer", |line| ack_count(line, 40) > 0);
    assert_eq!(a.command("show"), "state 4 40 [90] [20] [1, 2]");
    let first_a = a.command("peer");
    a.wait_for("peer", |line| ack_count(line, 40) > ack_count(&first_a, 40));
    b.wait_for("peer", |line| ack_count(line, 40) > ack_count(&first_b, 40));
    assert_eq!(a.command("show"), "state 4 40 [90] [20] [1, 2]");
    assert_eq!(b.command("show"), "state 5 40 [90] [20] [1, 2]");
}
