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
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use weekend_db::{
    Database, Error, Limits,
    replication::{Follower, Primary},
    storage::{ByteStore, FileStore},
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

struct PausedCopy {
    file: FileStore,
    pause: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
}
impl ByteStore for PausedCopy {
    fn lock(&mut self) -> weekend_db::Result<()> {
        self.file.lock()
    }
    fn unlock(&mut self) {
        self.file.unlock();
    }
    fn length(&mut self) -> weekend_db::Result<u64> {
        self.file.length()
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> weekend_db::Result<usize> {
        self.file.read(offset, out)
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> weekend_db::Result<usize> {
        self.file.write(offset, bytes)
    }
    fn sync(&mut self) -> weekend_db::Result<()> {
        self.file.sync()?;
        if let Some((arrived, resume)) = self.pause.take() {
            arrived.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        Ok(())
    }
}

#[test]
fn native_capture_factories_preserve_file_errors_and_exact_saved_bytes() {
    let dir = Directory::new();
    let limits = Limits::default();
    let mut source = Database::create_file(dir.0.join("source"), limits).unwrap();
    let mut foreign = Database::create_file(dir.0.join("foreign"), limits).unwrap();
    let mut primary = Primary::new(&source, [7; 16]).unwrap();
    let capture_path = dir.0.join("capture");
    assert!(matches!(
        primary.export_file(&mut foreign, &capture_path),
        Err(Error::ForeignStore)
    ));
    assert!(!capture_path.exists());

    assert_eq!(source.begin().commit(), Ok(1));
    let occupied = dir.0.join("occupied");
    fs::write(&occupied, b"keep").unwrap();
    assert!(matches!(
        primary.export_file(&mut source, &occupied),
        Err(Error::AlreadyExists)
    ));
    assert_eq!(fs::read(&occupied).unwrap(), b"keep");

    let export = primary.export_file(&mut source, &capture_path).unwrap();
    let descriptor = export.descriptor();
    assert_eq!(descriptor.revision, 1);
    drop(export);
    let original = fs::read(&capture_path).unwrap();
    assert!(matches!(
        primary.reopen_export_file(dir.0.join("missing"), descriptor, limits),
        Err(Error::NotFound)
    ));
    let reopened = primary
        .reopen_export_file(&capture_path, descriptor, limits)
        .unwrap();
    assert_eq!(reopened.descriptor(), descriptor);
    drop(reopened);
    assert_eq!(fs::read(&capture_path).unwrap(), original);
    assert_eq!(source.snapshot().revision(), 1);
}

#[test]
fn live_export_pins_old_roots_while_the_native_source_changes() {
    let dir = Directory::new();
    let limits = Limits::default();
    let source_path = dir.0.join("primary");
    let mut db = Database::create_file(&source_path, limits).unwrap();
    let collection = db.create_collection().unwrap();
    let put = |db: &Database, n| {
        let mut tx = db.begin();
        tx.put(collection, b"key", &db.unsigned(n).unwrap())
            .unwrap();
        tx.commit().unwrap()
    };
    assert_eq!(put(&db, 1), 2);
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let captured_revision = put(&db, 2);
    let mut primary = Primary::new(&db, [7; 16]).unwrap();
    let baseline_path = dir.0.join("baseline");
    let baseline = primary.export_file(&mut db, &baseline_path).unwrap();
    let descriptor = baseline.descriptor();
    drop(baseline);

    let export_path = dir.0.join("live");
    let (arrived, at_sync) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let copy = PausedCopy {
        file: FileStore::create(&export_path).unwrap(),
        pause: Some((arrived, resumed)),
    };
    let (mut export, latest, replacement) = std::thread::scope(|scope| {
        let capture = scope.spawn(|| primary.export_live_to(&db, copy, limits));
        at_sync.recv_timeout(Duration::from_secs(10)).unwrap();
        let (done, finished) = mpsc::channel();
        let db = &db;
        let writer = scope.spawn(move || {
            put(db, 3);
            db.release_retained(retained).unwrap();
            let replacement = db.retain_snapshot(&db.snapshot()).unwrap();
            let _ = db.create_collection().unwrap();
            done.send((db.snapshot().revision(), replacement)).unwrap();
        });
        let result = finished.recv_timeout(Duration::from_secs(5));
        resume.send(()).unwrap();
        writer.join().unwrap();
        let (latest, replacement) = result.expect("source waited for a destination sync");
        (capture.join().unwrap().unwrap(), latest, replacement)
    });
    assert_eq!(export.descriptor(), descriptor);
    assert_eq!(export.revision(), captured_revision);
    assert_eq!(
        Database::open_file(&source_path, limits).err(),
        Some(Error::Busy)
    );
    assert!(latest > captured_revision);
    assert_eq!(db.retained_ids().unwrap(), vec![replacement]);

    let follower_path = dir.0.join("follower");
    fs::create_dir(&follower_path).unwrap();
    let mut follower = Follower::create(&follower_path, primary.enrollment(), limits).unwrap();
    let (received, served) = exchange(&mut follower, |socket| primary.ship(&mut export, socket));
    served.unwrap();
    let received = received.unwrap();
    assert_eq!(received.revision, captured_revision);
    drop(export);
    assert_eq!(
        fs::read(&export_path).unwrap(),
        fs::read(&baseline_path).unwrap()
    );
    let (progress, served) = exchange(&mut follower, |socket| primary.poll_live(&db, socket));
    served.unwrap();
    let progress = progress.unwrap();
    assert_eq!(progress.local_revision, latest);
    assert_eq!(progress.follower, Some(received));
    assert_eq!(progress.revision_gap(), Some(latest - captured_revision));
    {
        let replica = follower.replica().unwrap();
        let collection = replica.collection_id(collection.as_u64());
        let old = replica.retained_snapshot(retained).unwrap();
        assert_eq!(old.revision(), 2);
        assert_eq!(
            old.get(collection, b"key").unwrap().unwrap().as_u64(),
            Ok(1)
        );
        assert_eq!(
            replica
                .snapshot()
                .get(collection, b"key")
                .unwrap()
                .unwrap()
                .as_u64(),
            Ok(2)
        );
    }
    let mut new = primary
        .export_live_file(&db, dir.0.join("updated"), limits)
        .unwrap();
    assert_eq!(new.revision(), latest);
    let (received, served) = exchange(&mut follower, |socket| primary.ship(&mut new, socket));
    served.unwrap();
    let received = received.unwrap();
    let enrollment = primary.enrollment();
    drop(follower);
    let mut follower = Follower::open(&follower_path, enrollment, limits).unwrap();
    let (progress, served) = exchange(&mut follower, |socket| primary.poll_live(&db, socket));
    served.unwrap();
    let progress = progress.unwrap();
    assert_eq!(progress.follower, Some(received));
    assert_eq!(progress.revision_gap(), Some(0));
    let replica = follower.replica().unwrap();
    let collection = replica.collection_id(collection.as_u64());
    assert_eq!(
        replica.retained_snapshot(retained).err(),
        Some(Error::NotFound)
    );
    let current = replica.retained_snapshot(replacement).unwrap();
    assert_eq!(
        current.get(collection, b"key").unwrap().unwrap().as_u64(),
        Ok(3)
    );
    assert_eq!(
        replica
            .snapshot()
            .get(collection, b"key")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(3)
    );
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

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn manifest_headroom_checkpoint_and_fresh_directory_reseed_preserve_ack() {
    use weekend_db::{replication::MANIFEST_LIMIT_BYTES, storage::MemoryStore};
    let dir = Directory::new();
    let follower_dir = dir.0.join("lifecycle");
    fs::create_dir(&follower_dir).unwrap();
    let mut db =
        Database::create_on(MemoryStore::new().unwrap(), [81; 16], Limits::default()).unwrap();
    let mut primary = Primary::new(&db, [82; 16]).unwrap();
    let enrollment = primary.enrollment();
    let mut follower = Follower::create(&follower_dir, enrollment, Limits::default()).unwrap();
    let collection = db.create_collection().unwrap();
    let mut first = primary
        .export_to(&mut db, MemoryStore::new().unwrap())
        .unwrap();
    let (ack, served) = exchange(&mut follower, |socket| primary.ship(&mut first, socket));
    served.unwrap();
    let ack = ack.unwrap();
    drop(follower);
    fs::OpenOptions::new()
        .write(true)
        .open(follower_dir.join("replica-manifest.wdb"))
        .unwrap()
        .set_len(MANIFEST_LIMIT_BYTES)
        .unwrap();
    let mut follower = Follower::open(&follower_dir, enrollment, Limits::default()).unwrap();
    let status = follower.status().unwrap();
    assert!(status.maintenance_required);
    assert_eq!(status.durable_ack, Some(ack));
    let (duplicate, served) = exchange(&mut follower, |socket| primary.ship(&mut first, socket));
    served.unwrap();
    assert_eq!(duplicate, Ok(ack));
    let mut tx = db.begin();
    tx.put(collection, b"x", &db.unsigned(9).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    let mut next = primary
        .export_to(&mut db, MemoryStore::new().unwrap())
        .unwrap();
    let (rejected, served) = exchange(&mut follower, |socket| primary.ship(&mut next, socket));
    assert_eq!(rejected, Err(Error::Limit));
    assert_eq!(served, Err(Error::Limit));
    assert_eq!(follower.progress(), Ok(Some(ack)));
    assert!(!follower_dir.join("replica-1.wdb").exists());
    let mut follower = follower.compact_manifest().unwrap();
    assert!(!follower.status().unwrap().maintenance_required);
    assert_eq!(follower.progress(), Ok(Some(ack)));
    let (new_ack, served) = exchange(&mut follower, |socket| primary.ship(&mut next, socket));
    served.unwrap();
    let new_ack = new_ack.unwrap();
    fs::write(follower_dir.join("application-note"), b"keep").unwrap();
    assert_eq!(follower.cleanup_inactive(), Ok(1));
    assert_eq!(follower.cleanup_inactive(), Ok(0));
    assert_eq!(
        fs::read(follower_dir.join("application-note")).unwrap(),
        b"keep"
    );
    drop(follower);
    let old = Follower::open(&follower_dir, enrollment, Limits::default()).unwrap();
    assert_eq!(old.progress(), Ok(Some(new_ack)));
    let replacement = dir.0.join("replacement");
    fs::create_dir(&replacement).unwrap();
    let mut reseeded = Follower::create(&replacement, enrollment, Limits::default()).unwrap();
    let (reseed_ack, served) = exchange(&mut reseeded, |socket| primary.ship(&mut next, socket));
    served.unwrap();
    assert_eq!(reseed_ack, Ok(new_ack));
    assert_eq!(old.progress(), Ok(Some(new_ack)));
}

#[test]
fn cleanup_only_removes_reserved_unselected_regular_files() {
    use weekend_db::{replication::Enrollment, storage::MemoryStore};
    let dir = Directory::new();
    let mut follower = Follower::create(
        &dir.0,
        Enrollment {
            stream_id: [91; 16],
            file_identity: [92; 16],
        },
        Limits::default(),
    )
    .unwrap();
    fs::write(dir.0.join("replica-0.wdb"), b"interrupted first upload").unwrap();
    assert_eq!(follower.cleanup_inactive(), Ok(1));
    assert_eq!(follower.progress(), Ok(None));
    fs::create_dir(dir.0.join("replica-1.wdb")).unwrap();
    assert_eq!(follower.cleanup_inactive(), Err(Error::Invalid));
    fs::remove_dir(dir.0.join("replica-1.wdb")).unwrap();
    let mut db =
        Database::create_on(MemoryStore::new().unwrap(), [92; 16], Limits::default()).unwrap();
    let mut primary = Primary::new(&db, [91; 16]).unwrap();
    let mut image = primary
        .export_to(&mut db, MemoryStore::new().unwrap())
        .unwrap();
    let (ack, served) = exchange(&mut follower, |socket| primary.ship(&mut image, socket));
    served.unwrap();
    ack.unwrap();
    fs::write(
        dir.0.join("replica-1.wdb"),
        b"leave inactive intact on error",
    )
    .unwrap();
    fs::write(dir.0.join("replica-0.wdb"), b"damaged selected").unwrap();
    assert_eq!(follower.cleanup_inactive(), Err(Error::Corrupt));
    assert_eq!(
        fs::read(dir.0.join("replica-1.wdb")).unwrap(),
        b"leave inactive intact on error"
    );
}

#[test]
#[cfg(all(
    feature = "fault-injection",
    any(target_os = "macos", target_os = "linux")
))]
fn interrupted_manifest_checkpoint_reopens_the_selected_follower_revision() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use weekend_db::storage::{MemoryStore, compact_file_with_hook};
    let dir = Directory::new();
    let original = dir.0.join("original");
    fs::create_dir(&original).unwrap();
    let mut db =
        Database::create_on(MemoryStore::new().unwrap(), [71; 16], Limits::default()).unwrap();
    let mut primary = Primary::new(&db, [72; 16]).unwrap();
    let enrollment = primary.enrollment();
    let mut follower = Follower::create(&original, enrollment, Limits::default()).unwrap();
    db.create_collection().unwrap();
    let mut image = primary
        .export_to(&mut db, MemoryStore::new().unwrap())
        .unwrap();
    let (ack, served) = exchange(&mut follower, |socket| primary.ship(&mut image, socket));
    served.unwrap();
    let ack = ack.unwrap();
    drop(follower);
    let template: Vec<_> = fs::read_dir(&original)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), fs::read(entry.path()).unwrap())
        })
        .collect();
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    compact_file_with_hook(
        &original.join("replica-manifest.wdb"),
        Limits::default(),
        move |_| {
            observed.fetch_add(1, Ordering::Relaxed);
            Ok(())
        },
    )
    .unwrap();
    for fail in 0..count.load(Ordering::Relaxed) {
        let path = dir.0.join(format!("fault-{fail}"));
        fs::create_dir(&path).unwrap();
        for (name, bytes) in &template {
            fs::write(path.join(name), bytes).unwrap();
        }
        let mut step = 0;
        let result = compact_file_with_hook(
            &path.join("replica-manifest.wdb"),
            Limits::default(),
            move |_| {
                let failed = step == fail;
                step += 1;
                if failed { Err(Error::Io) } else { Ok(()) }
            },
        );
        assert!(result.is_err(), "fault {fail}");
        let mut recovered = Follower::open(&path, enrollment, Limits::default()).unwrap();
        assert_eq!(recovered.progress(), Ok(Some(ack)), "fault {fail}");
        let (retry, served) = exchange(&mut recovered, |socket| primary.ship(&mut image, socket));
        served.unwrap();
        assert_eq!(retry, Ok(ack));
    }
}

