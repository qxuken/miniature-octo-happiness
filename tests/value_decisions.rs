use weekend_db::{Database, Error, Limits, Value};
#[test]
fn scalar_accessors_and_container_type_errors_are_explicit() {
    let db = Database::new().unwrap();
    let values = [
        db.none().unwrap(),
        db.boolean(true).unwrap(),
        db.integer(-1).unwrap(),
        db.unsigned(1).unwrap(),
        db.float(-0.0).unwrap(),
        db.bytes(b"x").unwrap(),
        db.map().unwrap(),
        db.list().unwrap(),
    ];
    for (kind, value) in values.iter().enumerate() {
        assert_eq!(value.as_bool().is_ok(), kind == 1);
        assert_eq!(value.as_i64().is_ok(), kind == 2);
        assert_eq!(value.as_u64().is_ok(), kind == 3);
        assert_eq!(value.as_f64().is_ok(), kind == 4);
        assert_eq!(value.as_bytes().is_ok(), kind == 5);
        assert_eq!(value.len().is_ok(), kind >= 5);
        assert_eq!(value.is_empty().is_ok(), kind >= 5);
        if kind != 6 {
            assert_eq!(value.map_get(b"x"), Err(Error::Type));
            assert_eq!(value.map_set(b"x", &values[0]), Err(Error::Type));
            assert_eq!(value.map_remove(b"x"), Err(Error::Type));
            assert_eq!(value.map_entries(), Err(Error::Type));
        }
        if kind != 7 {
            assert_eq!(value.list_get(0), Err(Error::Type));
            assert_eq!(value.list_set(0, &values[0]), Err(Error::Type));
            assert_eq!(value.list_push(&values[0]), Err(Error::Type));
            assert_eq!(value.list_insert(0, &values[0]), Err(Error::Type));
            assert_eq!(value.list_remove(0), Err(Error::Type));
        }
    }
}
#[test]
fn foreign_context_key_bounds_and_list_positions_have_valid_controls() {
    let db = Database::with_limits(Limits {
        max_bytes: 1,
        max_entries: 2,
        ..Default::default()
    })
    .unwrap();
    let other = Database::new().unwrap();
    let own = db.none().unwrap();
    let foreign = other.none().unwrap();
    let map = db.map().unwrap();
    let list = db.list().unwrap().list_push(&own).unwrap();
    assert_eq!(map.map_set(b"x", &foreign), Err(Error::ForeignStore));
    for result in [
        list.list_set(0, &foreign),
        list.list_insert(0, &foreign),
        list.list_push(&foreign),
    ] {
        assert_eq!(result, Err(Error::ForeignStore));
    }
    assert!(map.map_set(b"x", &own).is_ok());
    assert_eq!(map.map_get(b"xx"), Err(Error::Limit));
    assert_eq!(map.map_set(b"xx", &own), Err(Error::Limit));
    assert_eq!(map.map_remove(b"xx"), Err(Error::Limit));
    for result in [
        list.list_get(1),
        list.list_set(1, &own),
        list.list_remove(1),
        list.list_insert(2, &own),
    ] {
        assert_eq!(result, Err(Error::Bounds));
    }
    for position in [0, 1] {
        assert_eq!(list.list_insert(position, &own).unwrap().len(), Ok(2));
    }
    let full = list.list_push(&own).unwrap();
    assert_eq!(full.list_push(&own), Err(Error::Limit));
    assert!(full.list_set(1, &own).is_ok());
    assert!(list.list_remove(0).unwrap().is_empty().unwrap());
    assert_eq!(map.map_remove(b"x").unwrap(), map);
}
fn list(db: &Database, items: &[i64]) -> Value {
    items.iter().fold(db.list().unwrap(), |l, n| {
        l.list_push(&db.integer(*n).unwrap()).unwrap()
    })
}
#[test]
fn equality_distinguishes_pointer_kind_lengths_keys_and_contents() {
    let db = Database::new().unwrap();
    let own = db.integer(1).unwrap();
    assert_eq!(own, own.clone());
    assert_eq!(own, db.integer(1).unwrap());
    assert_ne!(own, db.integer(2).unwrap());
    assert_ne!(own, db.unsigned(1).unwrap());
    assert_eq!(db.unsigned(1).unwrap(), db.unsigned(1).unwrap());
    assert_ne!(db.unsigned(1).unwrap(), db.unsigned(2).unwrap());
    assert_eq!(db.map().unwrap(), db.map().unwrap());
    assert_eq!(db.list().unwrap(), db.list().unwrap());
    for (a, b) in [
        (vec![1, 2], vec![1, 2]),
        (vec![1], vec![1, 2]),
        (vec![1, 2], vec![1, 3]),
    ] {
        assert_eq!(list(&db, &a) == list(&db, &b), a == b);
    }
    let map = |pairs: &[(&[u8], i64)]| {
        pairs.iter().fold(db.map().unwrap(), |m, (k, v)| {
            m.map_set(k, &db.integer(*v).unwrap()).unwrap()
        })
    };
    let a = map(&[(b"a", 1), (b"b", 2)]);
    assert_eq!(a, map(&[(b"b", 2), (b"a", 1)]));
    assert_ne!(a, map(&[(b"a", 1)]));
    assert_ne!(a, map(&[(b"a", 1), (b"b", 3)]));
    assert_ne!(a, map(&[(b"a", 1), (b"c", 2)]));
    for (a, b) in [
        (0, 1 << 63),
        (u64::MAX, u64::MAX),
        (0x7ff8000000000001, 0x7ff8000000000002),
    ] {
        assert_eq!(
            db.float(f64::from_bits(a)).unwrap() == db.float(f64::from_bits(b)).unwrap(),
            a == b
        );
    }
    assert_eq!(db.bytes(b"x").unwrap(), db.bytes(b"x").unwrap());
    assert_ne!(db.bytes(b"x").unwrap(), db.bytes(b"y").unwrap());
    assert_eq!(db.boolean(false).unwrap(), db.boolean(false).unwrap());
    assert_ne!(db.boolean(false).unwrap(), db.boolean(true).unwrap());
    assert_eq!(db.none().unwrap(), db.none().unwrap());
}

#[test]
fn bytes_length_and_total_weight_guards_precede_allocation() {
    for max_bytes in [2, 3] {
        let db = Database::with_limits(Limits {
            max_bytes,
            max_value_bytes: 18,
            ..Default::default()
        })
        .unwrap();
        assert!(db.bytes(b"ab").is_ok());
        #[cfg(feature = "fault-injection")]
        db.fail_allocations_after(Some(0));
        assert_eq!(db.bytes(b"abc"), Err(Error::Limit));
        #[cfg(feature = "fault-injection")]
        db.fail_allocations_after(None);
    }
}
