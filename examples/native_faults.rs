//! Bounded Linux native error drill; invoked only in tools/native_faults.py's tmpfs.
use std::{fs, io::Write, path::Path};
use weekend_db::{
    Database, Error, Limits,
    storage::{self, NativeIoCategory},
};

fn fill(directory: &Path) {
    let mut ballast = fs::File::create_new(directory.join("ballast")).unwrap();
    let block = [0xa5u8; 65536];
    // The runner verifies a fresh, private 8 MiB tmpfs. This independent hard
    // ceiling also prevents an accidental unbounded host-filling loop.
    for _ in 0..256 {
        if let Err(error) = ballast.write_all(&block) {
            assert_eq!(error.kind(), std::io::ErrorKind::StorageFull);
            return;
        }
    }
    panic!("private tmpfs must fill within 16 MiB bound");
}

fn check(path: &Path, revision: u64, retained: u64, value: i64) {
    let db = Database::open_file(path, Limits::default()).unwrap();
    let c = db.collection_id(1);
    assert_eq!(db.snapshot().revision(), revision);
    assert_eq!(db.retained_ids().unwrap(), [retained]);
    assert_eq!(
        db.snapshot().get(c, b"row").unwrap().unwrap().as_i64(),
        Ok(value)
    );
    assert!(db.snapshot().get(c, b"payload").unwrap().is_none());
    let old = db.retained_snapshot(retained).unwrap();
    assert_eq!(old.revision(), 2);
    assert_eq!(old.get(c, b"row").unwrap().unwrap().as_i64(), Ok(17));
}
fn main() {
    assert_eq!(weekend_db::version::FEATURES, 0);
    assert_eq!(
        weekend_db::version::SOURCE_ID,
        std::env::var("WEEKEND_DB_SOURCE_ID").expect("runner supplies exact source identity")
    );
    let mode = std::env::args().nth(1).expect("mode");
    let directory = std::env::args().nth(2).expect("private directory");
    let path = Path::new(&directory).join("database.wdb");
    if mode == "permission" {
        let failure = Database::create_file_with_diagnostics(&path, Limits::default())
            .err()
            .expect("non-root child must be denied");
        assert_eq!(
            failure.native.unwrap().category,
            NativeIoCategory::Permission
        );
        assert!(!path.exists());
        println!("permission: {:?}", failure);
        return;
    }
    assert_eq!(mode, "diskfull");
    assert_eq!(std::env::var("WEEKEND_PRIVATE_TMPFS").as_deref(), Ok("1"));
    let db = Database::create_file(&path, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"row", &db.integer(17).unwrap()).unwrap();
    tx.commit().unwrap();
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let acknowledged = db.snapshot().revision();
    let ballast_path = Path::new(&directory).join("ballast");
    let block = [0xa5u8; 65536];
    fill(Path::new(&directory));
    let mut tx = db.begin();
    tx.put(c, b"row", &db.integer(18).unwrap()).unwrap();
    tx.put(c, b"payload", &db.bytes(&block).unwrap()).unwrap();
    let failure = tx.commit_with_diagnostics().unwrap_err();
    assert_eq!(failure.error, Error::Io); // append failed before root publication
    assert_eq!(failure.native.unwrap().category, NativeIoCategory::DiskFull);
    assert_eq!(db.snapshot().revision(), acknowledged);
    assert!(db.requires_recovery());
    println!("diskfull: acknowledged={acknowledged}, {failure:?}");
    drop(db);
    fs::remove_file(&ballast_path).unwrap();
    let db = Database::open_file(&path, Limits::default()).unwrap();
    let c = db.collection_id(1);
    assert_eq!(db.snapshot().revision(), acknowledged);
    assert_eq!(
        db.snapshot().get(c, b"row").unwrap().unwrap().as_i64(),
        Ok(17)
    );
    assert!(db.snapshot().get(c, b"payload").unwrap().is_none());
    assert_eq!(
        db.retained_snapshot(retained)
            .unwrap()
            .get(c, b"row")
            .unwrap()
            .unwrap()
            .as_i64(),
        Ok(17)
    );
    let mut tx = db.begin();
    tx.put(c, b"row", &db.integer(18).unwrap()).unwrap();
    assert_eq!(tx.commit(), Ok(acknowledged + 1));
    drop(db);
    let db = Database::open_file(&path, Limits::default()).unwrap();
    let c = db.collection_id(1);
    assert_eq!(db.snapshot().revision(), acknowledged + 1);
    assert_eq!(
        db.snapshot().get(c, b"row").unwrap().unwrap().as_i64(),
        Ok(18)
    );
    assert_eq!(
        db.retained_snapshot(retained)
            .unwrap()
            .get(c, b"row")
            .unwrap()
            .unwrap()
            .as_i64(),
        Ok(17)
    );
    println!(
        "reopen exact revision {acknowledged}; retry revision {} verified",
        acknowledged + 1
    );
    drop(db);

    // Prepare a valid backup while space is available. All later target paths
    // are fresh; failed copies are retained and retries use different paths.
    let directory = Path::new(&directory);
    let backup = directory.join("good-backup.wdb");
    storage::backup_file(&path, &backup, Limits::default()).unwrap();
    let source_image = fs::read(&path).unwrap();
    let backup_image = fs::read(&backup).unwrap();
    fill(directory);
    for (name, restoring) in [("full-backup.wdb", false), ("full-restore.wdb", true)] {
        let target = directory.join(name);
        let failure = storage::with_storage_diagnostics(|| {
            if restoring {
                storage::restore_file(&backup, &target, Limits::default())
            } else {
                storage::backup_file(&path, &target, Limits::default())
            }
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        assert_eq!(failure.native.unwrap().category, NativeIoCategory::DiskFull);
        assert_eq!(fs::read(&path).unwrap(), source_image);
        assert_eq!(fs::read(&backup).unwrap(), backup_image);
        println!("maintenance {name}: {failure:?}");
    }
    // Existing valid copies must not be overwritten even on a full filesystem.
    assert_eq!(
        storage::restore_file(&path, &backup, Limits::default()),
        Err(Error::AlreadyExists)
    );
    assert_eq!(fs::read(&backup).unwrap(), backup_image);
    let failure =
        storage::with_storage_diagnostics(|| Database::compact_file(&path, Limits::default()))
            .unwrap_err();
    assert_eq!(failure.error, Error::Io);
    assert_eq!(failure.native.unwrap().category, NativeIoCategory::DiskFull);
    println!("maintenance compact: {failure:?}");
    fs::remove_file(ballast_path).unwrap();
    check(&path, acknowledged + 1, retained, 18);
    check(&backup, acknowledged + 1, retained, 18);
    for name in ["full-backup.wdb", "full-restore.wdb"] {
        let partial = directory.join(name);
        if partial.exists() {
            assert!(storage::inspect_file(&partial, Limits::default()).is_err());
        }
    }
    let copied = directory.join("retry-backup.wdb");
    let restored = directory.join("retry-restore.wdb");
    storage::backup_file(&path, &copied, Limits::default()).unwrap();
    storage::restore_file(&backup, &restored, Limits::default()).unwrap();
    Database::compact_file(&path, Limits::default()).unwrap();
    for file in [&path, &copied, &restored] {
        check(file, acknowledged + 1, retained, 18);
    }
    println!(
        "maintenance: backup/restore/compact ENOSPC preserve current+retained roots; fresh retries verified"
    );
}
