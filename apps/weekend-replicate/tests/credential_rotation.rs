//! Operator stop/restart cutover; no runtime reload or enrollment change.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use weekend_db::{Database, Limits};
const BIN: &str = env!("CARGO_BIN_EXE_weekend-replicate");
const STREAM: &str = "11111111111111111111111111111111";
struct Directory(PathBuf);
impl Directory {
    fn create(path: PathBuf) -> std::io::Result<Self> {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }
    fn fresh() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self::create(std::env::temp_dir().join(format!(
            "weekend-cutover-{}-{nonce}-{serial}",
            std::process::id()
        )))
        .unwrap()
    }
}
#[test]
fn existing_directory_admission_never_claims_cleanup_ownership() {
    let parent = Directory::fresh();
    let existing = parent.0.join("existing");
    fs::create_dir(&existing).unwrap();
    let sentinel = existing.join("keep");
    fs::write(&sentinel, b"previous invocation").unwrap();
    assert_eq!(
        Directory::create(existing.clone()).err().unwrap().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&sentinel).unwrap(), b"previous invocation");
    assert!(existing.is_dir());
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn run(root: &Path, args: &[&str], success: bool) -> String {
    run_program(root, BIN, args, success)
}
fn run_program(root: &Path, binary: &str, args: &[&str], success: bool) -> String {
    let stdout = root.join("command.stdout");
    let stderr = root.join("command.stderr");
    let mut child = Process(
        Command::new(binary)
            .args(args)
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(40);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "CLI process timeout");
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        status.success(),
        success,
        "{}",
        fs::read_to_string(stderr).unwrap()
    );
    fs::read_to_string(stdout).unwrap()
}
fn start(
    root: &Path,
    follower: &str,
    identity: &str,
    credentials: &str,
    pin: &str,
) -> (Process, String) {
    let log = root.join("server.stdout");
    let mut child = Process(
        Command::new(BIN)
            .args([
                "serve",
                follower,
                STREAM,
                identity,
                "127.0.0.1:0",
                credentials,
                pin,
                "--max-exchanges",
                "20",
            ])
            .stdout(fs::File::create(&log).unwrap())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(address) = fs::read_to_string(&log)
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("ready "))
        {
            return (child, address.to_owned());
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "server exited before ready"
        );
        assert!(Instant::now() < deadline, "server readiness timeout");
        thread::sleep(Duration::from_millis(10));
    }
}
fn receipt(text: &str) -> &str {
    text.lines()
        .find(|l| l.starts_with("follower durable_revision="))
        .unwrap()
}
fn check_digest(root: &Path, export: &Path, ack: &str) {
    #[cfg(target_os = "macos")]
    let digest = run_program(
        root,
        "/usr/bin/shasum",
        &["-a", "256", export.to_str().unwrap()],
        true,
    );
    #[cfg(target_os = "linux")]
    let digest = run_program(
        root,
        "/usr/bin/sha256sum",
        &[export.to_str().unwrap()],
        true,
    );
    assert_eq!(
        ack.split("digest=").nth(1).unwrap(),
        digest.split_whitespace().next().unwrap()
    );
}
fn check_image(path: &Path, revision: u64, byte: u8) {
    let db = Database::open_file(path, Limits::default()).unwrap();
    let snapshot = db.snapshot();
    assert_eq!(snapshot.revision(), revision);
    let rows = snapshot.scan(db.collection_id(1)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, b"row");
    assert_eq!(
        snapshot
            .get(db.collection_id(1), b"row")
            .unwrap()
            .unwrap()
            .as_bytes()
            .unwrap(),
        vec![byte; 32768]
    );
    assert_eq!(db.retained_ids().unwrap(), vec![1]);
    let old = db.retained_snapshot(1).unwrap();
    assert_eq!(old.revision(), 2);
    let rows = old.scan(db.collection_id(1)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, b"row");
    assert_eq!(
        old.get(db.collection_id(1), b"row")
            .unwrap()
            .unwrap()
            .as_bytes()
            .unwrap(),
        vec![1; 32768]
    );
}
fn offline(
    root: &Path,
    follower: &str,
    identity: &str,
    ack: &str,
    revision: u64,
    byte: u8,
) -> Vec<u8> {
    let status = run(root, &["status", follower, STREAM, identity], true);
    assert_eq!(receipt(&status), ack);
    let slot = status
        .split("selected_slot=Some(")
        .nth(1)
        .unwrap()
        .split(')')
        .next()
        .unwrap();
    let path = Path::new(follower).join(format!("replica-{slot}.wdb"));
    let bytes = fs::read(&path).unwrap();
    check_image(&path, revision, byte);
    assert_eq!(fs::read(path).unwrap(), bytes);
    bytes
}
#[test]
fn stopped_endpoint_credential_cutover_preserves_receipt_history_and_retry() {
    let root = Directory::fresh();
    let root = &root.0;
    let source_path = root.join("primary.wdb");
    let source = source_path.to_str().unwrap();
    {
        let db = Database::create_file(source, Limits::default()).unwrap();
        let collection = db.create_collection().unwrap();
        let mut tx = db.begin();
        tx.put(collection, b"row", &db.bytes(&vec![1; 32768]).unwrap())
            .unwrap();
        assert_eq!(tx.commit().unwrap(), 2);
        assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 1);
    }
    let description = run(root, &["describe", source, STREAM], true);
    let identity = description
        .split_whitespace()
        .find_map(|s| s.strip_prefix("file_identity="))
        .unwrap();
    let follower_path = root.join("follower");
    fs::create_dir(&follower_path).unwrap();
    let follower = follower_path.to_str().unwrap();
    run(root, &["follower-init", follower, STREAM, identity], true);
    for name in ["client-old", "server-old", "client-new", "server-new"] {
        run(root, &["keygen", root.join(name).to_str().unwrap()], true);
    }
    let co = root.join("client-old");
    let so = root.join("server-old");
    let cn = root.join("client-new");
    let sn = root.join("server-new");
    let cop = co.join("cert.der");
    let sop = so.join("cert.der");
    let cnp = cn.join("cert.der");
    let snp = sn.join("cert.der");
    assert_ne!(fs::read(&cop).unwrap(), fs::read(&cnp).unwrap());
    assert_ne!(fs::read(&sop).unwrap(), fs::read(&snp).unwrap());
    let poll = |address: &str, creds: &Path, pin: &Path, success: bool| {
        let text = run(
            root,
            &[
                "poll",
                source,
                STREAM,
                address,
                creds.to_str().unwrap(),
                pin.to_str().unwrap(),
            ],
            success,
        );
        if !success {
            assert!(!text.contains("follower durable_revision="));
        }
        text
    };
    let (server, address) = start(
        root,
        follower,
        identity,
        so.to_str().unwrap(),
        cop.to_str().unwrap(),
    );
    let first_export = root.join("first.export");
    let shipped = run(
        root,
        &[
            "ship",
            source,
            STREAM,
            first_export.to_str().unwrap(),
            &address,
            co.to_str().unwrap(),
            sop.to_str().unwrap(),
        ],
        true,
    );
    assert!(shipped.contains("local captured_revision=3"));
    let old_ack = receipt(&shipped).to_owned();
    check_digest(root, &first_export, &old_ack);
    assert_eq!(receipt(&poll(&address, &co, &sop, true)), old_ack);
    drop(server);
    let original = offline(root, follower, identity, &old_ack, 3, 1);
    assert_eq!(original, fs::read(&first_export).unwrap());
    let manifest = follower_path.join("replica-manifest.wdb");
    let old_manifest = fs::read(&manifest).unwrap();
    // Source commits locally while follower stays at revision 3.
    {
        let db = Database::open_file(source, Limits::default()).unwrap();
        let mut tx = db.begin();
        tx.put(
            db.collection_id(1),
            b"row",
            &db.bytes(&vec![2; 32768]).unwrap(),
        )
        .unwrap();
        assert_eq!(tx.commit().unwrap(), 4);
    }
    check_image(&source_path, 4, 2);
    // Client-only leaf change, then pin-only mistake against an active old server.
    let (server, address) = start(
        root,
        follower,
        identity,
        so.to_str().unwrap(),
        cop.to_str().unwrap(),
    );
    poll(&address, &cn, &sop, false);
    poll(&address, &co, &snp, false);
    let unchanged = poll(&address, &co, &sop, true);
    assert!(unchanged.contains("local verified_revision=4"));
    assert_eq!(receipt(&unchanged), old_ack);
    drop(server);
    assert_eq!(offline(root, follower, identity, &old_ack, 3, 1), original);
    // Server-only leaf change: stale server pin fails; matching pin still admits old client.
    let (server, address) = start(
        root,
        follower,
        identity,
        sn.to_str().unwrap(),
        cop.to_str().unwrap(),
    );
    poll(&address, &co, &sop, false);
    assert_eq!(receipt(&poll(&address, &co, &snp, true)), old_ack);
    drop(server);
    assert_eq!(offline(root, follower, identity, &old_ack, 3, 1), original);
    // Both operator configuration paths now select fresh leaves/pins. Kill before sync.
    let (server, _) = start(
        root,
        follower,
        identity,
        sn.to_str().unwrap(),
        cnp.to_str().unwrap(),
    );
    drop(server);
    assert_eq!(offline(root, follower, identity, &old_ack, 3, 1), original);
    let (server, address) = start(
        root,
        follower,
        identity,
        sn.to_str().unwrap(),
        cnp.to_str().unwrap(),
    );
    poll(&address, &co, &snp, false); // Old client revoked by new exact pin.
    let rejected_export = root.join("rejected.export");
    let rejected = run(
        root,
        &[
            "ship",
            source,
            STREAM,
            rejected_export.to_str().unwrap(),
            &address,
            co.to_str().unwrap(),
            snp.to_str().unwrap(),
            "--attempts",
            "1",
        ],
        false,
    );
    assert!(rejected.contains("local captured_revision=4"));
    assert!(!rejected.contains("follower durable_revision="));
    poll(&address, &cn, &sop, false); // New client still needs fresh server pin.
    assert_eq!(receipt(&poll(&address, &cn, &snp, true)), old_ack);
    drop(server);
    assert_eq!(offline(root, follower, identity, &old_ack, 3, 1), original);
    assert_eq!(fs::read(&manifest).unwrap(), old_manifest);
    let (server, address) = start(
        root,
        follower,
        identity,
        sn.to_str().unwrap(),
        cnp.to_str().unwrap(),
    );
    let next_export = root.join("next.export");
    let shipped = run(
        root,
        &[
            "ship",
            source,
            STREAM,
            next_export.to_str().unwrap(),
            &address,
            cn.to_str().unwrap(),
            snp.to_str().unwrap(),
        ],
        true,
    );
    let new_ack = receipt(&shipped).to_owned();
    check_digest(root, &next_export, &new_ack);
    assert!(new_ack.starts_with("follower durable_revision=4 "));
    assert_ne!(new_ack, old_ack);
    assert_eq!(receipt(&poll(&address, &cn, &snp, true)), new_ack);
    assert_eq!(receipt(&poll(&address, &cn, &snp, true)), new_ack);
    drop(server);
    let current = offline(root, follower, identity, &new_ack, 4, 2);
    assert_eq!(current, fs::read(&next_export).unwrap());
    let current_manifest = fs::read(&manifest).unwrap();
    // A final independent endpoint restart neither changes receipt nor creates commits.
    let (server, address) = start(
        root,
        follower,
        identity,
        sn.to_str().unwrap(),
        cnp.to_str().unwrap(),
    );
    assert_eq!(receipt(&poll(&address, &cn, &snp, true)), new_ack);
    drop(server);
    assert_eq!(offline(root, follower, identity, &new_ack, 4, 2), current);
    assert_eq!(fs::read(&manifest).unwrap(), current_manifest);
    check_image(&source_path, 4, 2);
}
