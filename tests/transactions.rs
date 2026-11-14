use weekend_db::{CollectionId, Database, Error, TransactionState, Validation};

fn seeded() -> (Database, CollectionId) {
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    for k in [b"a", b"b"] {
        tx.put(c, k, &db.integer(0).unwrap()).unwrap();
    }
    tx.commit().unwrap();
    drop(tx);
    (db, c)
}
fn read(db: &Database, c: CollectionId, key: &[u8]) -> Option<i64> {
    db.snapshot()
        .get(c, key)
        .unwrap()
        .map(|v| v.as_i64().unwrap())
}

#[test]
fn existing_record_edits_follow_validation_mode() {
    // The draft for "a" is identical in each case. Only its validation mode
    // and whether "b" commits first vary.
    for (mode, intervening_commit, outcome, head_advance, values) in [
        (
            Validation::WholeDatabase,
            true,
            Err(Error::Conflict),
            1,
            [0, 2],
        ),
        (Validation::Records, true, Ok(2), 2, [1, 2]),
        (Validation::WholeDatabase, false, Ok(1), 1, [1, 0]),
        (Validation::Records, false, Ok(1), 1, [1, 0]),
    ] {
        let (db, c) = seeded();
        let old = db.snapshot();
        let base = old.revision();
        let mut a = db.begin_with_validation(mode);
        let mut b = db.begin();
        a.put(c, b"a", &db.integer(1).unwrap()).unwrap();
        b.put(c, b"b", &db.integer(2).unwrap()).unwrap();
        if intervening_commit {
            assert_eq!(b.commit(), Ok(base + 1));
        }
        assert_eq!(
            a.commit(),
            outcome.map(|advance| base + advance),
            "{mode:?}, intervening commit: {intervening_commit}",
        );
        assert_eq!(db.snapshot().revision(), base + head_advance);
        assert_eq!(read(&db, c, b"a"), Some(values[0]));
        assert_eq!(read(&db, c, b"b"), Some(values[1]));
        if !intervening_commit {
            assert_eq!(b.commit(), Ok(base + 2));
            assert_eq!(read(&db, c, b"a"), Some(1));
            assert_eq!(read(&db, c, b"b"), Some(2));
        }
        assert_eq!(old.get(c, b"a").unwrap().unwrap().as_i64(), Ok(0));
    }
}

#[test]
fn whole_root_baseline_rejects_unrelated_commits_and_no_partial_writes() {
    let (db, c) = seeded();
    let mut a = db.begin_with_validation(Validation::WholeDatabase);
    let mut b = db.begin_with_validation(Validation::WholeDatabase);
    a.put(c, b"a", &db.integer(1).unwrap()).unwrap();
    b.put(c, b"b", &db.integer(2).unwrap()).unwrap();
    b.put(c, b"new", &db.integer(3).unwrap()).unwrap();
    a.commit().unwrap();
    assert_eq!(b.commit(), Err(Error::Conflict));
    assert_eq!(read(&db, c, b"b"), Some(0));
    assert_eq!(read(&db, c, b"new"), None);
    assert_eq!(b.state(), TransactionState::Aborted);
    assert_eq!(b.get(c, b"a"), Err(Error::Closed));
    assert_eq!(b.commit(), Err(Error::Closed));
}

#[test]
fn final_overlay_read_your_writes_and_abort() {
    let (db, c) = seeded();
    let mut tx = db.begin();
    tx.put(c, b"a", &db.integer(1).unwrap()).unwrap();
    tx.put(c, b"a", &db.integer(2).unwrap()).unwrap();
    tx.delete(c, b"b").unwrap();
    tx.put(c, b"new", &db.none().unwrap()).unwrap();
    assert_eq!(tx.get(c, b"a").unwrap().unwrap().as_i64(), Ok(2));
    assert!(!tx.contains(c, b"b").unwrap());
    assert!(tx.contains(c, b"new").unwrap());
    assert_eq!(
        tx.scan(c)
            .unwrap()
            .iter()
            .map(|(k, _)| k.as_slice())
            .collect::<Vec<_>>(),
        vec![b"a".as_slice(), b"new".as_slice()]
    );
    assert_eq!(read(&db, c, b"a"), Some(0));
    tx.abort().unwrap();
    assert_eq!(read(&db, c, b"a"), Some(0));
    assert_eq!(tx.state(), TransactionState::Aborted);
    let mut tx = db.begin();
    tx.delete(c, b"a").unwrap();
    tx.put(c, b"a", &db.integer(9).unwrap()).unwrap();
    tx.commit().unwrap();
    assert_eq!(read(&db, c, b"a"), Some(9));
}

#[test]
fn equal_value_and_aba_changes_invalidate_present_reads() {
    for aba in [false, true] {
        let (db, c) = seeded();
        let mut stale = db.begin();
        stale.get(c, b"a").unwrap();
        stale.put(c, b"b", &db.integer(7).unwrap()).unwrap();
        let mut t = db.begin();
        t.put(c, b"a", &db.integer(if aba { 1 } else { 0 }).unwrap())
            .unwrap();
        t.commit().unwrap();
        if aba {
            let mut t = db.begin();
            t.put(c, b"a", &db.integer(0).unwrap()).unwrap();
            t.commit().unwrap();
        }
        assert_eq!(stale.commit(), Err(Error::Conflict));
        assert_eq!(read(&db, c, b"b"), Some(0));
    }
}

