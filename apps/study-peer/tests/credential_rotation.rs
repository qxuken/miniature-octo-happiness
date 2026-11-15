#![cfg(any(target_os = "macos", target_os = "linux", windows))]

#[allow(dead_code)]
#[path = "../../../examples/support/study.rs"]
mod study;

use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use study::StudyProgress;
use weekend_db::{
    Database, Limits,
    crdt::{Policy, Replica},
};

const ID: &str = "09090909090909090909090909090909";
const BIN: &str = env!("CARGO_BIN_EXE_study-peer");
const BUDGET: Duration = Duration::from_secs(20);
type Log = Arc<Mutex<File>>;

fn event(log: &Log, text: impl std::fmt::Display) {
    let mut file = log.lock().unwrap();
    writeln!(file, "{text}").unwrap();
    file.flush().unwrap();
}

fn private_dir(path: &Path) {
    #[cfg(unix)]
    fs::DirBuilder::new().mode(0o700).create(path).unwrap();
    #[cfg(windows)]
    fs::create_dir(path).unwrap();
}

struct Private(PathBuf);
impl Private {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("study-cutover-{}-{nonce}", std::process::id()));
        private_dir(&path);
        Self(path)
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wait(child: &mut Child, timeout: Duration) -> ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child {} exceeded bounded wait", child.id());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

struct OneShot(Child);
impl Drop for OneShot {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn once(program: &str, args: &[&std::ffi::OsStr], logs: &Path, label: &str) -> String {
    let stdout = logs.join(format!("{label}.stdout"));
    let stderr = logs.join(format!("{label}.stderr"));
    let mut child = OneShot(
        Command::new(program)
            .args(args)
            .stdout(File::create(&stdout).unwrap())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    assert!(
        wait(&mut child.0, Duration::from_secs(10)).success(),
        "{label}: {}",
        fs::read_to_string(stderr).unwrap()
    );
    fs::read_to_string(stdout).unwrap()
}

// Each relay owns its allocated port for the entire scenario. CLI listeners use
// port zero; no released port is assumed to remain available across restarts.
struct Relay {
    address: SocketAddr,
    target: Arc<Mutex<Option<SocketAddr>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Relay {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let target = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (destination, stopping) = (Arc::clone(&target), Arc::clone(&stop));
        let handle = thread::spawn(move || {
            let mut connections = Vec::new();
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut source, _)) => {
                        let Some(address) = *destination.lock().unwrap() else {
                            continue;
                        };
                        connections.push(thread::spawn(move || {
                            let Ok(mut peer) =
                                TcpStream::connect_timeout(&address, Duration::from_millis(500))
                            else {
                                return;
                            };
                            for stream in [&source, &peer] {
                                stream.set_nonblocking(false).unwrap();
                                stream
                                    .set_read_timeout(Some(Duration::from_secs(4)))
                                    .unwrap();
                                stream
                                    .set_write_timeout(Some(Duration::from_secs(4)))
                                    .unwrap();
                            }
                            let (mut source_read, mut peer_write) =
                                (source.try_clone().unwrap(), peer.try_clone().unwrap());
                            let forward = thread::spawn(move || {
                                let _ = std::io::copy(&mut source_read, &mut peer_write);
                                let _ = peer_write.shutdown(Shutdown::Write);
                            });
                            let _ = std::io::copy(&mut peer, &mut source);
                            let _ = source.shutdown(Shutdown::Write);
                            forward.join().unwrap();
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("relay accept: {error}"),
                }
            }
            for connection in connections {
                connection.join().unwrap();
            }
        });
        Self {
            address,
            target,
            stop,
            thread: Some(handle),
        }
    }
    fn point_to(&self, address: Option<SocketAddr>) {
        *self.target.lock().unwrap() = address;
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.thread.take().unwrap().join();
    }
}

struct Peer {
    child: Child,
    input: ChildStdin,
    lines: Receiver<String>,
    name: String,
    log: Log,
}
impl Peer {
    fn start(
        root: &Path,
        logs: &Path,
        actor: &str,
        phase: &str,
        outgoing: &Relay,
        incoming: &Relay,
        log: &Log,
    ) -> Self {
        let name = format!("{phase}-{actor}");
        let mut command = Command::new(BIN);
        command
            .arg("run")
            .arg(root.join(format!("{actor}.db")))
            .args([actor, ID, "127.0.0.1:0", &outgoing.address.to_string()])
            .arg(root.join(format!("{actor}-active")))
            .arg(root.join(format!("{actor}-pin.der")))
            .arg("--no-push");
        Self::spawn(command, name, logs, incoming, log)
    }

