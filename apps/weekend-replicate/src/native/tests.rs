use super::*;
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc,
    time::Instant,
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "weekend-replicate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
        }
        #[cfg(not(unix))]
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(dir: &Directory) -> PathBuf {
    let path = dir.0.join("source.wdb");
    let db = Database::create_file(&path, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"key", &db.unsigned(1).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    tx.put(c, b"key", &db.unsigned(2).unwrap()).unwrap();
    tx.commit().unwrap();
    path
}
fn enrollment_of(prepared: &Prepared) -> Enrollment {
    prepared.primary.enrollment()
}
fn follower(dir: &Directory, e: Enrollment) -> Follower {
    fs::create_dir(dir.0.join("follower")).unwrap();
    Follower::create(dir.0.join("follower"), e, Limits::default()).unwrap()
}
fn pair() -> (Tls, Tls) {
    let a = Credentials::generate().unwrap();
    let b = Credentials::generate().unwrap();
    (
        Tls::new(&a, b.certificate()).unwrap(),
        Tls::new(&b, a.certificate()).unwrap(),
    )
}
fn exchange<T>(
    follower: &mut Follower,
    server: &Tls,
    action: impl FnOnce(SocketAddr) -> AppResult<T>,
) -> (AppResult<T>, Result<(), SessionError>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::scope(|scope| {
        let receiver = scope.spawn(|| serve_one(follower, server, listener.accept().unwrap().0));
        let sent = action(address);
        (sent, receiver.join().unwrap())
    })
}
fn check(f: &Follower, ack: DurableAck, value: u64) {
    assert_eq!(f.progress().unwrap(), Some(ack));
    let r = f.replica().unwrap();
    assert_eq!(r.snapshot().revision(), ack.revision);
    assert_eq!(
        r.snapshot()
            .get(r.collection_id(1), b"key")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(value)
    );
    assert_eq!(r.retained_ids().unwrap(), [1]);
    assert_eq!(r.retained_snapshot(1).unwrap().revision(), 2);
    assert_eq!(
        r.retained_snapshot(1)
            .unwrap()
            .get(r.collection_id(1), b"key")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(1)
    );
}
#[test]
fn authorized_install_reopen_poll_and_retry_release_primary_lease() {
    let dir = Directory::new();
    let path = source(&dir);
    let mut prepared = prepare(&path, [1; 16], &dir.0.join("export.wdb")).unwrap();
    let e = enrollment_of(&prepared);
    assert_eq!(prepared.export.revision(), 4);
    // The network may run while a new writer holds and advances the primary.
    let db = Database::open_file(&path, Limits::default()).unwrap();
    let mut tx = db.begin();
    tx.put(db.collection_id(1), b"key", &db.unsigned(3).unwrap())
        .unwrap();
    assert_eq!(tx.commit(), Ok(5));
    drop(tx);
    assert_eq!(
        db.snapshot()
            .get(db.collection_id(1), b"key")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(3)
    );
    let (mut f, (client, server)) = (follower(&dir, e), pair());
    let (sent, served) = exchange(&mut f, &server, |address| {
        ship(&mut prepared, &client, address, 1)
    });
    served.unwrap();
    let ack = sent.unwrap();
    assert_eq!(ack.revision, 4);
    check(&f, ack, 2);
    drop(f);
    drop(db);
    let mut f = Follower::open(dir.0.join("follower"), e, Limits::default()).unwrap();
    check(&f, ack, 2);
    let before = f.status().unwrap().manifest_bytes;
    let (sent, served) = exchange(&mut f, &server, |address| {
        ship(&mut prepared, &client, address, 1)
    });
    assert_eq!(sent.unwrap(), ack);
    served.unwrap();
    assert_eq!(f.status().unwrap().manifest_bytes, before);
    let db = Database::open_file(&path, Limits::default()).unwrap();
    let mut restarted = Primary::new(&db, e.stream_id).unwrap();
    drop(db);
    let (sent, served) = exchange(&mut f, &server, |address| {
        Ok(restarted.poll(client.connect(address)?)?.unwrap())
    });
    assert_eq!(sent.unwrap(), ack);
    served.unwrap();
    check(&f, ack, 2);
}

struct StallUpload<S> {
    stream: S,
    received_ready: bool,
    follower_finished: mpsc::Receiver<()>,
}

impl<S: Read> Read for StallUpload<S> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.stream.read(bytes)?;
        self.received_ready |= count != 0;
        Ok(count)
    }
}

