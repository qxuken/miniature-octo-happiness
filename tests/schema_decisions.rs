use weekend_db::{
    CollectionId, Database, Error as EngineError, Kind, TransactionState, Value,
    schema::{Error, FieldSpec, ReadView, Record, Schema, descriptor_key, field_key},
};
fn spec(id: u64) -> FieldSpec<'static> {
    FieldSpec {
        id,
        name: "field",
        kind: Kind::Unsigned,
        required: false,
        nullable: false,
        default: None,
    }
}
fn bad_schema(raw: Value) {
    assert!(matches!(
        Schema::from_value(raw),
        Err(Error::InvalidDescriptor)
    ));
}
fn bad_record(raw: Value) {
    assert!(matches!(Record::from_value(raw), Err(Error::InvalidRecord)));
}

#[test]
fn descriptor_identity_names_and_container_guards_have_independent_controls() {
    let db = Database::new().unwrap();
    let schema = Schema::build(&db, 1, 1, &[spec(1)], &[]).unwrap();
    for (type_id, version) in [(0, 1), (1, 0)] {
        assert!(matches!(
            Schema::build(&db, type_id, version, &[], &[]),
            Err(Error::InvalidDescriptor)
        ));
        bad_schema(
            schema
                .as_value()
                .map_set(b"type", &db.unsigned(type_id).unwrap())
                .unwrap()
                .map_set(b"version", &db.unsigned(version).unwrap())
                .unwrap(),
        );
    }
    let boundary = Schema::build(&db, u64::MAX, u64::MAX, &[spec(u64::MAX)], &[1]).unwrap();
    assert_eq!(
        (boundary.type_id(), boundary.version()),
        (u64::MAX, u64::MAX)
    );
    assert_eq!(boundary.field(u64::MAX).unwrap().unwrap().name(), "field");
    for key in [b"type".as_slice(), b"version", b"fields", b"reserved"] {
        bad_schema(schema.as_value().map_remove(key).unwrap());
        bad_schema(
            schema
                .as_value()
                .map_set(key, &db.boolean(false).unwrap())
                .unwrap(),
        );
    }
    let fields = schema.as_value().map_get(b"fields").unwrap().unwrap();
    let field = fields.map_get(&field_key(1)).unwrap().unwrap();
    let replace = |field: &Value| {
        schema
            .as_value()
            .map_set(b"fields", &fields.map_set(&field_key(1), field).unwrap())
            .unwrap()
    };
    for name in [
        db.bytes(b"").unwrap(),
        db.bytes(&[255]).unwrap(),
        db.unsigned(1).unwrap(),
    ] {
        bad_schema(replace(&field.map_set(b"name", &name).unwrap()));
    }
    for key in [b"name".as_slice(), b"kind", b"required", b"nullable"] {
        bad_schema(replace(&field.map_remove(key).unwrap()));
        bad_schema(replace(&field.map_set(key, &db.list().unwrap()).unwrap()));
    }
    for kind in [0, 8, u64::MAX] {
        bad_schema(replace(
            &field.map_set(b"kind", &db.unsigned(kind).unwrap()).unwrap(),
        ));
    }
}

#[test]
fn descriptor_ids_reservations_and_duplicate_names_have_valid_controls() {
    let db = Database::new().unwrap();
    assert!(
        Schema::build(
            &db,
            1,
            1,
            &[
                spec(1),
                FieldSpec {
                    name: "other",
                    ..spec(2)
                }
            ],
            &[3, 4]
        )
        .is_ok()
    );
    for (fields, reserved) in [
        (vec![spec(0)], vec![]),
        (
            vec![
                spec(1),
                FieldSpec {
                    name: "other",
                    ..spec(1)
                },
            ],
            vec![],
        ),
        (vec![spec(1), spec(2)], vec![]),
        (vec![spec(1)], vec![0]),
        (vec![spec(1)], vec![1]),
        (vec![spec(1)], vec![2, 2]),
    ] {
        assert!(matches!(
            Schema::build(&db, 1, 1, &fields, &reserved),
            Err(Error::InvalidDescriptor)
        ));
    }
    let schema = Schema::build(&db, 1, 1, &[spec(1)], &[]).unwrap();
    let field = schema
        .as_value()
        .map_get(b"fields")
        .unwrap()
        .unwrap()
        .map_get(&field_key(1))
        .unwrap()
        .unwrap();
    for key in [vec![], vec![1; 7], vec![1; 9], vec![0; 8]] {
        bad_schema(
            schema
                .as_value()
                .map_set(b"fields", &db.map().unwrap().map_set(&key, &field).unwrap())
                .unwrap(),
        );
    }
    bad_schema(
        schema
            .as_value()
            .map_set(
                b"reserved",
                &db.list()
                    .unwrap()
                    .list_push(&db.integer(1).unwrap())
                    .unwrap(),
            )
            .unwrap(),
    );
}

