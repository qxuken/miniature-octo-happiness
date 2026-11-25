#[path = "support/allocator.rs"]
pub mod allocator;
use allocator::TestAllocator;
use std::{collections::BTreeMap, sync::Arc};
use weekend_db as allocator_engine;
use weekend_db::{Database, Error, Limits, Transaction, TransactionState, Validation};

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
    assert_eq!(tx.scan(c), Err(Error::Limit)); // the point consumes the only slot
    assert_eq!(tx.state(), TransactionState::Active);
    tx.commit().unwrap();

    let mut absent = db.begin();
    assert_eq!(absent.get(c, b"missing"), Ok(None));
    assert_eq!(absent.get(c, b"also-missing"), Ok(None)); // same collection reuse
    assert_eq!(absent.get(other, b"missing"), Err(Error::Limit));
    assert_eq!(absent.get(c, b"a"), Err(Error::Limit)); // the collection consumes the slot
    absent.abort().unwrap();
}

#[test]
fn overlay_lookup_and_replacement_match_both_identity_components() {
    let allocator = Arc::new(TestAllocator::default());
    let db = Database::with_options(allocator.options().with_limits(Limits {
        max_writes: 1,
        ..Limits::default()
    }))
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
    let replacement = db.integer(30).unwrap();
    allocator
        .with_refusal(0, || tx.put(c, b"a", &replacement))
        .unwrap();
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
    let db = Database::with_limits(Limits {
        max_entries: 2,
        ..Limits::default()
    })
    .unwrap();
    let c = db.create_collection().unwrap();
    let other = db.create_collection().unwrap();
    seed(&db, c);
    seed(&db, other);
    let mut tx = db.begin();
    // Stage insertions first. The overlay can only fit if deletions are
    // applied before additions, and writes to the other collection stay out.
    tx.put(c, b"\xff", &db.integer(5).unwrap()).unwrap();
    tx.delete(c, b"a").unwrap();
    tx.put(c, b"b", &db.integer(3).unwrap()).unwrap();
    tx.delete(other, b"b").unwrap();
    tx.put(other, b"z", &db.integer(4).unwrap()).unwrap();

    for (id, deleted, edits) in [
        (c, b"a".as_slice(), &[(b"b", 3), (b"\xff", 5)][..]),
        (other, b"b".as_slice(), &[(b"z", 4)][..]),
    ] {
        let mut model = BTreeMap::from([(b"a".to_vec(), 1), (b"b".to_vec(), 2)]);
        model.remove(deleted);
        model.extend(edits.iter().map(|(k, v)| (k.to_vec(), *v)));
        let rows = tx.scan(id).unwrap();
        assert!(
            rows.iter()
                .map(|(k, v)| (k, v.as_i64().unwrap()))
                .eq(model.iter().map(|(k, v)| (k, *v)))
        );
        assert_eq!(tx.count(id), Ok(model.len()));
    }
    tx.commit().unwrap();
    assert_eq!(db.snapshot().count(c), Ok(2));
    assert_eq!(db.snapshot().count(other), Ok(2));
}

#[test]
fn refused_observation_releases_scratch_and_retry_captures_dependency() {
    let allocator = Arc::new(TestAllocator::default());
    let db = Database::with_options(allocator.options()).unwrap();
    let c = db.create_collection().unwrap();
    seed(&db, c);

    for (key, allowed) in [
        (Some(b"a".as_slice()), 0),      // point key
        (Some(b"a".as_slice()), 1),      // point vector after copying the key
        (Some(b"absent".as_slice()), 0), // collection observation
        (None, 0),                       // collection observation via scan
    ] {
        let read = |tx: &mut Transaction<'_>| match key {
            Some(key) => tx.get(c, key).map(drop),
            None => tx.scan(c).map(drop),
        };
        let mut refused = db.begin();
        let mut retried = db.begin();
        let baseline = (db.memory_usage().live_bytes, allocator.live_bytes());
        for tx in [&mut refused, &mut retried] {
            assert_eq!(
                allocator.with_refusal(allowed, || read(tx)),
                Err(Error::OutOfMemory)
            );
            assert_eq!(tx.state(), TransactionState::Active);
            assert_eq!(
                (db.memory_usage().live_bytes, allocator.live_bytes()),
                baseline
            );
        }
        read(&mut retried).unwrap();
        let mut writer = db.begin();
        writer.put(c, b"a", &db.integer(7).unwrap()).unwrap();
        writer.commit().unwrap();
        assert!(refused.commit().is_ok());
        assert_eq!(retried.commit(), Err(Error::Conflict));
        assert_eq!(retried.state(), TransactionState::Aborted);
    }
}

