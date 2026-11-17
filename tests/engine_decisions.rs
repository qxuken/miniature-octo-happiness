use weekend_db::{Database, Error, Limits, TransactionState, Validation};

fn seed(db: &Database, c: weekend_db::CollectionId) {
    let mut tx = db.begin();
    tx.put(c, b"a", &db.integer(1).unwrap()).unwrap();
    tx.put(c, b"b", &db.integer(2).unwrap()).unwrap();
    tx.commit().unwrap();
}

#[test]
fn point_observation_matches_collection_and_key_independently() {
    let db = Database::with_limits(Limits {
        max_observations: 1,
        ..Limits::default()
    })
    .unwrap();
    let c = db.create_collection().unwrap();
    let other = db.create_collection().unwrap();
    seed(&db, c);
    seed(&db, other);
    let mut tx = db.begin();
    assert_eq!(tx.get(c, b"a").unwrap().unwrap().as_i64(), Ok(1));
    assert!(tx.get(c, b"a").is_ok()); // same collection, same key: reuse
    assert_eq!(tx.get(other, b"a"), Err(Error::Limit)); // collection differs
    assert_eq!(tx.get(c, b"b"), Err(Error::Limit)); // key differs
    assert_eq!(tx.state(), TransactionState::Active);
    tx.commit().unwrap();

    let mut absent = db.begin();
    assert_eq!(absent.get(c, b"missing"), Ok(None));
    assert_eq!(absent.get(c, b"also-missing"), Ok(None)); // same collection reuse
    assert_eq!(absent.get(other, b"missing"), Err(Error::Limit));
    absent.abort().unwrap();
}

#[test]
fn overlay_lookup_and_replacement_match_both_identity_components() {
    let db = Database::with_limits(Limits {
        max_writes: 1,
        ..Limits::default()
    })
    .unwrap();
    let c = db.create_collection().unwrap();
    let other = db.create_collection().unwrap();
    // Seed separately because the write cap itself is one.
    for collection in [c, other] {
        for (key, number) in [(b"a", 1), (b"b", 2)] {
            let mut tx = db.begin();
            tx.put(collection, key, &db.integer(number).unwrap())
                .unwrap();
            tx.commit().unwrap();
        }
    }
    let mut tx = db.begin();
    tx.put(c, b"a", &db.integer(10).unwrap()).unwrap();
    assert_eq!(tx.get(c, b"a").unwrap().unwrap().as_i64(), Ok(10));
    assert_eq!(tx.get(other, b"a").unwrap().unwrap().as_i64(), Ok(1));
    assert_eq!(tx.get(c, b"b").unwrap().unwrap().as_i64(), Ok(2));
    assert_eq!(
        tx.put(other, b"a", &db.integer(20).unwrap()),
        Err(Error::Limit)
    );
    assert_eq!(tx.put(c, b"b", &db.integer(20).unwrap()), Err(Error::Limit));
    tx.put(c, b"a", &db.integer(30).unwrap()).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        db.snapshot().get(c, b"a").unwrap().unwrap().as_i64(),
        Ok(30)
    );
    assert_eq!(
        db.snapshot().get(other, b"a").unwrap().unwrap().as_i64(),
        Ok(1)
    );
}

#[test]
fn scan_filters_collection_and_value_presence_independently() {
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let other = db.create_collection().unwrap();
    seed(&db, c);
    seed(&db, other);
    let mut tx = db.begin();
    tx.delete(c, b"a").unwrap();
    tx.put(c, b"b", &db.integer(3).unwrap()).unwrap();
    tx.delete(other, b"b").unwrap();
    tx.put(other, b"a", &db.integer(4).unwrap()).unwrap();
    let rows = tx.scan(c).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, b"b");
    assert_eq!(rows[0].1.as_i64(), Ok(3));
    let rows = tx.scan(other).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, b"a");
    assert_eq!(rows[0].1.as_i64(), Ok(4));
    tx.commit().unwrap();
    assert_eq!(db.snapshot().count(c), Ok(1));
    assert_eq!(db.snapshot().count(other), Ok(1));
}

