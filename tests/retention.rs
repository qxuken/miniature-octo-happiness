use weekend_db::{
    Database, Error, Limits, MAX_RETAINED_SNAPSHOTS, Validation,
    storage::{Crash, Fault, SimulatedDisk},
};
const ID: [u8; 16] = *b"retention-test-1";
fn fixture() -> Vec<u8> {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"a", &db.bytes(b"historical-only").unwrap())
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    let old = db.snapshot();
    assert_eq!(db.retain_snapshot(&old), Ok(1));
    let mut tx = db.begin();
    tx.put(c, b"a", &db.bytes(b"current").unwrap()).unwrap();
    tx.commit().unwrap();
    disk.durable_image().unwrap()
}
fn open(disk: &SimulatedDisk) -> Database {
    Database::open_on(disk.clone(), Limits::default()).unwrap()
}
fn bytes(snapshot: &weekend_db::Snapshot<'_>, db: &Database) -> Vec<u8> {
    snapshot
        .get(db.collection_id(1), b"a")
        .unwrap()
        .unwrap()
        .as_bytes()
        .unwrap()
        .to_vec()
}

#[test]
fn historical_retain_release_and_live_lifetimes_survive_reopening() {
    let disk = SimulatedDisk::from_image(&fixture()).unwrap();
    let db = open(&disk);
    let old = db.retained_snapshot(1).unwrap();
    let old_revision = old.revision();
    assert_eq!(bytes(&old, &db), b"historical-only");
    assert_eq!(bytes(&db.snapshot(), &db), b"current");
    let id = db.retain_snapshot(&old).unwrap();
    assert_eq!(id, 2);
    let data_revision = db.snapshot().data_revision();
    db.release_retained(1).unwrap();
    assert_eq!(db.snapshot().data_revision(), data_revision);
    assert_eq!(bytes(&old, &db), b"historical-only");
    drop(old);
    drop(db);
    disk.crash(Crash::DropAll).unwrap();
    let db = open(&disk);
    assert_eq!(db.retained_ids().unwrap(), vec![2]);
    let old = db.retained_snapshot(2).unwrap();
    assert_eq!(old.revision(), old_revision);
    assert_eq!(bytes(&old, &db), b"historical-only");
    db.release_retained(2).unwrap();
    assert_eq!(bytes(&old, &db), b"historical-only");
    drop(old);
    drop(db);
    let db = open(&disk);
    assert!(db.retained_ids().unwrap().is_empty());
    assert_eq!(db.release_retained(2), Err(Error::NotFound));
}

#[test]
fn stale_record_commits_preserve_current_retention_without_resurrection() {
    let disk = SimulatedDisk::from_image(&fixture()).unwrap();
    let db = open(&disk);
    let c = db.collection_id(1);
    let old = db.snapshot();
    let mut tx = db.begin();
    tx.get(c, b"a").unwrap();
    let mut baseline = db.begin_with_validation(Validation::WholeDatabase);
    let id = db.retain_snapshot(&old).unwrap();
    tx.put(c, b"a", &db.bytes(b"changed").unwrap()).unwrap();
    tx.commit().unwrap();
    assert_eq!(baseline.commit(), Err(Error::Conflict));
    assert_eq!(db.retained_ids().unwrap(), vec![1, id]);
    let mut tx = db.begin();
    tx.put(c, b"new", &db.none().unwrap()).unwrap();
    db.release_retained(1).unwrap();
    tx.commit().unwrap();
    assert_eq!(db.retained_ids().unwrap(), vec![id]);
    drop(tx);
    drop(baseline);
    drop(old);
    drop(db);
    let db = open(&disk);
    assert_eq!(db.retained_ids().unwrap(), vec![id]);
}

#[test]
fn retention_limit_foreign_roots_and_monotonic_ids() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
    let empty = db.snapshot();
    let foreign = Database::new().unwrap();
    assert_eq!(
        db.retain_snapshot(&foreign.snapshot()),
        Err(Error::ForeignStore)
    );
    for id in 1..=MAX_RETAINED_SNAPSHOTS {
        assert_eq!(db.retain_snapshot(&empty), Ok(id as u64));
    }
    let revision = db.snapshot().revision();
    assert_eq!(db.retain_snapshot(&empty), Err(Error::Limit));
    assert_eq!(db.snapshot().revision(), revision);
    drop(empty);
    drop(db);
    let db = open(&disk);
    assert_eq!(db.retained_ids().unwrap().len(), MAX_RETAINED_SNAPSHOTS);
    let old = db.retained_snapshot(1).unwrap();
    db.release_retained(2).unwrap();
    assert_eq!(
        db.retain_snapshot(&old),
        Ok(MAX_RETAINED_SNAPSHOTS as u64 + 1)
    );
    assert_eq!(old.revision(), 0);
}

