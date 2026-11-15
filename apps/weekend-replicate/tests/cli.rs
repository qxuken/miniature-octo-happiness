#![cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
mod support;
use support::credentials;
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
fn directory(scenario: &str) -> Directory {
    let path = std::env::temp_dir().join(format!(
        "weekend-replicate-{scenario}-{}",
        std::process::id()
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    }
    #[cfg(not(unix))]
    fs::create_dir(&path).unwrap();
    Directory(path)
}
fn seed_primary(primary: &Path) {
    let db = Database::create_file(primary, Limits::default()).unwrap();
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
}

#[test]
fn native_cli_enrollment_ship_restart_poll_and_bounded_server_lifetime() {
    let dir = directory("cli");
    let primary = dir.0.join("primary");
    seed_primary(&primary);
    let source = primary.to_str().unwrap();
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
    use weekend_db::replication::MANIFEST_LIMIT_BYTES;
    fs::OpenOptions::new()
        .write(true)
        .open(&manifest)
        .unwrap()
        .set_len(MANIFEST_LIMIT_BYTES)
        .unwrap();
    assert!(run(&["status", follower, stream, identity]).contains("maintenance_required=true"));
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
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
        assert!(run(&["status", follower, stream, identity]).contains("maintenance_required=true"));
    }
    assert_eq!(fs::read(&selected).unwrap(), selected_bytes);
    assert!(
        run(&["status", follower, stream, identity])
            .lines()
            .any(|line| line == ack)
    );
    verify_follower(follower_path, stream, identity);
}

fn verify_follower(path: &Path, stream: &str, identity: &str) {
    let parse_id = |s: &str| u128::from_str_radix(s, 16).unwrap().to_be_bytes();
    let follower = weekend_db::replication::Follower::open(
        path,
        weekend_db::replication::Enrollment {
            stream_id: parse_id(stream),
            file_identity: parse_id(identity),
        },
        Limits::default(),
    )
    .unwrap();
    let replica = follower.replica().unwrap();
    assert_eq!(replica.retained_ids().unwrap(), [1]);
    verify_records(
        replica.snapshot(),
        replica.retained_snapshot(1).unwrap(),
        replica.collection_id(1),
    );
}

