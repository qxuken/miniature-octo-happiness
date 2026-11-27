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

#[cfg(feature = "replication")]
#[path = "support/capacity_follower.rs"]
mod capacity_follower;
#[cfg(feature = "replication")]
#[test]
#[ignore = "1 GiB capacity native follower, backup and restore"]
fn gib_capacity_end_to_end() {
    let d = Dir::new();
    let path = d.path("primary");
    let db = create(&path, FileCapacity::GiB1);
    let c = db.collection_id(1);
    let mut bytes = vec![0x63; 1 << 20];
    let start = Instant::now();
    for group in 0..240u32 {
        let mut t = db.begin();
        for k in group * 4..group * 4 + 4 {
            bytes[..4].copy_from_slice(&k.to_le_bytes());
            t.put(c, &k.to_be_bytes(), &db.bytes(&bytes).unwrap())
                .unwrap();
        }
        t.commit().unwrap();
    }
    let r = db.retain_snapshot(&db.snapshot()).unwrap();
    put(&db, 200, b"new");
    drop(db);
    let size = fs::metadata(&path).unwrap().len();
    let t = Instant::now();
    let view = ReadOnlyDatabase::open_file(&path, limits().into()).unwrap();
    let open = t.elapsed();
    let t = Instant::now();
    view.check().unwrap();
    let full = t.elapsed();
    let peak = view.memory_usage().peak_bytes;
    drop(view);
    capacity_follower::roundtrip(
        &path,
        &d.path("replication"),
        limits(),
        r,
        1 << 30,
        |s, c, current| {
            assert_eq!(s.count(c), Ok(960));
            for k in [0u32, 200, 959] {
                let val = s.get(c, &k.to_be_bytes()).unwrap().unwrap();
                let b = val.as_bytes().unwrap();
                if current && k == 200 {
                    assert_eq!(b, b"new")
                } else {
                    assert_eq!(b.len(), 1 << 20);
                    assert_eq!(&b[..4], &k.to_le_bytes());
                    assert!(b[4..].iter().all(|b| *b == 0x63));
                }
            }
        },
    );
    println!(
        "gib primary={size} reader_peak={peak} open={open:?} full={full:?} including shipping/backup/restore={:?}",
        start.elapsed()
    );
}

#[cfg(all(unix, feature = "fault-injection"))]
#[test]
fn migration_interrupted_at_each_replacement_barrier() {
    use storage::CompactionStep::*;
    const ENV: &str = "WDB_CAPACITY_CHILD_PATH";
    if let Some(path) = std::env::var_os(ENV) {
        let point = std::env::var("WDB_CAPACITY_CHILD_CUT").unwrap();
        storage::migrate_file_with_hook(
            Path::new(&path),
            FileFormat::Packed,
            limits(),
            move |event| {
                if format!("{:?}-{}", event.step, event.after) == point {
                    std::process::exit(71);
                }
                Ok(())
            },
        )
        .unwrap();
        panic!("cut was not reached");
    }
    let dir = Dir::new();
    #[cfg(target_os = "linux")]
    if let Some(root) = std::env::var_os("WDB_MIGRATION_QUOTA_VOLUME") {
        quota_migration(Path::new(&root));
    }
    let seed = dir.path("seed");
    let (old, rev, id) = fixture(&seed);
    for step in [
        TargetWrite,
        TargetSync,
        CopyMetadata,
        SyncMetadata,
        CheckNames,
        Exchange,
        SyncDirectory,
        SyncReplacement,
        RemoveOld,
        SyncCleanupDirectory,
        SyncCleanupFile,
    ] {
        for after in [false, true] {
            let case = dir.path(&format!("{step:?}-{after}"));
            fs::create_dir(&case).unwrap();
            let source = case.join("db");
            fs::copy(&seed, &source).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "migration_interrupted_at_each_replacement_barrier",
                ])
                .env(ENV, &source)
                .env("WDB_CAPACITY_CHILD_CUT", format!("{step:?}-{after}"))
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(71), "{step:?} {after}");
            let exchanged = matches!(step, Exchange) && after
                || matches!(
                    step,
                    SyncDirectory
                        | SyncReplacement
                        | RemoveOld
                        | SyncCleanupDirectory
                        | SyncCleanupFile
                );
            expected(
                &source,
                old,
                rev,
                id,
                if exchanged {
                    FileFormat::Packed
                } else {
                    FileFormat::V2
                },
            );
            storage::migrate_file(&source, FileFormat::Packed, limits()).unwrap();
            expected(&source, old, rev, id, FileFormat::Packed);
        }
    }
}
#[test]
fn shared_pages_are_relocated_once_across_catalogs() {
    let d = Dir::new();
    let p = d.path("source");
    let mut db = create(&p, FileCapacity::MiB64);
    let c = db.collection_id(1);
    for batch in 0..18u32 {
        let mut t = db.begin();
        for k in batch * 64..batch * 64 + 64 {
            let mut b = [41u8; 512];
            b[..4].copy_from_slice(&k.to_le_bytes());
            t.put(c, &k.to_be_bytes(), &db.bytes(&b).unwrap()).unwrap();
        }
        t.commit().unwrap();
    }
    let before = d.path("before");
    db.compact_to(storage::FileStore::create(&before).unwrap())
        .unwrap();
    let old = db.retain_snapshot(&db.snapshot()).unwrap();
    put(&db, 550, b"replacement");
    drop(db);
    let to = d.path("after");
    storage::backup_file(&p, &to, limits()).unwrap();
    assert_eq!(
        fs::metadata(&to).unwrap().len() - fs::metadata(before).unwrap().len(),
        3 * 4096
    );
    let db = Database::open_file(&to, limits()).unwrap();
    assert_eq!(
        db.retained_snapshot(old)
            .unwrap()
            .count(db.collection_id(1)),
        Ok(1152)
    );
}

