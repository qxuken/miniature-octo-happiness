#![cfg(all(
    feature = "replication",
    any(
        target_os = "macos",
        windows,
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    )
))]

use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use weekend_db::{
    Database, Error, Limits,
    replication::{Follower, Primary},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "weekend-replication-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => panic!("{e}"),
            }
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn timeouts(stream: &TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
}

// The listener and client are owned by the same test. Deployment supplies
// a mutually authenticated channel instead of this trusted loopback connection.
fn exchange<T>(
    follower: &mut Follower,
    request: impl FnOnce(TcpStream) -> T,
) -> (T, weekend_db::Result<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    std::thread::scope(|scope| {
        let address = listener.local_addr().unwrap();
        let receiver = scope.spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            timeouts(&stream);
            follower.serve_authenticated(stream)
        });
        let stream = TcpStream::connect(address).unwrap();
        timeouts(&stream);
        let result = request(stream);
        (result, receiver.join().unwrap())
    })
}

#[test]
fn local_commit_and_follower_durability_are_separate_over_tcp() {
    let dir = Directory::new();
    let follower_dir = dir.0.join("follower");
    fs::create_dir(&follower_dir).unwrap();
    let mut db = Database::create_file(dir.0.join("primary"), Limits::default()).unwrap();
    let mut primary = Primary::new(&db, [1; 16]).unwrap();
    let enrollment = primary.enrollment();
    let mut follower = Follower::create(&follower_dir, enrollment, Limits::default()).unwrap();
    let (progress, served) = exchange(&mut follower, |socket| primary.poll(socket));
    served.unwrap();
    assert_eq!(progress.unwrap(), None);

    let id = db.create_collection().unwrap();
    let lookup = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(id, b"key", &db.unsigned(1).unwrap()).unwrap();
    tx.put(lookup, b"old-name", &db.bytes(b"key").unwrap())
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    tx.put(id, b"key", &db.unsigned(2).unwrap()).unwrap();
    tx.delete(lookup, b"old-name").unwrap();
    tx.put(lookup, b"new-name", &db.bytes(b"key").unwrap())
        .unwrap();
    let revision = tx.commit().unwrap();
    drop(tx);
    assert_eq!(follower.progress().unwrap(), None);
    let mut export = primary.export_file(&mut db, dir.0.join("export")).unwrap();

    let mut tx = db.begin();
    tx.put(id, b"key", &db.unsigned(3).unwrap()).unwrap();
    assert!(tx.commit().unwrap() > revision);
    drop(tx);

    let (ack, served) = exchange(&mut follower, |socket| primary.ship(&mut export, socket));
    served.unwrap();
    let ack = ack.unwrap();
    assert_eq!(ack.revision, revision);
    assert_eq!(follower.progress().unwrap(), Some(ack));

    drop(follower);
    let mut follower = Follower::open(&follower_dir, enrollment, Limits::default()).unwrap();
    let (progress, served) = exchange(&mut follower, |socket| primary.poll(socket));
    served.unwrap();
    assert_eq!(progress.unwrap(), Some(ack));
    let (retry, served) = exchange(&mut follower, |socket| primary.ship(&mut export, socket));
    served.unwrap();
    assert_eq!(retry.unwrap(), ack);
    let replica = follower.replica().unwrap();
    let id = replica.collection_id(id.as_u64());
    let lookup = replica.collection_id(lookup.as_u64());
    let current = replica.snapshot();
    let old = replica.retained_snapshot(retained).unwrap();
    assert!(current.get(lookup, b"old-name").unwrap().is_none());
    assert!(old.get(lookup, b"new-name").unwrap().is_none());
    for (view, name, expected) in [(current, b"new-name", 2), (old, b"old-name", 1)] {
        let key = view.get(lookup, name).unwrap().unwrap();
        let row = view.get(id, key.as_bytes().unwrap()).unwrap().unwrap();
        assert_eq!(row.as_u64(), Ok(expected));
    }
}