#[test]
fn native_captured_export_survives_sender_kill_and_primary_advancement() {
    use sha2::{Digest, Sha256};
    use std::net::TcpListener;

    let dir = directory("captured");
    let root = &dir.0;
    let primary = root.join("primary");
    seed_primary(&primary);
    let source = primary.to_str().unwrap();
    let image = root.join("capture.wdb");
    let record = root.join("descriptor.wdb");
    let stream = "aa11aa11aa11aa11aa11aa11aa11aa11";
    let capture = run(&[
        "capture",
        source,
        stream,
        image.to_str().unwrap(),
        record.to_str().unwrap(),
    ]);
    let identity = capture
        .split_whitespace()
        .find_map(|s| s.strip_prefix("file_identity="))
        .unwrap();
    let original = fs::read(&image).unwrap();
    let ticket = fs::read(&record).unwrap();
    let digest = format!("{:x}", Sha256::digest(&original));
    let local = format!("local captured_revision=4 stream={stream} file_identity={identity}");
    assert_eq!(
        capture,
        format!(
            "{local}\nlocal export_length={} export_digest={digest}\n",
            original.len()
        )
    );

    let sender = root.join("sender");
    let receiver = root.join("receiver");
    credentials(&sender);
    credentials(&receiver);
    let sender_cert = sender.join("cert.der");
    let receiver_cert = receiver.join("cert.der");
    let follower = root.join("follower");
    fs::create_dir(&follower).unwrap();
    let follower_path = follower.to_str().unwrap();
    run(&["follower-init", follower_path, stream, identity]);
    let empty_follower = files(&follower);
    let command = |stream: &str, image: &Path, descriptor: &Path, address: &str| {
        let mut cmd = Command::new(BIN);
        cmd.args(["ship-captured", source, stream])
            .args([image, descriptor])
            .arg(address)
            .arg(&sender)
            .arg(&receiver_cert)
            .args(["--attempts", "1"]);
        cmd
    };
    // The sender reaches TCP connect after verifying the pair and waits in
    // TLS while this listener withholds the handshake.
    let stalled = TcpListener::bind("127.0.0.1:0").unwrap();
    stalled.set_nonblocking(true).unwrap();
    let stalled_address = stalled.local_addr().unwrap().to_string();
    let mut interrupted = Server(
        command(stream, &image, &record, &stalled_address)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let socket = loop {
        match stalled.accept() {
            Ok((socket, _)) => break socket,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(interrupted.0.try_wait().unwrap().is_none());
                assert!(
                    Instant::now() < deadline,
                    "sender did not reach TCP connect"
                );
                thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("accepting sender: {e}"),
        }
    };
    interrupted.0.kill().unwrap();
    assert!(!interrupted.0.wait().unwrap().success());
    let mut output = String::new();
    std::io::Read::read_to_string(&mut interrupted.0.stdout.take().unwrap(), &mut output).unwrap();
    assert_eq!(output, format!("{local}\n"));
    drop(socket);
    assert_eq!(files(&follower), empty_follower);

    let db = Database::open_file(&primary, Limits::default()).unwrap();
    let mut tx = db.begin();
    tx.put(db.collection_id(1), b"key", &db.unsigned(3).unwrap())
        .unwrap();
    assert_eq!(tx.commit(), Ok(5));
    drop(tx);
    drop(db);

    let changed = root.join("changed-export");
    let absent_record = root.join("incomplete-descriptor");
    let absent_row = root.join("descriptor-without-row");
    let mut bad_bytes = original.clone();
    *bad_bytes.last_mut().unwrap() ^= 1;
    fs::write(&changed, bad_bytes).unwrap();
    Database::create_file(&absent_record, Limits::default()).unwrap();
    Database::create_file(&absent_row, Limits::default())
        .unwrap()
        .create_collection()
        .unwrap();
    for (s, file, descriptor, error) in [
        (stream, &changed, &record, "Corrupt"),
        (stream, &image, &absent_record, "UnsupportedFormat"),
        (stream, &image, &absent_row, "UnsupportedFormat"),
        (
            "bb11bb11bb11bb11bb11bb11bb11bb11",
            &image,
            &record,
            "ForeignStore",
        ),
    ] {
        let rejected = command(s, file, descriptor, &stalled_address)
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        assert!(rejected.stdout.is_empty());
        assert_eq!(
            String::from_utf8(rejected.stderr).unwrap().trim_end(),
            error
        );
    }
    assert!(matches!(stalled.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    assert_eq!(files(&follower), empty_follower);

    let receipt = format!("follower durable_revision=4 stream={stream} digest={digest}");
    let serve = || {
        Server::start(
            root,
            &[
                "serve",
                follower_path,
                stream,
                identity,
                "127.0.0.1:0",
                receiver.to_str().unwrap(),
                sender_cert.to_str().unwrap(),
                "--max-exchanges",
                "1",
            ],
        )
    };
    let mut installed = None;
    for _ in 0..2 {
        let (mut server, address) = serve();
        let shipped = command(stream, &image, &record, &address).output().unwrap();
        assert!(shipped.status.success(), "{shipped:?}");
        assert_eq!(
            String::from_utf8(shipped.stdout).unwrap(),
            format!("{local}\n{receipt}\n")
        );
        server.finished();
        verify_follower(&follower, stream, identity);
        let now = files(&follower);
        if let Some(before) = &installed {
            assert_eq!(&now, before);
        }
        installed = Some(now);
    }

    let (mut server, address) = serve();
    let newer = run(&[
        "ship",
        source,
        stream,
        root.join("newer-export").to_str().unwrap(),
        &address,
        sender.to_str().unwrap(),
        receiver_cert.to_str().unwrap(),
        "--attempts",
        "1",
    ]);
    server.finished();
    let newer_receipt = newer
        .lines()
        .find(|line| line.starts_with("follower durable_revision=5 "))
        .unwrap();
    let advanced = files(&follower);
    let (mut server, address) = serve();
    let shipped = command(stream, &image, &record, &address).output().unwrap();
    assert!(shipped.status.success(), "{shipped:?}");
    assert_eq!(
        String::from_utf8(shipped.stdout).unwrap(),
        format!("{local}\n{newer_receipt}\n")
    );
    server.finished();
    assert_eq!(files(&follower), advanced);
    assert_eq!(fs::read(&image).unwrap(), original);
    assert_eq!(fs::read(&record).unwrap(), ticket);
}

fn verify_records(
    current: weekend_db::Snapshot<'_>,
    retained: weekend_db::Snapshot<'_>,
    collection: weekend_db::CollectionId,
) {
    for (snapshot, revision, value) in [(current, 4, 2), (retained, 2, 1)] {
        assert_eq!(snapshot.revision(), revision);
        assert_eq!(snapshot.count(collection).unwrap(), 1);
        assert_eq!(
            snapshot.get(collection, b"key").unwrap().unwrap().as_u64(),
            Ok(value)
        );
    }
}

fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name().into(), fs::read(entry.path()).unwrap())
        })
        .collect()
}