#[test]
fn field_kind_default_presence_and_nullable_controls_are_explicit() {
    let db = Database::new().unwrap();
    let values = [
        db.boolean(true).unwrap(),
        db.integer(i64::MIN).unwrap(),
        db.unsigned(u64::MAX).unwrap(),
        db.float(f64::from_bits(u64::MAX)).unwrap(),
        db.bytes(&[0, 255]).unwrap(),
        db.map().unwrap(),
        db.list().unwrap(),
    ];
    for value in values {
        let field = FieldSpec {
            kind: value.kind(),
            required: true,
            default: Some(value.clone()),
            ..spec(1)
        };
        let schema = Schema::build(&db, 1, 1, &[field], &[]).unwrap();
        let record = Record::new(&db, &schema).unwrap();
        assert_eq!(schema.read_field(&record, 1).unwrap(), Some(value.clone()));
        assert!(record.field(1).unwrap().is_none());
        let present = record.set(1, &value).unwrap();
        assert_eq!(schema.validate(&present), Ok(()));
    }
    for required in [false, true] {
        for nullable in [false, true] {
            for default in [false, true] {
                let schema = Schema::build(
                    &db,
                    1,
                    1,
                    &[FieldSpec {
                        required,
                        nullable,
                        default: default.then(|| db.unsigned(7).unwrap()),
                        ..spec(1)
                    }],
                    &[],
                )
                .unwrap();
                let absent = Record::new(&db, &schema).unwrap();
                assert_eq!(
                    schema.read_field(&absent, 1),
                    if default {
                        Ok(Some(db.unsigned(7).unwrap()))
                    } else if required {
                        Err(Error::MissingField(1))
                    } else {
                        Ok(None)
                    }
                );
                assert_eq!(
                    schema.validate(&absent.set(1, &db.none().unwrap()).unwrap()),
                    if nullable {
                        Ok(())
                    } else {
                        Err(Error::UnexpectedNone(1))
                    }
                );
                assert_eq!(
                    schema.validate(&absent.set(1, &db.boolean(false).unwrap()).unwrap()),
                    Err(Error::WrongKind(1))
                );
                assert_eq!(schema.read_field(&absent, 2), Err(Error::UnknownField(2)));
            }
        }
    }
    for nullable in [false, true] {
        let result = Schema::build(
            &db,
            1,
            1,
            &[FieldSpec {
                nullable,
                default: Some(db.none().unwrap()),
                ..spec(1)
            }],
            &[],
        );
        assert_eq!(result.is_ok(), nullable);
    }
}

#[test]
fn successor_type_version_overflow_and_reserved_history_are_independent() {
    let db = Database::new().unwrap();
    let first = Schema::build(&db, 1, 1, &[spec(1)], &[3]).unwrap();
    for (type_id, version, want) in [(1, 2, true), (2, 2, false), (1, 1, false), (1, 3, false)] {
        let next = Schema::build(&db, type_id, version, &[spec(1)], &[3]).unwrap();
        assert_eq!(next.successor_of(&first).is_ok(), want);
    }
    let maximum = Schema::build(&db, 1, u64::MAX, &[], &[]).unwrap();
    assert_eq!(maximum.successor_of(&maximum), Err(Error::Incompatible));
    for (fields, reserved, want) in [
        (vec![spec(1)], vec![3], true),
        (
            vec![FieldSpec {
                kind: Kind::Bytes,
                ..spec(1)
            }],
            vec![3],
            false,
        ),
        (vec![], vec![1, 3], true),
        (vec![], vec![3], false),
        (vec![spec(1)], vec![], false),
    ] {
        let next = Schema::build(&db, 1, 2, &fields, &reserved).unwrap();
        assert_eq!(next.successor_of(&first).is_ok(), want);
    }
    // Requiredness/default/nullability are convention changes, not kind changes.
    let next = Schema::build(
        &db,
        1,
        2,
        &[FieldSpec {
            required: true,
            nullable: true,
            default: Some(db.none().unwrap()),
            ..spec(1)
        }],
        &[3],
    )
    .unwrap();
    assert_eq!(next.successor_of(&first), Ok(()));
}

