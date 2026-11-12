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
    // Offline maintenance: all process/file leases are closed before raw reads.
    let follower_path = Path::new(follower);
    let manifest = follower_path.join("replica-manifest.wdb");
    assert!(status.contains("selected_slot=Some(0)"));
    let selected = follower_path.join("replica-0.wdb");
    let inactive = follower_path.join("replica-1.wdb");
    let selected_bytes = fs::read(&selected).unwrap();
    fs::write(&inactive, b"abandoned incomplete image").unwrap();
    fs::write(follower_path.join("operator-note"), b"keep").unwrap();
    let before = fs::read(&manifest).unwrap();
    for command in ["cleanup-inactive", "compact-manifest"] {
        let rejected = Command::new(BIN)
            .args([
                command,
                follower,
                "22222222222222222222222222222222",
                identity,
            ])
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        let expected = if cfg!(windows) && command == "compact-manifest" {
            "compact-manifest unsupported on Windows; reseed a fresh follower directory"
        } else {
            "ForeignStore"
        };
        assert_eq!(
            String::from_utf8(rejected.stderr).unwrap().trim_end(),
            expected
        );
        assert_eq!(fs::read(&manifest).unwrap(), before);
        assert_eq!(fs::read(&inactive).unwrap(), b"abandoned incomplete image");
        assert_eq!(fs::read(&selected).unwrap(), selected_bytes);
    }
    let cleaned = run(&["cleanup-inactive", follower, stream, identity]);
    assert!(cleaned.contains("maintenance removed_inactive=1"));
    assert!(cleaned.lines().any(|line| line == ack));
    assert!(!inactive.exists());
    assert!(run(&["cleanup-inactive", follower, stream, identity]).contains("removed_inactive=0"));
    assert_eq!(
        fs::read(follower_path.join("operator-note")).unwrap(),
        b"keep"
    );
    assert_eq!(fs::read(&selected).unwrap(), selected_bytes);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use weekend_db::replication::MANIFEST_LIMIT_BYTES;
        fs::OpenOptions::new()
            .write(true)
            .open(&manifest)
            .unwrap()
            .set_len(MANIFEST_LIMIT_BYTES)
            .unwrap();
        assert!(run(&["status", follower, stream, identity]).contains("maintenance_required=true"));
        let compacted = run(&["compact-manifest", follower, stream, identity]);
        assert!(compacted.contains("maintenance_required=false"));
        assert!(compacted.lines().any(|line| line == ack));
        assert!(fs::metadata(&manifest).unwrap().len() < MANIFEST_LIMIT_BYTES);
    }
    #[cfg(windows)]
    {
        let before = fs::read(&manifest).unwrap();
        let rejected = Command::new(BIN)
            .args(["compact-manifest", follower, stream, identity])
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("unsupported on Windows"));
        assert_eq!(fs::read(&manifest).unwrap(), before);
    }
    assert_eq!(fs::read(&selected).unwrap(), selected_bytes);
    assert!(
        run(&["status", follower, stream, identity])
            .lines()
            .any(|line| line == ack)
    );
    let parse_id = |s: &str| u128::from_str_radix(s, 16).unwrap().to_be_bytes();
    let f = weekend_db::replication::Follower::open(
        follower,
        weekend_db::replication::Enrollment {
            stream_id: parse_id(stream),
            file_identity: parse_id(identity),
        },
        Limits::default(),
    )
    .unwrap();
    let replica = f.replica().unwrap();
    let c = replica.collection_id(1);
    assert_eq!(replica.snapshot().revision(), 4);
    assert_eq!(replica.retained_ids().unwrap(), [1]);
    assert_eq!(replica.retained_snapshot(1).unwrap().revision(), 2);
    assert_eq!(
        replica.snapshot().get(c, b"key").unwrap().unwrap().as_u64(),
        Ok(2)
    );
    assert_eq!(
        replica
            .retained_snapshot(1)
            .unwrap()
            .get(c, b"key")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(1)
    );
}
