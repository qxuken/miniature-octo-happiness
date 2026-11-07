#![cfg(any(
    target_os = "macos",
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
))]
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use weekend_db::{Database, Error, Limits};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "weekend-db-file-{}-{}",
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
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn seed(path: &Path) {
    let db = Database::create_file(path, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    let one = db.integer(1).unwrap();
    tx.put(c, b"a", &one).unwrap();
    tx.put(c, b"b", &one).unwrap();
    tx.commit().unwrap();
    drop(tx);
    db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    let two = db.integer(2).unwrap();
    tx.put(c, b"a", &two).unwrap();
    tx.put(c, b"b", &two).unwrap();
    tx.commit().unwrap();
}
fn number(snapshot: &weekend_db::Snapshot<'_>, db: &Database, key: &[u8]) -> i64 {
    snapshot
        .get(db.collection_id(1), key)
        .unwrap()
        .unwrap()
        .as_i64()
        .unwrap()
}
#[test]
fn file_reopen_retention_abort_and_live_reader_lifetimes() {
    let dir = Directory::new();
    let path = dir.file("space and λ.db");
    seed(&path);
    let db = Database::open_file(&path, Limits::default()).unwrap();
    let old = db.retained_snapshot(1).unwrap();
    assert_eq!(number(&old, &db, b"a"), 1);
    assert_eq!(number(&db.snapshot(), &db, b"a"), 2);
    let revision = db.snapshot().revision();
    let mut aborted = db.begin();
    aborted
        .put(db.collection_id(1), b"a", &db.integer(999).unwrap())
        .unwrap();
    aborted.abort().unwrap();
    drop(aborted);
    assert_eq!(db.snapshot().revision(), revision);
    db.release_retained(1).unwrap();
    assert_eq!(number(&old, &db, b"b"), 1);
    drop(old);
    drop(db);
    let db = Database::open_file(&path, Limits::default()).unwrap();
    assert!(db.retained_ids().unwrap().is_empty());
    assert_eq!(number(&db.snapshot(), &db, b"a"), 2);
}
#[test]
fn creation_is_exclusive_and_aliases_cannot_bypass_locking() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = Directory::new();
    let path = dir.file("store.db");
    seed(&path);
    let original = fs::read(&path).unwrap();
    assert!(matches!(
        Database::create_file(&path, Limits::default()),
        Err(Error::AlreadyExists)
    ));
    assert_eq!(fs::read(&path).unwrap(), original);
    for suffix in ["/", "/.", "/.."] {
        let malformed = PathBuf::from(format!("{}{suffix}", path.display()));
        assert!(matches!(
            Database::open_file(&malformed, Limits::default()),
            Err(Error::Invalid)
        ));
    }
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let db = Database::open_file(&path, Limits::default()).unwrap();
    assert!(matches!(
        Database::open_file(&path, Limits::default()),
        Err(Error::Busy)
    ));
    let hard = dir.file("hard.db");
    fs::hard_link(&path, &hard).unwrap();
    assert!(matches!(
        Database::open_file(&hard, Limits::default()),
        Err(Error::Busy)
    ));
    let link = dir.file("symbolic.db");
    symlink(&path, &link).unwrap();
    assert!(matches!(
        Database::open_file(&link, Limits::default()),
        Err(Error::Invalid)
    ));
    drop(db);
    drop(Database::open_file(&hard, Limits::default()).unwrap());
    assert!(matches!(
        Database::open_file(dir.file("missing"), Limits::default()),
        Err(Error::NotFound)
    ));
}
#[test]
fn malformed_files_are_rejected_without_repair_or_truncation() {
    let dir = Directory::new();
    let path = dir.file("store.db");
    seed(&path);
    let mut bytes = fs::read(&path).unwrap();
    let at = bytes.len() - weekend_db::storage::REGION + 64;
    bytes[at] ^= 1;
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        Database::open_file(&path, Limits::default()),
        Err(Error::Corrupt)
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[cfg(feature = "fault-injection")]
mod interruption {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    struct Worker {
        child: Child,
        lines: mpsc::Receiver<String>,
    }
    impl Worker {
        fn spawn(path: &Path, op: &str, phase: &str, index: usize, after: bool) -> Self {
            let mut child = Command::new(env!("CARGO_BIN_EXE_file-worker"))
                .arg(path)
                .args([
                    op,
                    phase,
                    &index.to_string(),
                    if after { "after" } else { "before" },
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let stdout = child.stdout.take().unwrap();
            let (sender, lines) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    if sender.send(line.unwrap()).is_err() {
                        break;
                    }
                }
            });
            Self { child, lines }
        }
        fn line(&self) -> String {
            self.lines
                .recv_timeout(Duration::from_secs(30))
                .expect("worker exited or timed out before its boundary marker")
        }
        fn kill(&mut self) {
            self.child.kill().unwrap();
            assert!(!self.child.wait().unwrap().success());
        }
    }
    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    fn check(path: &Path, op: &str, acknowledged: bool) {
        match Database::open_file(path, Limits::default()) {
            Err(e) if op == "create" && !acknowledged => {
                assert!(matches!(e, Error::Corrupt | Error::UnsupportedFormat))
            }
            Ok(db) if op == "create" => {
                assert_eq!(db.snapshot().revision(), 0);
                assert!(db.retained_ids().unwrap().is_empty());
            }
            result => {
                let db = result.unwrap();
                let a = number(&db.snapshot(), &db, b"a");
                let b = number(&db.snapshot(), &db, b"b");
                assert_eq!(a, b, "partial record batch after {op}");
                let ids = db.retained_ids().unwrap();
                match op {
                    "data" => {
                        assert!(a == 2 || a == 3);
                        if acknowledged {
                            assert_eq!(a, 3);
                        }
                        assert_eq!(ids, vec![1]);
                    }
                    "retain" => {
                        assert_eq!(a, 2);
                        assert!(ids == vec![1] || ids == vec![1, 2]);
                        if acknowledged {
                            assert_eq!(ids, vec![1, 2]);
                        }
                    }
                    "release" => {
                        assert_eq!(a, 2);
                        assert!(ids == vec![1] || ids.is_empty());
                        if acknowledged {
                            assert!(ids.is_empty());
                        }
                    }
                    "recover" | "compact" => {
                        assert_eq!(a, 2);
                        assert_eq!(ids, vec![1]);
                    }
                    _ => panic!(),
                }
                for id in ids {
                    assert_eq!(
                        number(&db.retained_snapshot(id).unwrap(), &db, b"a"),
                        if id == 1 { 1 } else { 2 }
                    );
                }
            }
        }
    }
    #[test]
    fn operating_system_locks_exclude_another_process_and_release_on_sigkill() {
        let dir = Directory::new();
        let path = dir.file("store.db");
        seed(&path);
        let mut holder = Worker::spawn(&path, "hold", "hold", 0, false);
        assert_eq!(holder.line(), "READY");
        assert!(matches!(
            Database::open_file(&path, Limits::default()),
            Err(Error::Busy)
        ));
        let mut other = Worker::spawn(&path, "try", "trace", 0, false);
        assert_eq!(other.line(), "BUSY");
        assert!(other.child.wait().unwrap().success());
        holder.kill();
        check(&path, "recover", true);
    }
    #[test]
    fn sigkill_at_file_io_boundaries_preserves_acknowledged_data_and_catalogs() {
        let dir = Directory::new();
        let base = dir.file("base.db");
        seed(&base);
        let mut cases = 0;
        for op in ["data", "retain", "release", "recover", "create", "compact"] {
            let trace_path = dir.file(&format!("trace-{op}.db"));
            if op != "create" {
                fs::copy(&base, &trace_path).unwrap();
            }
            let mut trace = Worker::spawn(&trace_path, op, "trace", 0, false);
            let report = trace.line();
            assert!(report.starts_with("ACK "), "{report}");
            assert!(trace.child.wait().unwrap().success());
            let fields: Vec<_> = report.split_whitespace().collect();
            let events: Vec<_> = fields[3].split(',').collect();
            assert_eq!(fields[1].parse::<usize>().unwrap(), events.len());
            for (index, kind) in events.iter().enumerate() {
                // Process-kill recovery tests focus on verification entry and the
                // three stabilization I/O calls; simulator tests cover every read.
                if op == "recover" && !matches!(*kind, "sync" | "write") && index != 0 {
                    continue;
                }
                for after in [false, true] {
                    let path = dir.file(&format!("cut-{op}-{index}-{after}.db"));
                    if op != "create" {
                        fs::copy(&base, &path).unwrap();
                    }
                    let mut worker = Worker::spawn(&path, op, "cut", index, after);
                    let line = worker.line();
                    assert!(line.starts_with("CUT "), "{op} {index} {line}");
                    worker.kill();
                    check(&path, op, false);
                    cases += 1;
                }
            }
            let path = dir.file(&format!("ack-{op}.db"));
            if op != "create" {
                fs::copy(&base, &path).unwrap();
            }
            let mut worker = Worker::spawn(&path, op, "ack", 0, false);
            assert!(worker.line().starts_with("ACK "));
            worker.kill();
            check(&path, op, true);
            cases += 1;
        }
        eprintln!("real-file SIGKILL boundary/acknowledgment cases: {cases}");
    }
}