#[test]
fn record_envelope_unknown_fields_and_access_guards_are_explicit() {
    let db = Database::new().unwrap();
    let schema = Schema::build(&db, 1, 1, &[spec(1)], &[3]).unwrap();
    let record = Record::new(&db, &schema).unwrap();
    for (type_id, version, fields) in [
        (0, 1, db.map().unwrap()),
        (1, 0, db.map().unwrap()),
        (1, 1, db.list().unwrap()),
    ] {
        bad_record(
            record
                .as_value()
                .map_set(b"type", &db.unsigned(type_id).unwrap())
                .unwrap()
                .map_set(b"version", &db.unsigned(version).unwrap())
                .unwrap()
                .map_set(b"fields", &fields)
                .unwrap(),
        );
    }
    for key in [b"type".as_slice(), b"version", b"fields"] {
        bad_record(record.as_value().map_remove(key).unwrap());
    }
    for key in [b"type".as_slice(), b"version"] {
        bad_record(
            record
                .as_value()
                .map_set(key, &db.integer(1).unwrap())
                .unwrap(),
        );
    }
    bad_record(db.none().unwrap());
    for key in [vec![1; 7], vec![1; 9], vec![0; 8]] {
        bad_record(
            record
                .as_value()
                .map_set(
                    b"fields",
                    &db.map()
                        .unwrap()
                        .map_set(&key, &db.none().unwrap())
                        .unwrap(),
                )
                .unwrap(),
        );
    }
    for (type_id, version) in [(2, 1), (1, 2)] {
        let other = Schema::build(&db, type_id, version, &[], &[]).unwrap();
        let other_record = Record::new(&db, &other).unwrap();
        assert_eq!(schema.validate(&other_record), Err(Error::Incompatible));
    }
    assert_eq!(record.field(0), Err(Error::InvalidRecord));
    assert_eq!(
        schema.validate(&record.set(3, &db.unsigned(3).unwrap()).unwrap()),
        Err(Error::ReservedField(3))
    );
    let unknown = record.set(u64::MAX, &db.bytes(b"future").unwrap()).unwrap();
    assert_eq!(schema.validate(&unknown), Ok(()));
    let envelope = Record::from_value(
        unknown
            .as_value()
            .map_set(b"future-envelope", &db.unsigned(9).unwrap())
            .unwrap(),
    )
    .unwrap();
    let updated = envelope
        .set(1, &db.unsigned(1).unwrap())
        .unwrap()
        .remove(1)
        .unwrap();
    assert_eq!(updated.as_value(), envelope.as_value());
    assert_eq!(updated.remove(99).unwrap().as_value(), updated.as_value());
    let next = Schema::build(&db, 1, 2, &[spec(1)], &[3]).unwrap();
    assert_eq!(updated.with_schema(&db, &next).unwrap().version(), 2);
    let other = Schema::build(&db, 2, 2, &[], &[]).unwrap();
    assert!(matches!(
        updated.with_schema(&db, &other),
        Err(Error::Incompatible)
    ));
}

struct FailingView;
impl ReadView for FailingView {
    fn get_record(&mut self, _: CollectionId, _: &[u8]) -> weekend_db::Result<Option<Value>> {
        Err(EngineError::Io)
    }
}
#[test]
fn load_checks_requested_type_and_version_and_propagates_read_failure() {
    let db = Database::new().unwrap();
    let collection = db.create_collection().unwrap();
    assert!(matches!(
        Schema::load(&mut FailingView, collection, 1, 1),
        Err(Error::Engine(EngineError::Io))
    ));
    assert!(matches!(
        Schema::load(&mut db.snapshot(), collection, 1, 1),
        Err(Error::Engine(EngineError::NotFound))
    ));
    for (type_id, version, valid) in [(1, 1, true), (2, 1, false), (1, 2, false)] {
        let schema = Schema::build(&db, type_id, version, &[], &[]).unwrap();
        let mut tx = db.begin();
        tx.put(collection, &descriptor_key(1, 1), schema.as_value())
            .unwrap();
        tx.commit().unwrap();
        let loaded = Schema::load(&mut db.snapshot(), collection, 1, 1);
        assert_eq!(loaded.is_ok(), valid);
        if !valid {
            assert!(matches!(loaded, Err(Error::InvalidDescriptor)));
        }
    }
    let mut tx = db.begin();
    tx.put(collection, &descriptor_key(1, 1), &db.none().unwrap())
        .unwrap();
    tx.commit().unwrap();
    assert!(matches!(
        Schema::load(&mut db.snapshot(), collection, 1, 1),
        Err(Error::Engine(EngineError::Type))
    ));
}