#[test]
fn follower_manifest_does_not_consume_replica_value_budget() {
    let dir = Directory::new();
    let follower_dir = dir.0.join("follower");
    fs::create_dir(&follower_dir).unwrap();
    let mut db = Database::create_file(dir.0.join("primary"), Limits::default()).unwrap();
    let mut primary = Primary::new(&db, [3; 16]).unwrap();
    let enrollment = primary.enrollment();
    let limits = Limits {
        max_value_bytes: 64,
        ..Limits::default()
    };
    let mut follower = Follower::create(&follower_dir, enrollment, limits).unwrap();
    let id = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(id, b"key", &db.unsigned(42).unwrap()).unwrap();
    let revision = tx.commit().unwrap();
    drop(tx);
    let mut export = primary.export_file(&mut db, dir.0.join("export")).unwrap();
    let (ack, served) = exchange(&mut follower, |socket| primary.ship(&mut export, socket));
    served.unwrap();
    let ack = ack.unwrap();
    assert_eq!(ack.revision, revision);
    drop(follower);

    let mut follower = Follower::open(&follower_dir, enrollment, limits).unwrap();
    assert_eq!(follower.progress(), Ok(Some(ack)));
    let replica = follower.replica().unwrap();
    let row = replica
        .snapshot()
        .get(replica.collection_id(id.as_u64()), b"key")
        .unwrap()
        .unwrap();
    assert_eq!(row.as_u64(), Ok(42));

    let mut tx = db.begin();
    tx.put(id, b"key", &db.bytes(&[0; 64]).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    let mut too_large = primary.export_file(&mut db, dir.0.join("large")).unwrap();
    let (result, served) = exchange(&mut follower, |socket| primary.ship(&mut too_large, socket));
    assert_eq!(served, Err(Error::Limit));
    assert_eq!(result, Err(Error::Limit));
    assert_eq!(follower.progress(), Ok(Some(ack)));
}

#[test]
fn process_kill_during_upload_or_before_reply_recovers_durable_progress() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        process::{Child, Command, Stdio},
        sync::mpsc,
    };
    const PHASE: &str = "WDB_REPLICATION_TEST_PHASE";
    const DIR: &str = "WDB_REPLICATION_TEST_DIR";
    const ADDRESS: &str = "WDB_REPLICATION_TEST_ADDRESS";
    const ID: [u8; 16] = [31; 16];
    let enrollment = weekend_db::replication::Enrollment {
        stream_id: [32; 16],
        file_identity: ID,
    };

    if let Ok(phase) = std::env::var(PHASE) {
        struct StopAt {
            stream: TcpStream,
            bytes_read: usize,
            phase: String,
        }
        fn checkpoint() -> ! {
            println!("checkpoint");
            loop {
                std::thread::park();
            }
        }
        impl Read for StopAt {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if self.phase == "upload" && self.bytes_read >= 89 + weekend_db::storage::REGION {
                    checkpoint();
                }
                let n = self.stream.read(out)?;
                self.bytes_read += n;
                Ok(n)
            }
        }
        impl Write for StopAt {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.phase == "reply" && bytes.first() == Some(&2) {
                    checkpoint();
                }
                self.stream.write(bytes)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.stream.flush()
            }
        }

        let mut follower = Follower::open(
            std::env::var_os(DIR).unwrap(),
            enrollment,
            Limits::default(),
        )
        .unwrap();
        let stream = TcpStream::connect(std::env::var(ADDRESS).unwrap()).unwrap();
        timeouts(&stream);
        println!("connected");
        follower
            .serve_authenticated(StopAt {
                stream,
                bytes_read: 0,
                phase,
            })
            .unwrap();
        panic!("worker returned before its checkpoint");
    }

    struct KillOnDrop(Child);
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = Directory::new();
    let follower_dir = dir.0.join("follower");
    fs::create_dir(&follower_dir).unwrap();
    let limits = Limits::default();
    let mut db = Database::create_on(
        weekend_db::storage::FileStore::create(dir.0.join("primary")).unwrap(),
        ID,
        limits,
    )
    .unwrap();
    let mut primary = Primary::new(&db, enrollment.stream_id).unwrap();
    let mut follower = Follower::create(&follower_dir, enrollment, limits).unwrap();
    let mut initial = primary.export_file(&mut db, dir.0.join("initial")).unwrap();
    let (first, served) = exchange(&mut follower, |socket| primary.ship(&mut initial, socket));
    served.unwrap();
    let first = first.unwrap();
    drop(follower);
    let id = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(id, b"key", &db.unsigned(42).unwrap()).unwrap();
    let revision = tx.commit().unwrap();
    drop(tx);
    let mut export = primary.export_file(&mut db, dir.0.join("export")).unwrap();

    for (phase, expected) in [("upload", first.revision), ("reply", revision)] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut worker = KillOnDrop(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process_kill_during_upload_or_before_reply_recovers_durable_progress",
                    "--nocapture",
                ])
                .env(PHASE, phase)
                .env(DIR, &follower_dir)
                .env(ADDRESS, listener.local_addr().unwrap().to_string())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let stdout = worker.0.stdout.take().unwrap();
        let (send, receive) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if line
                    .split_whitespace()
                    .last()
                    .is_some_and(|last| last == "connected" || last == "checkpoint")
                {
                    send.send(line).unwrap();
                }
            }
        });
        let marker = || receive.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(marker().ends_with("connected"));
        let (socket, _) = listener.accept().unwrap();
        timeouts(&socket);
        std::thread::scope(|scope| {
            let sender = scope.spawn(|| primary.ship(&mut export, socket));
            assert!(marker().ends_with("checkpoint"));
            worker.0.kill().unwrap();
            assert!(!worker.0.wait().unwrap().success());
            assert_eq!(sender.join().unwrap(), Err(Error::Io));
        });
        reader.join().unwrap();

        let mut follower = Follower::open(&follower_dir, enrollment, limits).unwrap();
        let (progress, served) = exchange(&mut follower, |socket| primary.poll(socket));
        served.unwrap();
        assert_eq!(progress.unwrap().unwrap().revision, expected);
        if phase == "reply" {
            let (retry, served) =
                exchange(&mut follower, |socket| primary.ship(&mut export, socket));
            served.unwrap();
            assert_eq!(retry.unwrap().revision, revision);
        }
    }

    // The old slot is still present. A damaged selected image must surface as
    // failure, never as progress from the previous generation.
    assert!(follower_dir.join("replica-0.wdb").is_file());
    let selected = follower_dir.join("replica-1.wdb");
    let image = fs::read(&selected).unwrap();
    let mut corrupt = image.clone();
    corrupt[0] ^= 1;
    fs::write(&selected, &corrupt).unwrap();
    assert!(matches!(
        Follower::open(&follower_dir, enrollment, limits),
        Err(Error::Corrupt)
    ));
    fs::remove_file(selected).unwrap();
    assert!(matches!(
        Follower::open(&follower_dir, enrollment, limits),
        Err(Error::Corrupt)
    ));
}