#[test]
fn native_authenticated_follower_backup_restore_and_reseed_keep_old_receipt() {
    let dir = directory("follower-reseed");
    let root = &dir.0;
    let primary = root.join("primary");
    seed_primary(&primary);
    let source = primary.to_str().unwrap();
    let old_stream = "11221122112211221122112211221122";
    let new_stream = "22112211221122112211221122112211";
    let description = run(&["describe", source, old_stream]);
    let identity = description
        .split_whitespace()
        .find_map(|part| part.strip_prefix("file_identity="))
        .unwrap();
    let sender = root.join("sender");
    let receiver = root.join("receiver");
    credentials(&sender);
    credentials(&receiver);
    let sender_cert = sender.join("cert.der");
    let receiver_cert = receiver.join("cert.der");
    let old = root.join("old-follower");
    let new = root.join("new-follower");
    fs::create_dir(&old).unwrap();
    fs::create_dir(&new).unwrap();
    let old_path = old.to_str().unwrap();
    let new_path = new.to_str().unwrap();
    let serve = |path, stream| {
        Server::start(
            root,
            &[
                "serve",
                path,
                stream,
                identity,
                "127.0.0.1:0",
                receiver.to_str().unwrap(),
                sender_cert.to_str().unwrap(),
                "--max-exchanges",
                "1",
            ],
        )
    };
    let ship = |file: &Path, stream: &str, export: &str, address: &str| {
        Command::new(BIN)
            .args([
                "ship",
                file.to_str().unwrap(),
                stream,
                root.join(export).to_str().unwrap(),
                address,
                sender.to_str().unwrap(),
                receiver_cert.to_str().unwrap(),
                "--attempts",
                "1",
            ])
            .output()
            .unwrap()
    };
    let acknowledged = |output: std::process::Output, stream: &str| {
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.lines().any(|line| line
            == format!("local captured_revision=4 stream={stream} file_identity={identity}")));
        let prefix = format!("follower durable_revision=4 stream={stream} digest=");
        let receipt = text.lines().find(|line| line.starts_with(&prefix)).unwrap();
        let digest = receipt.strip_prefix(&prefix).unwrap();
        assert!(digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        receipt.to_owned()
    };
    run(&["follower-init", old_path, old_stream, identity]);
    let (mut server, address) = serve(old_path, old_stream);
    let old_receipt = acknowledged(
        ship(&primary, old_stream, "export-original", &address),
        old_stream,
    );
    server.finished();
    verify_follower(&old, old_stream, identity);

    let selected = old.join("replica-0.wdb");
    let old_files = files(&old);
    let original_bytes = fs::read(&primary).unwrap();
    let backup = root.join("backup");
    let restored = root.join("restored");
    weekend_db::storage::backup_file(&selected, &backup, Limits::default()).unwrap();
    weekend_db::storage::restore_file(&backup, &restored, Limits::default()).unwrap();
    let backup_bytes = fs::read(&backup).unwrap();
    let restored_bytes = fs::read(&restored).unwrap();
    assert_eq!(
        weekend_db::storage::restore_file(&backup, &restored, Limits::default()),
        Err(weekend_db::Error::AlreadyExists)
    );
    assert_eq!(fs::read(&restored).unwrap(), restored_bytes);
    for file in [&backup, &restored] {
        let database = Database::open_file(file, Limits::default()).unwrap();
        assert_eq!(database.retained_ids().unwrap(), [1]);
        verify_records(
            database.snapshot(),
            database.retained_snapshot(1).unwrap(),
            database.collection_id(1),
        );
    }
    let wrong = Command::new(BIN)
        .args(["status", old_path, new_stream, identity])
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert_eq!(
        String::from_utf8(wrong.stderr).unwrap().trim_end(),
        "ForeignStore"
    );

    let (mut server, address) = serve(old_path, old_stream);
    let rejected = ship(&restored, new_stream, "wrong-stream-export", &address);
    server.finished();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ForeignStore"));
    let local = String::from_utf8(rejected.stdout).unwrap();
    assert!(local.contains("local captured_revision=4"));
    assert!(!local.contains("follower durable_revision="));
    assert_eq!(files(&old), old_files);

    run(&["follower-init", new_path, new_stream, identity]);
    let (mut server, address) = serve(new_path, new_stream);
    let new_receipt = acknowledged(
        ship(&restored, new_stream, "new-stream-export", &address),
        new_stream,
    );
    server.finished();
    assert!(
        run(&["status", new_path, new_stream, identity])
            .lines()
            .any(|line| line == new_receipt)
    );
    assert!(
        run(&["status", old_path, old_stream, identity])
            .lines()
            .any(|line| line == old_receipt)
    );
    verify_follower(&new, new_stream, identity);
    assert_eq!(files(&old), old_files);
    assert_eq!(fs::read(&primary).unwrap(), original_bytes);
    assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
    assert_eq!(fs::read(&restored).unwrap(), restored_bytes);
}
