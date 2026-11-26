use weekend_db::{
    Database, DatabaseOptions, Error, Limits,
    storage::{self, ByteStore, Crash, Fault, IoKind, SimulatedDisk},
};

fn paged() -> DatabaseOptions {
    DatabaseOptions::default().with_paged_reads()
}
fn fixture() -> Vec<u8> {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [47; 16], Limits::default()).unwrap();
    let a = db.create_collection().unwrap();
    let b = db.create_collection().unwrap();
    let bytes = db.bytes(b"retained shared bytes").unwrap();
    let list = db
        .list()
        .unwrap()
        .list_push(&bytes)
        .unwrap()
        .list_push(&bytes)
        .unwrap();
    let map = db
        .map()
        .unwrap()
        .map_set(b"", &list)
        .unwrap()
        .map_set(b"\xff", &bytes)
        .unwrap();
    let mut tx = db.begin();
    for (i, value) in [
        db.none().unwrap(),
        db.boolean(true).unwrap(),
        db.integer(-3).unwrap(),
        db.unsigned(4).unwrap(),
        db.float(-1.25).unwrap(),
        bytes.clone(),
        map.clone(),
        list.clone(),
    ]
    .iter()
    .enumerate()
    {
        tx.put(a, &[i as u8], value).unwrap();
    }
    tx.put(b, &[], &map).unwrap();
    tx.put(b, &[255; 5000], &list).unwrap();
    for i in 0..400u16 {
        tx.put(b, &i.to_be_bytes(), &bytes).unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 1);
    let mut tx = db.begin();
    tx.put(b, &[], &db.none().unwrap()).unwrap();
    tx.commit().unwrap();
    disk.durable_image().unwrap()
}

#[test]
fn all_kinds_and_binary_keys_survive_overlay_conflicts_compaction_and_reopen() {
    let image = fixture();
    let mut outputs = Vec::new();
    for opts in [DatabaseOptions::default(), paged()] {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let mut db = Database::open_on_with_options(disk.clone(), opts).unwrap();
        let a = db.collection_id(1);
        let b = db.collection_id(2);
        let old = db
            .retained_snapshot(1)
            .unwrap()
            .get(b, b"")
            .unwrap()
            .unwrap();
        assert_eq!(old.map_get(b"").unwrap().unwrap().len(), Ok(2));
        let old_rows = db.snapshot().scan(a).unwrap();
        let mut conflict = db.begin();
        conflict.get(a, b"\x03").unwrap();
        let mut independent = db.begin();
        independent.put(a, b"\x01", &db.none().unwrap()).unwrap();
        let snap = db.snapshot();
        let mut tx = db.begin();
        tx.delete(a, b"\0").unwrap();
        tx.put(a, b"\x03", &db.integer(-8).unwrap()).unwrap();
        tx.put(b, b"new", &db.bytes(b"insert").unwrap()).unwrap();
        // Reinsert a separately materialized child, including across collections.
        let child = old.map_get(b"\xff").unwrap().unwrap();
        tx.put(b, b"alias", &child).unwrap();
        let staged = tx.scan(b).unwrap();
        tx.commit().unwrap();
        assert_eq!(conflict.commit(), Err(Error::Conflict));
        independent.commit().unwrap();
        assert_eq!(db.snapshot().scan(b).unwrap(), staged);
        let mut ordered = Vec::new();
        db.snapshot()
            .visit(b, |k, v| {
                ordered.push((k.to_vec(), v.clone()));
                Ok(())
            })
            .unwrap();
        assert_eq!(ordered, staged);
        let mut calls = 0;
        assert_eq!(
            db.snapshot().visit(b, |_, _| {
                calls += 1;
                Err(Error::Invalid)
            }),
            Err(Error::Invalid)
        );
        assert_eq!(calls, 1);
        assert_eq!(snap.scan(a).unwrap(), old_rows);
        let mut tx = db.begin();
        tx.put(a, b"\0", &db.none().unwrap()).unwrap();
        tx.delete(b, b"new").unwrap();
        tx.commit().unwrap();
        let destination = SimulatedDisk::new().unwrap();
        db.compact_to(destination.clone()).unwrap();
        let rows = db.snapshot().scan(a).unwrap();
        let revision = db.snapshot().revision();
        drop(db);
        assert_eq!(
            old.map_get(b"\xff").unwrap().unwrap().as_bytes(),
            Ok(&b"retained shared bytes"[..])
        );
        let compact = destination.durable_image().unwrap();
        for medium in [disk.clone(), destination] {
            medium.crash(Crash::DropAll).unwrap();
            let db = Database::open_on_with_options(medium, paged()).unwrap();
            assert_eq!(db.snapshot().scan(db.collection_id(1)).unwrap(), rows);
            assert_eq!(db.snapshot().revision(), revision);
            assert_eq!(
                db.retained_snapshot(1)
                    .unwrap()
                    .get(db.collection_id(2), b"")
                    .unwrap(),
                Some(old.clone())
            );
        }
        outputs.push((disk.durable_image().unwrap(), compact, rows, old_rows));
    }
    for (a, b, name) in [
        (&outputs[0].0, &outputs[1].0, "append"),
        (&outputs[0].1, &outputs[1].1, "compact"),
    ] {
        assert!(
            a == b,
            "{name} differs at {:?}, lengths {} / {}",
            a.iter().zip(b).position(|(x, y)| x != y),
            a.len(),
            b.len()
        );
    }
    assert_eq!(outputs[0].2, outputs[1].2);
    assert_eq!(outputs[0].3, outputs[1].3);
}

