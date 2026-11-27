#![cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use weekend_db::{
    Database, DatabaseOptions, Error, Limits, ReadOnlyDatabase,
    storage::{self, FileCapacity, FileFormat},
};

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "wdb-cap-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn limits() -> Limits {
    Limits {
        max_live_bytes: 64 << 20,
        max_entries: 1_000_000,
        ..Limits::default()
    }
}
fn expected(path: &Path, retained: u64, rev: u64, identity: [u8; 16], format: FileFormat) {
    let info = storage::inspect_file(path, limits()).unwrap();
    assert_eq!(
        (info.format_version, info.revision, info.identity),
        (format.version(), rev, identity)
    );
    let db = Database::open_file(path, limits()).unwrap();
    assert_eq!(
        db.storage_space().unwrap().capacity_bytes,
        format.capacity()
    );
    let c = db.collection_id(1);
    assert_eq!(
        db.snapshot().get(c, b"x").unwrap().unwrap().as_u64(),
        Ok(53)
    );
    assert_eq!(
        db.retained_snapshot(retained)
            .unwrap()
            .get(c, b"x")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(17)
    );
}
fn fixture(path: &Path) -> (u64, u64, [u8; 16]) {
    let db = Database::create_file(path, limits()).unwrap();
    let c = db.create_collection().unwrap();
    let spare = db.create_collection().unwrap();
    db.drop_collection(spare).unwrap();
    let put = |v| {
        let mut t = db.begin();
        t.put(c, b"x", &db.unsigned(v).unwrap()).unwrap();
        t.commit().unwrap()
    };
    put(17);
    let old = db.retain_snapshot(&db.snapshot()).unwrap();
    let rev = put(53);
    (old, rev, db.storage_identity().unwrap())
}
#[test]
fn migrate_keeps_history_identity_and_next_ids() {
    let d = Dir::new();
    let source = d.path("v2");
    let (old, rev, id) = fixture(&source);
    let mut from = source.clone();
    for (i, format) in [
        FileFormat::Packed,
        FileFormat::Extended(FileCapacity::MiB256),
        FileFormat::Extended(FileCapacity::GiB1),
        FileFormat::Extended(FileCapacity::MiB64),
    ]
    .into_iter()
    .enumerate()
    {
        let to = d.path(&format!("copy-{i}"));
        let original = fs::read(&from).unwrap();
        storage::migrate_file_to(&from, &to, format, limits()).unwrap();
        assert_eq!(fs::read(&from).unwrap(), original);
        expected(&to, old, rev, id, format);
        from = to;
    }
    let db = Database::open_file(&from, limits()).unwrap();
    assert_eq!(db.create_collection().unwrap(), db.collection_id(3));
    assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), old + 1);
    #[cfg(unix)]
    {
        storage::migrate_file(&source, FileFormat::Packed, limits()).unwrap();
        expected(&source, old, rev, id, FileFormat::Packed);
    }
}
fn put(db: &Database, k: u32, bytes: &[u8]) {
    let mut tx = db.begin();
    tx.put(
        db.collection_id(1),
        &k.to_be_bytes(),
        &db.bytes(bytes).unwrap(),
    )
    .unwrap();
    tx.commit().unwrap();
}
fn create(path: &Path, cap: FileCapacity) -> Database {
    let db = Database::create_file_with_options(
        path,
        DatabaseOptions::from(limits()).with_file_capacity(cap),
    )
    .unwrap();
    db.create_collection().unwrap();
    db
}
#[test]
fn blob_corruption_surfaces_on_affected_read_and_full_check() {
    use std::io::{Seek, SeekFrom, Write};
    let dir = Dir::new();
    let p = dir.path("blob");
    let db = create(&p, FileCapacity::MiB64);
    put(&db, 1, &[0x56; 32768]);
    let id = db.retain_snapshot(&db.snapshot()).unwrap();
    put(&db, 1, &[0x91; 32768]);
    put(&db, 2, b"small");
    drop(db);
    let clean = fs::read(&p).unwrap();
    for (byte, retained) in [(0x56, true), (0x91, false)] {
        let bad = dir.path(&format!("bad-{byte}"));
        fs::copy(&p, &bad).unwrap();
        let off = clean.windows(64).position(|w| w == [byte; 64]).unwrap() + 129;
        let mut file = fs::OpenOptions::new().write(true).open(&bad).unwrap();
        file.seek(SeekFrom::Start(off as u64)).unwrap();
        file.write_all(&[0]).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let view = ReadOnlyDatabase::open_file(&bad, limits().into()).unwrap();
        let c = view.collection_id(1);
        let current = view.snapshot();
        let old = view.retained_snapshot(id).unwrap();
        assert_eq!(
            current
                .get(c, &2u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .as_bytes()
                .unwrap(),
            b"small"
        );
        let (broken, other) = if retained {
            (&old, &current)
        } else {
            (&current, &old)
        };
        assert_eq!(
            broken.get(c, &1u32.to_be_bytes()).err(),
            Some(Error::Corrupt)
        );
        assert!(other.get(c, &1u32.to_be_bytes()).unwrap().is_some());
        assert_eq!(view.check(), Err(Error::Corrupt));
        drop(old);
        drop(current);
        drop(view);
        assert_eq!(
            Database::open_file(&bad, limits()).err(),
            Some(Error::Corrupt)
        );
    }
}
#[test]
fn last_fitting_commit_and_first_refusal_at_each_fixed_capacity() {
    let dir = Dir::new();
    for cap in [
        FileCapacity::MiB64,
        FileCapacity::MiB256,
        FileCapacity::GiB1,
    ] {
        let p = dir.path(&format!("{cap:?}"));
        let db = create(&p, cap);
        put(&db, 1, b"a");
        drop(db);
        let size = FileFormat::Extended(cap).capacity();
        let file = fs::OpenOptions::new().write(true).open(&p).unwrap();
        file.set_len(size - 16384).unwrap();
        drop(file);
        let db = Database::open_file(&p, limits()).unwrap();
        put(&db, 1, b"b");
        assert_eq!(fs::metadata(&p).unwrap().len(), size);
        let mut t = db.begin();
        let c = db.collection_id(1);
        t.put(c, &1u32.to_be_bytes(), &db.bytes(b"c").unwrap())
            .unwrap();
        assert_eq!(t.commit(), Err(Error::Limit));
        drop(t);
        drop(db);
        let v = ReadOnlyDatabase::open_file(&p, limits().into()).unwrap();
        v.check().unwrap();
        assert_eq!(
            v.snapshot()
                .get(v.collection_id(1), &1u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .as_bytes()
                .unwrap(),
            b"b"
        );
    }
}
#[test]
fn sparse_high_offset_reopen_and_compaction_fit_budget() {
    let dir = Dir::new();
    let p = dir.path("sparse");
    let db = create(&p, FileCapacity::Unbounded);
    put(&db, 1, &[0x77; 8192]);
    let id = db.retain_snapshot(&db.snapshot()).unwrap();
    drop(db);
    let hole = (1u64 << 33) + 4096;
    let file = fs::OpenOptions::new().write(true).open(&p).unwrap();
    file.set_len(hole).unwrap();
    drop(file);
    let db = Database::open_file(&p, limits()).unwrap();
    put(&db, 1, &[0x12; 8192]);
    drop(db);
    let now = Instant::now();
    let v = ReadOnlyDatabase::open_file(&p, limits().into()).unwrap();
    let open = now.elapsed();
    let now = Instant::now();
    v.check().unwrap();
    let check = now.elapsed();
    assert_eq!(
        v.retained_snapshot(id)
            .unwrap()
            .get(v.collection_id(1), &1u32.to_be_bytes())
            .unwrap()
            .unwrap()
            .as_bytes()
            .unwrap(),
        &[0x77; 8192]
    );
    println!(
        "sparse physical={} peak={} open={open:?} full={check:?}",
        fs::metadata(&p).unwrap().len(),
        v.memory_usage().peak_bytes
    );
    drop(v);
    let to = dir.path("copy");
    let stats =
        storage::migrate_file_to(&p, &to, FileFormat::Extended(FileCapacity::MiB64), limits())
            .unwrap();
    assert!(stats.after_bytes < 1 << 20);
    let v = ReadOnlyDatabase::open_file(to, limits().into()).unwrap();
    v.check().unwrap();
    assert_eq!(
        v.snapshot()
            .get(v.collection_id(1), &1u32.to_be_bytes())
            .unwrap()
            .unwrap()
            .as_bytes()
            .unwrap(),
        &[0x12; 8192]
    );
}
#[test]
#[ignore = "dense native capacity qualification"]
fn dense_over_four_gib_with_bounded_reader() {
    let dir = Dir::new();
    let p = dir.path("dense");
    let db = create(&p, FileCapacity::Unbounded);
    let mut bytes = vec![0x75; 1 << 20];
    let c = db.collection_id(1);
    let start = Instant::now();
    for group in 0..1024u32 {
        let mut tx = db.begin();
        for k in group * 4..group * 4 + 4 {
            bytes[..4].copy_from_slice(&k.to_le_bytes());
            tx.put(c, &k.to_be_bytes(), &db.bytes(&bytes).unwrap())
                .unwrap();
        }
        tx.commit().unwrap();
    }
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    put(&db, 2048, b"new");
    drop(db);
    let physical = fs::metadata(&p).unwrap().len();
    assert!(physical > 1u64 << 32);
    let fill = start.elapsed();
    let t = Instant::now();
    let v = ReadOnlyDatabase::open_file(&p, limits().into()).unwrap();
    let open = t.elapsed();
    let t = Instant::now();
    v.check().unwrap();
    let full = t.elapsed();
    let old = v.retained_snapshot(retained).unwrap();
    let c = v.collection_id(1);
    assert_eq!(old.count(c), Ok(4096));
    for k in [0u32, 1, 2048, 4095] {
        let value = old.get(c, &k.to_be_bytes()).unwrap().unwrap();
        assert_eq!(&value.as_bytes().unwrap()[..4], &k.to_le_bytes());
    }
    assert_eq!(
        v.snapshot()
            .get(c, &2048u32.to_be_bytes())
            .unwrap()
            .unwrap()
            .as_bytes()
            .unwrap(),
        b"new"
    );
    println!(
        "dense physical={physical} fill={fill:?} open={open:?} full={full:?} peak={}",
        v.memory_usage().peak_bytes
    );
    assert!(v.memory_usage().peak_bytes <= 64 << 20);
    drop(old);
    drop(v);
    let to = dir.path("small");
    assert_eq!(
        storage::migrate_file_to(&p, to, FileFormat::Extended(FileCapacity::MiB64), limits()).err(),
        Some(Error::Limit)
    );
    assert_eq!(fs::metadata(&p).unwrap().len(), physical);
}