#[test]
fn unlocked_adapter_cannot_reacquire_a_replaced_inode() {
    use weekend_db::storage::{ByteStore, FileStore};
    let dir = Directory::new();
    let path = dir.file("store.db");
    seed(&path);
    let mut stale = FileStore::open(&path).unwrap();
    stale.lock().unwrap();
    stale.unlock();
    Database::compact_file(&path, Limits::default()).unwrap();
    assert_eq!(stale.lock(), Err(Error::Busy));
    assert_eq!(stale.write(0, b"bad"), Err(Error::Io));
    let db = Database::open_file(&path, Limits::default()).unwrap();
    assert_eq!(number(&db.snapshot(), &db, b"a"), 2);
}
#[test]
fn native_byte_names_and_a_renamed_parent_preserve_the_held_file() {
    #[cfg(target_os = "linux")]
    use std::os::unix::ffi::OsStringExt;
    let dir = Directory::new();
    let parent = dir.file("parent");
    fs::create_dir(&parent).unwrap();
    #[cfg(target_os = "linux")]
    let name = std::ffi::OsString::from_vec(b"native-\xff.db".to_vec());
    #[cfg(target_os = "macos")]
    let name = std::ffi::OsString::from("native-λ.db");
    let path = parent.join(&name);
    let db = Database::create_file(&path, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let moved = dir.file("moved");
    fs::rename(&parent, &moved).unwrap();
    let mut tx = db.begin();
    tx.put(c, b"a", &db.integer(7).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    drop(db);
    let db = Database::open_file(moved.join(&name), Limits::default()).unwrap();
    assert_eq!(number(&db.snapshot(), &db, b"a"), 7);
}

#[test]
fn nonregular_leaves_and_parents_are_rejected_without_blocking() {
    use std::{
        ffi::{CString, c_char, c_int},
        os::unix::ffi::OsStrExt,
    };
    #[cfg(target_os = "macos")]
    type Mode = u16;
    #[cfg(target_os = "linux")]
    type Mode = u32;
    unsafe extern "C" {
        fn mkfifo(path: *const c_char, mode: Mode) -> c_int;
    }
    let dir = Directory::new();
    let fifo = dir.file("fifo");
    let native = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { mkfifo(native.as_ptr(), 0o600) }, 0);
    let plain = dir.file("plain");
    fs::write(&plain, b"").unwrap();
    // macOS can reject the FIFO's O_EXLOCK before the regular-file check;
    // either result rejects it immediately, without interpreting database bytes.
    assert!(matches!(
        Database::open_file(&fifo, Limits::default()),
        Err(Error::Invalid | Error::Io)
    ));
    for path in [&dir.0, &fifo.join("child"), &plain.join("child")] {
        assert!(
            matches!(
                Database::open_file(path, Limits::default()),
                Err(Error::Invalid)
            ),
            "{path:?}"
        );
    }
}