#[test]
fn full_check_is_read_only_refusals_release_the_lease_and_failed_reads_retry() {
    let image = fixture();
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let info = storage::inspect_on(disk.clone(), Limits::default()).unwrap();
    assert_eq!(
        storage::inspect_on_with_options(disk.clone(), paged()).unwrap(),
        info
    );
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|e| !matches!(e.kind, IoKind::Sync | IoKind::Write))
    );
    let tight = paged().with_limits(Limits {
        max_live_bytes: 1024,
        ..Limits::default()
    });
    assert_eq!(
        Database::open_on_with_options(disk.clone(), tight).err(),
        Some(Error::Limit)
    );
    assert_eq!(disk.durable_image().unwrap(), image);
    let db = Database::open_on_with_options(disk.clone(), paged()).unwrap();
    let a = db.collection_id(1);
    disk.arm(0, Fault::Error);
    assert_eq!(db.snapshot().get(a, &[6]), Err(Error::Io));
    disk.clear_fault();
    assert_eq!(db.snapshot().get(a, &[6]).unwrap().unwrap().len(), Ok(2));
}

// Change bytes supplied by reads only, preserving the source and its lease.
struct Flip {
    disk: SimulatedDisk,
    offset: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
impl ByteStore for Flip {
    fn lock(&mut self) -> weekend_db::Result<()> {
        self.disk.lock()
    }
    fn unlock(&mut self) {
        self.disk.unlock()
    }
    fn length(&mut self) -> weekend_db::Result<u64> {
        self.disk.length()
    }
    fn sync(&mut self) -> weekend_db::Result<()> {
        self.disk.sync()
    }
    fn write(&mut self, o: u64, b: &[u8]) -> weekend_db::Result<usize> {
        self.disk.write(o, b)
    }
    fn read(&mut self, o: u64, b: &mut [u8]) -> weekend_db::Result<usize> {
        let n = self.disk.read(o, b)?;
        let at = self.offset.load(std::sync::atomic::Ordering::Relaxed);
        if (o..o + n as u64).contains(&at) {
            b[(at - o) as usize] ^= 1;
        }
        Ok(n)
    }
}
#[test]
fn disk_reads_verify_page_and_nested_value_bytes_against_open_time_checksums() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let original = fixture();
    let disk = SimulatedDisk::from_image(&original).unwrap();
    let at = Arc::new(AtomicU64::new(u64::MAX));
    let db = Database::open_on_with_options(
        Flip {
            disk: disk.clone(),
            offset: at.clone(),
        },
        paged(),
    )
    .unwrap();
    let a = db.collection_id(1);
    let child = original
        .windows(21)
        .position(|w| w == b"retained shared bytes")
        .unwrap() as u64;
    at.store(child, Ordering::Relaxed);
    assert_eq!(db.snapshot().get(a, &[6]), Err(Error::Corrupt));
    at.store(u64::MAX, Ordering::Relaxed);
    disk.arm(usize::MAX, Fault::Error);
    assert_eq!(db.snapshot().get(a, &[6]).unwrap().unwrap().len(), Ok(2));
    let directory = disk.trace().unwrap()[0].offset;
    // Change another row's write revision without changing row lengths or key order.
    at.store(directory + 8 + 1, Ordering::Relaxed);
    assert_eq!(db.snapshot().get(a, &[6]), Err(Error::Corrupt));
    drop(db);
    let mut damaged = original;
    damaged[child as usize] ^= 1;
    let disk = SimulatedDisk::from_image(&damaged).unwrap();
    assert_eq!(
        Database::open_on_with_options(disk.clone(), paged()).err(),
        Some(Error::Corrupt)
    );
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|e| !matches!(e.kind, IoKind::Sync | IoKind::Write))
    );
}