impl<S: Write> Write for StallUpload<S> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.received_ready {
            return self.stream.write(bytes);
        }
        // Complete one region through real TLS, then keep the connection open
        // while the follower waits for the next region.
        assert_eq!(bytes.len(), weekend_db::storage::REGION);
        self.stream.write_all(bytes)?;
        self.stream.flush()?;
        self.follower_finished
            .recv_timeout(Duration::from_secs(10))
            .expect("stalled follower did not finish");
        Err(io::ErrorKind::TimedOut.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

#[test]
fn stalled_authenticated_upload_preserves_the_previous_receipt() {
    let dir = Directory::new();
    let path = source(&dir);
    let initial = dir.0.join("initial");
    let mut prepared = prepare(&path, [1; 16], &initial).unwrap();
    let e = enrollment_of(&prepared);
    let (mut f, (client, server)) = (follower(&dir, e), pair());
    let (sent, served) = exchange(&mut f, &server, |address| {
        ship(&mut prepared, &client, address, 1)
    });
    served.unwrap();
    let old_ack = sent.unwrap();
    assert_eq!(old_ack.revision, 4);
    check(&f, old_ack, 2);
    drop(prepared);
    drop(f);
    let folder = dir.0.join("follower");
    let manifest = folder.join("replica-manifest.wdb");
    let old_bytes = fs::read(&manifest).unwrap();

    let db = Database::open_file(&path, Limits::default()).unwrap();
    let mut tx = db.begin();
    tx.put(db.collection_id(1), b"key", &db.unsigned(3).unwrap())
        .unwrap();
    assert_eq!(tx.commit(), Ok(5));
    drop(tx);
    drop(db);
    let image = dir.0.join("next");
    let mut prepared = prepare(&path, e.stream_id, &image).unwrap();
    let descriptor = prepared.export.descriptor();
    let mut f = Follower::open(&folder, e, Limits::default()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (finished, wait) = mpsc::channel();
    thread::scope(|scope| {
        let receiver = scope.spawn(|| {
            let mut channel = server.accept(listener.accept().unwrap().0).unwrap();
            let start = Instant::now();
            channel.set_test_deadline(start + Duration::from_secs(2));
            let result = f.serve_authenticated(channel);
            let elapsed = start.elapsed();
            finished.send(()).unwrap();
            assert_eq!(result, Err(Error::Io));
            assert!(elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(8));
        });
        let stalled = StallUpload {
            stream: client.connect(address).unwrap(),
            received_ready: false,
            follower_finished: wait,
        };
        assert_eq!(
            prepared.primary.ship(&mut prepared.export, stalled),
            Err(Error::Io)
        );
        receiver.join().unwrap();
    });
    check(&f, old_ack, 2);
    drop(f);
    assert_eq!(fs::read(&manifest).unwrap(), old_bytes);
    assert_eq!(
        fs::read(folder.join("replica-0.wdb")).unwrap(),
        fs::read(initial).unwrap()
    );
    assert_eq!(
        fs::metadata(folder.join("replica-1.wdb")).unwrap().len(),
        weekend_db::storage::REGION as u64
    );

    let mut f = Follower::open(&folder, e, Limits::default()).unwrap();
    let (polled, served) = exchange(&mut f, &server, |address| {
        Ok(prepared.primary.poll(client.connect(address)?)?.unwrap())
    });
    served.unwrap();
    assert_eq!(polled.unwrap(), old_ack);
    let (retried, served) = exchange(&mut f, &server, |address| {
        ship(&mut prepared, &client, address, 1)
    });
    served.unwrap();
    let installed = retried.unwrap();
    assert_eq!(installed.revision, 5);
    assert_eq!(installed.digest, descriptor.digest);
    let manifest_size = f.status().unwrap().manifest_bytes;
    let (repeated, served) = exchange(&mut f, &server, |address| {
        ship(&mut prepared, &client, address, 1)
    });
    served.unwrap();
    assert_eq!(repeated.unwrap(), installed);
    assert_eq!(f.status().unwrap().manifest_bytes, manifest_size);
    drop(prepared);
    drop(f);
    assert_eq!(
        fs::read(folder.join("replica-1.wdb")).unwrap(),
        fs::read(image).unwrap()
    );
    let f = Follower::open(&folder, e, Limits::default()).unwrap();
    check(&f, installed, 3);
}

#[test]
fn untrusted_leaf_and_wrong_or_absent_alpn_never_reach_core() {
    let a = Credentials::generate().unwrap();
    let b = Credentials::generate().unwrap();
    let unknown = Credentials::generate().unwrap();
    let server = Tls::new(&b, a.certificate()).unwrap();
    for mode in 0..4 {
        let dir = Directory::new();
        let path = source(&dir);
        let mut prepared = prepare(&path, [1; 16], &dir.0.join("export")).unwrap();
        let e = enrollment_of(&prepared);
        let f = follower(&dir, e);
        let mut client = match mode {
            0 => Tls::new(&unknown, b.certificate()).unwrap(),
            1 => Tls::new(&a, unknown.certificate()).unwrap(),
            _ => Tls::new(&a, b.certificate()).unwrap(),
        };
        if mode == 2 {
            client.test_alpn(vec![]);
        }
        if mode == 3 {
            client.test_alpn(vec![b"other-protocol".to_vec()]);
        }
        // Native Windows leases deny external reads while the follower is open.
        drop(f);
        let manifest = fs::read(dir.0.join("follower/replica-manifest.wdb")).unwrap();
        let mut f = Follower::open(dir.0.join("follower"), e, Limits::default()).unwrap();
        let (sent, served) = exchange(&mut f, &server, |address| {
            ship(&mut prepared, &client, address, 1)
        });
        assert!(sent.is_err());
        assert!(matches!(served, Err(SessionError::Authentication(_))));
        assert_eq!(f.progress().unwrap(), None);
        drop(f);
        assert_eq!(
            fs::read(dir.0.join("follower/replica-manifest.wdb")).unwrap(),
            manifest
        );
        assert_eq!(fs::read_dir(dir.0.join("follower")).unwrap().count(), 1);
        let reopened = Follower::open(dir.0.join("follower"), e, Limits::default()).unwrap();
        assert_eq!(reopened.progress().unwrap(), None);
    }
}
#[test]
fn authenticated_enrollment_mismatch_preserves_follower() {
    for wrong_stream in [true, false] {
        let dir = Directory::new();
        let path = source(&dir);
        let mut prepared = prepare(&path, [1; 16], &dir.0.join("export")).unwrap();
        let mut e = enrollment_of(&prepared);
        if wrong_stream {
            e.stream_id[0] ^= 1;
        } else {
            e.file_identity[0] ^= 1;
        }
        let f = follower(&dir, e);
        let (client, server) = pair();
        // Native Windows leases deny external reads while the follower is open.
        drop(f);
        let manifest = fs::read(dir.0.join("follower/replica-manifest.wdb")).unwrap();
        let mut f = Follower::open(dir.0.join("follower"), e, Limits::default()).unwrap();
        let (sent, served) = exchange(&mut f, &server, |address| {
            ship(&mut prepared, &client, address, 1)
        });
        assert!(sent.is_err());
        assert!(matches!(
            served,
            Err(SessionError::Replication(Error::ForeignStore))
        ));
        // The rejection reply communicates ForeignStore on this valid TLS link.
        assert_eq!(
            sent.unwrap_err().downcast_ref::<Error>(),
            Some(&Error::ForeignStore)
        );
        assert_eq!(f.progress().unwrap(), None);
        drop(f);
        assert_eq!(
            fs::read(dir.0.join("follower/replica-manifest.wdb")).unwrap(),
            manifest
        );
        assert_eq!(fs::read_dir(dir.0.join("follower")).unwrap().count(), 1);
        let reopened = Follower::open(dir.0.join("follower"), e, Limits::default()).unwrap();
        assert_eq!(reopened.progress().unwrap(), None);
    }
}

struct LoseTerminal<S> {
    stream: S,
    ready: bool,
}
impl<S: Read> Read for LoseTerminal<S> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.stream.read(b)
    }
}
impl<S: Write> Write for LoseTerminal<S> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.ready {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            let n = self.stream.write(b)?;
            self.ready = n != 0;
            Ok(n)
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
#[test]
fn lost_terminal_reply_reopens_exact_receipt_and_retries_same_export() {
    let dir = Directory::new();
    let path = source(&dir);
    let image = dir.0.join("export");
    let record = dir.0.join("descriptor");
    let mut prepared = prepare(&path, [1; 16], &image).unwrap();
    let descriptor = prepared.export.descriptor();
    capture_record::store(&record, descriptor).unwrap();
    let e = enrollment_of(&prepared);
    let mut f = follower(&dir, e);
    let (client, server) = pair();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, recovered) = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let channel = server.accept(listener.accept().unwrap().0).unwrap();
            assert_eq!(
                f.serve_authenticated(LoseTerminal {
                    stream: channel,
                    ready: false
                }),
                Err(Error::Io)
            );
            let before = f.progress().unwrap().unwrap();
            drop(f);
            let mut f = Follower::open(dir.0.join("follower"), e, Limits::default()).unwrap();
            check(&f, before, 2);
            let bytes = f.status().unwrap().manifest_bytes;
            serve_one(&mut f, &server, listener.accept().unwrap().0).unwrap();
            assert_eq!(f.status().unwrap().manifest_bytes, bytes);
            check(&f, before, 2);
            before
        });
        assert!(ship(&mut prepared, &client, address, 1).is_err());
        drop(prepared);
        let db = Database::open_file(&path, Limits::default()).unwrap();
        assert_eq!(db.begin().commit(), Ok(5));
        drop(db);
        let mut restarted = prepare_saved(&path, e.stream_id, &image, &record).unwrap();
        assert_eq!(restarted.export.descriptor(), descriptor);
        (
            ship(&mut restarted, &client, address, 1),
            worker.join().unwrap(),
        )
    });
    assert_eq!(sent.unwrap(), recovered);
    assert_eq!(recovered.revision, 4);
    assert_eq!(recovered.digest, descriptor.digest);
}