#[test]
fn retained_graph_is_verified_but_released_garbage_is_not_a_root() {
    let image = fixture();
    let at = image
        .windows(b"historical-only".len())
        .position(|w| w == b"historical-only")
        .unwrap();
    let mut corrupt = image.clone();
    corrupt[at] ^= 1;
    assert!(matches!(
        Database::open_on(
            SimulatedDisk::from_image(&corrupt).unwrap(),
            Limits::default()
        ),
        Err(Error::Corrupt)
    ));
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let db = open(&disk);
    db.release_retained(1).unwrap();
    drop(db);
    let mut image = disk.durable_image().unwrap();
    image[at] ^= 1;
    let db = open(&SimulatedDisk::from_image(&image).unwrap());
    assert!(db.retained_ids().unwrap().is_empty());
    assert_eq!(bytes(&db.snapshot(), &db), b"current");
}

#[test]
fn retain_release_faults_recover_whole_catalogs_at_every_io_boundary() {
    let image = fixture();
    for releasing in [false, true] {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let db = open(&disk);
        let snap = db.snapshot();
        disk.arm(usize::MAX, Fault::Error);
        if releasing {
            db.release_retained(1).unwrap();
        } else {
            db.retain_snapshot(&snap).unwrap();
        }
        let trace = disk.trace().unwrap();
        let expected = db.retained_ids().unwrap();
        drop(snap);
        drop(db);
        for step in 0..trace.len() {
            for policy in [
                Crash::DropAll,
                Crash::PersistAll,
                Crash::Mask(0x5555),
                Crash::Torn {
                    write: 0,
                    prefix: 63,
                },
            ] {
                let disk = SimulatedDisk::from_image(&image).unwrap();
                let db = open(&disk);
                let live = db.retained_snapshot(1).unwrap();
                let snap = db.snapshot();
                disk.arm(step, Fault::Error);
                let result = if releasing {
                    db.release_retained(1)
                } else {
                    db.retain_snapshot(&snap).map(|_| ())
                };
                let error = result.unwrap_err();
                assert!(matches!(error, Error::Io | Error::OutcomeUnknown));
                assert!(db.requires_recovery());
                assert_eq!(db.retained_ids().unwrap(), vec![1]);
                assert_eq!(bytes(&live, &db), b"historical-only");
                disk.crash(policy).unwrap();
                drop(live);
                drop(snap);
                drop(db);
                let db = open(&disk);
                let actual = db.retained_ids().unwrap();
                assert!(
                    actual == vec![1] || actual == expected,
                    "release={releasing}, step={step}, policy={policy:?}"
                );
                if error == Error::Io {
                    assert_eq!(actual, vec![1]);
                }
                assert_eq!(bytes(&db.snapshot(), &db), b"current");
                for id in actual {
                    assert_eq!(
                        bytes(&db.retained_snapshot(id).unwrap(), &db),
                        if id == 1 {
                            b"historical-only".as_slice()
                        } else {
                            b"current".as_slice()
                        }
                    );
                }
            }
        }
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn retention_preparation_allocation_failures_are_atomic() {
    let image = fixture();
    for releasing in [false, true] {
        let mut succeeded = false;
        for budget in 0..512 {
            let disk = SimulatedDisk::from_image(&image).unwrap();
            let db = open(&disk);
            let snap = db.snapshot();
            let rev = snap.revision();
            db.fail_allocations_after(Some(budget));
            let result = if releasing {
                db.release_retained(1)
            } else {
                db.retain_snapshot(&snap).map(|_| ())
            };
            db.fail_allocations_after(None);
            match result {
                Ok(()) => {
                    succeeded = true;
                    break;
                }
                Err(e) => {
                    assert_eq!(e, Error::OutOfMemory);
                    assert!(!db.requires_recovery());
                    assert_eq!(db.snapshot().revision(), rev);
                    assert_eq!(db.retained_ids().unwrap(), vec![1]);
                    assert_eq!(disk.durable_image().unwrap(), image);
                }
            }
        }
        assert!(succeeded, "release={releasing}");
    }
}
