use weekend_db_typed::{
    Error, Result, Store,
    engine::{Database, Limits, storage::SimulatedDisk},
};

const FORMAT: &[u8] = b"\0weekend-typed";

#[test]
fn initialization_requires_revision_zero_and_preserves_nonfresh_state() -> Result<()> {
    let fresh = Store::initialize(Database::new()?)?;
    assert_eq!(fresh.database().snapshot().revision(), 4);
    for id in 1..=3 {
        assert_eq!(
            fresh
                .database()
                .snapshot()
                .count(fresh.database().collection_id(id))?,
            usize::from(id == 1)
        );
    }

    let disk = SimulatedDisk::new()?;
    let db = Database::create_on(disk.clone(), [51; 16], Limits::default())?;
    let collection = db.create_collection()?;
    let mut tx = db.begin();
    tx.put(collection, b"caller-data", &db.integer(17)?)?;
    tx.commit()?;
    drop(tx);
    let image = disk.durable_image()?;
    assert!(matches!(Store::initialize(db), Err(Error::InvalidCatalog)));
    assert_eq!(disk.durable_image()?, image);
    let reopened = Database::open_on(disk, Limits::default())?;
    assert_eq!(reopened.snapshot().revision(), 2);
    assert_eq!(
        reopened
            .snapshot()
            .get(reopened.collection_id(1), b"caller-data")?
            .unwrap()
            .as_i64()?,
        17
    );
    Ok(())
}

#[test]
fn typed_open_requires_the_exact_format_marker_without_changing_images() -> Result<()> {
    // Absent catalog, absent marker, wrong unsigned versions, wrong kind, and
    // the valid control all traverse the public from_database admission path.
    for case in 0..6 {
        let disk = SimulatedDisk::new()?;
        let db = Database::create_on(disk.clone(), [52; 16], Limits::default())?;
        if case != 0 {
            for _ in 0..3 {
                db.create_collection()?;
            }
        }
        if case >= 2 {
            let value = match case {
                2 => db.unsigned(0)?,
                3 => db.unsigned(2)?,
                4 => db.bytes(b"1")?,
                _ => db.unsigned(1)?,
            };
            let mut tx = db.begin();
            tx.put(db.collection_id(1), FORMAT, &value)?;
            tx.commit()?;
        }
        let image = disk.durable_image()?;
        let opened = Store::from_database(db);
        if case == 5 {
            let store = opened?;
            assert_eq!(
                store
                    .database()
                    .snapshot()
                    .get(store.database().collection_id(1), FORMAT)?
                    .unwrap()
                    .as_u64()?,
                1
            );
            drop(store);
        } else {
            assert!(matches!(opened, Err(Error::NotInitialized)));
        }
        assert_eq!(disk.durable_image()?, image, "case {case}");
    }
    Ok(())
}