#[test]
fn structured_connect_refusal_after_lost_reply_does_not_deny_a_durable_install() {
    let dir = Directory::new();
    let path = source(&dir);
    let image = dir.0.join("export");
    let mut prepared = prepare(&path, [1; 16], &image).unwrap();
    let descriptor = prepared.export.descriptor();
    let enrollment = enrollment_of(&prepared);
    let mut receiver = follower(&dir, enrollment);
    let folder = dir.0.join("follower");
    let (client, server) = pair();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let settings = diagnostics::settings_from_args(
        diagnostics::App::Snapshot,
        &mut vec!["--diagnostics=json".into(), "ship".into()],
    );
    let (sent, diagnostic, recovered) = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let channel = server.accept(listener.accept().unwrap().0).unwrap();
            drop(listener);
            assert_eq!(
                receiver.serve_authenticated(LoseTerminal {
                    stream: channel,
                    ready: false
                }),
                Err(Error::Io)
            );
            let ack = receiver.progress().unwrap().unwrap();
            drop(receiver);
            let reopened = Follower::open(&folder, enrollment, Limits::default()).unwrap();
            check(&reopened, ack, 2);
            ack
        });
        let (sent, diagnostic) = diagnostics::capture(settings, Phase::Replication, || {
            ship(&mut prepared, &client, address, 2)
        });
        (sent, diagnostic.unwrap(), worker.join().unwrap())
    });
    assert_eq!(
        sent.as_ref()
            .err()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::ConnectionRefused
    );
    let json = diagnostic.json(settings).to_string();
    assert!(
        json.contains("\"phase\":\"connect\",\"category\":\"io\",\"code\":\"connection_refused\"")
    );
    assert!(json.contains("\"follower_outcome\":\"unconfirmed\""));
    assert!(json.contains("\"guidance\":\"poll_or_reconcile_before_retry\""));
    assert_eq!(recovered.revision, 4);
    assert_eq!(recovered.digest, descriptor.digest);
    assert!(image.is_file());
    let mut receiver = Follower::open(&folder, enrollment, Limits::default()).unwrap();
    let manifest_bytes = receiver.status().unwrap().manifest_bytes;
    let (polled, served) = exchange(&mut receiver, &server, |address| {
        Ok(prepared.primary.poll(client.connect(address)?)?.unwrap())
    });
    served.unwrap();
    assert_eq!(polled.unwrap(), recovered);
    let (retried, served) = exchange(&mut receiver, &server, |address| {
        ship(&mut prepared, &client, address, 1)
    });
    served.unwrap();
    assert_eq!(retried.unwrap(), recovered);
    assert_eq!(receiver.status().unwrap().manifest_bytes, manifest_bytes);
    check(&receiver, recovered, 2);
}
#[test]
fn credential_bounds_and_fixture_loading() {
    let dir = Directory::new();
    let a = Credentials::generate().unwrap();
    let b = Credentials::generate().unwrap();
    a.write_fixture(&dir.0.join("a"));
    fs::write(dir.0.join("b.der"), b.certificate()).unwrap();
    assert!(transport(dir.0.join("a").as_os_str(), dir.0.join("b.der").as_os_str()).is_ok());
    fs::write(dir.0.join("cap"), vec![0; 65536]).unwrap();
    assert_eq!(
        bounded_file(&dir.0.join("cap"), 65536).unwrap().len(),
        65536
    );
    fs::write(dir.0.join("cap"), vec![0; 65537]).unwrap();
    assert_eq!(
        bounded_file(&dir.0.join("cap"), 65536)
            .err()
            .unwrap()
            .to_string(),
        "credential size outside bounds"
    );
    fs::write(dir.0.join("b.der"), vec![0; 65537]).unwrap();
    assert!(transport(dir.0.join("a").as_os_str(), dir.0.join("b.der").as_os_str()).is_err());
    fs::write(dir.0.join("a/key.der"), vec![0; 16385]).unwrap();
    assert!(Credentials::load(&dir.0.join("a")).is_err());
    assert!(identity(OsStr::new("00000000000000000000000000000000")).is_err());
    assert!(identity(OsStr::new("x")).is_err());
}

