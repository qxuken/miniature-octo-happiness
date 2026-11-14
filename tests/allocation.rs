#![cfg(feature = "fault-injection")]
use weekend_db::{Database, Error, Limits, TransactionState};

#[test]
fn allocation_limits_reject_each_independent_invalid_configuration() {
    let valid = Limits {
        max_live_bytes: usize::MAX,
        max_depth: 64,
        max_entries: 1_000_000,
        max_writes: 1_000_000,
        max_observations: 1_000_000,
        max_bytes: isize::MAX as usize,
        max_value_bytes: isize::MAX as usize,
        max_transaction_bytes: isize::MAX as usize,
    };
    assert!(Database::with_limits(valid).is_ok());
    for invalid in [
        Limits {
            max_depth: 0,
            ..valid
        },
        Limits {
            max_depth: 65,
            ..valid
        },
        Limits {
            max_entries: 1_000_001,
            ..valid
        },
        Limits {
            max_writes: 1_000_001,
            ..valid
        },
        Limits {
            max_observations: 1_000_001,
            ..valid
        },
        Limits {
            max_bytes: isize::MAX as usize + 1,
            ..valid
        },
        Limits {
            max_value_bytes: isize::MAX as usize + 1,
            ..valid
        },
        Limits {
            max_transaction_bytes: isize::MAX as usize + 1,
            ..valid
        },
    ] {
        assert!(
            matches!(Database::with_limits(invalid), Err(Error::Invalid)),
            "{invalid:?}"
        );
    }
    // Zero collection/key budgets are legal configuration values. They need
    // not permit every application operation.
    assert!(
        Database::with_limits(Limits {
            max_live_bytes: usize::MAX,
            max_depth: 1,
            max_entries: 0,
            max_writes: 0,
            max_observations: 0,
            max_bytes: 0,
            max_value_bytes: 0,
            max_transaction_bytes: 0,
        })
        .is_ok()
    );
}

#[test]
fn every_nested_edit_allocation_can_fail_without_changing_inputs() {
    let db = Database::new().unwrap();
    let item = db.bytes(b"original").unwrap();
    let list = db.list().unwrap().list_push(&item).unwrap();
    let map = db.map().unwrap().map_set(b"old", &list).unwrap();
    for operation in 0..7 {
        let mut success = false;
        let mut failures = 0;
        for budget in 0..160 {
            db.fail_allocations_after(Some(budget));
            let result = match operation {
                0 => map.map_set(b"new", &item),
                1 => map.map_remove(b"old"),
                2 => list.list_set(0, &map),
                3 => list.list_push(&map),
                4 => list.list_insert(0, &map),
                5 => list.list_remove(0),
                _ => db.bytes(b"copied"),
            };
            db.fail_allocations_after(None);
            assert_eq!(map.map_get(b"old").unwrap().unwrap(), list);
            assert!(map.map_get(b"new").unwrap().is_none());
            assert_eq!(list.list_get(0).unwrap(), item);
            match result {
                Ok(_) => {
                    success = true;
                    break;
                }
                Err(e) => {
                    assert_eq!(e, Error::OutOfMemory);
                    failures += 1;
                }
            }
        }
        assert!(success, "operation={operation}");
        // Removing a singleton list needs only a new empty value, still fallible.
        assert!(failures > 0);
    }
}

#[test]
fn every_candidate_allocation_failure_leaves_all_records_and_history_unchanged() {
    let mut success = false;
    for budget in 0..256 {
        let db = Database::new().unwrap();
        let a = db.create_collection().unwrap();
        let b = db.create_collection().unwrap();
        let mut seed = db.begin();
        seed.put(a, b"x", &db.integer(0).unwrap()).unwrap();
        seed.put(b, b"y", &db.integer(0).unwrap()).unwrap();
        seed.commit().unwrap();
        let old = db.snapshot();
        let rev = old.revision();
        let mut tx = db.begin();
        tx.put(a, b"x", &db.integer(1).unwrap()).unwrap();
        tx.put(b, b"y", &db.integer(2).unwrap()).unwrap();
        tx.delete(a, b"absent").unwrap();
        db.fail_allocations_after(Some(budget));
        let result = tx.commit();
        db.fail_allocations_after(None);
        assert_eq!(old.get(a, b"x").unwrap().unwrap().as_i64(), Ok(0));
        match result {
            Ok(_) => {
                assert_eq!(db.snapshot().get(a, b"x").unwrap().unwrap().as_i64(), Ok(1));
                assert_eq!(db.snapshot().get(b, b"y").unwrap().unwrap().as_i64(), Ok(2));
                success = true;
                break;
            }
            Err(e) => {
                assert_eq!(e, Error::OutOfMemory, "budget={budget}");
                assert_eq!(tx.state(), TransactionState::Failed);
                assert_eq!(db.snapshot().revision(), rev);
                assert_eq!(db.snapshot().get(a, b"x").unwrap().unwrap().as_i64(), Ok(0));
                assert_eq!(db.snapshot().get(b, b"y").unwrap().unwrap().as_i64(), Ok(0));
                assert_eq!(tx.commit(), Err(Error::Closed));
            }
        }
    }
    assert!(success);
}

#[test]
fn observation_overlay_and_scan_failures_never_publish() {
    for operation in 0..4 {
        let mut success = false;
        for budget in 0..160 {
            let db = Database::new().unwrap();
            let c = db.create_collection().unwrap();
            let item = db.integer(7).unwrap();
            let mut seed = db.begin();
            seed.put(c, b"x", &item).unwrap();
            seed.commit().unwrap();
            let rev = db.snapshot().revision();
            let mut tx = db.begin();
            db.fail_allocations_after(Some(budget));
            let result = match operation {
                0 => tx.get(c, b"x").map(|_| ()),
                1 => tx.put(c, b"new", &item),
                2 => tx.scan(c).map(|_| ()),
                _ => tx.delete(c, b"x"),
            };
            db.fail_allocations_after(None);
            assert_eq!(db.snapshot().revision(), rev);
            assert_eq!(db.snapshot().get(c, b"x").unwrap(), Some(item));
            match result {
                Ok(()) => {
                    success = true;
                    break;
                }
                Err(e) => assert_eq!(e, Error::OutOfMemory),
            }
        }
        assert!(success, "operation={operation}");
    }
}

#[test]
fn failed_collection_publication_is_atomic_and_ids_are_not_reused() {
    for budget in 0..12 {
        let db = Database::new().unwrap();
        let c = db.create_collection().unwrap();
        let old = db.snapshot();
        db.fail_allocations_after(Some(budget));
        let result = db.drop_collection(c);
        db.fail_allocations_after(None);
        if result.is_err() {
            assert_eq!(result, Err(Error::OutOfMemory));
            assert_eq!(db.snapshot().revision(), old.revision());
            assert_eq!(db.snapshot().count(c), Ok(0));
        } else {
            assert_eq!(db.snapshot().count(c), Err(Error::NotFound));
        }
        let next = db.create_collection().unwrap();
        assert!(next.as_u64() > c.as_u64());
        assert_eq!(old.count(c), Ok(0));
    }
}
