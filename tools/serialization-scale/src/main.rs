use std::{collections::BTreeSet, fs, path::Path, time::Instant};
use weekend_db::{Database, Error, Limits, Snapshot, storage};
fn payload(epoch: u64) -> [u8; 16] {
    let mut bytes = [0x5a; 16];
    bytes[..8].copy_from_slice(&epoch.to_be_bytes());
    bytes
}
fn limits() -> Limits {
    Limits {
        max_entries: 4096,
        max_writes: 4096,
        max_observations: 4096,
        max_live_bytes: 128 * 1024 * 1024,
        ..Limits::default()
    }
}
fn root(db: &Database, view: Snapshot<'_>, width: u64, epoch: u64) {
    let rows = view.scan(db.collection_id(1)).unwrap();
    assert_eq!(rows.len(), width as usize);
    let mut seen = BTreeSet::new();
    for (key, value) in rows {
        let id = u64::from_be_bytes(key.try_into().unwrap());
        assert!(id < width && seen.insert(id));
        assert_eq!(value.as_bytes().unwrap(), payload(epoch));
    }
    assert_eq!(seen, (0..width).collect());
}
fn verify(db: &Database, width: u64, epochs: u64) {
    assert_eq!(db.snapshot().revision(), 1 + 2 * epochs);
    root(db, db.snapshot(), width, epochs);
    assert_eq!(db.retained_ids().unwrap(), (1..=epochs).collect::<Vec<_>>());
    for epoch in 1..=epochs {
        let view = db.retained_snapshot(epoch).unwrap();
        assert_eq!(view.revision(), 2 * epoch);
        root(db, view, width, epoch);
    }
    assert!(!db.requires_recovery());
}
fn measured<T>(phase: &str, operation: impl FnOnce() -> T) -> T {
    println!("{{\"event\":\"start\",\"phase\":\"{phase}\"}}");
    let start = Instant::now();
    let result = operation();
    println!(
        "{{\"event\":\"end\",\"phase\":\"{phase}\",\"ns\":{}}}",
        start.elapsed().as_nanos()
    );
    result
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!([4, 5].contains(&args.len()));
    let requested: u64 = args.get(4).map_or(8, |s| s.parse().unwrap());
    let history = args.len() == 5;
    let shared = match args[1].as_str() {
        "shared" => true,
        "distinct" => false,
        _ => panic!("shape"),
    };
    let width: u64 = args[2].parse().unwrap();
    if history {
        assert_eq!(width, 8);
        assert!([1, 8, 32, 64].contains(&requested));
    } else {
        assert!([128, 512, 2048].contains(&width));
    }
    let directory = Path::new(&args[3]);
    fs::create_dir(directory).unwrap();
    let path = directory.join("source.wdb");
    let db = measured("create", || Database::create_file(&path, limits()).unwrap());
    let collection = db.create_collection().unwrap();
    let mut epochs = 0;
    let mut limited_at = 0;
    let mut limit_headroom = 0;
    let mut limit_core_live = 0;
    let mut limit_core_peak = 0;
    for epoch in 1..=requested {
        let before_attempt = fs::metadata(&path).unwrap().len();
        let mut tx = db.begin();
        let value = db.bytes(&payload(epoch)).unwrap();
        for key in 0..width {
            let cell = if shared {
                value.clone()
            } else {
                db.bytes(&payload(epoch)).unwrap()
            };
            tx.put(collection, &key.to_be_bytes(), &cell).unwrap();
        }
        let committed = measured("commit", || tx.commit());
        drop(tx);
        match committed {
            Ok(revision) => assert_eq!(revision, 2 * epoch),
            Err(Error::Limit) => {
                assert!(!history && !shared && width == 2048 && epoch == 8 && epochs == 7);
                assert!(
                    before_attempt + width * storage::REGION as u64
                        > storage::MAX_IMAGE_BYTES as u64
                );
                limit_headroom = db.storage_space().unwrap().remaining_append_bytes;
                assert!(width * storage::REGION as u64 > limit_headroom);
                let usage = db.memory_usage();
                limit_core_live = usage.live_bytes;
                limit_core_peak = usage.peak_bytes;
                assert_eq!(fs::metadata(&path).unwrap().len(), before_attempt);
                measured("verify-limit", || verify(&db, width, epochs));
                limited_at = epoch;
                break;
            }
            Err(other) => panic!("unexpected commit error: {other:?}"),
        }
        measured("retain", || {
            assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), epoch)
        });
        measured("verify", || verify(&db, width, epoch));
        epochs = epoch;
    }
    assert_eq!(
        (epochs, limited_at),
        if !shared && width == 2048 {
            (7, 8)
        } else {
            (requested, 0)
        }
    );
    drop(db);
    let db = measured("reopen", || Database::open_file(&path, limits()).unwrap());
    measured("verify-reopen", || verify(&db, width, epochs));
    drop(db);
    let before = fs::metadata(&path).unwrap().len();
    let backup = directory.join("backup.wdb");
    measured("backup", || {
        storage::backup_file(&path, &backup, limits()).unwrap()
    });
    let restored = directory.join("restored.wdb");
    measured("restore", || {
        storage::restore_file(&backup, &restored, limits()).unwrap()
    });
    for file in [&backup, &restored] {
        let db = measured("reopen-copy", || {
            Database::open_file(file, limits()).unwrap()
        });
        measured("verify-copy", || verify(&db, width, epochs));
    }
    measured("compact", || {
        Database::compact_file(&path, limits()).unwrap()
    });
    let db = measured("reopen-compact", || {
        Database::open_file(&path, limits()).unwrap()
    });
    measured("verify-compact", || verify(&db, width, epochs));
    drop(db);
    #[cfg(feature = "probe")]
    let (value_comparisons, data_comparisons) = storage::serialization_probe_counts();
    #[cfg(not(feature = "probe"))]
    let (value_comparisons, data_comparisons) = (0u64, 0u64);
    println!(
        "{{\"event\":\"complete\",\"verified\":true,\"width\":{width},\"epochs\":{epochs},\"limited_at\":{limited_at},\"limit_headroom\":{limit_headroom},\"limit_core_live\":{limit_core_live},\"limit_core_peak\":{limit_core_peak},\"shared\":{shared},\"before_bytes\":{before},\"after_bytes\":{},\"value_comparisons\":{value_comparisons},\"data_comparisons\":{data_comparisons}}}",
        fs::metadata(path).unwrap().len()
    );
}
