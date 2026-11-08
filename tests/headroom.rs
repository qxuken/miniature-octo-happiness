use weekend_db::{
    Database, Error, Limits, MAX_RETAINED_SNAPSHOTS,
    storage::{Crash, Fault, IoKind, MAX_IMAGE_BYTES, REGION, SimulatedDisk},
};

fn image(retain: bool) -> Vec<u8> {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [9; 16], Limits::default()).unwrap();
    let collection = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(collection, b"record", &db.integer(7).unwrap())
        .unwrap();
    tx.commit().unwrap();
    if retain {
        for _ in 0..MAX_RETAINED_SNAPSHOTS {
            db.retain_snapshot(&db.snapshot()).unwrap();
        }
    }
    disk.durable_image().unwrap()
}

#[test]
fn impossible_lengths_block_publication() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    use weekend_db::{Result, storage::ByteStore};
    struct ReportedLength {
        disk: SimulatedDisk,
        length: Arc<AtomicU64>,
    }
    impl ByteStore for ReportedLength {
        fn lock(&mut self) -> Result<()> {
            self.disk.lock()
        }
        fn unlock(&mut self) {
            self.disk.unlock();
        }
        fn length(&mut self) -> Result<u64> {
            match self.length.load(Ordering::Relaxed) {
                u64::MAX => self.disk.length(),
                value => Ok(value),
            }
        }
        fn read(&mut self, at: u64, out: &mut [u8]) -> Result<usize> {
            self.disk.read(at, out)
        }
        fn write(&mut self, at: u64, bytes: &[u8]) -> Result<usize> {
            self.disk.write(at, bytes)
        }
        fn sync(&mut self) -> Result<()> {
            self.disk.sync()
        }
    }
    for invalid in [0, MAX_IMAGE_BYTES as u64 + 1] {
        let disk = SimulatedDisk::from_image(&image(false)).unwrap();
        let length = Arc::new(AtomicU64::new(u64::MAX));
        let db = Database::open_on(
            ReportedLength {
                disk: disk.clone(),
                length: length.clone(),
            },
            Limits::default(),
        )
        .unwrap();
        let before = disk.durable_image().unwrap();
        db.storage_space().unwrap();
        disk.arm(usize::MAX, Fault::Error);
        length.store(invalid, Ordering::Relaxed);
        assert_eq!(db.storage_space(), Err(Error::Corrupt));
        assert!(db.requires_recovery());
        length.store(u64::MAX, Ordering::Relaxed);
        assert_eq!(db.storage_space(), Err(Error::RecoveryRequired));
        assert_eq!(db.begin().commit(), Err(Error::RecoveryRequired));
        assert!(disk.trace().unwrap().is_empty());
        assert_eq!(disk.durable_image().unwrap(), before);
    }
}

#[test]
fn headroom_counts_uncommitted_tail_and_uses_only_length() {
    let mut original = image(false);
    let committed = original.len();
    original.extend_from_slice(&[0; 7]);
    let disk = SimulatedDisk::from_image(&original).unwrap();
    let db = Database::open_on(disk.clone(), Limits::default()).unwrap();
    disk.arm(usize::MAX, Fault::Error);
    let space = db.storage_space().unwrap();
    assert_eq!(space.revision, db.snapshot().revision());
    assert_eq!(space.physical_bytes, (committed + 7) as u64);
    assert_eq!(space.committed_bytes, committed as u64);
    assert_eq!(space.next_append_offset, (committed + REGION) as u64);
    assert_eq!(space.capacity_bytes, MAX_IMAGE_BYTES as u64);
    assert_eq!(
        space.remaining_append_bytes,
        (MAX_IMAGE_BYTES - committed - REGION) as u64
    );
    let trace = disk.trace().unwrap();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].kind, IoKind::Length);
    assert_eq!(disk.durable_image().unwrap(), original);
    disk.arm(0, Fault::Error);
    assert_eq!(db.storage_space(), Err(Error::Io));
    assert!(!db.requires_recovery());
    assert_eq!(db.storage_space(), Ok(space));
    assert_eq!(
        Database::new().unwrap().storage_space(),
        Err(Error::Invalid)
    );
}

#[test]
fn full_image_preserves_acknowledged_and_retained_roots_then_compacts() {
    let mut original = image(true);
    original.resize(MAX_IMAGE_BYTES - REGION, 0);
    let disk = SimulatedDisk::from_image(&original).unwrap();
    drop(original);
    let mut db = Database::open_on(disk.clone(), Limits::default()).unwrap();
    assert_eq!(
        db.storage_space().unwrap().remaining_append_bytes,
        REGION as u64
    );
    let revision = db.snapshot().revision();
    assert_eq!(db.begin().commit(), Ok(revision + 1));
    assert_eq!(db.storage_space().unwrap().remaining_append_bytes, 0);
    disk.arm(usize::MAX, Fault::Error);
    assert_eq!(db.begin().commit(), Err(Error::Limit));
    assert_eq!(db.retain_snapshot(&db.snapshot()), Err(Error::Limit));
    assert!(!db.requires_recovery());
    assert_eq!(db.snapshot().revision(), revision + 1);
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
    );
    let compact = SimulatedDisk::new().unwrap();
    let stats = db.compact_to(compact.clone()).unwrap();
    assert_eq!(stats.before_bytes, MAX_IMAGE_BYTES as u64);
    assert!(stats.after_bytes < stats.before_bytes);
    drop(db);
    compact.crash(Crash::DropAll).unwrap();
    db = Database::open_on(compact, Limits::default()).unwrap();
    assert!(db.storage_space().unwrap().remaining_append_bytes > 0);
    assert_eq!(db.retained_ids().unwrap().len(), MAX_RETAINED_SNAPSHOTS);
    for id in db.retained_ids().unwrap() {
        assert_eq!(
            db.retained_snapshot(id)
                .unwrap()
                .get(db.collection_id(1), b"record")
                .unwrap()
                .unwrap()
                .as_i64(),
            Ok(7)
        );
    }
    assert_eq!(db.begin().commit(), Ok(revision + 2));
}