#[test]
fn manifest_admission_accepts_exact_reserved_headroom() {
    use weekend_db::{
        replication::MANIFEST_LIMIT_BYTES,
        storage::{MemoryStore, REGION},
    };
    let dir = Directory::new();
    let mut db =
        Database::create_on(MemoryStore::new().unwrap(), [61; 16], Limits::default()).unwrap();
    let mut primary = Primary::new(&db, [62; 16]).unwrap();
    let enrollment = primary.enrollment();
    let follower = Follower::create(&dir.0, enrollment, Limits::default()).unwrap();
    drop(follower);
    fs::OpenOptions::new()
        .write(true)
        .open(dir.0.join("replica-manifest.wdb"))
        .unwrap()
        .set_len(MANIFEST_LIMIT_BYTES - 4 * REGION as u64)
        .unwrap();
    let mut follower = Follower::open(&dir.0, enrollment, Limits::default()).unwrap();
    assert!(!follower.status().unwrap().maintenance_required);
    let mut export = primary
        .export_to(&mut db, MemoryStore::new().unwrap())
        .unwrap();
    let (ack, served) = exchange(&mut follower, |socket| primary.ship(&mut export, socket));
    served.unwrap();
    ack.unwrap();
    let status = follower.status().unwrap();
    assert!(status.manifest_bytes <= MANIFEST_LIMIT_BYTES);
    assert!(status.maintenance_required);
}

