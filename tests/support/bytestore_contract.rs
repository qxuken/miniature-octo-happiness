use weekend_db::{
    Database, Error, Limits, Result, Snapshot,
    storage::{ByteStore, backup_on, inspect_on},
};

pub const ID: [u8; 16] = *b"adapter-test-001";

pub fn seed(db: &Database) {
    assert_eq!(db.create_collection().unwrap().as_u64(), 1);
    assert_eq!(write_value(db, 1), Ok(2));
    assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(1));
    check_views(db, 3, 2, 1);
}

pub fn write_value(db: &Database, value: u8) -> Result<u64> {
    let collection = db.collection_id(1);
    let mut draft = db.begin();
    draft.put(collection, b"number", &db.integer(i64::from(value))?)?;
    draft.put(collection, b"blob", &db.bytes(&[value; 6_200])?)?;
    draft.commit()
}

fn check_value(db: &Database, view: &Snapshot<'_>, value: u8) {
    let collection = db.collection_id(1);
    let rows = view.scan(collection).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, b"blob");
    assert_eq!(rows[0].1.as_bytes().unwrap(), &[value; 6_200]);
    assert_eq!(rows[1].0, b"number");
    assert_eq!(rows[1].1.as_i64(), Ok(i64::from(value)));
}

pub fn check_views(db: &Database, revision: u64, data_revision: u64, value: u8) {
    assert_eq!(db.storage_identity(), Ok(ID));
    let current = db.snapshot();
    assert_eq!(current.revision(), revision);
    assert_eq!(current.data_revision(), data_revision);
    check_value(db, &current, value);
    assert_eq!(db.retained_ids().unwrap(), [1]);
    let retained = db.retained_snapshot(1).unwrap();
    assert_eq!(retained.revision(), 2);
    assert_eq!(retained.data_revision(), 2);
    check_value(db, &retained, 1);
}

/// Exercise a backend through public APIs. Every `open` must construct a fresh
/// handle to the same store, and all three names must start out unused.
pub fn check_lifecycle<B: ByteStore + 'static>(
    create: impl Fn(&str) -> Result<B>,
    open: impl Fn(&str) -> Result<B>,
) {
    let limits = Limits::default();
    let mut db = Database::create_on(create("source").unwrap(), ID, limits).unwrap();
    seed(&db);
    for _ in 0..2 {
        assert_eq!(
            open("source").and_then(|store| inspect_on(store, limits)),
            Err(Error::Busy)
        );
    }
    assert_eq!(write_value(&db, 2), Ok(4));

    let compact = db.compact_to(create("compact").unwrap()).unwrap();
    check_views(&db, 4, 4, 2);
    drop(db);

    let before = inspect_on(open("source").unwrap(), limits).unwrap();
    let backup = backup_on(open("source").unwrap(), create("backup").unwrap(), limits).unwrap();
    assert_eq!(inspect_on(open("source").unwrap(), limits).unwrap(), before);
    assert_eq!(before.revision, 4);
    assert_eq!(before.data_revision, 4);
    assert!(before.witnessed);
    for (name, stats) in [("compact", compact), ("backup", backup)] {
        let copy = inspect_on(open(name).unwrap(), limits).unwrap();
        assert_eq!(copy.identity, before.identity);
        assert_eq!(copy.retained, before.retained);
        assert_eq!(copy.physical_bytes, stats.after_bytes);
        assert_eq!(copy.committed_bytes, stats.after_bytes);
        assert_eq!(stats.before_bytes, before.physical_bytes);
        assert!(copy.witnessed);
    }
    for name in ["source", "compact", "backup"] {
        let reopened = Database::open_on(open(name).unwrap(), limits).unwrap();
        check_views(&reopened, 4, 4, 2);
    }
}
