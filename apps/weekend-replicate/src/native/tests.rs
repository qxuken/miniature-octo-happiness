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
fn exchange(
    follower: &mut Follower,
    server: &Tls,
    action: impl FnOnce(SocketAddr) -> AppResult<DurableAck>,
) -> (AppResult<DurableAck>, Result<(), SessionError>) {
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
