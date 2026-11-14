use weekend_db::{
    Database, Error, Limits,
    storage::{Crash, SimulatedDisk},
};
fn limits(cap: usize) -> Limits {
    Limits {
        max_live_bytes: cap,
        ..Limits::default()
    }
}
#[test]
fn owned_values_and_clones_share_one_reclaimable_budget() {
    let db = Database::with_limits(limits(8192)).unwrap();
    let baseline = db.memory_usage().live_bytes;
    let mut values = Vec::new();
    loop {
        match db.bytes(&[7; 512]) {
            Ok(value) => values.push(value),
            Err(error) => {
                assert_eq!(error, Error::Limit);
                break;
            }
        }
    }
    assert!(!values.is_empty());
    assert!(db.memory_usage().live_bytes <= 8192);
    let before = db.memory_usage().live_bytes;
    let last = values.last().unwrap().clone();
    assert_eq!(db.memory_usage().live_bytes, before);
    drop(values);
    assert!(db.memory_usage().live_bytes > baseline);
    drop(db);
    assert_eq!(last.as_bytes().unwrap(), &[7; 512]);
    assert!(last.memory_usage().live_bytes > 512);
    assert!(last.memory_usage().live_bytes < before);
    assert_eq!(last.memory_usage().limit_bytes, 8192);
}
#[test]
fn pinned_snapshot_and_drafts_release_on_drop() {
    let db = Database::with_limits(limits(65536)).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"k", &db.bytes(&[1; 1024]).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    let snapshot = db.snapshot();
    let original = db.memory_usage().live_bytes;
    let mut draft = db.begin();
    draft
        .put(c, b"other", &db.bytes(&[3; 2048]).unwrap())
        .unwrap();
    assert!(db.memory_usage().live_bytes > original);
    drop(draft);
    assert_eq!(db.memory_usage().live_bytes, original);
    let mut tx = db.begin();
    tx.put(c, b"k", &db.bytes(&[2; 1024]).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    assert_eq!(
        snapshot.get(c, b"k").unwrap().unwrap().as_bytes().unwrap(),
        &[1; 1024]
    );
    let pinned = db.memory_usage().live_bytes;
    drop(snapshot);
    assert!(db.memory_usage().live_bytes < pinned);
}
#[test]
fn storage_recovery_and_copy_obey_budget_without_touching_source() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [4; 16], Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"k", &db.bytes(&[8; 4096]).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    db.retain_snapshot(&db.snapshot()).unwrap();
    drop(db);
    let image = disk.durable_image().unwrap();
    assert!(matches!(
        Database::open_on(disk.clone(), limits(4096)),
        Err(Error::Limit)
    ));
    assert_eq!(disk.durable_image().unwrap(), image);
    let destination = SimulatedDisk::new().unwrap();
    assert!(matches!(
        weekend_db::storage::backup_on(disk.clone(), destination.clone(), limits(4096)),
        Err(Error::Limit)
    ));
    assert!(destination.durable_image().unwrap().is_empty());
    assert!(matches!(
        weekend_db::storage::export_logical_on(disk.clone(), limits(4096)),
        Err(Error::Limit)
    ));
    let logical = weekend_db::storage::export_logical_on(disk.clone(), Limits::default()).unwrap();
    assert_eq!(
        weekend_db::storage::import_logical_on(&logical, destination.clone(), limits(4096)),
        Err(Error::Limit)
    );
    assert!(destination.durable_image().unwrap().is_empty());
    assert_eq!(disk.durable_image().unwrap(), image);
    let db = Database::open_on(disk.clone(), limits(1024 * 1024)).unwrap();
    assert!(db.memory_usage().live_bytes < db.memory_usage().peak_bytes);
    assert!(db.memory_usage().peak_bytes <= 1024 * 1024);
    drop(db);
    disk.crash(Crash::DropAll).unwrap();
    assert_eq!(disk.durable_image().unwrap(), image);
}

#[test]
fn budget_exhaustion_before_publication_preserves_head_and_can_retry() {
    let db = Database::with_limits(limits(32768)).unwrap();
    let c = db.create_collection().unwrap();
    let revision = db.snapshot().revision();
    let mut tx = db.begin();
    tx.put(c, b"row", &db.integer(3).unwrap()).unwrap();
    let mut pinned = Vec::new();
    while let Ok(value) = db.bytes(&[0; 128]) {
        pinned.push(value);
    }
    assert_eq!(tx.commit(), Err(Error::Limit));
    assert_eq!(db.snapshot().revision(), revision);
    assert!(db.snapshot().get(c, b"row").unwrap().is_none());
    assert!(!db.requires_recovery());
    drop(tx);
    drop(pinned);
    let mut retry = db.begin();
    retry.put(c, b"row", &db.integer(3).unwrap()).unwrap();
    assert_eq!(retry.commit(), Ok(revision + 1));
}

#[test]
fn concurrent_drafts_share_the_database_budget() {
    let db = Database::with_limits(limits(4096)).unwrap();
    let c = db.create_collection().unwrap();
    let baseline = db.memory_usage().live_bytes;
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8u8)
            .map(|key| {
                let db = &db;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    let mut draft = db.begin();
                    let result = db
                        .bytes(&[key; 512])
                        .and_then(|value| draft.put(c, &[key], &value));
                    assert!(result.is_ok() || result == Err(Error::Limit));
                    assert!(db.memory_usage().live_bytes <= 4096);
                    barrier.wait(); // successful drafts remain pinned until all finish admission
                    result.is_ok()
                })
            })
            .collect();
        let successes = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|v| *v)
            .count();
        assert!(successes > 0 && successes < 8);
    });
    assert_eq!(db.memory_usage().live_bytes, baseline);
    assert!(db.memory_usage().peak_bytes <= 4096);
}