#[test]
fn nested_fields_and_list_positions_share_one_conflict_scope() {
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let list = db
        .list()
        .unwrap()
        .list_push(&db.integer(0).unwrap())
        .unwrap()
        .list_push(&db.integer(0).unwrap())
        .unwrap();
    let record = db.map().unwrap().map_set(b"tags", &list).unwrap();
    let mut seed = db.begin();
    seed.put(c, b"user", &record).unwrap();
    seed.commit().unwrap();
    let mut a = db.begin();
    let mut b = db.begin();
    let av = a.get(c, b"user").unwrap().unwrap();
    let bv = b.get(c, b"user").unwrap().unwrap();
    let av = av.map_set(b"name", &db.bytes(b"Alice").unwrap()).unwrap();
    let tags = bv
        .map_get(b"tags")
        .unwrap()
        .unwrap()
        .list_set(1, &db.integer(3).unwrap())
        .unwrap();
    let bv = bv.map_set(b"tags", &tags).unwrap();
    a.put(c, b"user", &av).unwrap();
    b.put(c, b"user", &bv).unwrap();
    a.commit().unwrap();
    assert_eq!(b.commit(), Err(Error::Conflict));
}

#[test]
fn missing_reads_new_inserts_empty_scans_and_noop_deletes_conflict() {
    for mode in 0..5 {
        let (db, c) = seeded();
        let mut a = db.begin();
        match mode {
            0 => {
                assert!(a.get(c, b"missing").unwrap().is_none());
            }
            1 => {
                a.put(c, b"new-a", &db.integer(1).unwrap()).unwrap();
            }
            2 => {
                a.scan(c).unwrap();
            }
            3 => {
                a.count(c).unwrap();
            }
            _ => {
                a.delete(c, b"missing").unwrap();
                assert!(a.get(c, b"missing").unwrap().is_none());
            }
        }
        let mut b = db.begin();
        if mode == 4 {
            b.delete(c, b"another-missing").unwrap();
        } else {
            b.put(c, b"new-b", &db.integer(2).unwrap()).unwrap();
        }
        b.commit().unwrap();
        assert_eq!(a.commit(), Err(Error::Conflict));
        assert_eq!(read(&db, c, b"new-a"), None);
    }
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let mut a = db.begin();
    assert!(a.scan(c).unwrap().is_empty());
    let mut b = db.begin();
    b.put(c, b"x", &db.none().unwrap()).unwrap();
    b.commit().unwrap();
    assert_eq!(a.commit(), Err(Error::Conflict));
}

#[test]
fn missing_insert_delete_aba_and_overlay_base_dependencies() {
    let (db, c) = seeded();
    let mut stale = db.begin();
    assert_eq!(stale.get(c, b"x").unwrap(), None);
    let mut insert = db.begin();
    insert.put(c, b"x", &db.none().unwrap()).unwrap();
    insert.commit().unwrap();
    let mut delete = db.begin();
    delete.delete(c, b"x").unwrap();
    delete.commit().unwrap();
    assert_eq!(stale.commit(), Err(Error::Conflict));
    let mut own = db.begin();
    own.put(c, b"new", &db.integer(1).unwrap()).unwrap();
    assert!(own.get(c, b"new").unwrap().is_some());
    own.delete(c, b"new").unwrap();
    assert!(own.get(c, b"new").unwrap().is_none());
    own.commit().unwrap();
    let mut own = db.begin();
    own.delete(c, b"a").unwrap();
    assert!(own.get(c, b"a").unwrap().is_none());
    let mut other = db.begin();
    other.put(c, b"a", &db.integer(1).unwrap()).unwrap();
    other.commit().unwrap();
    assert_eq!(own.commit(), Err(Error::Conflict));
}

#[test]
fn write_skew_and_metadata_lookup_dependencies() {
    let (db, c) = seeded();
    let mut a = db.begin();
    let mut b = db.begin();
    a.get(c, b"b").unwrap();
    b.get(c, b"a").unwrap();
    a.put(c, b"a", &db.integer(1).unwrap()).unwrap();
    b.put(c, b"b", &db.integer(1).unwrap()).unwrap();
    a.commit().unwrap();
    assert_eq!(b.commit(), Err(Error::Conflict));
    let catalog = db.create_collection().unwrap();
    let mut t = db.begin();
    t.put(catalog, b"schema", &db.integer(1).unwrap()).unwrap();
    t.commit().unwrap();
    let mut reader = db.begin();
    reader.get(catalog, b"schema").unwrap();
    reader.put(c, b"b", &db.integer(8).unwrap()).unwrap();
    let mut change = db.begin();
    change
        .put(catalog, b"schema", &db.integer(2).unwrap())
        .unwrap();
    change.commit().unwrap();
    assert_eq!(reader.commit(), Err(Error::Conflict));
}