#[test]
fn publish_existing_equality_predecessor_and_abort_state_have_controls() {
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let schema = Schema::build(&db, 1, 1, &[spec(1)], &[]).unwrap();
    let mut tx = db.begin();
    schema.publish(&mut tx, c).unwrap();
    assert_eq!(tx.state(), TransactionState::Active);
    tx.commit().unwrap();
    let mut tx = db.begin();
    schema.publish(&mut tx, c).unwrap();
    assert_eq!(tx.state(), TransactionState::Active);
    tx.commit().unwrap();
    let changed = Schema::build(&db, 1, 1, &[], &[]).unwrap();
    let mut tx = db.begin();
    tx.put(c, b"unrelated", &db.none().unwrap()).unwrap();
    assert_eq!(changed.publish(&mut tx, c), Err(Error::ImmutableDescriptor));
    assert_eq!(tx.state(), TransactionState::Aborted);
    assert!(db.snapshot().get(c, b"unrelated").unwrap().is_none());
    let mut tx = db.begin();
    tx.abort().unwrap();
    assert_eq!(
        schema.publish(&mut tx, c),
        Err(Error::Engine(EngineError::Closed))
    );
    assert_eq!(tx.state(), TransactionState::Aborted);
    let next = Schema::build(&db, 1, 2, &[spec(1)], &[]).unwrap();
    let mut tx = db.begin();
    next.publish(&mut tx, c).unwrap();
    tx.commit().unwrap();
    let missing = Schema::build(&db, 2, 2, &[], &[]).unwrap();
    let mut tx = db.begin();
    assert_eq!(
        missing.publish(&mut tx, c),
        Err(Error::Engine(EngineError::NotFound))
    );
    assert_eq!(tx.state(), TransactionState::Aborted);
}

#[test]
fn foreign_values_and_incompatible_publication_abort_without_partial_catalog() {
    let db = Database::new().unwrap();
    let foreign = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let first = Schema::build(&db, 1, 1, &[spec(1)], &[]).unwrap();
    let mut tx = db.begin();
    first.publish(&mut tx, c).unwrap();
    tx.commit().unwrap();
    let bad = Schema::build(
        &db,
        1,
        2,
        &[FieldSpec {
            kind: Kind::Bytes,
            ..spec(1)
        }],
        &[],
    )
    .unwrap();
    let other = Schema::build(&foreign, 2, 1, &[], &[]).unwrap();
    for (schema, error) in [
        (&bad, Error::Incompatible),
        (&other, Error::Engine(EngineError::ForeignStore)),
    ] {
        let before = db.snapshot().revision();
        let mut tx = db.begin();
        tx.put(c, b"prior", &db.none().unwrap()).unwrap();
        assert_eq!(schema.publish(&mut tx, c), Err(error));
        assert_eq!(tx.state(), TransactionState::Aborted);
        assert_eq!(db.snapshot().revision(), before);
        assert!(db.snapshot().get(c, b"prior").unwrap().is_none());
    }
    let record = Record::new(&db, &first).unwrap();
    assert!(matches!(
        record.set(1, &foreign.unsigned(1).unwrap()),
        Err(Error::Engine(EngineError::ForeignStore))
    ));
    assert!(matches!(
        record.with_schema(&foreign, &first),
        Err(Error::Engine(EngineError::ForeignStore))
    ));
}

#[test]
fn valid_empty_records_can_have_a_smaller_key_budget_than_field_ids() {
    let db = Database::new().unwrap();
    let schema = Schema::build(&db, 1, 1, &[spec(1)], &[]).unwrap();
    let small = Database::with_limits(weekend_db::Limits {
        max_bytes: 7,
        ..weekend_db::Limits::default()
    })
    .unwrap();
    let record = Record::new(&small, &schema).unwrap();
    assert_eq!(record.field(1), Err(Error::Engine(EngineError::Limit)));
    assert_eq!(
        schema.read_field(&record, 1),
        Err(Error::Engine(EngineError::Limit))
    );
    assert!(matches!(
        record.set(1, &small.unsigned(1).unwrap()),
        Err(Error::Engine(EngineError::Limit))
    ));
    assert!(matches!(
        Schema::build(&small, 1, 1, &[spec(1)], &[]),
        Err(Error::Engine(EngineError::Limit))
    ));
    let control = Record::new(&db, &schema).unwrap();
    assert_eq!(control.field(1), Ok(None));
    assert_eq!(schema.read_field(&control, 1), Ok(None));
}