#[test]
fn resealed_blob_descriptors_reject_bad_bounds_digest_length_and_revision() {
    use weekend_db::storage::SimulatedDisk;
    fn seal(bytes: &mut [u8], field: usize) -> u64 {
        bytes[field..field + 8].fill(0);
        let mut crc = 0u64;
        for &b in bytes.iter() {
            crc ^= u64::from(b) << 56;
            for _ in 0..8 {
                crc = (crc << 1) ^ (0x42f0_e1eb_a9ea_3693 & 0u64.wrapping_sub(crc >> 63));
            }
        }
        bytes[field..field + 8].copy_from_slice(&crc.to_le_bytes());
        crc
    }
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(
        disk.clone(),
        [19; 16],
        DatabaseOptions::from(limits()).with_file_capacity(FileCapacity::GiB1),
    )
    .unwrap();
    db.create_collection().unwrap();
    put(&db, 777, &[87; 8000]);
    db.create_collection().unwrap(); // Head can be newer than the blob-bearing row.
    drop(db);
    let clean = disk.durable_image().unwrap();
    let object = |kind: u32| {
        clean
            .as_chunks::<4096>()
            .0
            .iter()
            .position(|p| &p[..8] == b"WDBOBJ01" && p[8..12] == kind.to_le_bytes())
            .unwrap()
            * 4096
    };
    let blob = object(6);
    let leaf = object(4);
    let desc = leaf + 64 + 4 + 4 + 8 + 4;
    assert_eq!(clean[desc], 8);
    let blob_size = (64 + 8000usize).next_multiple_of(4096);
    for case in [
        "forward",
        "unaligned",
        "threshold",
        "extent length",
        "digest",
        "zero birth",
        "future birth",
    ] {
        let mut image = clean.clone();
        let (field, n) = match case {
            "forward" => (1, leaf as u64),
            "unaligned" => (1, blob as u64 + 1),
            "threshold" => (9, 4032),
            "extent length" => (9, 7999),
            "digest" => (17, 99),
            "zero birth" | "future birth" => {
                let birth = if case == "zero birth" { 0u64 } else { 3 };
                image[blob + 24..blob + 32].copy_from_slice(&birth.to_le_bytes());
                (17, seal(&mut image[blob..blob + blob_size], 48))
            }
            _ => unreachable!(),
        };
        image[desc + field..desc + field + 8].copy_from_slice(&n.to_le_bytes());
        seal(&mut image[leaf..leaf + 4096], 48);
        let disk = SimulatedDisk::from_image(&image).unwrap();
        assert_eq!(
            ReadOnlyDatabase::open_on(disk, limits().into()).err(),
            Some(Error::Corrupt),
            "{case}"
        );
    }
}

#[path = "support/capacity_benchmark.rs"]
mod capacity_benchmark;

#[cfg(all(target_os = "linux", feature = "fault-injection"))]
fn quota_migration(root: &Path) {
    use std::io::Write;
    let area = Dir(root.join(format!("wdb-space-{}", std::process::id())));
    fs::create_dir(&area.0).unwrap();
    let path = area.path("source");
    let db = Database::create_file(&path, limits()).unwrap();
    let c = db.create_collection().unwrap();
    let bytes = vec![23; 1 << 20];
    for k in 0..6u32 {
        put(&db, k, &bytes);
    }
    let r = db.retain_snapshot(&db.snapshot()).unwrap();
    put(&db, 3, b"new");
    let rev = db.snapshot().revision();
    drop(db);
    let fill = area.path("reservation");
    let mut f = fs::File::create_new(&fill).unwrap();
    for i in 0..49 {
        match f.write_all(&bytes).and_then(|_| f.sync_data()) {
            Ok(()) => assert!(i < 48, "use a dedicated filesystem smaller than 48 MiB"),
            Err(e) => {
                assert_eq!(e.raw_os_error(), Some(28));
                break;
            }
        }
    }
    let n = f.metadata().unwrap().len();
    f.set_len(n.saturating_sub(512 << 10)).unwrap();
    f.sync_all().unwrap();
    drop(f);
    let err = storage::with_native_diagnostics(|| {
        storage::migrate_file(&path, FileFormat::Packed, limits())
    })
    .unwrap_err();
    assert_eq!(err.native.unwrap().raw_os_error, Some(28));
    fs::remove_file(fill).unwrap();
    for format in [FileFormat::V2, FileFormat::Packed] {
        if format == FileFormat::Packed {
            storage::migrate_file(&path, format, limits()).unwrap();
        }
        let info = storage::inspect_file(&path, limits()).unwrap();
        assert_eq!(
            (info.format_version, info.revision),
            (format.version(), rev)
        );
        let db = Database::open_file(&path, limits()).unwrap();
        let c = db.collection_id(c.as_u64());
        assert_eq!(
            db.snapshot()
                .get(c, &3u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .as_bytes()
                .unwrap(),
            b"new"
        );
        assert_eq!(
            db.retained_snapshot(r)
                .unwrap()
                .get(c, &3u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .as_bytes()
                .unwrap(),
            bytes
        );
    }
    println!("native migration ENOSPC, retained root and retry passed");
}
