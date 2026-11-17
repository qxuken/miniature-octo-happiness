use weekend_db::{
    Database, Error, Limits,
    storage::{self, Crash, Fault, IoKind, SimulatedDisk},
};

fn fixture() -> Vec<u8> {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"a", &db.integer(1).unwrap()).unwrap();
    tx.commit().unwrap();
    db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    tx.put(c, b"a", &db.integer(2).unwrap()).unwrap();
    tx.commit().unwrap();
    disk.durable_image().unwrap()
}
fn unchanged(disk: &SimulatedDisk, original: &[u8]) {
    assert_eq!(disk.durable_image().unwrap(), original);
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
    );
}
fn verify_rows(disk: SimulatedDisk) {
    let db = Database::open_on(disk, Limits::default()).unwrap();
    let c = db.collection_id(1);
    assert_eq!(db.snapshot().revision(), 4);
    assert_eq!(db.snapshot().get(c, b"a").unwrap().unwrap().as_i64(), Ok(2));
    let old = db.retained_snapshot(1).unwrap();
    assert_eq!(old.revision(), 2);
    assert_eq!(old.get(c, b"a").unwrap().unwrap().as_i64(), Ok(1));
    assert_eq!(db.retain_snapshot(&old), Ok(2));
}
#[test]
fn inspection_verifies_all_roots_without_stabilizing_or_writing() {
    let original = fixture();
    for witness in [true, false] {
        let mut image = original.clone();
        if !witness {
            image[3 * storage::REGION..4 * storage::REGION].fill(0);
        }
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let info = storage::inspect_on(disk.clone(), Limits::default()).unwrap();
        assert_eq!(info.identity, [7; 16]);
        assert_eq!((info.revision, info.data_revision), (4, 4));
        assert_eq!(info.witnessed, witness);
        assert_eq!(info.retained.len(), 1);
        assert_eq!((info.retained[0].id, info.retained[0].revision), (1, 2));
        unchanged(&disk, &image);
        disk.crash(Crash::DropAll).unwrap();
        assert_eq!(storage::inspect_on(disk, Limits::default()).unwrap(), info);
    }
    let mut damaged = original;
    damaged[0] ^= 1;
    let disk = SimulatedDisk::from_image(&damaged).unwrap();
    assert_eq!(
        storage::inspect_on(disk.clone(), Limits::default()),
        Err(Error::UnsupportedFormat)
    );
    unchanged(&disk, &damaged);
}
#[test]
fn backup_preserves_history_and_rejects_busy_or_nonempty_stores() {
    let image = fixture();
    let source = SimulatedDisk::from_image(&image).unwrap();
    let target = SimulatedDisk::new().unwrap();
    storage::backup_on(source.clone(), target.clone(), Limits::default()).unwrap();
    unchanged(&source, &image);
    target.crash(Crash::DropAll).unwrap();
    let info = storage::inspect_on(target.clone(), Limits::default()).unwrap();
    assert_eq!(info.identity, [7; 16]);
    verify_rows(target);
    let existing = SimulatedDisk::from_image(&image).unwrap();
    assert_eq!(
        storage::backup_on(source.clone(), existing.clone(), Limits::default()),
        Err(Error::Invalid)
    );
    unchanged(&existing, &image);
    let db = Database::open_on(source.clone(), Limits::default()).unwrap();
    assert_eq!(
        storage::inspect_on(source.clone(), Limits::default()),
        Err(Error::Busy)
    );
    assert_eq!(
        storage::backup_on(source, SimulatedDisk::new().unwrap(), Limits::default()),
        Err(Error::Busy)
    );
    drop(db);
}
#[test]
fn backup_faults_preserve_source_and_never_report_partial_success() {
    let image = fixture();
    let clean = SimulatedDisk::new().unwrap();
    storage::backup_on(
        SimulatedDisk::from_image(&image).unwrap(),
        clean.clone(),
        Limits::default(),
    )
    .unwrap();
    let steps = clean.trace().unwrap().len();
    for step in 0..steps {
        let source = SimulatedDisk::from_image(&image).unwrap();
        let target = SimulatedDisk::new().unwrap();
        target.arm(step, Fault::Error);
        assert!(
            storage::backup_on(source.clone(), target.clone(), Limits::default()).is_err(),
            "step {step}"
        );
        unchanged(&source, &image);
        target.clear_fault();
        target.crash(Crash::DropAll).unwrap();
        if storage::inspect_on(target.clone(), Limits::default()).is_ok() {
            verify_rows(target);
        }
        assert_eq!(
            storage::inspect_on(source, Limits::default())
                .unwrap()
                .revision,
            4
        );
    }
}

#[test]
fn inspection_and_backup_source_read_faults_release_lease_without_writes() {
    let image = fixture();
    let clean = SimulatedDisk::from_image(&image).unwrap();
    storage::inspect_on(clean.clone(), Limits::default()).unwrap();
    for step in 0..clean.trace().unwrap().len() {
        let source = SimulatedDisk::from_image(&image).unwrap();
        source.arm(step, Fault::Error);
        assert_eq!(
            storage::inspect_on(source.clone(), Limits::default()),
            Err(Error::Io)
        );
        unchanged(&source, &image);
        source.clear_fault();
        assert_eq!(
            storage::inspect_on(source.clone(), Limits::default())
                .unwrap()
                .revision,
            4
        );
        source.arm(step, Fault::Error);
        let target = SimulatedDisk::new().unwrap();
        assert_eq!(
            storage::backup_on(source.clone(), target.clone(), Limits::default()),
            Err(Error::Io)
        );
        unchanged(&source, &image);
        assert!(target.durable_image().unwrap().is_empty());
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[test]
fn native_backup_restore_and_check_run_in_independent_processes() {
    use std::{fs, process::Command};
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let path = std::env::temp_dir().join(format!("weekend-inspection-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let directory = Directory(path);
    let source = directory.0.join("source.wdb");
    let backup = directory.0.join("backup.wdb");
    let restored = directory.0.join("restored.wdb");
    let image = fixture();
    fs::write(&source, &image).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o400)).unwrap();
    }
    for (command, args) in [
        ("check", vec![&source]),
        ("backup", vec![&source, &backup]),
        ("restore", vec![&backup, &restored]),
        ("check", vec![&restored]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_weekend-admin"))
            .arg(command)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(fs::read(&source).unwrap(), image);
    assert_eq!(
        storage::backup_file(&source, &backup, Limits::default()),
        Err(Error::AlreadyExists)
    );
    let db = Database::open_file(&restored, Limits::default()).unwrap();
    let c = db.collection_id(1);
    assert_eq!(db.snapshot().get(c, b"a").unwrap().unwrap().as_i64(), Ok(2));
    assert_eq!(
        db.retained_snapshot(1)
            .unwrap()
            .get(c, b"a")
            .unwrap()
            .unwrap()
            .as_i64(),
        Ok(1)
    );
    assert_eq!(
        storage::inspect_file(&restored, Limits::default()),
        Err(Error::Busy)
    );
    drop(db);
    let broken = directory.0.join("broken.wdb");
    let absent = directory.0.join("must-not-exist.wdb");
    fs::write(&broken, b"corrupt").unwrap();
    assert!(storage::restore_file(&broken, &absent, Limits::default()).is_err());
    assert!(!absent.exists());
}
