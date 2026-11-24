#![cfg(target_os = "linux")]

use std::{
    fs,
    path::{Path, PathBuf},
};
use weekend_db::{
    Database, Error, Limits,
    storage::{FileStore, SimulatedDisk, StorageFailure, backup_file, inspect_file},
};

const NATIVE_ADAPTER: bool = cfg!(any(target_arch = "x86_64", target_arch = "aarch64"));

struct Directory(PathBuf);

impl Directory {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("weekend-abi-{test}-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_unsupported<T>(result: Result<T, StorageFailure>) {
    assert_eq!(
        result.err(),
        Some(StorageFailure {
            error: Error::UnsupportedPlatform,
            native: None,
            verification: None,
        })
    );
}

#[test]
fn database_create_checks_entropy_architecture_before_path_conversion() {
    if NATIVE_ADAPTER {
        let dir = Directory::new("entropy");
        let path = dir.0.join("database");
        let db = Database::create_file_with_diagnostics(&path, Limits::default()).unwrap();
        let collection = db.create_collection().unwrap();
        let mut transaction = db.begin();
        transaction
            .put(collection, b"native", &db.integer(11).unwrap())
            .unwrap();
        let revision = transaction.commit().unwrap();
        drop(transaction);
        drop(db);
        let reopened = Database::open_file_with_diagnostics(&path, Limits::default()).unwrap();
        assert_eq!(reopened.snapshot().revision(), revision);
        assert_eq!(
            reopened
                .snapshot()
                .get(reopened.collection_id(collection.as_u64()), b"native")
                .unwrap()
                .unwrap()
                .as_i64(),
            Ok(11)
        );
    } else {
        struct UnreachablePath;
        impl AsRef<Path> for UnreachablePath {
            fn as_ref(&self) -> &Path {
                panic!("unsupported Linux target evaluated the create pathname");
            }
        }
        assert_unsupported(Database::create_file_with_diagnostics(
            UnreachablePath,
            Limits::default(),
        ));
    }
}

#[test]
fn file_adapter_checks_architecture_before_path_admission() {
    let dir = Directory::new("file");
    let present = dir.0.join("present");
    let absent = dir.0.join("absent");
    let invalid = dir.0.join("bad\0name");
    fs::write(&present, b"unchanged").unwrap();
    if NATIVE_ADAPTER {
        assert_eq!(
            FileStore::open_with_diagnostics(&invalid)
                .err()
                .unwrap()
                .error,
            Error::Invalid
        );
        assert_eq!(
            FileStore::open_with_diagnostics(&absent)
                .err()
                .unwrap()
                .error,
            Error::NotFound
        );
        assert_eq!(
            FileStore::create_with_diagnostics(&present)
                .err()
                .unwrap()
                .error,
            Error::AlreadyExists
        );
        drop(FileStore::create_with_diagnostics(&absent).unwrap());
        drop(FileStore::open_read_only_with_diagnostics(&present).unwrap());
    } else {
        assert_unsupported(FileStore::create_with_diagnostics(&absent));
        assert_unsupported(FileStore::open_with_diagnostics(&present));
        assert_unsupported(FileStore::open_read_only_with_diagnostics(&invalid));
        assert_unsupported(Database::open_file_with_diagnostics(
            &present,
            Limits::default(),
        ));
        assert_unsupported(Database::compact_file_with_diagnostics(
            &present,
            Limits::default(),
        ));
        assert_eq!(
            inspect_file(&present, Limits::default()).err(),
            Some(Error::UnsupportedPlatform)
        );
        assert_eq!(
            backup_file(&present, &absent, Limits::default()).err(),
            Some(Error::UnsupportedPlatform)
        );
        assert!(!absent.exists());
    }
    assert_eq!(fs::read(&present).unwrap(), b"unchanged");
}

#[test]
fn portable_engine_commits_and_reopens_on_each_linux_architecture() {
    let db = Database::new().unwrap();
    let collection = db.create_collection().unwrap();
    let mut transaction = db.begin();
    transaction
        .put(collection, b"key", &db.integer(17).unwrap())
        .unwrap();
    let revision = transaction.commit().unwrap();
    assert_eq!(db.snapshot().revision(), revision);
    assert_eq!(
        db.snapshot()
            .get(collection, b"key")
            .unwrap()
            .unwrap()
            .as_i64(),
        Ok(17)
    );

    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), *b"architecture-abi", Limits::default()).unwrap();
    let collection = db.create_collection().unwrap();
    let mut transaction = db.begin();
    transaction
        .put(collection, b"key", &db.integer(19).unwrap())
        .unwrap();
    let revision = transaction.commit().unwrap();
    drop(transaction);
    drop(db);
    let reopened = Database::open_on(disk, Limits::default()).unwrap();
    assert_eq!(reopened.snapshot().revision(), revision);
    assert_eq!(
        reopened
            .snapshot()
            .get(reopened.collection_id(collection.as_u64()), b"key")
            .unwrap()
            .unwrap()
            .as_i64(),
        Ok(19)
    );
}