#[cfg(unix)]
#[test]
fn credential_fifo_rejects_before_open_and_keygen_uses_private_modes() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let dir = Directory::new();
    let fifo = dir.0.join("fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert!(bounded_file(&fifo, 65536).is_err());
    let credentials = dir.0.join("credentials");
    Credentials::create(&credentials).unwrap();
    assert_eq!(
        fs::metadata(&credentials).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(credentials.join("key.der"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let key = fs::read(credentials.join("key.der")).unwrap();
    assert!(Credentials::create(&credentials).is_err());
    assert!(fs::read(credentials.join("key.der")).unwrap() == key);
    fs::set_permissions(
        credentials.join("key.der"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(Credentials::load(&credentials).is_err());
    fs::set_permissions(
        credentials.join("key.der"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(Credentials::load(&credentials).is_ok());
}

#[test]
fn capture_record_has_exact_bytes_and_rejects_incomplete_or_malformed_rows() {
    use weekend_db::replication::ExportDescriptor;

    let dir = Directory::new();
    let path = dir.0.join("capture-record.wdb");
    let descriptor = ExportDescriptor {
        enrollment: Enrollment {
            stream_id: std::array::from_fn(|i| i as u8),
            file_identity: std::array::from_fn(|i| 16 + i as u8),
        },
        revision: 0x2726_2524_2322_2120,
        length: 0x2f2e_2d2c_2b2a_2928,
        digest: std::array::from_fn(|i| 48 + i as u8),
    };
    let mut golden = b"WDBCAP01".to_vec();
    golden.extend(0..80);
    let key = b"snapshot-capture";
    assert_eq!(capture_record::load(&path), Err(Error::NotFound));
    capture_record::store(&path, descriptor).unwrap();
    let original = fs::read(&path).unwrap();
    assert_eq!(
        capture_record::store(&path, descriptor),
        Err(Error::AlreadyExists)
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    let db = Database::open_file(&path, Limits::default()).unwrap();
    let row = db
        .snapshot()
        .get(db.collection_id(1), key)
        .unwrap()
        .unwrap();
    assert_eq!(row.as_bytes().unwrap(), golden);
    assert_eq!(capture_record::load(&path), Err(Error::Busy));
    drop(db);
    assert_eq!(capture_record::load(&path), Ok(descriptor));

    let path = dir.0.join("partial-record.wdb");
    drop(Database::create_file(&path, Limits::default()).unwrap());
    assert_eq!(capture_record::load(&path), Err(Error::UnsupportedFormat));
    let db = Database::open_file(&path, Limits::default()).unwrap();
    db.create_collection().unwrap();
    drop(db);
    assert_eq!(capture_record::load(&path), Err(Error::UnsupportedFormat));
    let mut wrong_format = golden.clone();
    wrong_format[7] = b'2';
    for bytes in [
        None,
        Some(Vec::new()),
        Some(golden[..87].to_vec()),
        Some([golden.as_slice(), &[0]].concat()),
        Some(wrong_format),
        Some(golden.clone()),
    ] {
        let db = Database::open_file(&path, Limits::default()).unwrap();
        let value = match &bytes {
            None => db.unsigned(7).unwrap(),
            Some(bytes) => db.bytes(bytes).unwrap(),
        };
        let mut tx = db.begin();
        tx.put(db.collection_id(1), key, &value).unwrap();
        tx.commit().unwrap();
        drop(tx);
        drop(db);
        let expected = if bytes.as_ref() == Some(&golden) {
            Ok(descriptor)
        } else {
            Err(Error::Corrupt)
        };
        assert_eq!(capture_record::load(&path), expected);
    }
}

#[test]
fn saved_capture_checks_lineage_revision_length_and_digest_before_delivery() {
    let dir = Directory::new();
    let path = source(&dir);
    let image = dir.0.join("export");
    let stream = [1; 16];
    let prepared = prepare(&path, stream, &image).unwrap();
    let original = prepared.export.descriptor();
    drop(prepared);
    let image_bytes = fs::read(&image).unwrap();
    let primary_bytes = fs::read(&path).unwrap();
    let mut wrong = original;
    wrong.enrollment.stream_id[0] ^= 1;
    let mut foreign_file = original;
    foreign_file.enrollment.file_identity[0] ^= 1;
    let mut digest = original;
    digest.digest[0] ^= 1;
    for (name, descriptor, expected) in [
        ("stream", wrong, Error::ForeignStore),
        ("file", foreign_file, Error::ForeignStore),
        (
            "future",
            weekend_db::replication::ExportDescriptor {
                revision: original.revision + 1,
                ..original
            },
            Error::ForeignStore,
        ),
        (
            "past",
            weekend_db::replication::ExportDescriptor {
                revision: original.revision - 1,
                ..original
            },
            Error::Corrupt,
        ),
        (
            "length",
            weekend_db::replication::ExportDescriptor {
                length: 0,
                ..original
            },
            Error::Limit,
        ),
        ("digest", digest, Error::Corrupt),
    ] {
        let record = dir.0.join(name);
        capture_record::store(&record, descriptor).unwrap();
        let before = fs::read(&record).unwrap();
        let error = prepare_saved(&path, stream, &image, &record)
            .err()
            .expect(name);
        assert_eq!(error.downcast_ref::<Error>(), Some(&expected), "{name}");
        assert_eq!(fs::read(&record).unwrap(), before);
    }
    let record = dir.0.join("valid");
    capture_record::store(&record, original).unwrap();
    let reopened = prepare_saved(&path, stream, &image, &record).unwrap();
    assert_eq!(reopened.export.descriptor(), original);
    drop(reopened);
    assert_eq!(fs::read(&image).unwrap(), image_bytes);
    assert_eq!(fs::read(&path).unwrap(), primary_bytes);
}

struct FailingFlush {
    bytes: Vec<u8>,
    remaining_flushes: usize,
}
impl Write for FailingFlush {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.remaining_flushes == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.remaining_flushes -= 1;
        Ok(())
    }
}

#[test]
fn capture_output_and_durable_acknowledgment_fail_independently() {
    let dir = Directory::new();
    let path = source(&dir);
    let image = dir.0.join("export");
    let stream = [1; 16];
    let prepared = prepare(&path, stream, &image).unwrap();
    let descriptor = prepared.export.descriptor();
    let enrollment = descriptor.enrollment;
    let local = format!(
        "local captured_revision={} stream={} file_identity={}\n",
        descriptor.revision,
        hex(&enrollment.stream_id),
        hex(&enrollment.file_identity)
    );
    let (client, server) = pair();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut output = FailingFlush {
        bytes: Vec::new(),
        remaining_flushes: 0,
    };
    let error = deliver(
        prepared,
        &client,
        listener.local_addr().unwrap(),
        1,
        &mut output,
        "",
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(output.bytes, local.as_bytes());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );

    let record = dir.0.join("descriptor");
    capture_record::store(&record, descriptor).unwrap();
    let prepared = prepare_saved(&path, stream, &image, &record).unwrap();
    let mut receiver = follower(&dir, enrollment);
    let mut output = vec![0; local.len()];
    let settings = diagnostics::settings_from_args(
        diagnostics::App::Snapshot,
        &mut vec!["--diagnostics=json".into(), "ship-captured".into()],
    );
    let mut diagnostic = None;
    let (sent, served) = exchange(&mut receiver, &server, |address| {
        let (result, detail) = diagnostics::capture(settings, Phase::Arguments, || {
            deliver(
                prepared,
                &client,
                address,
                1,
                &mut output.as_mut_slice(),
                "",
            )
        });
        diagnostic = detail;
        result
    });
    served.unwrap();
    assert_eq!(
        sent.unwrap_err()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::WriteZero
    );
    assert_eq!(output, local.as_bytes());
    let json = diagnostic.unwrap().json(settings).to_string();
    assert!(json.contains(r#""phase":"local_command","category":"io""#));
    assert!(json.contains(r#""follower_outcome":"unconfirmed""#));
    let ack = DurableAck {
        stream_id: stream,
        revision: descriptor.revision,
        digest: descriptor.digest,
    };
    let manifest = receiver.status().unwrap().manifest_bytes;
    drop(receiver);

    let mut reopened =
        Follower::open(dir.0.join("follower"), enrollment, Limits::default()).unwrap();
    check(&reopened, ack, 2);
    let prepared = prepare_saved(&path, stream, &image, &record).unwrap();
    let mut output = FailingFlush {
        bytes: Vec::new(),
        remaining_flushes: 1,
    };
    let (retried, served) = exchange(&mut reopened, &server, |address| {
        deliver(prepared, &client, address, 1, &mut output, "")
    });
    served.unwrap();
    assert_eq!(
        retried
            .unwrap_err()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        output.bytes,
        format!("{local}{}\n", ack_line(Some(ack))).as_bytes()
    );

    let mut prepared = prepare_saved(&path, stream, &image, &record).unwrap();
    let (polled, served) = exchange(&mut reopened, &server, |address| {
        Ok(prepared.primary.poll(client.connect(address)?)?)
    });
    served.unwrap();
    assert_eq!(polled.unwrap(), Some(ack));
    assert_eq!(reopened.status().unwrap().manifest_bytes, manifest);
}

#[test]
fn poll_output_preserves_local_revision_and_follower_receipt() {
    let dir = Directory::new();
    let path = source(&dir);
    let stream = [1; 16];
    let mut prepared = prepare(&path, stream, &dir.0.join("export")).unwrap();
    let enrollment = prepared.primary.enrollment();
    let (client, server) = pair();
    let mut receiver = follower(&dir, enrollment);
    let (sent, served) = exchange(&mut receiver, &server, |address| {
        ship(&mut prepared, &client, address, 1)
    });
    let receipt = sent.unwrap();
    served.unwrap();
    assert_eq!(receipt.revision, 4);
    drop(receiver);
    let follower_path = dir.0.join("follower");
    let manifest = follower_path.join("replica-manifest.wdb");
    let original = fs::read(&manifest).unwrap();
    let mut receiver = Follower::open(&follower_path, enrollment, Limits::default()).unwrap();

    let db = Database::open_file(&path, Limits::default()).unwrap();
    let mut tx = db.begin();
    tx.put(db.collection_id(1), b"key", &db.unsigned(3).unwrap())
        .unwrap();
    assert_eq!(tx.commit(), Ok(5));
    drop(tx);
    let revision = db.snapshot().revision();
    let mut primary = Primary::new(&db, stream).unwrap();
    drop(db);
    let local = "local verified_revision=5\n";
    let reports = format!("{local}{}\n", ack_line(Some(receipt)));
    let settings = diagnostics::settings_from_args(
        diagnostics::App::Snapshot,
        &mut vec!["--diagnostics=json".into(), "poll".into()],
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut output = FailingFlush {
        bytes: Vec::new(),
        remaining_flushes: 0,
    };
    let (failed, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
        poll(
            &mut primary,
            &client,
            listener.local_addr().unwrap(),
            revision,
            &mut output,
        )
    });
    let error = failed.unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().map(io::Error::kind),
        Some(io::ErrorKind::BrokenPipe)
    );
    assert!(
        diagnostic
            .unwrap()
            .json(settings)
            .to_string()
            .contains(r#""phase":"local_command","category":"io","code":"broken_pipe""#)
    );
    assert_eq!(output.bytes, local.as_bytes());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );

    let mut output = FailingFlush {
        bytes: Vec::new(),
        remaining_flushes: 1,
    };
    let (observed, served) = exchange(&mut receiver, &server, |address| {
        poll(&mut primary, &client, address, revision, &mut output)
    });
    served.unwrap();
    assert_eq!(
        observed
            .unwrap_err()
            .downcast_ref::<io::Error>()
            .map(io::Error::kind),
        Some(io::ErrorKind::BrokenPipe)
    );
    assert_eq!(output.bytes, reports.as_bytes());

    let mut output = Vec::new();
    let (observed, served) = exchange(&mut receiver, &server, |address| {
        poll(&mut primary, &client, address, revision, &mut output)
    });
    served.unwrap();
    observed.unwrap();
    assert_eq!(output, reports.as_bytes());
    check(&receiver, receipt, 2);
    drop(receiver);
    assert_eq!(fs::read(&manifest).unwrap(), original);
}

#[test]
fn export_preparation_reports_the_first_error_and_preserves_existing_files() {
    let dir = Directory::new();
    let primary = source(&dir);
    let missing = dir.0.join("absent");
    let image = dir.0.join("image");
    let record = dir.0.join("record");
    let existing = dir.0.join("existing");
    fs::write(&existing, b"preserve this file").unwrap();

    let settings = diagnostics::settings_from_args(
        diagnostics::App::Snapshot,
        &mut vec!["--diagnostics=json".into(), "capture".into()],
    );
    let observe = |result: AppResult<Prepared>,
                   diagnostic: Option<diagnostics::Diagnostic>,
                   expected: Error,
                   phase: &str| {
        let error = result.err().expect("preparation passed unexpectedly");
        assert_eq!(error.downcast_ref::<Error>(), Some(&expected));
        let json = diagnostic.unwrap().json(settings).to_string();
        assert!(json.contains(&format!("\"phase\":\"{phase}\"")), "{json}");
    };

    let stream = [1; 16];
    for (source, stream, output, expected, phase) in [
        (&missing, stream, &image, Error::NotFound, "storage_open"),
        (&primary, [0; 16], &image, Error::Invalid, "replication"),
        (&primary, stream, &existing, Error::AlreadyExists, "export"),
    ] {
        let (result, detail) = diagnostics::capture(settings, Phase::Arguments, || {
            prepare(source, stream, output)
        });
        observe(result, detail, expected, phase);
        assert!(!image.exists());
    }
    assert_eq!(fs::read(&existing).unwrap(), b"preserve this file");

    let prepared = prepare(&primary, stream, &image).unwrap();
    let descriptor = prepared.export.descriptor();
    capture_record::store(&record, descriptor).unwrap();
    drop(prepared);
    let before = [&primary, &image, &record].map(|path| fs::read(path).unwrap());
    let corrupt = dir.0.join("corrupt-record");
    fs::write(&corrupt, b"not a database").unwrap();
    for (source, stream, export, descriptor_path, expected, phase) in [
        (
            &missing,
            stream,
            &image,
            &corrupt,
            Error::Corrupt,
            "storage_open",
        ),
        (
            &missing,
            stream,
            &image,
            &record,
            Error::NotFound,
            "storage_open",
        ),
        (
            &primary,
            [0; 16],
            &image,
            &record,
            Error::Invalid,
            "replication",
        ),
        (
            &primary,
            stream,
            &missing,
            &record,
            Error::NotFound,
            "storage_verify",
        ),
    ] {
        let (result, detail) = diagnostics::capture(settings, Phase::Arguments, || {
            prepare_saved(source, stream, export, descriptor_path)
        });
        observe(result, detail, expected, phase);
        assert_eq!(
            [&primary, &image, &record].map(|path| fs::read(path).unwrap()),
            before
        );
    }
    assert_eq!(
        prepare_saved(&primary, stream, &image, &record)
            .unwrap()
            .export
            .descriptor(),
        descriptor
    );
}

#[test]
fn retry_preserves_terminal_errors_and_tolerates_a_closed_notice_pipe() {
    let ack = DurableAck {
        revision: 4,
        stream_id: [1; 16],
        digest: [2; 32],
    };
    let permanent_failures = [
        Error::ForeignStore,
        Error::UnsupportedFormat,
        Error::Conflict,
        Error::Invalid,
        Error::Corrupt,
        Error::Limit,
        Error::RecoveryRequired,
    ];
    let cases = permanent_failures
        .into_iter()
        .map(|error| (2, error, 1, Err(error)))
        .chain([
            (1, Error::Io, 1, Err(Error::Io)),
            (2, Error::Io, 2, Ok(ack)),
        ]);
    for (attempts, failure, calls, expected) in cases {
        let mut count = 0;
        let mut report = Vec::new();
        let actual = retry_ship(attempts, &mut report, || {
            count += 1;
            if count == 1 {
                Err(failure.into())
            } else {
                Ok(ack)
            }
        });
        assert_eq!(
            actual.map_err(|error| *error.downcast::<Error>().unwrap()),
            expected
        );
        assert_eq!(count, calls);
        if calls == 2 {
            assert_eq!(
                report,
                b"exchange unconfirmed; reconnecting with the same export (attempt 2)\n"
            );
        } else {
            assert!(report.is_empty());
        }
    }

    let mut report = Vec::new();
    let error = retry_ship(0, &mut report, || {
        panic!("zero attempts reached the exchange")
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "no transfer attempt configured");
    assert!(report.is_empty());

    let (reader, mut writer) = io::pipe().unwrap();
    drop(reader);
    let mut count = 0;
    let observed = retry_ship(2, &mut writer, || {
        count += 1;
        if count == 1 {
            Err(io::Error::from(io::ErrorKind::ConnectionReset).into())
        } else {
            Ok(ack)
        }
    });
    assert_eq!(observed.unwrap(), ack);
    assert_eq!(count, 2);
}

#[test]
fn interrupted_authenticated_exchange_retries_the_same_export() {
    use sha2::{Digest, Sha256};

    let dir = Directory::new();
    let source = source(&dir);
    let image = dir.0.join("retry-export.wdb");
    let mut prepared = prepare(&source, [1; 16], &image).unwrap();
    let descriptor = prepared.export.descriptor();
    let mut follower = follower(&dir, descriptor.enrollment);
    let (client, server) = pair();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let accept = || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "snapshot retry did not connect");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("fixture accept failed: {error}"),
            }
        }
    };

    let ack = thread::scope(|scope| {
        let receiver = scope.spawn(|| {
            let mut interrupted = server.accept(accept()).unwrap();
            interrupted.read_exact(&mut [0]).unwrap();
            drop(interrupted);
            serve_one(&mut follower, &server, accept()).unwrap();
        });
        let result = ship(&mut prepared, &client, address, 2);
        receiver.join().unwrap();
        result.unwrap()
    });
    assert_eq!(ack.revision, descriptor.revision);
    assert_eq!(ack.stream_id, descriptor.enrollment.stream_id);
    assert_eq!(ack.digest, descriptor.digest);
    assert_eq!(prepared.export.descriptor(), descriptor);
    drop(prepared);
    assert_eq!(
        Into::<[u8; 32]>::into(Sha256::digest(fs::read(image).unwrap())),
        ack.digest
    );
    check(&follower, ack, 2);
    drop(follower);
    let reopened = Follower::open(
        dir.0.join("follower"),
        descriptor.enrollment,
        Limits::default(),
    )
    .unwrap();
    check(&reopened, ack, 2);
}

#[test]
fn listener_setup_failure_preserves_the_receipt_and_closes_queued_clients() {
    use std::cell::Cell;

    let settings = diagnostics::settings_from_args(
        diagnostics::App::Snapshot,
        &mut vec!["--diagnostics=json".into(), "serve".into()],
    );
    for (mode_fails, kind, code, detail) in [
        (
            true,
            io::ErrorKind::PermissionDenied,
            "permission_denied",
            "set nonblocking",
        ),
        (
            false,
            io::ErrorKind::InvalidInput,
            "invalid_input",
            "read local address",
        ),
    ] {
        let dir = Directory::new();
        let path = source(&dir);
        let image = dir.0.join("export.wdb");
        let mut prepared = prepare(&path, [1; 16], &image).unwrap();
        let enrollment = prepared.export.descriptor().enrollment;
        let mut follower = follower(&dir, enrollment);
        let (client, tls) = pair();
        let (sent, served) = exchange(&mut follower, &tls, |address| {
            ship(&mut prepared, &client, address, 1)
        });
        let receipt = sent.unwrap();
        served.unwrap();
        let slot = follower.status().unwrap().selected_slot.unwrap();
        drop(follower);

        let follower_path = dir.0.join("follower");
        let manifest = follower_path.join("replica-manifest.wdb");
        let before = fs::read(&manifest).unwrap();
        let image = follower_path.join(format!("replica-{slot}.wdb"));
        let image_before = fs::read(&image).unwrap();
        let follower = Follower::open(&follower_path, enrollment, Limits::default()).unwrap();
        let mut queued = None;
        let queried = Cell::new(false);
        let mut output = Vec::new();
        let (result, diagnostic) = diagnostics::capture(settings, Phase::Arguments, || {
            serve_with(
                follower,
                "127.0.0.1:0".parse().unwrap(),
                &tls,
                1,
                |listener| {
                    let peer = TcpStream::connect(listener.local_addr()?)?;
                    peer.set_read_timeout(Some(Duration::from_secs(2)))?;
                    queued = Some(peer);
                    if mode_fails {
                        Err(io::Error::new(kind, detail))
                    } else {
                        listener.set_nonblocking(true)
                    }
                },
                |_| {
                    queried.set(true);
                    Err(io::Error::new(kind, detail))
                },
                &mut output,
            )
        });

        let error = result.unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(kind)
        );
        assert_eq!(error.to_string(), detail);
        assert!(
            diagnostic
                .unwrap()
                .json(settings)
                .to_string()
                .contains(&format!(
                    r#""phase":"listen","category":"io","code":"{code}""#
                ))
        );
        assert_eq!(queried.get(), !mode_fails);
        assert!(output.is_empty());

        let mut queued = queued.unwrap();
        match queued.read(&mut [0]) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("listener setup left a queued connection open: {other:?}"),
        }
        assert_eq!(fs::read(&manifest).unwrap(), before);
        assert_eq!(fs::read(&image).unwrap(), image_before);
        let reopened = Follower::open(&follower_path, enrollment, Limits::default()).unwrap();
        check(&reopened, receipt, 2);
    }
}

fn installed_follower(dir: &Directory) -> (Follower, Enrollment, DurableAck) {
    let mut prepared = prepare(&source(dir), [1; 16], &dir.0.join("export.wdb")).unwrap();
    let enrollment = enrollment_of(&prepared);
    let (mut follower, (client, server)) = (follower(dir, enrollment), pair());
    let (sent, served) = exchange(&mut follower, &server, |address| {
        ship(&mut prepared, &client, address, 1)
    });
    served.unwrap();
    let ack = sent.unwrap();
    check(&follower, ack, 2);
    (follower, enrollment, ack)
}

#[test]
fn snapshot_identity_errors_preserve_each_argument() {
    use std::ffi::OsString;
    #[cfg(unix)]
    let broken = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xff])
    };
    #[cfg(windows)]
    let broken = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xd800])
    };
    let valid = OsStr::new("aB112233445566778899aabbccddeeff");
    assert_eq!(
        identity(valid).unwrap(),
        [
            0xab, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ]
    );
    let digits = "identity must be 32 hexadecimal digits";
    for (input, expected) in [
        (broken.as_os_str(), "expected UTF-8 argument"),
        (OsStr::new("1"), digits),
        (OsStr::new("+1111111111111111111111111111111"), digits),
        (
            OsStr::new("00000000000000000000000000000000"),
            "identity must be nonzero",
        ),
    ] {
        assert_eq!(identity(input).unwrap_err().to_string(), expected);
        for (stream, file) in [(input, valid), (valid, input)] {
            let error = enrollment(stream, file).expect_err("invalid identity accepted");
            assert_eq!(error.to_string(), expected);
        }
    }
    let e = enrollment(valid, OsStr::new("11111111111111111111111111111111")).unwrap();
    assert_eq!(e.stream_id, identity(valid).unwrap());
    assert_eq!(e.file_identity, [0x11; 16]);
}