    fn spawn(mut command: Command, name: String, logs: &Path, incoming: &Relay, log: &Log) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(File::create(logs.join(format!("{name}.stderr"))).unwrap())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        let (copy_log, copy_name) = (Arc::clone(log), name.clone());
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                event(&copy_log, format!("{copy_name} stdout {line}"));
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let peer = Self {
            child,
            input,
            lines,
            name,
            log: Arc::clone(log),
        };
        let ready = peer.line();
        let address = ready
            .strip_prefix("ready ")
            .expect("CLI readiness")
            .parse()
            .unwrap();
        incoming.point_to(Some(address));
        peer
    }
    fn line(&self) -> String {
        self.lines
            .recv_timeout(Duration::from_secs(8))
            .expect("CLI output deadline")
    }
    fn command(&mut self, command: &str) -> String {
        event(&self.log, format!("{} command {command}", self.name));
        writeln!(self.input, "{command}").unwrap();
        self.input.flush().unwrap();
        self.line()
    }
    fn state(&mut self) -> State {
        State::parse(&self.command("show"))
    }
    fn report(&mut self) -> (u64, String) {
        let line = self.command("peer");
        let count = line.split_whitespace().nth(1).unwrap().parse().unwrap();
        (count, line)
    }
    fn local(&mut self, command: &str) -> u64 {
        self.command(command)
            .strip_prefix("local ")
            .expect("local commit report, not a peer acknowledgment")
            .parse()
            .unwrap()
    }
    fn stop(&mut self, killed: bool) {
        if killed {
            self.child.kill().unwrap();
        } else {
            self.command_quit();
        }
        assert_eq!(
            wait(&mut self.child, Duration::from_secs(8)).success(),
            !killed
        );
    }
    fn command_quit(&mut self) {
        writeln!(self.input, "quit").unwrap();
        self.input.flush().unwrap();
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    revision: u64,
    minutes: u64,
    goal: Vec<u64>,
    limit: Vec<u64>,
    completed: Vec<u64>,
}
impl State {
    fn parse(line: &str) -> Self {
        let (head, rest) = line.split_once('[').unwrap();
        let numbers: Vec<_> = head.split_whitespace().collect();
        assert_eq!(numbers[0], "state");
        let arrays: Vec<Vec<u64>> = rest
            .split('[')
            .map(|s| {
                s.split(']')
                    .next()
                    .unwrap()
                    .split(',')
                    .filter_map(|n| {
                        let n = n.trim();
                        if n.is_empty() {
                            None
                        } else {
                            Some(n.parse().unwrap())
                        }
                    })
                    .collect()
            })
            .collect();
        assert_eq!(arrays.len(), 3);
        Self {
            revision: numbers[1].parse().unwrap(),
            minutes: numbers[2].parse().unwrap(),
            goal: arrays[0].clone(),
            limit: arrays[1].clone(),
            completed: arrays[2].clone(),
        }
    }
    fn assert_values(&self, minutes: u64, goal: u64, limit: u64, completed: &[u64]) {
        assert_eq!(
            (
                self.minutes,
                self.goal.as_slice(),
                self.limit.as_slice(),
                self.completed.as_slice()
            ),
            (minutes, &[goal][..], &[limit][..], completed)
        );
    }
}

fn converge(
    a: &mut Peer,
    b: &mut Peer,
    minutes: u64,
    goal: u64,
    limit: u64,
    completed: &[u64],
) -> [u64; 2] {
    assert_eq!(a.command("push on"), "push on");
    assert_eq!(b.command("push on"), "push on");
    let deadline = Instant::now() + BUDGET;
    let mut stable: Option<([u64; 2], [u64; 2])> = None;
    loop {
        let states = [a.state(), b.state()];
        let reports = [a.report(), b.report()];
        if states.iter().all(|s| {
            s.minutes == minutes
                && s.goal == [goal]
                && s.limit == [limit]
                && s.completed == completed
        }) && reports
            .iter()
            .all(|(_, line)| line.ends_with(&format!(" ack {minutes}")))
        {
            let revisions = [states[0].revision, states[1].revision];
            if let Some((old, counts)) = stable
                && old == revisions
                && reports[0].0 >= counts[0] + 2
                && reports[1].0 >= counts[1] + 2
            {
                return revisions;
            }
            if stable.is_none_or(|(old, _)| old != revisions) {
                stable = Some((revisions, [reports[0].0, reports[1].0]));
            }
        }
        assert!(
            Instant::now() < deadline,
            "convergence deadline: {states:?} {reports:?}"
        );
        thread::sleep(Duration::from_millis(30));
    }
}

fn rejected(a: &mut Peer, b: &mut Peer) {
    let before = [a.state(), b.state()];
    assert_eq!(a.command("push on"), "push on");
    assert_eq!(b.command("push on"), "push on");
    let deadline = Instant::now() + BUDGET;
    loop {
        let reports = [a.report(), b.report()];
        for (count, _) in &reports {
            assert_eq!(*count, 0, "mismatch must never acknowledge");
        }
        if reports.iter().all(|(_, line)| {
            let text = line.to_lowercase();
            text.contains(" error ")
                && (text.contains("certificate") || text.contains("alert"))
                && !text.contains("refused")
                && !text.contains("timed out")
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "expected reachable TLS rejection: {reports:?}"
        );
        thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(
        [a.state(), b.state()],
        before,
        "TLS rejection must publish no remote commit"
    );
}

fn policy() -> Policy {
    Policy {
        collection: [9; 16],
        generation: 7,
        actors: [[1; 16], [2; 16]],
    }
}
fn core(root: &Path, actor: &str) -> (StudyProgress, u64, Option<(StudyProgress, u64, u64)>) {
    let db = Database::open_file(root.join(format!("{actor}.db")), Limits::default()).unwrap();
    let replica = Replica::<StudyProgress>::open(
        &db,
        db.collection_id(1),
        policy(),
        if actor == "a" { [1; 16] } else { [2; 16] },
    )
    .unwrap();
    let current = db.snapshot();
    let state = replica.read_at(&current).unwrap();
    let ids = db.retained_ids().unwrap();
    assert!(ids.len() <= 1);
    let retained = ids.first().map(|id| {
        let old = db.retained_snapshot(*id).unwrap();
        (replica.read_at(&old).unwrap(), old.revision(), *id)
    });
    (state, current.revision(), retained)
}

fn activate(root: &Path, actor: &str, generation: &str, log: &Log) {
    let active = root.join(format!("{actor}-active"));
    if active.exists() {
        fs::remove_dir_all(&active).unwrap();
    }
    private_dir(&active);
    for name in ["cert.der", "key.der"] {
        fs::copy(
            root.join(format!("{actor}-{generation}")).join(name),
            active.join(name),
        )
        .unwrap();
    }
    #[cfg(unix)]
    fs::set_permissions(active.join("key.der"), fs::Permissions::from_mode(0o600)).unwrap();
    let other = if actor == "a" { "b" } else { "a" };
    let temporary = root.join(format!("{actor}-pin.next"));
    fs::copy(
        root.join(format!("{other}-{generation}/cert.der")),
        &temporary,
    )
    .unwrap();
    fs::rename(temporary, root.join(format!("{actor}-pin.der"))).unwrap();
    event(
        log,
        format!(
            "operator configured {actor} credential={generation} peer-pin={other}-{generation}; endpoints stopped"
        ),
    );
}

fn provision(root: &Path, actor: &str, label: &str, logs: &Path) -> Vec<u8> {
    let credentials = root.join(format!("{actor}-{label}"));
    #[cfg(unix)]
    once(
        BIN,
        &["keygen".as_ref(), credentials.as_os_str()],
        logs,
        &format!("keygen-{actor}-{label}"),
    );
    #[cfg(windows)]
    {
        let _ = logs;
        // Use the Windows operator provisioning path for synthetic keys.
        private_dir(&credentials);
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(["study-peer".into()]).unwrap();
        fs::write(credentials.join("cert.der"), cert.der()).unwrap();
        fs::write(credentials.join("key.der"), signing_key.serialize_der()).unwrap();
    }
    fs::read(credentials.join("cert.der")).unwrap()
}

#[test]
fn native_roster_cutover_stages_once_and_rejects_authenticated_old_peers() {
    let private = Private::new();
    let root = &private.0;
    let logs =
        std::env::var_os("STUDY_ROSTER_EVIDENCE").map_or_else(|| root.join("logs"), PathBuf::from);
    private_dir(&logs);
    let log = Arc::new(Mutex::new(File::create(logs.join("events.log")).unwrap()));
    let old = [root.join("a.db"), root.join("b.db")];
    let new = [root.join("fresh-a.db"), root.join("fresh-b.db")];
    let old_policy = root.join("old.policy");
    let new_policy = root.join("new.policy");
    let policy_text = |collection: &str, generation: u64, a: u8, b: u8| {
        format!(
            "study-policy-v1 {collection} {generation} {} {}\n",
            format!("{a:02x}").repeat(16),
            format!("{b:02x}").repeat(16)
        )
    };
    fs::write(&old_policy, policy_text(ID, 7, 1, 2)).unwrap();
    let next_id = "0a".repeat(16);
    fs::write(&new_policy, policy_text(&next_id, 8, 3, 4)).unwrap();
    for (slot, actor) in ["a", "b"].into_iter().enumerate() {
        once(
            BIN,
            &[
                "init".as_ref(),
                old[slot].as_os_str(),
                actor.as_ref(),
                ID.as_ref(),
            ],
            &logs,
            &format!("initialize-{actor}"),
        );
        assert_ne!(
            provision(root, actor, "old", &logs),
            provision(root, actor, "new", &logs)
        );
        let db = Database::open_file(&old[slot], Limits::default()).unwrap();
        db.retain_snapshot(&db.snapshot()).unwrap();
        if slot == 0 {
            Replica::<StudyProgress>::open(&db, db.collection_id(1), policy(), [1; 16])
                .unwrap()
                .update(|state, actor| {
                    state.planned.insert(4, actor)?;
                    state.planned.insert(6, actor)?;
                    state.planned.remove(6)?;
                    Ok(())
                })
                .unwrap();
        }
    }
    for actor in ["a", "b"] {
        activate(root, actor, "old", &log);
    }

    let stage = |configuration: &Path, targets: [&Path; 2]| {
        Command::new(BIN)
            .arg("stage-roster")
            .args(&old)
            .arg(&old_policy)
            .args(targets)
            .arg(configuration)
            .output()
            .unwrap()
    };
    let reject = |output: std::process::Output, expected: &str| {
        assert!(!output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8(output.stderr).unwrap().trim(), expected);
    };
    let targets = [&new[0] as &Path, &new[1] as &Path];
    reject(stage(&new_policy, targets), "Conflict");
    assert!(new.iter().all(|p| !p.exists()));
    let invalid = root.join("invalid.policy");
    for configuration in [
        policy_text(ID, 8, 3, 4),
        policy_text(&next_id, 7, 3, 4),
        policy_text(&next_id, 8, 1, 4),
        policy_text(&next_id, 8, 4, 3),
    ] {
        fs::write(&invalid, configuration).unwrap();
        reject(stage(&invalid, targets), "Invalid");
        assert!(new.iter().all(|p| !p.exists()));
    }

    let (ra, rb) = (Relay::new(), Relay::new());
    let run_old = |actor: &str, name| {
        let (outgoing, incoming) = if actor == "a" { (&rb, &ra) } else { (&ra, &rb) };
        Peer::start(root, &logs, actor, name, outgoing, incoming, &log)
    };
    let run_new = |slot: usize, name: &str, sync_only: bool| {
        let (actor, outgoing, incoming) = if slot == 0 {
            ("a", &rb, &ra)
        } else {
            ("b", &ra, &rb)
        };
        let mut cli = Command::new(BIN);
        cli.arg("run-policy")
            .arg(&new[slot])
            .arg(actor)
            .arg(&new_policy)
            .args(["127.0.0.1:0", &outgoing.address.to_string()])
            .arg(root.join(format!("{actor}-active")))
            .arg(root.join(format!("{actor}-pin.der")))
            .arg("--no-push");
        if sync_only {
            cli.arg("--sync-only");
        }
        Peer::spawn(cli, name.into(), &logs, incoming, &log)
    };
    let (mut a, mut b) = (run_old("a", "old"), run_old("b", "old"));
    a.local("edit 1 5 30 60");
    b.local("edit 2 7 40 50");
    converge(&mut a, &mut b, 12, 40, 50, &[1, 2]);
    reject(stage(&new_policy, targets), "Busy");
    assert!(new.iter().all(|p| !p.exists()));
    a.stop(false);
    b.stop(false);

    let old_bytes = old.each_ref().map(|path| fs::read(path).unwrap());
    let old_ids = ["a", "b"].map(|actor| core(root, actor));
    assert_eq!(old_ids[0].0, old_ids[1].0);
    assert_eq!(old_ids[0].0.planned.iter().collect::<Vec<_>>(), [4]);
    assert!(
        old_ids
            .iter()
            .all(|(_, _, prior)| prior.unwrap().0 == StudyProgress::default())
    );

    let partial = root.join("partial-a");
    let occupied = root.join("occupied");
    fs::write(&occupied, b"preserve").unwrap();
    reject(stage(&new_policy, [&partial, &occupied]), "AlreadyExists");
    assert_eq!(fs::read(&occupied).unwrap(), b"preserve");
    let partial_db = Database::open_file(partial, Limits::default()).unwrap();
    assert_eq!(partial_db.snapshot().revision(), 0);
    drop(partial_db);

    let staged = stage(&new_policy, targets);
    assert!(staged.status.success(), "{staged:?}");
    let visible = "visible minutes=12 completed=[1, 2] planned=[4] goal=Some(40) limit=Some(50)";
    let staged = String::from_utf8(staged.stdout).unwrap();
    event(&log, staged.trim());
    assert!(staged.lines().any(|line| line == visible));
    let verify = || {
        Command::new(BIN)
            .arg("verify-roster")
            .args(&new)
            .arg(&new_policy)
            .output()
            .unwrap()
    };
    reject(verify(), "Conflict");

    // Reuse the old TLS pins for this controlled attempt so the core sees
    // the authenticated frame with the wrong collection and actor roster.
    let (mut stale, mut staged_b) = (run_old("a", "stale"), run_new(1, "staged-b", true));
    let before = [stale.state(), staged_b.state()];
    assert_eq!(
        staged_b.command("practice 100"),
        "error local edits are disabled during staging"
    );
    assert_eq!(stale.command("push on"), "push on");
    assert_eq!(staged_b.command("push on"), "push on");
    let deadline = Instant::now() + BUDGET;
    loop {
        assert_eq!(stale.report().0, 0);
        assert_eq!(staged_b.report().0, 0);
        if ["stale-a", "staged-b"].iter().all(|name| {
            fs::read_to_string(logs.join(format!("{name}.stderr")))
                .unwrap()
                .contains("incoming sync: ForeignStore")
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no authenticated old-roster rejection"
        );
        thread::sleep(Duration::from_millis(30));
    }
    assert_eq!([stale.state(), staged_b.state()], before);
    stale.stop(false);
    staged_b.stop(false);
    for actor in ["a", "b"] {
        activate(root, actor, "new", &log);
    }

    let (mut a, mut b) = (
        run_new(0, "synchronize-a", true),
        run_new(1, "synchronize-b", true),
    );
    assert_eq!(
        a.command("edit 8 10 40 30"),
        "error local edits are disabled during staging"
    );
    let staged_revisions = converge(&mut a, &mut b, 12, 40, 50, &[1, 2]);
    a.stop(true);
    b.stop(false);
    let verified = verify();
    assert!(verified.status.success(), "{verified:?}");
    let verified = String::from_utf8(verified.stdout).unwrap();
    event(&log, verified.trim());
    assert_eq!(
        verified,
        format!(
            "verified generation=8 revisions={} {}\n{visible}\n",
            staged_revisions[0], staged_revisions[1]
        )
    );

    let (mut a, mut b) = (run_new(0, "live-a", false), run_new(1, "live-b", false));
    assert_eq!([a.state().revision, b.state().revision], staged_revisions);
    a.local("edit 3 3 50 45");
    b.local("edit 5 4 45 40");
    converge(&mut a, &mut b, 19, 50, 40, &[1, 2, 3, 5]);
    a.stop(false);
    b.stop(false);
    let final_report = verify();
    assert!(final_report.status.success(), "{final_report:?}");
    assert!(String::from_utf8(final_report.stdout).unwrap().lines().any(|line|
        line == "visible minutes=19 completed=[1, 2, 3, 5] planned=[4] goal=Some(50) limit=Some(40)"
    ));
    assert_eq!(
        old.each_ref().map(|path| fs::read(path).unwrap()),
        old_bytes
    );
}

#[test]
fn persistent_study_progress_survives_interrupted_credential_cutover() {
    let private = Private::new();
    let root = &private.0;
    let logs = if let Some(path) = std::env::var_os("STUDY_CUTOVER_EVIDENCE") {
        let path = PathBuf::from(path);
        private_dir(&path);
        path
    } else {
        let path = root.join("logs");
        fs::create_dir(&path).unwrap();
        path
    };
    let log = Arc::new(Mutex::new(File::create(logs.join("events.log")).unwrap()));
    let mut fingerprints = BTreeSet::new();
    for actor in ["a", "b"] {
        for generation in ["old", "new"] {
            let certificate = provision(root, actor, generation, &logs);
            let fingerprint = Sha256::digest(certificate);
            assert!(fingerprints.insert(fingerprint));
            event(
                &log,
                format!("public certificate fingerprint {actor}-{generation}: {fingerprint:x}"),
            );
        }
    }
    for actor in ["a", "b"] {
        once(
            BIN,
            &[
                "init".as_ref(),
                root.join(format!("{actor}.db")).as_os_str(),
                actor.as_ref(),
                ID.as_ref(),
            ],
            &logs,
            &format!("init-{actor}"),
        );
        activate(root, actor, "old", &log);
    }
    let (ra, rb) = (Relay::new(), Relay::new());
    let start = |actor, phase| {
        Peer::start(
            root,
            &logs,
            actor,
            phase,
            if actor == "a" { &rb } else { &ra },
            if actor == "a" { &ra } else { &rb },
            &log,
        )
    };
    let (mut a, mut b) = (start("a", "old"), start("b", "old"));
    a.local("edit 1 5 30 60");
    b.local("edit 2 7 40 50");
    converge(&mut a, &mut b, 12, 40, 50, &[1, 2]);
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    let mut historical = Vec::new();
    for actor in ["a", "b"] {
        let db = Database::open_file(root.join(format!("{actor}.db")), Limits::default()).unwrap();
        let snapshot = db.snapshot();
        let revision = snapshot.revision();
        let id = db.retain_snapshot(&snapshot).unwrap();
        historical.push((revision, id));
    }
    let old = [core(root, "a").0, core(root, "b").0];
    assert_eq!(old[0], old[1]);
    activate(root, "a", "new", &log);
    let (mut a, mut b) = (start("a", "mismatch"), start("b", "mismatch"));
    let ar = a.local("edit 3 11 80 45");
    let br = b.local("edit 4 13 70 20");
    a.state().assert_values(23, 80, 45, &[1, 2, 3]);
    b.state().assert_values(25, 70, 20, &[1, 2, 4]);
    rejected(&mut a, &mut b);
    assert_eq!(a.state().revision, ar);
    assert_eq!(b.state().revision, br);
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    activate(root, "b", "new", &log);
    let mut interrupted = start("a", "interrupted-after-replacement");
    assert_eq!(interrupted.report().1, "peer 0 idle");
    let interrupted_revision = interrupted.local("practice 17");
    interrupted.state().assert_values(40, 80, 45, &[1, 2, 3]);
    interrupted.stop(true);
    ra.point_to(None);
    event(
        &log,
        "operator interruption: killed A after replacement and local commit, before any successful new exchange",
    );
    let (mut a, mut b) = (start("a", "new"), start("b", "new"));
    assert_eq!(a.state().revision, interrupted_revision);
    a.state().assert_values(40, 80, 45, &[1, 2, 3]);
    b.state().assert_values(25, 70, 20, &[1, 2, 4]);
    let final_revisions = converge(&mut a, &mut b, 53, 80, 20, &[1, 2, 3, 4]);
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    activate(root, "a", "old", &log);
    let (mut a, mut b) = (start("a", "stale-old"), start("b", "stale-old"));
    assert_eq!([a.state().revision, b.state().revision], final_revisions);
    rejected(&mut a, &mut b);
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    activate(root, "a", "new", &log);
    let (mut a, mut b) = (start("a", "fresh-repeat"), start("b", "fresh-repeat"));
    assert_eq!(
        converge(&mut a, &mut b, 53, 80, 20, &[1, 2, 3, 4]),
        final_revisions,
        "matching repeated sync must add no commit"
    );
    a.stop(false);
    b.stop(false);
    ra.point_to(None);
    rb.point_to(None);
    let final_states = [core(root, "a"), core(root, "b")];
    assert_eq!(
        final_states[0].0, final_states[1].0,
        "entire public record, including causal state, converges"
    );
    for (index, (state, revision, retained)) in final_states.iter().enumerate() {
        assert_eq!(state.practice_minutes.total(), 53);
        assert_eq!(state.planned, Default::default());
        assert_eq!(state.completed.iter().collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(state.weekly_goal.values().collect::<Vec<_>>(), [80]);
        assert_eq!(state.session_limit.values().collect::<Vec<_>>(), [20]);
        let (historical_state, historical_revision, id) = retained.as_ref().unwrap();
        assert_eq!(*historical_state, old[index]);
        assert_eq!((*historical_revision, *id), historical[index]);
        assert_eq!(*revision, final_revisions[index]);
    }
    event(
        &log,
        format!(
            "PASS full-record convergence total=53 goal=80 limit=20 completed=[1,2,3,4], planned empty; retained old roots valid; revisions={final_revisions:?}; stale credentials rejected; repeat sync no commit"
        ),
    );
}
