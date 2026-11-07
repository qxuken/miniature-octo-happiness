use weekend_db::{
    Database, Error, Limits, Result,
    storage::{Crash, Fault, IoKind, SimulatedDisk},
};

fn main() -> Result<()> {
    let disk = SimulatedDisk::new()?;
    let db = Database::create_on(disk.clone(), *b"example-disk-001", Limits::default())?;
    let collection = db.create_collection()?;
    let mut tx = db.begin();
    tx.put(collection, b"answer", &db.integer(42)?)?;
    tx.commit()?;
    drop(tx);
    let acknowledged = db.snapshot().revision();
    disk.crash(Crash::DropAll)?;
    drop(db);
    let db = Database::open_on(disk.clone(), Limits::default())?;
    assert_eq!(db.snapshot().revision(), acknowledged);
    assert_eq!(
        db.snapshot()
            .get(db.collection_id(1), b"answer")?
            .unwrap()
            .as_i64()?,
        42
    );

    // Find the exact root-publication sync boundary using a separate medium.
    let baseline = disk.durable_image()?;
    let trial = SimulatedDisk::from_image(&baseline)?;
    let trial_db = Database::open_on(trial.clone(), Limits::default())?;
    let mut tx = trial_db.begin();
    tx.put(trial_db.collection_id(1), b"answer", &trial_db.integer(43)?)?;
    trial.arm(usize::MAX, Fault::Error);
    tx.commit()?;
    let root_sync = trial
        .trace()?
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == IoKind::Sync)
        .nth(1)
        .unwrap()
        .0;

    let mut tx = db.begin();
    tx.put(db.collection_id(1), b"answer", &db.integer(43)?)?;
    disk.arm(root_sync, Fault::SyncError(Crash::PersistAll));
    assert_eq!(tx.commit(), Err(Error::OutcomeUnknown));
    assert!(db.requires_recovery());
    assert_eq!(db.begin().commit(), Err(Error::RecoveryRequired));
    drop(tx);
    drop(db);
    disk.crash(Crash::DropAll)?;
    let db = Database::open_on(disk, Limits::default())?;
    assert_eq!(
        db.snapshot()
            .get(db.collection_id(1), b"answer")?
            .unwrap()
            .as_i64()?,
        43
    );
    println!("Acknowledged commit survived; ambiguous publication resolved by recovery.");
    Ok(())
}
