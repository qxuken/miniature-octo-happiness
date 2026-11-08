use weekend_db::{
    Database, Limits,
    storage::{self, Crash, SimulatedDisk},
};

// Frozen image from the first assurance checkpoint. Do not regenerate this
// fixture with the reader under test when changing the file format.
const PRE1: &[u8] = include_bytes!("fixtures/pre1-v2.wdb");

#[test]
fn archived_v2_image_can_be_read_updated_and_copied_with_retained_history() {
    let disk = SimulatedDisk::from_image(PRE1).unwrap();
    let info = storage::inspect_on(disk.clone(), Limits::default()).unwrap();
    assert_eq!((info.format_version, info.revision), (2, 4));
    assert_eq!(disk.durable_image().unwrap(), PRE1);
    {
        let db = Database::open_on(disk.clone(), Limits::default()).unwrap();
        let collection = db.collection_id(1);
        assert_eq!(
            db.snapshot()
                .get(collection, &[0])
                .unwrap()
                .unwrap()
                .as_i64(),
            Ok(21)
        );
        assert_eq!(
            db.snapshot()
                .get(collection, &[1])
                .unwrap()
                .unwrap()
                .as_i64(),
            Ok(12)
        );
        let retained = db.retained_snapshot(1).unwrap();
        assert_eq!(retained.revision(), 2);
        assert_eq!(
            retained.get(collection, &[0]).unwrap().unwrap().as_i64(),
            Ok(11)
        );
        let mut tx = db.begin();
        tx.put(collection, &[0], &db.integer(31).unwrap()).unwrap();
        assert_eq!(tx.commit(), Ok(5));
    }
    disk.crash(Crash::DropAll).unwrap();
    let restored = SimulatedDisk::new().unwrap();
    storage::backup_on(disk, restored.clone(), Limits::default()).unwrap();
    let db = Database::open_on(restored, Limits::default()).unwrap();
    let collection = db.collection_id(1);
    assert_eq!(db.snapshot().revision(), 5);
    assert_eq!(
        db.snapshot()
            .get(collection, &[0])
            .unwrap()
            .unwrap()
            .as_i64(),
        Ok(31)
    );
    assert_eq!(
        db.retained_snapshot(1)
            .unwrap()
            .get(collection, &[0])
            .unwrap()
            .unwrap()
            .as_i64(),
        Ok(11)
    );
}
