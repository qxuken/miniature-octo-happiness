//! Bounded Linux native error drill; invoked only in tools/native_faults.py's tmpfs.
use std::{fs, io::Write, path::Path};
use weekend_db::{Database, Error, Limits, storage::NativeIoCategory};
fn main() {
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
    let mut ballast = fs::File::create(&ballast_path).unwrap();
    let block = [0xa5u8; 65536];
    let mut full = false;
    // Hard upper bound independent of the mount configuration. Never attempt
    // this example on a host filesystem: the runner verifies private tmpfs.
    for _ in 0..256 {
        if let Err(error) = ballast.write_all(&block) {
            assert_eq!(error.kind(), std::io::ErrorKind::StorageFull);
            full = true;
            break;
        }
    }
    assert!(full, "private tmpfs must fill within 16 MiB bound");
    drop(ballast);
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
    fs::remove_file(ballast_path).unwrap();
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
}
