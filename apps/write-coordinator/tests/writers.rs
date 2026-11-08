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
    sync::mpsc::{self, Receiver},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const BIN: &str = env!("CARGO_BIN_EXE_write-coordinator");

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("write-coordinator-{}-{nonce}", std::process::id()));
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
