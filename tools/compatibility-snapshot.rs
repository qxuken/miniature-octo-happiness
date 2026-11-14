//! Private mixed-checkpoint consumer. Loopback peers are owned by the runner;
//! deployment authentication is deliberately not implemented by this fixture.
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use weekend_db::{
    Database, Error, Limits,
    replication::{DurableAck, Enrollment, Follower, Primary},
};
fn publish(path: &Path, bytes: impl AsRef<[u8]>) {
    let pending = path.with_extension("pending");
    fs::write(&pending, bytes).unwrap();
    fs::rename(pending, path).unwrap();
}
fn wait(path: &Path) -> Vec<u8> {
    let start = Instant::now();
    loop {
        if let Ok(bytes) = fs::read(path) {
            return bytes;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "handshake timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn socket(address: &str) -> TcpStream {
    let s = TcpStream::connect(address).unwrap();
    timeouts(&s);
    s
}
fn timeouts(s: &TcpStream) {
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(15))).unwrap();
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|x| format!("{x:02x}")).collect()
}
fn evidence(path: PathBuf, ack: DurableAck) {
    fs::write(
        path,
        format!(
            "{{\"revision\":{},\"digest\":\"{}\",\"stream\":\"{}\",\"retained_revision\":2}}\n",
            ack.revision,
            hex(&ack.digest),
            hex(&ack.stream_id)
        ),
    )
    .unwrap();
}
fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let p = entry.unwrap().path();
            assert!(p.is_file());
            (p.clone(), fs::read(p).unwrap())
        })
        .collect()
}
fn verify(follower: &Follower) -> DurableAck {
    let ack = follower.progress().unwrap().unwrap();
    assert_eq!(ack.revision, 4);
    let replica = follower.replica().unwrap();
    let c = replica.collection_id(1);
    let current = replica.snapshot();
    assert_eq!(current.revision(), 4);
    assert_eq!(current.scan(c).unwrap().len(), 1);
    assert_eq!(
        current.get(c, b"key").unwrap().unwrap().as_u64().unwrap(),
        2
    );
    assert_eq!(replica.retained_ids().unwrap(), vec![1]);
    let old = replica.retained_snapshot(1).unwrap();
    assert_eq!(old.revision(), 2);
    assert_eq!(old.scan(c).unwrap().len(), 1);
    assert_eq!(old.get(c, b"key").unwrap().unwrap().as_u64().unwrap(), 1);
    ack
}
struct Unsupported(TcpStream, bool);
impl Read for Unsupported {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(b)
    }
}
impl Write for Unsupported {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if !self.1 {
            assert!(b.len() >= 8);
            let mut changed = b.to_vec();
            changed[7] = 255;
            self.1 = true;
            self.0.write_all(&changed)?;
            Ok(b.len())
        } else {
            self.0.write(b)
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let dir = Path::new(&args[2]);
    match args[1].as_str() {
        "primary" => {
            let mut db = Database::create_file(dir.join("primary.wdb"), Limits::default()).unwrap();
            let c = db.create_collection().unwrap();
            let mut tx = db.begin();
            tx.put(c, b"key", &db.unsigned(1).unwrap()).unwrap();
            assert_eq!(tx.commit().unwrap(), 2);
            drop(tx);
            assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 1);
            let mut tx = db.begin();
            tx.put(c, b"key", &db.unsigned(2).unwrap()).unwrap();
            assert_eq!(tx.commit().unwrap(), 4);
            drop(tx);
            let mut primary = Primary::new(&db, [31; 16]).unwrap();
            let enrollment = primary.enrollment();
            let mut export = primary
                .export_file(&mut db, dir.join("export.wdb"))
                .unwrap();
            let mut tx = db.begin();
            tx.put(c, b"key", &db.unsigned(3).unwrap()).unwrap();
            assert_eq!(tx.commit().unwrap(), 5);
            drop(tx);
            let mut bytes = enrollment.stream_id.to_vec();
            bytes.extend_from_slice(&enrollment.file_identity);
            publish(&dir.join("enrollment"), bytes);
            let address = String::from_utf8(wait(&dir.join("address"))).unwrap();
            assert_eq!(primary.poll(socket(&address)).unwrap(), None);
            let ack = primary.ship(&mut export, socket(&address)).unwrap();
            assert_eq!(ack.revision, 4);
            assert_eq!(primary.ship(&mut export, socket(&address)).unwrap(), ack);
            assert_eq!(
                primary.poll(Unsupported(socket(&address), false)),
                Err(Error::UnsupportedFormat)
            );
            evidence(dir.join("primary.json"), ack);
        }
        "follower" => {
            let bytes = wait(&dir.join("enrollment"));
            let enrollment = Enrollment {
                stream_id: bytes[..16].try_into().unwrap(),
                file_identity: bytes[16..].try_into().unwrap(),
            };
            let root = dir.join("follower");
            fs::create_dir(&root).unwrap();
            let mut follower = Follower::create(&root, enrollment, Limits::default()).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            publish(
                &dir.join("address"),
                listener.local_addr().unwrap().to_string(),
            );
            let mut selected = None;
            for step in 0..4 {
                drop(follower);
                let before = files(&root);
                follower = Follower::open(&root, enrollment, Limits::default()).unwrap();
                let (s, _) = listener.accept().unwrap();
                timeouts(&s);
                let result = follower.serve_authenticated(s);
                drop(follower);
                if step == 3 {
                    assert_eq!(result, Err(Error::UnsupportedFormat));
                    assert_eq!(files(&root), before);
                } else {
                    result.unwrap();
                }
                follower = Follower::open(&root, enrollment, Limits::default()).unwrap();
                if step == 0 {
                    assert_eq!(follower.progress().unwrap(), None);
                } else {
                    let ack = verify(&follower);
                    if let Some(previous) = selected {
                        assert_eq!(ack, previous);
                    }
                    selected = Some(ack);
                }
            }
            evidence(dir.join("follower.json"), selected.unwrap());
        }
        _ => panic!("unknown role"),
    }
}
