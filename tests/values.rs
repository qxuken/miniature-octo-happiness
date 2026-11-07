mod support;
use std::collections::BTreeMap;
use support::{Model, Rng};
use weekend_db::{Database, Error, Limits};

#[test]
fn scalars_binary_keys_absence_and_owned_lifetimes() {
    let db = Database::new().unwrap();
    assert_eq!(db.integer(i64::MIN).unwrap().as_i64(), Ok(i64::MIN));
    assert_eq!(db.unsigned(u64::MAX).unwrap().as_u64(), Ok(u64::MAX));
    assert_eq!(db.boolean(true).unwrap().as_bool(), Ok(true));
    let nan = f64::from_bits(0x7ff8000000000001);
    assert_eq!(db.float(nan).unwrap(), db.float(nan).unwrap());
    assert_ne!(db.float(0.0).unwrap(), db.float(-0.0).unwrap());
    assert_eq!(db.none().unwrap().as_bytes(), Err(Error::Type));
    let original = db
        .map()
        .unwrap()
        .map_set(b"\0", &db.bytes(b"a\0b").unwrap())
        .unwrap();
    let clone = original.map_set(b"", &db.none().unwrap()).unwrap();
    assert!(original.map_get(b"").unwrap().is_none());
    assert_eq!(
        clone.map_get(b"").unwrap().unwrap().kind(),
        weekend_db::Kind::None
    );
    let child = original.map_get(b"\0").unwrap().unwrap();
    drop(original);
    drop(clone);
    drop(db);
    assert_eq!(child.as_bytes(), Ok(b"a\0b".as_slice()));
}

#[test]
fn random_nested_operations_match_copying_model_and_history() {
    for seed in 1..=8 {
        let db = Database::new().unwrap();
        let mut rng = Rng(seed);
        let mut value = db.map().unwrap();
        let mut model: BTreeMap<Vec<u8>, Model> = BTreeMap::new();
        let mut history = Vec::new();
        for step in 0..1200 {
            let key = vec![rng.n(24) as u8, 0];
            match rng.n(5) {
                0 => {
                    value = value.map_remove(&key).unwrap();
                    model.remove(&key);
                }
                1 => {
                    let scalar = rng.next() as i64;
                    value = value.map_set(&key, &db.integer(scalar).unwrap()).unwrap();
                    model.insert(key, Model::Int(scalar));
                }
                _ => {
                    let mut list = match value.map_get(&key).unwrap() {
                        Some(v) if v.kind() == weekend_db::Kind::List => v,
                        _ => db.list().unwrap(),
                    };
                    let mut items = match model.get(&key) {
                        Some(Model::List(v)) => v.clone(),
                        _ => Vec::new(),
                    };
                    let scalar = rng.next() as i64;
                    let item = db.integer(scalar).unwrap();
                    match rng.n(4) {
                        0 if !items.is_empty() => {
                            let i = rng.n(items.len());
                            list = list.list_remove(i).unwrap();
                            items.remove(i);
                        }
                        1 if !items.is_empty() => {
                            let i = rng.n(items.len());
                            list = list.list_set(i, &item).unwrap();
                            items[i] = Model::Int(scalar);
                        }
                        2 => {
                            let i = rng.n(items.len() + 1);
                            list = list.list_insert(i, &item).unwrap();
                            items.insert(i, Model::Int(scalar));
                        }
                        _ => {
                            list = list.list_push(&item).unwrap();
                            items.push(Model::Int(scalar));
                        }
                    }
                    value = value.map_set(&key, &list).unwrap();
                    model.insert(key, Model::List(items));
                }
            }
            assert_eq!(
                Model::read(&value),
                Model::Map(model.clone()),
                "seed={seed}, step={step}"
            );
            if step % 113 == 0 {
                history.push((value.clone(), Model::Map(model.clone())));
            }
        }
        for (v, expected) in history {
            assert_eq!(Model::read(&v), expected);
        }
    }
}