#[test]
fn overlay_scan_refusals_keep_only_the_dependencies_already_recorded() {
    for allowed in 0..128 {
        let allocator = Arc::new(TestAllocator::default());
        let db = Database::with_options(allocator.options()).unwrap();
        let c = db.create_collection().unwrap();
        seed(&db, c);
        let untouched = db.integer(3).unwrap();
        let replacement = db.integer(7).unwrap();
        let mut writer = db.begin();
        writer.put(c, b"untouched", &untouched).unwrap();
        writer.commit().unwrap();
        drop(writer);
        let old = db.snapshot();
        let mut draft = db.begin();
        draft.delete(c, b"a").unwrap();
        draft.put(c, b"b", &replacement).unwrap();
        let before = (db.memory_usage().live_bytes, allocator.live_bytes());
        let result = allocator.with_refusal(allowed, || draft.scan_iter(c));
        assert_eq!(draft.state(), TransactionState::Active);
        assert_eq!(draft.get(c, b"a"), Ok(None));
        assert_eq!(draft.get(c, b"b"), Ok(Some(replacement.clone())));
        assert_eq!(db.snapshot().revision(), old.revision());
        assert_eq!(db.snapshot().get(c, b"a").unwrap().unwrap().as_i64(), Ok(1));
        assert_eq!(db.snapshot().get(c, b"b").unwrap().unwrap().as_i64(), Ok(2));
        match result {
            Ok(rows) => {
                assert!(allowed > 1);
                let observed: Vec<_> = rows
                    .map(|(key, value)| (key.as_ref().to_vec(), value.as_i64().unwrap()))
                    .collect();
                assert_eq!(observed, [(b"b".to_vec(), 7), (b"untouched".to_vec(), 3)]);
                return;
            }
            Err(e) => assert_eq!(e, Error::OutOfMemory, "allowance {allowed}"),
        }
        let recorded = if allowed == 0 {
            0
        } else {
            std::mem::size_of::<(weekend_db::CollectionId, u64)>()
        };
        assert_eq!(
            (db.memory_usage().live_bytes, allocator.live_bytes()),
            (before.0 + recorded, before.1 + recorded)
        );
        let mut writer = db.begin();
        writer.put(c, b"untouched", &untouched).unwrap();
        writer.commit().unwrap();
        // This edit leaves both originally observed points alone. Only a
        // successfully recorded scan observation can make the draft conflict.
        if allowed == 0 {
            assert!(draft.commit().is_ok());
        } else {
            assert_eq!(draft.commit(), Err(Error::Conflict));
            assert_eq!(draft.state(), TransactionState::Aborted);
        }
    }
    panic!("scan allocation sweep never reached success");
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
    assert_eq!(db.begin().get(c, b"x"), Err(Error::Limit));
    assert_eq!(db.begin().contains(c, b"x"), Err(Error::Limit));
    assert_eq!(s.scan(c), Ok(vec![]));
    assert_eq!(s.scan_iter(c).unwrap().count(), 0);
    assert_eq!(s.count(c), Ok(0));
    for (id, expected) in [
        (foreign.collection_id(1), Error::ForeignStore),
        (db.collection_id(999), Error::NotFound),
    ] {
        assert_eq!(s.get(id, b""), Err(expected));
        assert_eq!(s.scan(id), Err(expected));
        assert_eq!(s.scan_iter(id).err(), Some(expected));
        assert_eq!(s.count(id), Err(expected));
        assert_eq!(db.drop_collection(id), Err(expected));
        assert_eq!(db.begin().get(id, b""), Err(expected));
        assert_eq!(db.begin().scan(id), Err(expected));
    }
    db.drop_collection(c).unwrap();
    assert_eq!(db.snapshot().count(c), Err(Error::NotFound));
    assert_eq!(s.count(c), Ok(0));
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

#[test]
fn retained_id_output_allocation_failure_does_not_change_catalog() {
    let allocator = Arc::new(TestAllocator::default());
    let db = Database::with_options(allocator.options()).unwrap();
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let revision = db.snapshot().revision();
    assert_eq!(
        allocator.with_refusal(0, || db.retained_ids()),
        Err(Error::OutOfMemory)
    );
    assert_eq!(db.retained_ids(), Ok(vec![retained]));
    assert_eq!(db.snapshot().revision(), revision);
}

#[test]
fn catalog_preparation_refusals_preserve_live_and_pinned_collections() {
    for removing in [false, true] {
        let allocator = Arc::new(TestAllocator::default());
        let db = Database::with_options(allocator.options()).unwrap();
        let ids: Vec<_> = (0..8).map(|_| db.create_collection().unwrap()).collect();
        let first = ids[0];
        seed(&db, first);
        let pinned = db.snapshot();
        let live = db.memory_usage().live_bytes;
        let allocated = allocator.live_bytes();
        let mut completed = false;
        for allowance in 0..128 {
            let result = allocator.with_refusal(allowance, || {
                if removing {
                    db.drop_collection(first)
                } else {
                    db.create_collection().map(|id| assert_eq!(id.as_u64(), 9))
                }
            });
            assert_eq!(pinned.count(first), Ok(2));
            if result.is_ok() {
                assert!(allowance > 0);
                let current = db.snapshot();
                assert_eq!(current.revision(), pinned.revision() + 1);
                assert_eq!(current.data_revision(), current.revision());
                if removing {
                    assert_eq!(current.count(first), Err(Error::NotFound));
                } else {
                    assert_eq!(current.count(db.collection_id(9)), Ok(0));
                }
                completed = true;
                break;
            }
            assert_eq!(result, Err(Error::OutOfMemory));
            assert_eq!(db.snapshot().revision(), pinned.revision());
            assert_eq!(db.snapshot().data_revision(), pinned.data_revision());
            assert_eq!(db.snapshot().count(first), Ok(2));
            assert_eq!(
                db.snapshot().count(db.collection_id(9)),
                Err(Error::NotFound)
            );
            assert_eq!(db.memory_usage().live_bytes, live);
            assert_eq!(allocator.live_bytes(), allocated);
        }
        assert!(completed);
        drop(pinned);
        drop(db);
        assert_eq!(allocator.live(), 0);
    }
}

#[test]
fn catalog_io_failure_preserves_reopened_current_and_retained_roots() {
    use weekend_db::storage::{Crash, Fault, SimulatedDisk};

    for removing in [false, true] {
        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), [9; 16], Limits::default()).unwrap();
        let first = db.create_collection().unwrap();
        seed(&db, first);
        let retained = db.retain_snapshot(&db.snapshot()).unwrap();
        let revision = db.snapshot().revision();
        disk.arm(0, Fault::Error);
        let result = if removing {
            db.drop_collection(first)
        } else {
            db.create_collection().map(|_| ())
        };
        assert_eq!(result, Err(Error::Io));
        assert_eq!(db.create_collection(), Err(Error::RecoveryRequired));
        assert_eq!(db.drop_collection(first), Err(Error::RecoveryRequired));
        assert_eq!(db.snapshot().revision(), revision);
        drop(db);
        disk.crash(Crash::DropAll).unwrap();
        let db = Database::open_on(disk, Limits::default()).unwrap();
        let first = db.collection_id(first.as_u64());
        assert_eq!(db.snapshot().revision(), revision);
        assert_eq!(db.snapshot().count(first), Ok(2));
        assert_eq!(db.retained_snapshot(retained).unwrap().count(first), Ok(2));
        assert_eq!(db.create_collection().unwrap().as_u64(), 2);
        db.drop_collection(first).unwrap();
        assert_eq!(db.snapshot().count(first), Err(Error::NotFound));
        assert_eq!(db.retained_snapshot(retained).unwrap().count(first), Ok(2));
    }
}