#[test]
fn witnessed_read_only_open_defers_big_blobs_and_check_visits_retained_only_data() {
    use weekend_db::ReadOnlyDatabase;
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [84; 16], Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    for byte in [0xa6, 0x91] {
        let mut tx = db.begin();
        tx.put(c, b"blob", &db.bytes(&vec![byte; 12000]).unwrap())
            .unwrap();
        tx.put(c, b"small", &db.unsigned(3).unwrap()).unwrap();
        tx.commit().unwrap();
        if byte == 0xa6 {
            db.retain_snapshot(&db.snapshot()).unwrap();
        }
    }
    drop(db);
    let pristine = disk.durable_image().unwrap();
    for (byte, retained) in [(0xa6, true), (0x91, false)] {
        let at = pristine.windows(64).position(|w| w == [byte; 64]).unwrap() + 8000;
        let mut image = pristine.clone();
        image[at] ^= 1;
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let view = ReadOnlyDatabase::open_on(disk.clone(), DatabaseOptions::default()).unwrap();
        let c = view.collection_id(1);
        assert_eq!(
            view.snapshot().get(c, b"small").unwrap().unwrap().as_u64(),
            Ok(3)
        );
        let result = view.snapshot().get(c, b"blob");
        let old = view.retained_snapshot(1).unwrap().get(c, b"blob");
        if retained {
            assert!(result.is_ok());
            assert_eq!(old, Err(Error::Corrupt));
        } else {
            assert_eq!(result, Err(Error::Corrupt));
            assert!(old.is_ok());
        }
        assert_eq!(view.check(), Err(Error::Corrupt));
        drop(view);
        assert!(
            !disk
                .trace()
                .unwrap()
                .iter()
                .any(|e| matches!(e.kind, IoKind::Write | IoKind::Sync))
        );
        assert_eq!(
            Database::open_on_with_options(disk, paged()).err(),
            Some(Error::Corrupt)
        );
        // Remove both witnesses. Read-only open now fully checks the newest graph.
        image[3 * 4096..5 * 4096].fill(0);
        let disk = SimulatedDisk::from_image(&image).unwrap();
        assert_eq!(
            ReadOnlyDatabase::open_on(disk, paged()).err(),
            Some(Error::Corrupt)
        );
    }
    assert_eq!(
        ReadOnlyDatabase::open_on(disk, paged()).unwrap().check(),
        Ok(())
    );
}

#[test]
fn interrupted_paged_commits_preserve_the_acknowledged_head_and_retention() {
    let image = fixture();
    let run = |cut: usize| {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let db = Database::open_on_with_options(disk.clone(), paged()).unwrap();
        let id = db.collection_id(1);
        let before = db.snapshot().revision();
        let mut tx = db.begin();
        tx.put(id, &[3], &db.integer(99).unwrap()).unwrap();
        disk.arm(cut, Fault::Error);
        let result = tx.commit();
        let trace = disk.trace().unwrap();
        drop(tx);
        drop(db);
        disk.crash(Crash::DropAll).unwrap();
        let db = Database::open_on_with_options(disk, paged()).unwrap();
        let id = db.collection_id(1);
        let rev = db.snapshot().revision();
        let value = db.snapshot().get(id, &[3]).unwrap().unwrap();
        let retained = db
            .retained_snapshot(1)
            .unwrap()
            .get(id, &[3])
            .unwrap()
            .unwrap();
        assert_eq!(retained.as_u64(), Ok(4));
        if rev == before {
            assert_eq!(value.as_u64(), Ok(4));
        } else {
            assert_eq!(rev, before + 1);
            assert_eq!(value.as_i64(), Ok(99));
        }
        (result, before, rev, trace)
    };
    let (ok, before, reopened, events) = run(usize::MAX);
    assert_eq!(ok, Ok(before + 1));
    assert_eq!(reopened, before + 1);
    let barriers: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| (e.kind == IoKind::Sync).then_some(i))
        .collect();
    assert_eq!(barriers.len(), 3);
    for cut in 0..events.len() {
        let (result, _, rev, _) = run(cut);
        assert!(result.is_err());
        assert_eq!(rev, before + u64::from(cut > barriers[1]), "cut {cut}");
    }
}

#[test]
fn paged_reads_open_and_visit_distinct_blobs_below_the_eager_budget() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [68; 16], Limits::default()).unwrap();
    let id = db.create_collection().unwrap();
    let mut tx = db.begin();
    for i in 0..8 {
        tx.put(id, &[i], &db.bytes(&vec![i; 96 * 1024]).unwrap())
            .unwrap();
    }
    tx.commit().unwrap();
    drop(tx);
    drop(db);
    let limits = Limits {
        max_live_bytes: 384 * 1024,
        ..Limits::default()
    };
    assert_eq!(
        Database::open_on(disk.clone(), limits).err(),
        Some(Error::Limit)
    );
    let view = Database::open_on_with_options(disk, paged().with_limits(limits)).unwrap();
    let id = view.collection_id(1);
    assert_eq!(view.snapshot().scan(id).err(), Some(Error::Limit));
    let mut count = 0;
    view.snapshot()
        .visit(id, |k, v| {
            assert_eq!(k, [count]);
            assert!(v.as_bytes()?.iter().all(|&b| b == count));
            count += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(count, 8);
    assert!(view.memory_usage().peak_bytes <= limits.max_live_bytes);
}
