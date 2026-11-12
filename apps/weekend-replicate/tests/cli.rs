#![cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use weekend_db::{Database, Limits};
const BIN: &str = env!("CARGO_BIN_EXE_weekend-replicate");
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(args: &[&str]) -> String {
    let output = Command::new(BIN).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn credentials(path: &Path) {
    fs::create_dir(path).unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(["weekend-replicate".into()]).unwrap();
    fs::write(path.join("cert.der"), cert.der()).unwrap();
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path.join("key.der"))
        .unwrap()
        .write_all(&signing_key.serialize_der())
        .unwrap();
}
struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Server {
    fn start(root: &Path, args: &[&str]) -> (Self, String) {
        let log = root.join("server.log");
        let output = fs::File::create(&log).unwrap();
        let mut server = Self(
            Command::new(BIN)
                .args(args)
                .stdout(output)
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let lines = fs::read_to_string(&log).unwrap();
            if let Some(address) = lines.lines().find_map(|line| line.strip_prefix("ready ")) {
                return (server, address.into());
            }
            assert!(
                server.0.try_wait().unwrap().is_none(),
                "server exited before readiness"
            );
            assert!(Instant::now() < deadline, "server readiness timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn finished(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "server lifetime exceeded");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
#[test]
fn native_cli_enrollment_ship_restart_poll_and_bounded_server_lifetime() {
    let path = std::env::temp_dir().join(format!("weekend-replicate-cli-{}", std::process::id()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    }
    #[cfg(not(unix))]
    fs::create_dir(&path).unwrap();
    let dir = Directory(path);
    let primary = dir.0.join("primary");
    let source = primary.to_str().unwrap();
    let db = Database::create_file(&primary, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"key", &db.unsigned(1).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    tx.put(c, b"key", &db.unsigned(2).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    drop(db);
    let stream = "11111111111111111111111111111111";
    let description = run(&["describe", source, stream]);
    assert!(description.contains("verified_revision=4"));
    let identity = description
        .split_whitespace()
        .find_map(|s| s.strip_prefix("file_identity="))
        .unwrap();
    assert_eq!(identity.len(), 32);
    let follower = dir.0.join("follower");
    fs::create_dir(&follower).unwrap();
    let follower = follower.to_str().unwrap();
    assert_eq!(
        run(&["follower-init", follower, stream, identity]),
        "follower empty\n"
    );
    let a = dir.0.join("a");
    let b = dir.0.join("b");
    credentials(&a);
    credentials(&b);
    let a_pin = a.join("cert.der");
    let b_pin = b.join("cert.der");
    let server_args = [
        "serve",
        follower,
        stream,
        identity,
        "127.0.0.1:0",
        b.to_str().unwrap(),
        a_pin.to_str().unwrap(),
        "--max-exchanges",
        "1",
    ];
    let (mut server, address) = Server::start(&dir.0, &server_args);
    let export = dir.0.join("export");
    let sent = run(&[
        "ship",
        source,
        stream,
        export.to_str().unwrap(),
        &address,
        a.to_str().unwrap(),
        b_pin.to_str().unwrap(),
        "--attempts",
        "1",
    ]);
    server.finished();
    assert!(sent.contains("local captured_revision=4"));
    let ack = sent
        .lines()
        .find(|l| l.starts_with("follower durable_revision=4 "))
        .unwrap()
        .to_owned();
    assert!(ack.contains("digest="));
    let status = run(&["status", follower, stream, identity]);
    assert!(status.lines().any(|l| l == ack));
    let (mut server, address) = Server::start(&dir.0, &server_args);
    let polled = run(&[
        "poll",
        source,
        stream,
        &address,
        a.to_str().unwrap(),
        b_pin.to_str().unwrap(),
    ]);
    server.finished();
    assert!(polled.contains("local verified_revision=4"));
    assert!(polled.lines().any(|l| l == ack));
    let forbidden_export = dir.0.join("not-created");
    let rejected = Command::new(BIN)
        .args([
            "ship",
            source,
            stream,
            forbidden_export.to_str().unwrap(),
            &address,
            a.to_str().unwrap(),
            b_pin.to_str().unwrap(),
            "--attempts",
            "4",
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("option outside bounds"));
    assert!(!forbidden_export.exists());
}
