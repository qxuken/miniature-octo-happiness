use weekend_db::{
    Database, Error, Limits,
    storage::{self, Fault, IoKind, REGION, SimulatedDisk},
};

fn fixture() -> Vec<u8> {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [77; 16], Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"n", &db.unsigned(10).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    tx.put(c, b"n", &db.unsigned(20).unwrap()).unwrap();
    tx.commit().unwrap();
    disk.durable_image().unwrap()
}
fn unchanged(disk: &SimulatedDisk, bytes: &[u8]) {
    assert_eq!(disk.durable_image().unwrap(), bytes);
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|event| !matches!(event.kind, IoKind::Write | IoKind::Sync))
    );
}
#[test]
fn salvage_selects_highest_complete_graph_without_changing_strict_recovery() {
    for mode in 0..4 {
        let mut bytes = fixture();
        let expected = match mode {
            1 => {
                let descriptor = bytes.len() - REGION;
                bytes[descriptor + 64] ^= 1;
                3
            }
            2 => {
                bytes[3 * REGION..5 * REGION].fill(0);
                4
            }
            3 => {
                bytes[REGION..2 * REGION].fill(0);
                3
            }
            _ => 4,
        };
        let source = SimulatedDisk::from_image(&bytes).unwrap();
        if mode == 1 || mode == 3 {
            assert_eq!(
                storage::inspect_on(source.clone(), Limits::default()),
                Err(Error::Corrupt)
            );
        }
        let destination = SimulatedDisk::new().unwrap();
        let report =
            storage::salvage_on(source.clone(), destination.clone(), Limits::default()).unwrap();
        assert_eq!(report.revision, expected);
        assert!(report.acknowledged_data_loss_possible);
        assert_eq!(report.retained_roots, 1);
        unchanged(&source, &bytes);
        let db = Database::open_on(destination, Limits::default()).unwrap();
        assert_eq!(db.snapshot().revision(), expected);
        let c = db.collection_id(1);
        assert_eq!(
            db.snapshot().get(c, b"n").unwrap().unwrap().as_u64(),
            Ok(if expected == 4 { 20 } else { 10 })
        );
        assert_eq!(
            db.retained_snapshot(1)
                .unwrap()
                .get(c, b"n")
                .unwrap()
                .unwrap()
                .as_u64(),
            Ok(10)
        );
    }
}
#[test]
fn salvage_requires_verified_graph_and_never_treats_io_failure_as_damage() {
    let clean = fixture();
    for mode in 0..4 {
        let mut bytes = clean.clone();
        if mode == 0 {
            bytes[REGION..3 * REGION].fill(0);
        }
        if mode == 1 {
            bytes[0] ^= 1;
        }
        if mode == 3 {
            for slot in [1, 2] {
                let start = slot * REGION + 40;
                let descriptor =
                    u64::from_le_bytes(bytes[start..start + 8].try_into().unwrap()) as usize;
                bytes[descriptor + 64] ^= 1;
            }
        }
        let source = SimulatedDisk::from_image(&bytes).unwrap();
        if mode == 2 {
            source.arm(0, Fault::Error);
        }
        let destination = SimulatedDisk::new().unwrap();
        assert_eq!(
            storage::salvage_on(source.clone(), destination.clone(), Limits::default()),
            Err(match mode {
                0 => Error::Corrupt,
                1 => Error::UnsupportedFormat,
                2 => Error::Io,
                _ => Error::Corrupt,
            })
        );
        source.clear_fault();
        unchanged(&source, &bytes);
        assert!(destination.durable_image().unwrap().is_empty());
    }
    let existing = SimulatedDisk::from_image(&clean).unwrap();
    assert_eq!(
        storage::salvage_on(
            SimulatedDisk::from_image(&clean).unwrap(),
            existing.clone(),
            Limits::default()
        ),
        Err(Error::Invalid)
    );
    unchanged(&existing, &clean);
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[test]
fn salvage_admin_creates_only_a_new_destination_after_verification() {
    use std::{fs, process::Command};
    let directory = std::env::temp_dir().join(format!("weekend-salvage-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let source = directory.join("source");
    let target = directory.join("target");
    let mut bytes = fixture();
    let root = bytes.len() - REGION;
    bytes[root + 64] ^= 1;
    fs::write(&source, &bytes).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_weekend-admin"))
        .arg("salvage")
        .arg(&source)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("acknowledged data loss possible"));
    assert_eq!(
        storage::inspect_file(&target, Limits::default())
            .unwrap()
            .revision,
        3
    );
    assert_eq!(
        storage::salvage_file(&source, &target, Limits::default()),
        Err(Error::AlreadyExists)
    );
    assert_eq!(fs::read(&source).unwrap(), bytes);
    fs::write(&source, b"unreadable").unwrap();
    let missing = directory.join("not-created");
    assert!(storage::salvage_file(&source, &missing, Limits::default()).is_err());
    assert!(!missing.exists());
    fs::remove_dir_all(directory).unwrap();
}
