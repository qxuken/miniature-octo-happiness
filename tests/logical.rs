use weekend_db::{
    Database, Error, Limits,
    storage::{self, Crash, IoKind, SimulatedDisk},
};
fn fixture() -> Vec<u8> {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [3; 16], Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut list = db.list().unwrap();
    for value in [
        db.none().unwrap(),
        db.boolean(true).unwrap(),
        db.integer(-9).unwrap(),
        db.unsigned(u64::MAX).unwrap(),
        db.float(f64::from_bits(0x7ff8000000000017)).unwrap(),
        db.bytes(b"\0unknown").unwrap(),
        db.map().unwrap(),
        db.list().unwrap(),
    ] {
        list = list.list_push(&value).unwrap();
    }
    let value = db
        .map()
        .unwrap()
        .map_set(b"z", &list)
        .unwrap()
        .map_set(b"a", &list)
        .unwrap();
    let mut tx = db.begin();
    tx.put(c, b"key", &value).unwrap();
    tx.commit().unwrap();
    assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(1));
    db.begin().commit().unwrap();
    assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(2));
    db.release_retained(1).unwrap();
    let unused = db.create_collection().unwrap();
    db.drop_collection(unused).unwrap();
    disk.durable_image().unwrap()
}
fn crc(bytes: &[u8]) -> u64 {
    let mut crc = 0;
    for b in bytes {
        crc ^= u64::from(*b) << 56;
        for _ in 0..8 {
            crc = (crc << 1) ^ (0x42f0_e1eb_a9ea_3693 & 0u64.wrapping_sub(crc >> 63));
        }
    }
    crc
}
fn sign(bytes: &mut [u8]) {
    let end = bytes.len() - 8;
    let checksum = crc(&bytes[..end]);
    bytes[end..].copy_from_slice(&checksum.to_le_bytes());
}
#[test]
fn logical_export_import_preserves_values_roots_revisions_and_future_ids() {
    let image = fixture();
    let source = SimulatedDisk::from_image(&image).unwrap();
    let bytes = storage::export_logical_on(source.clone(), Limits::default()).unwrap();
    assert_eq!(source.durable_image().unwrap(), image);
    assert!(
        source
            .trace()
            .unwrap()
            .iter()
            .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
    );
    let target = SimulatedDisk::new().unwrap();
    storage::import_logical_on(&bytes, target.clone(), Limits::default()).unwrap();
    target.crash(Crash::DropAll).unwrap();
    assert_eq!(
        storage::export_logical_on(target.clone(), Limits::default()).unwrap(),
        bytes
    );
    let db = Database::open_on(target, Limits::default()).unwrap();
    assert_eq!(db.retained_ids().unwrap(), [2]);
    let view = db.retained_snapshot(2).unwrap();
    assert_eq!(view.revision(), 4);
    let value = view.get(db.collection_id(1), b"key").unwrap().unwrap();
    let list = value.map_get(b"a").unwrap().unwrap();
    assert_eq!(list.list_get(2).unwrap().as_i64(), Ok(-9));
    assert_eq!(
        list.list_get(4).unwrap().as_f64().unwrap().to_bits(),
        0x7ff8000000000017
    );
    assert_eq!(db.retain_snapshot(&view), Ok(3));
    assert_eq!(db.create_collection().unwrap().as_u64(), 3);
}
#[test]
fn malformed_logical_streams_never_touch_destination() {
    let valid = storage::export_logical_on(
        SimulatedDisk::from_image(&fixture()).unwrap(),
        Limits::default(),
    )
    .unwrap();
    let mut cases = Vec::new();
    for n in 0..valid.len() {
        cases.push((valid[..n].to_vec(), None));
    }
    for (offset, value, error) in [
        (8, 2, Error::UnsupportedFormat),
        (48, 0, Error::Corrupt),
        (56, 65, Error::Limit),
        (64, 0, Error::Corrupt),
        (72, u64::MAX, Error::Corrupt),
        (88, 66, Error::Limit),
    ] {
        let mut bytes = valid.clone();
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        sign(&mut bytes);
        cases.push((bytes, Some(error)));
    }
    let mut trailing = valid.clone();
    let end = trailing.len() - 8;
    trailing.insert(end, 0);
    sign(&mut trailing);
    cases.push((trailing, Some(Error::Corrupt)));
    for (bytes, expected) in cases {
        let target = SimulatedDisk::new().unwrap();
        let result = storage::import_logical_on(&bytes, target.clone(), Limits::default());
        assert!(result.is_err());
        if let Some(error) = expected {
            assert_eq!(result, Err(error));
        }
        assert!(target.durable_image().unwrap().is_empty());
        assert!(target.trace().unwrap().is_empty());
    }
    let target = SimulatedDisk::new().unwrap();
    assert_eq!(
        storage::import_logical_on(
            &valid,
            target.clone(),
            Limits {
                max_depth: 1,
                ..Limits::default()
            }
        ),
        Err(Error::Limit)
    );
    assert!(target.trace().unwrap().is_empty());
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[test]
fn logical_cli_export_import_and_invalid_input_preserve_files() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let directory = Directory(std::env::temp_dir().join(format!(
            "logical-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    fs::create_dir(&directory.0).unwrap();
    let source = directory.0.join("source");
    let logical = directory.0.join("logical");
    let restored = directory.0.join("restored");
    let image = fixture();
    fs::write(&source, &image).unwrap();
    for (command, from, to) in [
        ("export", &source, &logical),
        ("import", &logical, &restored),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_weekend-admin"))
            .arg(command)
            .arg(from)
            .arg(to)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(fs::read(&source).unwrap(), image);
    let bytes = fs::read(&logical).unwrap();
    assert_eq!(
        storage::export_logical_file(&restored, Limits::default()).unwrap(),
        bytes
    );
    let before = fs::read(&restored).unwrap();
    assert_eq!(
        storage::import_logical_file(&bytes, &restored, Limits::default()),
        Err(Error::AlreadyExists)
    );
    assert_eq!(fs::read(&restored).unwrap(), before);
    let absent = directory.0.join("absent");
    let mut broken = bytes;
    broken[0] ^= 1;
    assert_eq!(
        storage::import_logical_file(&broken, &absent, Limits::default()),
        Err(Error::UnsupportedFormat)
    );
    assert!(!absent.exists());
}

#[test]
fn version_one_fixture_replays_with_current_and_retained_values() {
    let bytes = include_bytes!("fixtures/logical-v1.bin");
    let target = SimulatedDisk::new().unwrap();
    storage::import_logical_on(bytes, target.clone(), Limits::default()).unwrap();
    assert_eq!(
        storage::export_logical_on(target.clone(), Limits::default()).unwrap(),
        bytes
    );
    let db = Database::open_on(target, Limits::default()).unwrap();
    let c = db.collection_id(1);
    assert_eq!(db.snapshot().revision(), 4);
    assert_eq!(
        db.snapshot().get(c, &[0]).unwrap().unwrap().as_i64(),
        Ok(21)
    );
    assert_eq!(
        db.retained_snapshot(1)
            .unwrap()
            .get(c, &[0])
            .unwrap()
            .unwrap()
            .as_i64(),
        Ok(11)
    );
}