#[test]
fn offline_command_refusals_preserve_the_follower() {
    let dir = Directory::new();
    let (follower, enrollment, ack) = installed_follower(&dir);
    let path = dir.0.join("follower");
    let manifest = path.join("replica-manifest.wdb");
    let selected = path.join(format!(
        "replica-{}.wdb",
        follower.status().unwrap().selected_slot.unwrap()
    ));
    drop(follower);
    let original = [&manifest, &selected].map(|file| fs::read(file).unwrap());
    let stream = hex(&enrollment.stream_id);
    let file = hex(&enrollment.file_identity);
    let foreign = hex(&[2; 16]);
    let fail = |verb: &str, path: &Path, ids: &[&str], expected: &str, phase: &str| {
        let mut args = vec!["--diagnostics=json".into(), verb.into(), path.into()];
        args.extend(ids.iter().copied().map(std::ffi::OsString::from));
        let settings = diagnostics::settings_from_args(diagnostics::App::Snapshot, &mut args);
        let (result, detail) = diagnostics::capture(settings, Phase::Arguments, || main(args));
        assert_eq!(result.unwrap_err().to_string(), expected, "{verb}");
        let detail = detail.unwrap().json(settings).to_string();
        assert!(
            detail.contains(&format!(r#""phase":"{phase}""#)),
            "{detail}"
        );
    };
    let corrupt = dir.0.join("corrupt-source.wdb");
    fs::write(&corrupt, b"incomplete image").unwrap();
    fail(
        "describe",
        &corrupt,
        &["1"],
        "identity must be 32 hexadecimal digits",
        "arguments",
    );
    fail(
        "describe",
        &corrupt,
        &[&stream],
        "Corrupt",
        "storage_verify",
    );
    assert_eq!(fs::read(&corrupt).unwrap(), b"incomplete image");

    let zero = "00000000000000000000000000000000";
    for verb in [
        "follower-init",
        "status",
        "cleanup-inactive",
        "compact-manifest",
    ] {
        if cfg!(windows) && verb == "compact-manifest" {
            continue;
        }
        fail(
            verb,
            &path,
            &["1", &file],
            "identity must be 32 hexadecimal digits",
            "arguments",
        );
        fail(
            verb,
            &path,
            &[&stream, zero],
            "identity must be nonzero",
            "arguments",
        );
        if verb == "follower-init" {
            fail(
                verb,
                &path,
                &[&stream, &file],
                "AlreadyExists",
                "storage_create",
            );
        } else {
            fail(
                verb,
                &path,
                &[&foreign, &file],
                "ForeignStore",
                "storage_open",
            );
        }
        assert_eq!(
            [&manifest, &selected].map(|file| fs::read(file).unwrap()),
            original
        );
    }

    #[cfg(unix)]
    {
        let alias = path.join("manifest-alias");
        fs::hard_link(&manifest, &alias).unwrap();
        fail(
            "compact-manifest",
            &path,
            &[&stream, &file],
            "Invalid",
            "storage_maintenance",
        );
        assert_eq!(fs::read(&alias).unwrap(), original[0]);
        fs::remove_file(alias).unwrap();
    }
    assert_eq!(
        [&manifest, &selected].map(|file| fs::read(file).unwrap()),
        original
    );
    let reopened = Follower::open(&path, enrollment, Limits::default()).unwrap();
    check(&reopened, ack, 2);
}

#[cfg(unix)]
fn verify_manifest_status_failure(
    command: &str,
    phase: &str,
    call: impl FnOnce(&mut Vec<u8>) -> AppResult<()>,
) {
    let settings = diagnostics::settings_from_args(
        diagnostics::App::Snapshot,
        &mut vec!["--diagnostics=json".into(), command.into()],
    );
    let mut output = Vec::new();
    let (result, detail) = diagnostics::capture(settings, Phase::Arguments, || call(&mut output));
    assert_eq!(
        result.unwrap_err().downcast_ref::<Error>(),
        Some(&Error::NotFound)
    );
    let detail = detail.unwrap().json(settings).to_string();
    assert!(
        detail.contains(&format!(
            r#""phase":"{phase}","category":"engine","code":"NotFound""#
        )),
        "{detail}"
    );
    assert!(output.is_empty());
}

#[cfg(unix)]
#[test]
fn manifest_status_failures_report_the_operation_phase() {
    use weekend_db::replication::MANIFEST_LIMIT_BYTES;

    let dir = Directory::new();
    let (follower, enrollment, ack) = installed_follower(&dir);
    let path = dir.0.join("follower");
    let manifest = path.join("replica-manifest.wdb");
    let hidden = path.join("temporarily-hidden-manifest");
    let selected = path.join(format!(
        "replica-{}.wdb",
        follower.status().unwrap().selected_slot.unwrap()
    ));
    drop(follower);
    let selected_bytes = fs::read(&selected).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&manifest)
        .unwrap()
        .set_len(MANIFEST_LIMIT_BYTES)
        .unwrap();
    let original = fs::read(&manifest).unwrap();
    let open = || Follower::open(&path, enrollment, Limits::default()).unwrap();

    let follower = open();
    let mut healthy = Vec::new();
    report_follower_status(&follower, &mut healthy).unwrap();
    assert!(
        String::from_utf8(healthy)
            .unwrap()
            .lines()
            .any(|line| line == ack_line(Some(ack)))
    );
    // Move only the private fixture name to force the native metadata error.
    fs::rename(&manifest, &hidden).unwrap();
    verify_manifest_status_failure("status", "storage_verify", |output| {
        report_follower_status(&follower, output)
    });
    verify_manifest_status_failure("compact-manifest", "storage_maintenance", |output| {
        compact_follower_with(
            follower,
            |_| panic!("compaction ran after the initial status failed"),
            output,
        )
    });
    fs::rename(&hidden, &manifest).unwrap();
    assert_eq!(fs::read(&manifest).unwrap(), original);

    verify_manifest_status_failure("compact-manifest", "storage_maintenance", |output| {
        compact_follower_with(
            open(),
            |follower| {
                let replacement = follower.compact_manifest()?;
                fs::rename(&manifest, &hidden).unwrap();
                Ok(replacement)
            },
            output,
        )
    });
    fs::rename(&hidden, &manifest).unwrap();
    assert!(fs::metadata(&manifest).unwrap().len() < MANIFEST_LIMIT_BYTES);
    assert_eq!(fs::read(&selected).unwrap(), selected_bytes);
    check(&open(), ack, 2);
}