#[test]
fn collection_identity_and_unrelated_catalog_changes() {
    let (db, c) = seeded();
    let old = db.snapshot();
    let mut a = db.begin();
    a.get(c, b"a").unwrap();
    let other = db.create_collection().unwrap();
    db.drop_collection(other).unwrap();
    a.put(c, b"b", &db.integer(5).unwrap()).unwrap();
    a.commit().unwrap();
    let mut stale = db.begin();
    stale.get(c, b"a").unwrap();
    db.drop_collection(c).unwrap();
    let replacement = db.create_collection().unwrap();
    assert_ne!(replacement, c);
    assert_eq!(stale.commit(), Err(Error::Conflict));
    assert_eq!(db.snapshot().get(c, b"a"), Err(Error::NotFound));
    assert_eq!(old.get(c, b"a").unwrap().unwrap().as_i64(), Ok(0));
    let foreign = Database::new().unwrap();
    assert_eq!(
        db.begin().get(foreign.collection_id(1), b"a"),
        Err(Error::ForeignStore)
    );
    assert_eq!(
        db.begin().put(replacement, b"a", &foreign.none().unwrap()),
        Err(Error::ForeignStore)
    );
}

#[test]
fn record_and_collection_validation_distinguish_changes_and_removal() {
    for (observation, change, conflict) in [
        ("point", "other-record", false),
        ("point", "same-record", true),
        ("point", "delete-record", true),
        ("point", "drop-collection", true),
        ("scan", "other-collection", false),
        ("scan", "other-record", true),
        ("scan", "drop-collection", true),
        ("absence", "other-collection", false),
        ("absence", "other-record", true),
        ("absence", "drop-collection", true),
    ] {
        let (db, c) = seeded();
        let other = db.create_collection().unwrap();
        let old = db.snapshot();
        let mut draft = db.begin();
        match observation {
            "point" => assert_eq!(draft.get(c, b"a").unwrap().unwrap().as_i64(), Ok(0)),
            "scan" => assert_eq!(draft.count(c), Ok(2)),
            "absence" => assert_eq!(draft.get(c, b"missing"), Ok(None)),
            _ => unreachable!(),
        }
        if change == "drop-collection" {
            db.drop_collection(c).unwrap();
        } else {
            let mut edit = db.begin();
            match change {
                "other-collection" => edit.put(other, b"b", &db.integer(1).unwrap()).unwrap(),
                "other-record" => edit.put(c, b"b", &db.integer(1).unwrap()).unwrap(),
                "same-record" => edit.put(c, b"a", &db.integer(1).unwrap()).unwrap(),
                "delete-record" => edit.delete(c, b"a").unwrap(),
                _ => unreachable!(),
            }
            edit.commit().unwrap();
        }
        let before = db.snapshot();
        let intervening = before.revision();
        let rows = before.scan(c);
        let expected = if conflict {
            Err(Error::Conflict)
        } else {
            Ok(intervening + 1)
        };
        assert_eq!(draft.commit(), expected, "{observation}, {change}");
        assert_eq!(
            draft.state(),
            if conflict {
                TransactionState::Aborted
            } else {
                TransactionState::Committed
            }
        );
        assert_eq!(draft.commit(), Err(Error::Closed));
        assert_eq!(db.snapshot().revision(), intervening + u64::from(!conflict));
        assert_eq!(db.snapshot().scan(c), rows);
        assert_eq!(old.get(c, b"a").unwrap().unwrap().as_i64(), Ok(0));
    }
}

#[test]
fn final_assignments_can_replace_records_at_collection_limit() {
    let db = Database::with_limits(weekend_db::Limits {
        max_entries: 2,
        ..Default::default()
    })
    .unwrap();
    let c = db.create_collection().unwrap();
    let mut seed = db.begin();
    seed.put(c, b"a", &db.none().unwrap()).unwrap();
    seed.put(c, b"b", &db.none().unwrap()).unwrap();
    seed.commit().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"c", &db.none().unwrap()).unwrap();
    tx.delete(c, b"a").unwrap();
    assert_eq!(tx.count(c), Ok(2));
    tx.commit().unwrap();
    assert_eq!(db.snapshot().count(c), Ok(2));
}

#[test]
fn bounded_observations_writes_and_empty_user_keys() {
    let db = Database::with_limits(weekend_db::Limits {
        max_bytes: 0,
        max_writes: 1,
        max_observations: 1,
        ..Default::default()
    })
    .unwrap();
    // Internal collection IDs do not consume the user byte-key limit.
    let c = db.create_collection().unwrap();
    let item = db.none().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"", &item).unwrap();
    tx.put(c, b"", &item).unwrap();
    assert_eq!(tx.put(c, b"x", &item), Err(Error::Limit));
    tx.commit().unwrap();
    let db = Database::with_limits(weekend_db::Limits {
        max_writes: 1,
        max_observations: 2,
        ..Default::default()
    })
    .unwrap();
    let c = db.create_collection().unwrap();
    let item = db.none().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"a", &item).unwrap();
    assert_eq!(tx.put(c, b"b", &item), Err(Error::Limit));
    tx.commit().unwrap();
    assert_eq!(db.snapshot().count(c), Ok(1));
}