#[test]
fn all_terminal_states_reject_reads_edits_and_second_completion() {
    for terminal in [
        TransactionState::Committed,
        TransactionState::Aborted,
        TransactionState::Failed,
    ] {
        let db = Database::with_limits(Limits {
            max_entries: 1,
            ..Limits::default()
        })
        .unwrap();
        let c = db.create_collection().unwrap();
        let value = db.integer(1).unwrap();
        let mut tx = db.begin();
        tx.put(c, b"a", &value).unwrap();
        match terminal {
            TransactionState::Committed => {
                tx.commit().unwrap();
            }
            TransactionState::Aborted => {
                tx.abort().unwrap();
            }
            TransactionState::Failed => {
                tx.put(c, b"b", &value).unwrap();
                assert_eq!(tx.commit(), Err(Error::Limit));
            }
            _ => unreachable!(),
        }
        assert_eq!(tx.state(), terminal);
        let revision = db.snapshot().revision();
        assert_eq!(tx.get(c, b"a"), Err(Error::Closed));
        assert_eq!(tx.contains(c, b"a"), Err(Error::Closed));
        assert_eq!(tx.scan(c), Err(Error::Closed));
        assert_eq!(tx.count(c), Err(Error::Closed));
        assert_eq!(tx.put(c, b"a", &value), Err(Error::Closed));
        assert_eq!(tx.delete(c, b"a"), Err(Error::Closed));
        assert_eq!(tx.commit(), Err(Error::Closed));
        assert_eq!(tx.abort(), Err(Error::Closed));
        assert_eq!(db.snapshot().revision(), revision);
    }
}

#[test]
fn observation_and_replacement_byte_limits_preserve_the_draft() {
    let zero = Database::with_limits(Limits {
        max_transaction_bytes: 0,
        ..Limits::default()
    })
    .unwrap();
    let c = zero.create_collection().unwrap();
    let mut tx = zero.begin();
    assert_eq!(tx.get(c, b"missing"), Err(Error::Limit));
    assert_eq!(tx.scan(c), Err(Error::Limit));
    tx.commit().unwrap(); // failed observation did not fabricate a dependency

    let disk = weekend_db::storage::MemoryStore::new().unwrap();
    let stored = Database::create_on(disk.clone(), [9; 16], Limits::default()).unwrap();
    let stored_id = stored.create_collection().unwrap().as_u64();
    seed(&stored, stored.collection_id(stored_id));
    drop(stored);
    let tight = Database::open_on(
        disk,
        Limits {
            max_transaction_bytes: 0,
            ..Limits::default()
        },
    )
    .unwrap();
    let mut present = tight.begin();
    assert_eq!(
        present.get(tight.collection_id(stored_id), b"a"),
        Err(Error::Limit)
    );
    present.abort().unwrap();

    let db = Database::with_limits(Limits {
        max_transaction_bytes: 1024,
        ..Limits::default()
    })
    .unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"a", &db.integer(1).unwrap()).unwrap();
    assert_eq!(
        tx.put(c, b"new", &db.bytes(&[9; 2048]).unwrap()),
        Err(Error::Limit)
    );
    assert_eq!(tx.get(c, b"new"), Ok(None));
    assert_eq!(
        tx.put(c, b"a", &db.bytes(&[9; 2048]).unwrap()),
        Err(Error::Limit)
    );
    assert_eq!(tx.get(c, b"a").unwrap().unwrap().as_i64(), Ok(1));
    tx.put(c, b"a", &db.bytes(&[9; 8]).unwrap()).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        db.snapshot().get(c, b"a").unwrap().unwrap().as_bytes(),
        Ok(&[9; 8][..])
    );
}

#[test]
fn read_only_commit_advances_publication_but_not_data_revision() {
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let before = db.snapshot();
    let mut reader = db.begin_with_validation(Validation::Records);
    reader.get(c, b"missing").unwrap();
    assert_eq!(reader.commit(), Ok(before.revision() + 1));
    assert_eq!(db.snapshot().data_revision(), before.data_revision());
    let mut writer = db.begin();
    writer.delete(c, b"missing").unwrap();
    let next = writer.commit().unwrap();
    assert_eq!(db.snapshot().data_revision(), next); // accepted no-op delete is a data change
}