#[test]
#[ignore = "fills the native manifest and is run by the maintenance qualification"]
fn native_manifest_growth_and_reseed_preserve_receipts() {
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, path::Path};
    use weekend_db::{
        replication::{DurableAck, MANIFEST_LIMIT_BYTES},
        storage::{MemoryStore, REGION},
    };

    let dir = Directory::new();
    let old_path = dir.0.join("old");
    let replacement_path = dir.0.join("replacement");
    fs::create_dir(&old_path).unwrap();
    fs::create_dir(&replacement_path).unwrap();
    let mut db = Database::create_file(dir.0.join("primary.wdb"), Limits::default()).unwrap();
    let collection = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(collection, b"key", &db.unsigned(1).unwrap())
        .unwrap();
    assert_eq!(tx.commit().unwrap(), 2);
    drop(tx);
    assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 1);
    let mut primary = Primary::new(&db, [74; 16]).unwrap();
    let enrollment = primary.enrollment();
    let mut old = Follower::create(&old_path, enrollment, Limits::default()).unwrap();
    fs::write(old_path.join("operator-note"), b"preserve this file").unwrap();

    let inventory = |path: &Path| {
        fs::read_dir(path)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    PathBuf::from(entry.file_name()),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let export = |db: &mut Database, primary: &mut Primary, value| {
        let mut tx = db.begin();
        tx.put(collection, b"key", &db.unsigned(value).unwrap())
            .unwrap();
        let revision = tx.commit().unwrap();
        drop(tx);
        let image = primary.export_to(db, MemoryStore::new().unwrap()).unwrap();
        assert_eq!(image.revision(), revision);
        image
    };
    let verify = |follower: &Follower, ack: DurableAck, value| {
        assert_eq!(follower.progress(), Ok(Some(ack)));
        let replica = follower.replica().unwrap();
        let collection = replica.collection_id(collection.as_u64());
        assert_eq!(replica.retained_ids().unwrap(), [1]);
        for (snapshot, revision, number) in [
            (replica.snapshot(), ack.revision, value),
            (replica.retained_snapshot(1).unwrap(), 2, 1),
        ] {
            assert_eq!(snapshot.revision(), revision);
            assert_eq!(snapshot.count(collection).unwrap(), 1);
            assert_eq!(
                snapshot.get(collection, b"key").unwrap().unwrap().as_u64(),
                Ok(number)
            );
        }
    };
    let mut capped = None;
    let mut previous_length = old.status().unwrap().manifest_bytes;
    for installs in 1..=MANIFEST_LIMIT_BYTES / REGION as u64 {
        let value = installs + 1;
        let mut image = export(&mut db, &mut primary, value);
        let (ack, served) = exchange(&mut old, |socket| primary.ship(&mut image, socket));
        served.unwrap();
        let ack = ack.unwrap();
        let status = old.status().unwrap();
        assert_eq!(ack.revision, image.revision());
        assert_eq!(ack.digest, image.descriptor().digest);
        assert_eq!(status.durable_ack, Some(ack));
        assert!(
            previous_length < status.manifest_bytes
                && status.manifest_bytes <= MANIFEST_LIMIT_BYTES
        );
        previous_length = status.manifest_bytes;
        if status.maintenance_required {
            capped = Some((installs, value, image, ack, status));
            break;
        }
    }
    let (installs, value, mut capped_export, capped_ack, status) =
        capped.expect("native manifest did not reach admission ceiling");
    assert!(installs > 1);
    assert!(status.manifest_bytes > MANIFEST_LIMIT_BYTES - 4 * REGION as u64);
    verify(&old, capped_ack, value);
    drop(old);
    let slot = status.selected_slot.unwrap();
    let selected = PathBuf::from(format!("replica-{slot}.wdb"));
    let inactive = PathBuf::from(format!("replica-{}.wdb", 1 - slot));
    let before = inventory(&old_path);
    assert!(before.contains_key(&inactive));
    assert_eq!(Sha256::digest(&before[&selected])[..], capped_ack.digest);

    let mut next_export = export(&mut db, &mut primary, value + 1);
    assert_eq!(next_export.revision(), capped_ack.revision + 1);
    let mut old = Follower::open(&old_path, enrollment, Limits::default()).unwrap();
    assert_eq!(old.status().unwrap(), status);
    assert_eq!(
        exchange(&mut old, |socket| primary.poll(socket)),
        (Ok(Some(capped_ack)), Ok(()))
    );
    assert_eq!(
        exchange(&mut old, |socket| primary.ship(&mut capped_export, socket)),
        (Ok(capped_ack), Ok(()))
    );
    assert_eq!(
        exchange(&mut old, |socket| primary.ship(&mut next_export, socket)),
        (Err(Error::Limit), Err(Error::Limit))
    );
    verify(&old, capped_ack, value);
    drop(old);
    assert_eq!(
        inventory(&old_path),
        before,
        "capped exchanges changed a follower file"
    );

    let mut old = Follower::open(&old_path, enrollment, Limits::default()).unwrap();
    assert_eq!(old.cleanup_inactive(), Ok(1));
    assert_eq!(old.cleanup_inactive(), Ok(0));
    assert_eq!(old.status().unwrap(), status);
    drop(old);
    let mut expected = before.clone();
    expected.remove(&inactive).unwrap();
    assert_eq!(inventory(&old_path), expected);

    let mut replacement =
        Follower::create(&replacement_path, enrollment, Limits::default()).unwrap();
    assert_eq!(
        exchange(&mut replacement, |socket| primary
            .ship(&mut capped_export, socket)),
        (Ok(capped_ack), Ok(()))
    );
    verify(&replacement, capped_ack, value);
    drop(replacement);
    assert_eq!(
        fs::read(replacement_path.join("replica-0.wdb")).unwrap(),
        before[&selected]
    );
    let mut replacement = Follower::open(&replacement_path, enrollment, Limits::default()).unwrap();
    let (next_ack, served) = exchange(&mut replacement, |socket| {
        primary.ship(&mut next_export, socket)
    });
    served.unwrap();
    let next_ack = next_ack.unwrap();
    assert_eq!(next_ack.revision, next_export.revision());
    assert_eq!(next_ack.digest, next_export.descriptor().digest);
    drop(replacement);
    let mut replacement = Follower::open(&replacement_path, enrollment, Limits::default()).unwrap();
    assert_eq!(
        exchange(&mut replacement, |socket| primary.poll(socket)),
        (Ok(Some(next_ack)), Ok(()))
    );
    let new_status = replacement.status().unwrap();
    assert!(!new_status.maintenance_required);
    verify(&replacement, next_ack, value + 1);
    drop(replacement);
    let old = Follower::open(&old_path, enrollment, Limits::default()).unwrap();
    verify(&old, capped_ack, value);
    drop(old);
    assert_eq!(
        inventory(&old_path),
        expected,
        "reseeding modified the capped follower"
    );
    let new_files = inventory(&replacement_path);
    let current = PathBuf::from(format!("replica-{}.wdb", new_status.selected_slot.unwrap()));
    assert_eq!(Sha256::digest(&new_files[&current])[..], next_ack.digest);

    println!(
        "manifest_rotation installs={installs} limit={MANIFEST_LIMIT_BYTES} old_bytes={} old_revision={} old_slot={slot} old_digest={:x} new_bytes={} new_revision={} new_slot={} new_digest={:x} retained=2",
        status.manifest_bytes,
        capped_ack.revision,
        Sha256::digest(&before[&selected]),
        new_status.manifest_bytes,
        next_ack.revision,
        new_status.selected_slot.unwrap(),
        Sha256::digest(&new_files[&current]),
    );
    if let Some(path) = std::env::var_os("WEEKEND_MANIFEST_ROTATION_EVIDENCE") {
        let output = PathBuf::from(path);
        fs::create_dir(&output).unwrap();
        for (label, entries) in [
            ("before", before),
            ("old", expected),
            ("replacement", new_files),
        ] {
            let destination = output.join(label);
            fs::create_dir(&destination).unwrap();
            for (name, bytes) in entries {
                fs::write(destination.join(name), bytes).unwrap();
            }
        }
    }
}