#[test]
fn radix_growth_shrink_and_middle_edits_preserve_old_roots() {
    let db = Database::new().unwrap();
    let mut list = db.list().unwrap();
    for i in 0..4096 {
        list = list.list_push(&db.unsigned(i).unwrap()).unwrap();
    }
    let old = list.clone();
    assert_eq!(list.list_push(&db.none().unwrap()), Err(Error::Limit));
    assert_eq!(list.list_get(4096), Err(Error::Bounds));
    for i in 0..4096 {
        assert_eq!(list.list_get(i).unwrap().as_u64(), Ok(i as u64));
    }
    for i in (0..4096).rev() {
        assert_eq!(list.list_get(i).unwrap().as_u64(), Ok(i as u64));
        list = list.list_remove(i).unwrap();
    }
    assert_eq!(list.len(), Ok(0));
    assert_eq!(list.list_remove(0), Err(Error::Bounds));
    assert_eq!(old.list_get(4095).unwrap().as_u64(), Ok(4095));
    let middle = old
        .list_remove(255)
        .unwrap()
        .list_insert(255, &db.unsigned(99999).unwrap())
        .unwrap();
    assert_eq!(middle.list_get(254).unwrap().as_u64(), Ok(254));
    assert_eq!(middle.list_get(255).unwrap().as_u64(), Ok(99999));
    assert_eq!(middle.list_get(256).unwrap().as_u64(), Ok(256));
}

#[test]
fn depth_size_and_store_boundaries() {
    let db = Database::with_limits(Limits {
        max_depth: 4,
        max_bytes: 8,
        max_value_bytes: 128,
        ..Limits::default()
    })
    .unwrap();
    let leaf = db.bytes(b"12345678").unwrap();
    assert_eq!(db.bytes(b"123456789"), Err(Error::Limit));
    let mut nested = leaf.clone();
    for _ in 0..3 {
        nested = db.list().unwrap().list_push(&nested).unwrap();
    }
    assert_eq!(db.list().unwrap().list_push(&nested), Err(Error::Limit));
    let other = Database::new().unwrap();
    assert_eq!(
        db.list().unwrap().list_push(&other.none().unwrap()),
        Err(Error::ForeignStore)
    );
    assert!(
        Database::with_limits(Limits {
            max_depth: 65,
            ..Limits::default()
        })
        .is_err()
    );
    let mut list = db.list().unwrap();
    for _ in 0..4 {
        list = list.list_push(&leaf).unwrap();
    }
    assert_eq!(list.list_push(&leaf), Err(Error::Limit));
}

#[test]
fn nested_map_scans_enforce_zero_and_exact_buffer_budgets() {
    use std::mem::size_of;
    use weekend_db::Value;
    let row_bytes = size_of::<(Vec<u8>, Value)>();
    for (budget, keys, fits) in [
        (0, vec![], true),
        (0, vec![b"large".to_vec()], false),
        (row_bytes - 1, vec![vec![]], false),
        (row_bytes, vec![vec![]], true),
        (row_bytes + 1023, vec![vec![b'x'; 1024]], false),
        (row_bytes + 1024, vec![vec![b'x'; 1024]], true),
        (
            2 * row_bytes + 2,
            vec![b"z\0".to_vec(), b"a".to_vec()],
            false,
        ),
        (
            2 * row_bytes + 3,
            vec![b"z\0".to_vec(), b"a".to_vec()],
            true,
        ),
    ] {
        let db = Database::with_limits(Limits {
            max_transaction_bytes: budget,
            ..Limits::default()
        })
        .unwrap();
        let mut map = db.map().unwrap();
        // Scans share child values; their payload is not copied into the buffer.
        let child = db.bytes(&vec![42; 2048]).unwrap();
        for key in &keys {
            map = map.map_set(key, &child).unwrap();
        }
        let nested = db
            .list()
            .unwrap()
            .list_push(&map)
            .unwrap()
            .list_get(0)
            .unwrap();
        let scanned = nested.map_entries();
        if fits {
            let rows = scanned.unwrap();
            let mut ordered = keys.clone();
            ordered.sort();
            assert_eq!(
                rows.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>(),
                ordered
            );
            assert!(rows.iter().all(|(_, value)| value == &child));
        } else {
            assert_eq!(scanned, Err(Error::Limit), "budget={budget}");
        }
        for key in keys {
            assert_eq!(nested.map_get(&key).unwrap(), Some(child.clone()));
        }
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn oversized_nested_map_scan_is_rejected_before_allocating_output() {
    let db = Database::with_limits(Limits {
        max_transaction_bytes: 0,
        ..Limits::default()
    })
    .unwrap();
    let map = db
        .map()
        .unwrap()
        .map_set(&vec![b'x'; 1024], &db.none().unwrap())
        .unwrap();
    db.fail_allocations_after(Some(0));
    let result = map.map_entries();
    db.fail_allocations_after(None);
    assert_eq!(result, Err(Error::Limit));
    assert_eq!(
        map.map_get(&vec![b'x'; 1024]).unwrap().unwrap().kind(),
        weekend_db::Kind::None
    );
}
