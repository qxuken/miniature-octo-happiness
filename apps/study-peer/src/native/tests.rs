use super::*;
use std::{
    fs,
    io::{BufReader, Read},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use weekend_db::crdt::Policy;

const WORKER: &str = "STUDY_PEER_TEST_WORKER";
const COLLECTION: [u8; 16] = [9; 16];

pub(super) struct Directory(pub(super) PathBuf);
impl Directory {
    pub(super) fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "study-peer-{}-{nonce}-{sequence}",
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

fn roster_policy() -> Policy {
    Enrollment::new("a", COLLECTION).unwrap().policy
}

fn hex_identity(bytes: [u8; 16]) -> String {
    format!("{:032x}", u128::from_be_bytes(bytes))
}

fn write_roster_policy(file: &Path, policy: Policy) {
    fs::write(
        file,
        format!(
            "study-policy-v1 {} {} {} {}\n",
            hex_identity(policy.collection),
            policy.generation,
            hex_identity(policy.actors[0]),
            hex_identity(policy.actors[1]),
        ),
    )
    .unwrap();
}

#[test]
fn roster_policy_requires_exact_syntax_and_preserves_input_errors() {
    let directory = Directory::new();
    let path = directory.0.join("policy");
    let error = || roster::policy_file(&path).expect_err("invalid policy");
    assert_eq!(
        error().downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::NotFound
    );
    fs::write(&path, [b'a'; 257]).unwrap();
    assert_eq!(error().to_string(), "credential size outside bounds");
    fs::write(&path, [0xff]).unwrap();
    assert!(error().is::<std::str::Utf8Error>());

    let policy = roster_policy();
    write_roster_policy(&path, policy);
    let valid = fs::read_to_string(&path).unwrap();
    let words: Vec<_> = valid.split_ascii_whitespace().collect();
    assert_eq!(roster::policy_file(&path).unwrap(), policy);
    for invalid in [
        valid.replace("study-policy-v1", "study-policy-v2"),
        words[..4].join(" "),
        format!("{valid} unexpected"),
    ] {
        fs::write(&path, invalid).unwrap();
        assert_eq!(
            error().downcast_ref::<Error>(),
            Some(&Error::UnsupportedFormat)
        );
    }
    for field in [1, 3, 4] {
        for invalid in ["1".repeat(31), "g".repeat(32)] {
            let mut malformed = words.clone();
            malformed[field] = &invalid;
            fs::write(&path, malformed.join(" ")).unwrap();
            assert_eq!(
                error().to_string(),
                "identity must be 32 hexadecimal digits"
            );
        }
    }
    let mut malformed = words;
    malformed[2] = "18446744073709551616";
    fs::write(&path, malformed.join(" ")).unwrap();
    assert!(error().is::<std::num::ParseIntError>());
}

#[test]
fn roster_policy_identity_generation_and_actor_checks_are_independent() {
    let directory = Directory::new();
    let path = directory.0.join("policy");
    let valid = roster_policy();
    write_roster_policy(&path, valid);
    assert_eq!(roster::policy_file(&path).unwrap(), valid);
    for invalid in [
        Policy {
            collection: [0; 16],
            ..valid
        },
        Policy {
            generation: 0,
            ..valid
        },
        Policy {
            actors: [[0; 16], valid.actors[1]],
            ..valid
        },
        Policy {
            actors: [valid.actors[1]; 2],
            ..valid
        },
    ] {
        write_roster_policy(&path, invalid);
        assert_eq!(
            roster::policy_file(&path)
                .err()
                .unwrap()
                .downcast_ref::<Error>(),
            Some(&Error::Invalid)
        );
    }
    for (name, actor) in ["a", "b"].into_iter().zip(valid.actors) {
        let enrollment = roster::enrollment(name, valid).unwrap();
        assert_eq!(enrollment.actor, actor);
        assert_eq!(enrollment.policy, valid);
    }
    assert_eq!(
        roster::enrollment("", valid).err().unwrap().to_string(),
        "actor must be a or b"
    );
}

#[test]
fn roster_stage_requires_fresh_collection_generation_and_actors() {
    let directory = Directory::new();
    let old_files = ["old-a", "old-b"].map(|name| directory.0.join(name));
    let new_files = ["new-a", "new-b"].map(|name| directory.0.join(name));
    let old = roster_policy();
    let new = Policy {
        collection: [10; 16],
        generation: old.generation + 1,
        actors: [[3; 16], [4; 16]],
    };
    for (name, path) in ["a", "b"].into_iter().zip(&old_files) {
        let db = Database::create_file(path, Limits::default()).unwrap();
        enroll(&db, roster::enrollment(name, old).unwrap()).unwrap();
        db.retain_snapshot(&db.snapshot()).unwrap();
    }
    let original = old_files.each_ref().map(|file| fs::read(file).unwrap());
    let stage = |policy| {
        roster::stage(
            old_files.each_ref().map(|file| file.as_path()),
            old,
            new_files.each_ref().map(|file| file.as_path()),
            policy,
            None,
            &mut io::sink(),
        )
    };
    for invalid in [
        Policy {
            collection: old.collection,
            ..new
        },
        Policy {
            generation: old.generation,
            ..new
        },
        Policy {
            actors: [old.actors[0], new.actors[1]],
            ..new
        },
    ] {
        assert_eq!(
            stage(invalid).unwrap_err().downcast_ref::<Error>(),
            Some(&Error::Invalid)
        );
        assert!(new_files.iter().all(|path| !path.exists()));
        assert_eq!(
            old_files.each_ref().map(|file| fs::read(file).unwrap()),
            original
        );
    }
    stage(new).unwrap();
    for (path, actor) in new_files.iter().zip(new.actors) {
        let db = Database::open_file(path, Limits::default()).unwrap();
        let replica =
            StudyReplica::open(&db, db.collection_id(COLLECTION_ROW), new, actor).unwrap();
        assert_eq!(replica.read().unwrap(), study::StudyProgress::default());
    }
    assert_eq!(
        old_files.each_ref().map(|file| fs::read(file).unwrap()),
        original
    );
}

#[test]
fn cli_command_observation_preserves_storage_context_errors_and_retry_budget() {
    let directory = Directory::new();
    let settings = diagnostics::settings_from_args(
        diagnostics::App::Study,
        &mut vec!["--diagnostics=json".into(), "run".into()],
    );
    let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
        observe_local_command(|| {
            Ok(
                Database::open_file(directory.0.join("missing"), Limits::default())?
                    .snapshot()
                    .revision(),
            )
        })
    });
    assert_eq!(
        result.as_ref().err().unwrap().downcast_ref::<Error>(),
        Some(&Error::NotFound)
    );
    let json = diagnostic.unwrap().json(settings).to_string();
    assert!(
        json.contains("\"phase\":\"local_command\",\"category\":\"engine\",\"code\":\"NotFound\"")
    );
    assert!(json.contains("\"native\":{\"operation\":"));

    let database = Database::new().unwrap();
    let replica = enroll(&database, Enrollment::new("a", COLLECTION).unwrap()).unwrap();
    let before = database.snapshot().revision();
    let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
        observe_local_command(|| command(&["practice", "PRIVATE_CANARY"], &database, &replica))
    });
    assert!(
        result
            .as_ref()
            .err()
            .unwrap()
            .is::<std::num::ParseIntError>()
    );
    let json = diagnostic.unwrap().json(settings).to_string();
    assert!(
        json.contains(
            "\"phase\":\"local_command\",\"category\":\"input\",\"code\":\"invalid_input\""
        )
    );
    assert!(json.contains("\"native\":null,\"verification\":null"));
    assert!(!json.contains("PRIVATE_CANARY"));
    assert_eq!(database.snapshot().revision(), before);

    let calls = std::cell::Cell::new(0);
    let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
        observe_local_command(|| {
            Ok(protocol::edit(&replica, |_, _| {
                calls.set(calls.get() + 1);
                Err(Error::Conflict)
            })?)
        })
    });
    assert_eq!(calls.get(), 3);
    assert_eq!(
        result.as_ref().err().unwrap().downcast_ref::<Error>(),
        Some(&Error::Conflict)
    );
    assert!(
        diagnostic
            .unwrap()
            .json(settings)
            .to_string()
            .contains("\"code\":\"Conflict\",\"native\":null,\"verification\":null")
    );
    assert_eq!(database.snapshot().revision(), before);
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
        let options = RunOptions {
            pushing: false,
            sync_only: false,
        };
        run(&db, enrollment, network, options, |stream| PauseReply {
            stream,
            enabled,
        })
        .unwrap();
        return;
    }

    let dir = Directory::new();
    for actor in ["a", "b"] {
        let (path, identity, _, enrollment) = config(&dir.0, actor);
        Credentials::generate().unwrap().write_fixture(&identity);
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

#[test]
fn paused_peer_reports_idle_errors_and_quits_cleanly() {
    let dir = Directory::new();
    let (path, identity, _, enrollment) = config(&dir.0, "a");
    Credentials::generate().unwrap().write_fixture(&identity);
    let (_, other, _, _) = config(&dir.0, "b");
    Credentials::generate().unwrap().write_fixture(&other);
    initialize(&path, enrollment).unwrap();
    let dynamic = "127.0.0.1:0".parse().unwrap();
    let mut worker = Worker::start(&dir.0, "a", dynamic, dynamic, false);
    worker.ready();
    assert_eq!(worker.command("peer"), "peer 0 idle");
    assert!(worker.command("unknown").starts_with("error "));
    assert_eq!(worker.command("push off"), "push off");
    worker.input.write_all(b"\nquit\n").unwrap();
    worker.input.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(status) = worker.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() > deadline {
            worker.kill();
            panic!("paused peer failed to stop");
        }
        thread::sleep(Duration::from_millis(10));
    }
}