#[test]
fn snapshot_collection_and_key_errors_have_valid_controls() {
    let db = Database::with_limits(Limits {
        max_bytes: 0,
        ..Limits::default()
    })
    .unwrap();
    let c = db.create_collection().unwrap();
    let foreign = Database::new().unwrap();
    let s = db.snapshot();
    assert_eq!(s.get(c, b""), Ok(None));
    assert_eq!(s.get(c, b"x"), Err(Error::Limit));
    assert_eq!(s.scan(c), Ok(vec![]));
    assert_eq!(s.count(c), Ok(0));
    for (id, expected) in [
        (foreign.collection_id(1), Error::ForeignStore),
        (db.collection_id(999), Error::NotFound),
    ] {
        assert_eq!(s.get(id, b""), Err(expected));
        assert_eq!(s.scan(id), Err(expected));
        assert_eq!(s.count(id), Err(expected));
        assert_eq!(db.drop_collection(id), Err(expected));
        assert_eq!(db.begin().get(id, b""), Err(expected));
        assert_eq!(db.begin().scan(id), Err(expected));
    }
}

#[test]
fn absent_and_poisoned_storage_reject_publication_and_compaction() {
    use weekend_db::storage::{Fault, MemoryStore, SimulatedDisk};
    let mut memory = Database::new().unwrap();
    assert_eq!(memory.storage_space(), Err(Error::Invalid));
    assert_eq!(
        memory.compact_to(MemoryStore::new().unwrap()),
        Err(Error::Invalid)
    );
    assert!(!memory.requires_recovery());
    let disk = SimulatedDisk::new().unwrap();
    let mut db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    assert!(db.storage_space().is_ok());
    assert!(!db.requires_recovery());
    let revision = db.snapshot().revision();
    disk.arm(0, Fault::Error);
    assert_eq!(db.begin().commit(), Err(Error::Io));
    assert!(db.requires_recovery());
    assert_eq!(db.storage_space(), Err(Error::RecoveryRequired));
    assert_eq!(db.drop_collection(c), Err(Error::RecoveryRequired));
    assert_eq!(db.release_retained(retained), Err(Error::RecoveryRequired));
    assert_eq!(
        db.retain_snapshot(&db.snapshot()),
        Err(Error::RecoveryRequired)
    );
    assert_eq!(
        db.compact_to(MemoryStore::new().unwrap()),
        Err(Error::RecoveryRequired)
    );
    assert_eq!(db.snapshot().revision(), revision);
    assert_eq!(db.snapshot().count(c), Ok(0));
    assert!(db.retained_snapshot(retained).is_ok());
}

#[test]
fn abort_and_failed_commit_clear_owned_payloads_before_draft_drop() {
    for fail in [false, true] {
        let db = Database::with_limits(Limits {
            max_entries: 1,
            ..Limits::default()
        })
        .unwrap();
        let c = db.create_collection().unwrap();
        let baseline = db.memory_usage().live_bytes;
        let mut tx = db.begin();
        tx.put(c, b"a", &db.bytes(&[1; 2048]).unwrap()).unwrap();
        tx.put(c, b"b", &db.bytes(&[2; 2048]).unwrap()).unwrap();
        let pinned = db.memory_usage().live_bytes;
        if fail {
            assert_eq!(tx.commit(), Err(Error::Limit));
        } else {
            tx.abort().unwrap();
        }
        assert!(db.memory_usage().live_bytes <= pinned - 4096);
        assert_eq!(db.snapshot().count(c), Ok(0));
        drop(tx);
        assert_eq!(db.memory_usage().live_bytes, baseline);
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn retained_id_output_allocation_failure_does_not_change_catalog() {
    let db = Database::new().unwrap();
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let revision = db.snapshot().revision();
    db.fail_allocations_after(Some(0));
    assert_eq!(db.retained_ids(), Err(Error::OutOfMemory));
    db.fail_allocations_after(None);
    assert_eq!(db.retained_ids(), Ok(vec![retained]));
    assert_eq!(db.snapshot().revision(), revision);
}
