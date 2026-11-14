use weekend_db_typed::{
    __private, DecodeError, Error, FieldValue, Index, LookupValue, Model, Presence, Result, Store,
    engine::{
        Database, Kind, Limits, Value,
        schema::{FieldSpec, Record, Schema},
    },
};

#[derive(Model)]
#[weekend(type_id = 701, version = 1)]
struct Row {
    #[weekend(id = 1, unique)]
    slug: String,
}

#[test]
fn caught_callback_panic_closes_the_write_before_another_callback() -> Result<()> {
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
    };
    let mut store = Store::memory()?;
    let table = store.table::<Row>("rows")?;
    store.write(|w| {
        w.insert(&table, 1, &Row { slug: "a".into() })?;
        w.insert(&table, 2, &Row { slug: "b".into() })
    })?;
    let panicked = Cell::new(false);
    let resumed = Cell::new(false);
    let followup = |_: &mut Row| -> Result<()> {
        resumed.set(true);
        Ok(())
    };
    // Reuse the same closure type/value for the clean and terminal cases, so
    // both outcomes also occur within the same checked() instantiation.
    store.write(|w| w.update(&table, 2, followup))?;
    assert!(resumed.replace(false));
    let retained = store.checkpoint()?;
    let revision = store.database().snapshot().revision();
    let result = store.write(|w| {
        w.update(&table, 1, |row| {
            row.slug = "staged".into();
            Ok(())
        })?;
        assert!(
            catch_unwind(AssertUnwindSafe(|| w.update(&table, 2, |_| {
                panicked.set(true);
                panic!("intentional callback failure");
            })))
            .is_err()
        );
        assert_eq!(
            w.update(&table, 2, followup),
            Err(Error::Engine(weekend_db_typed::engine::Error::Closed))
        );
        Ok(())
    });
    assert_eq!(
        result,
        Err(Error::Engine(weekend_db_typed::engine::Error::Closed))
    );
    assert!(panicked.get());
    assert!(!resumed.get());
    assert_eq!(store.database().snapshot().revision(), revision);
    for (id, slug) in [(1, "a"), (2, "b")] {
        assert_eq!(
            store.read(|r| Ok(r.get(&table, id)?.unwrap().slug.clone()))?,
            slug
        );
        assert_eq!(
            store.read(|r| Ok(r.find(&table, Row::SLUG, slug)?.unwrap().id()))?,
            id
        );
        assert_eq!(
            store.read_retained(retained, |r| Ok(r.get(&table, id)?.unwrap().slug.clone()))?,
            slug
        );
    }
    assert!(
        store
            .read(|r| r.find(&table, Row::SLUG, "staged"))?
            .is_none()
    );
    assert!(store.read(|r| r.check_indexes(&table))?.is_consistent());
    // The failed Write is terminal; a fresh Write on the same Store still works.
    store.write(|w| {
        w.insert(
            &table,
            3,
            &Row {
                slug: "fresh".into(),
            },
        )
    })?;
    Ok(())
}

#[test]
fn duplicate_primary_insert_aborts_prior_work_without_changing_unique_claims() -> Result<()> {
    let mut store = Store::memory()?;
    let table = store.table::<Row>("rows")?;
    store.write(|w| w.insert(&table, 1, &Row { slug: "a".into() }))?;
    let retained = store.checkpoint()?;
    let revision = store.database().snapshot().revision();
    let result = store.write(|w| {
        w.insert(
            &table,
            2,
            &Row {
                slug: "staged".into(),
            },
        )?;
        assert_eq!(
            w.insert(
                &table,
                1,
                &Row {
                    slug: "replacement".into()
                }
            ),
            Err(Error::Engine(
                weekend_db_typed::engine::Error::AlreadyExists
            ))
        );
        Ok(())
    });
    assert_eq!(
        result,
        Err(Error::Engine(weekend_db_typed::engine::Error::Closed))
    );
    assert_eq!(store.database().snapshot().revision(), revision);
    assert_eq!(
        store.read(|r| Ok(r.get(&table, 1)?.unwrap().slug.clone()))?,
        "a"
    );
    assert!(store.read(|r| r.get(&table, 2))?.is_none());
    for slug in ["replacement", "staged"] {
        assert!(store.read(|r| r.find(&table, Row::SLUG, slug))?.is_none());
    }
    assert_eq!(
        store.read(|r| Ok(r.find(&table, Row::SLUG, "a")?.unwrap().id()))?,
        1
    );
    assert!(store.read(|r| r.check_indexes(&table))?.is_consistent());
    assert_eq!(
        store.read_retained(retained, |r| Ok(r.get(&table, 1)?.unwrap().slug.clone()))?,
        "a"
    );
    Ok(())
}

#[test]
fn deleting_an_absent_primary_leaves_existing_unique_entries_alone() -> Result<()> {
    let mut store = Store::memory()?;
    let table = store.table::<Row>("rows")?;
    store.write(|w| w.insert(&table, 1, &Row { slug: "a".into() }))?;
    let retained = store.checkpoint()?;
    let primary = store.database().snapshot().scan(table.collection())?;
    let indexes = store
        .database()
        .snapshot()
        .scan(store.database().collection_id(3))?;
    store.write(|w| {
        assert!(!w.delete(&table, 999)?);
        assert_eq!(w.find(&table, Row::SLUG, "a")?.unwrap().id(), 1);
        Ok(())
    })?;
    assert_eq!(
        store.database().snapshot().scan(table.collection())?,
        primary
    );
    assert_eq!(
        store
            .database()
            .snapshot()
            .scan(store.database().collection_id(3))?,
        indexes
    );
    // Positive deletion is the other outcome of primary-presence admission.
    assert!(store.write(|w| w.delete(&table, 1))?);
    assert!(store.read(|r| r.get(&table, 1))?.is_none());
    assert!(store.read(|r| r.find(&table, Row::SLUG, "a"))?.is_none());
    assert!(store.read(|r| r.check_indexes(&table))?.is_consistent());
    assert_eq!(
        store.read_retained(retained, |r| Ok(r
            .find(&table, Row::SLUG, "a")?
            .unwrap()
            .id()))?,
        1
    );
    Ok(())
}

#[test]
fn deleting_with_a_missing_wrong_or_malformed_claim_aborts_the_entire_draft() -> Result<()> {
    for corruption in 0..3 {
        let mut store = Store::memory()?;
        let table = store.table::<Row>("rows")?;
        store.write(|w| {
            w.insert(&table, 1, &Row { slug: "a".into() })?;
            w.insert(&table, 2, &Row { slug: "b".into() })
        })?;
        let retained = store.checkpoint()?;
        let db = store.database();
        let indexes = db.collection_id(3);
        // Explicit index wire fixture, independent of the runtime key encoder.
        let mut key = table.collection().as_u64().to_le_bytes().to_vec();
        key.extend_from_slice(&1u64.to_le_bytes());
        key.push(Kind::Bytes as u8);
        key.extend_from_slice(b"a");
        let mut tx = db.begin();
        match corruption {
            0 => tx.delete(indexes, &key)?,
            1 => tx.put(indexes, &key, &db.unsigned(999)?)?,
            _ => tx.put(indexes, &key, &db.bytes(b"invalid owner kind")?)?,
        }
        tx.commit()?;
        drop(tx);
        let before = db.snapshot();
        let records_before = before.scan(table.collection())?;
        let indexes_before = before.scan(indexes)?;
        let result = store.write(|w| {
            w.insert(
                &table,
                3,
                &Row {
                    slug: "staged".into(),
                },
            )?;
            assert_eq!(w.delete(&table, 1), Err(Error::IndexInconsistent));
            Ok(())
        });
        assert_eq!(
            result,
            Err(Error::Engine(weekend_db_typed::engine::Error::Closed))
        );
        assert_eq!(db.snapshot().revision(), before.revision());
        assert_eq!(db.snapshot().scan(table.collection())?, records_before);
        assert_eq!(db.snapshot().scan(indexes)?, indexes_before);
        assert!(store.read(|r| r.get(&table, 3))?.is_none());
        assert!(
            store
                .read(|r| r.find(&table, Row::SLUG, "staged"))?
                .is_none()
        );
        assert!(!store.read(|r| r.check_indexes(&table))?.is_consistent());
        store.read_retained(retained, |r| {
            assert_eq!(r.find(&table, Row::SLUG, "a")?.unwrap().id(), 1);
            assert_eq!(r.find(&table, Row::SLUG, "b")?.unwrap().id(), 2);
            assert!(r.check_indexes(&table)?.is_consistent());
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn catalog_identity_unique_and_name_guards_have_independent_controls() -> Result<()> {
    for name in ["".to_owned(), "x".repeat(128), "x".repeat(129)] {
        let mut store = Store::memory()?;
        let before = store.database().snapshot().revision();
        let result = store.table::<Row>(&name);
        if name.len() == 128 {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(Error::InvalidCatalog)));
            assert_eq!(store.database().snapshot().revision(), before);
        }
    }
    for case in 0..8 {
        let mut store = Store::memory()?;
        store.table::<Row>("rows")?;
        let db = store.database();
        let catalog = db.collection_id(1);
        let base = db.snapshot().get(catalog, b"table/rows")?.unwrap();
        let altered = match case {
            0 => base,
            1 => base.map_set(b"records", &db.unsigned(3)?)?,
            2 => base.map_set(b"type", &db.unsigned(0)?)?,
            3 => base.map_set(b"version", &db.unsigned(0)?)?,
            4 => base.map_set(b"unique", &db.list()?.list_push(&db.unsigned(0)?)?)?,
            5 => base.map_set(
                b"unique",
                &db.list()?
                    .list_push(&db.unsigned(1)?)?
                    .list_push(&db.unsigned(1)?)?,
            )?,
            6 => base.map_set(b"unique", &db.boolean(true)?)?,
            _ => base.map_set(b"records", &db.bytes(b"wrong-kind")?)?,
        };
        let mut tx = db.begin();
        tx.put(catalog, b"table/rows", &altered)?;
        tx.commit()?;
        drop(tx);
        let before = db.snapshot().revision();
        let result = store.table::<Row>("rows");
        if case == 0 {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(Error::InvalidCatalog)));
        }
        assert_eq!(store.database().snapshot().revision(), before);
    }
    Ok(())
}

struct Encoded(Value);
impl<T: FieldValue> LookupValue<T> for Encoded {
    fn encode_lookup(&self, _: &Database) -> Result<Value> {
        Ok(self.0.clone())
    }
}
#[test]
fn lookup_declared_kind_encoded_kind_and_null_exemption_have_pairs() -> Result<()> {
    let mut store = Store::memory()?;
    let table = store.table::<Row>("rows")?;
    store.write(|w| w.insert(&table, 1, &Row { slug: "a".into() }))?;
    let null = Encoded(store.database().none()?);
    assert!(store.read(|r| r.find(&table, Row::SLUG, &null))?.is_none());
    assert!(matches!(
        store.read(|r| r.find(&table, Index::<Row, u64>::new(1), &null)),
        Err(Error::InvalidField { id: 1, .. })
    ));
    let valid = Encoded(store.database().bytes(b"a")?);
    assert_eq!(
        store.read(|r| Ok(r.find(&table, Row::SLUG, &valid)?.unwrap().id()))?,
        1
    );
    let wrong = Encoded(store.database().unsigned(7)?);
    assert!(matches!(
        store.read(|r| r.find(&table, Row::SLUG, &wrong)),
        Err(Error::InvalidField { id: 1, .. })
    ));
    let before = store.database().snapshot().revision();
    assert!(
        store
            .write(|w| {
                w.insert(&table, 2, &Row { slug: "b".into() })?;
                let _ = w.find(&table, Row::SLUG, &wrong);
                Ok(())
            })
            .is_err()
    );
    assert_eq!(store.database().snapshot().revision(), before);
    assert!(store.read(|r| r.get(&table, 2))?.is_none());
    Ok(())
}

#[derive(Clone)]
struct Flags<const O: bool, const N: bool>;
impl<const O: bool, const N: bool> FieldValue for Flags<O, N> {
    const KIND: Kind = Kind::Bytes;
    const OPTIONAL: bool = O;
    const NULLABLE: bool = N;
    fn encode(&self, db: &Database) -> Result<Value> {
        Ok(db.bytes(b"x")?)
    }
    fn decode(_: &Value) -> std::result::Result<Self, DecodeError> {
        Ok(Self)
    }
}
#[test]
fn option_optional_and_nullable_flags_are_independent() {
    assert!(<Option<Flags<false, false>>>::validate_type().is_ok());
    assert!(matches!(
        <Option<Flags<true, false>>>::validate_type(),
        Err(Error::InvalidField { id: 0, .. })
    ));
    assert!(matches!(
        <Option<Flags<false, true>>>::validate_type(),
        Err(Error::InvalidField { id: 0, .. })
    ));
}

#[test]
fn generated_helpers_preserve_missing_defaults_and_separate_type_ids() -> Result<()> {
    let db = Database::new()?;
    let schema = Schema::build(
        &db,
        42,
        1,
        &[FieldSpec {
            id: 1,
            name: "field",
            kind: Kind::Bytes,
            required: false,
            nullable: true,
            default: Some(db.bytes(b"stored")?),
        }],
        &[],
    )?;
    let absent = Record::new(&db, &schema)?;
    let present = absent.set(1, &db.bytes(b"physical")?)?;
    assert_eq!(
        __private::read::<Presence<String>>(&absent, &schema, 1, 1, None)?,
        Presence::Missing
    );
    assert_eq!(
        __private::read::<Presence<String>>(&present, &schema, 1, 1, None)?,
        Presence::Value("physical".into())
    );
    assert_eq!(
        __private::read::<Option<String>>(&absent, &schema, 1, 1, None)?,
        Some("stored".into())
    );
    let other = Schema::build(&db, 43, 1, &[], &[])?;
    let other_record = Record::new(&db, &other)?;
    assert!(__private::validate(&absent, &schema, 42).is_ok());
    assert_eq!(
        __private::validate(&other_record, &schema, 42),
        Err(Error::TypeMismatch)
    );
    assert_eq!(
        __private::validate(&absent, &other, 42),
        Err(Error::TypeMismatch)
    );
    // Absent descriptor: same/newer version rejects, older version can default;
    // a physical unknown field at that prospective ID is a collision.
    let old = Schema::build(&db, 42, 1, &[], &[])?;
    let raw = Record::new(&db, &old)?;
    assert_eq!(
        __private::read::<String>(&raw, &old, 1, 1, Some(|| "fallback".into())),
        Err(Error::UnsupportedVersion)
    );
    assert_eq!(
        __private::read::<String>(&raw, &old, 1, 2, Some(|| "fallback".into()))?,
        "fallback"
    );
    let unknown = raw.set(1, &db.bytes(b"unknown")?)?;
    assert!(matches!(
        __private::read::<String>(&unknown, &old, 1, 2, Some(|| "fallback".into())),
        Err(Error::Schema(
            weekend_db_typed::engine::schema::Error::MigrationCollision
        ))
    ));
    Ok(())
}

#[test]
fn migration_batch_limits_independently_prevent_callbacks() -> Result<()> {
    for (writes, observations, expected_limit) in
        [(16, 1024, true), (1024, 16, true), (1024, 1024, false)]
    {
        let mut store = Store::initialize(Database::with_limits(Limits {
            max_writes: writes,
            max_observations: observations,
            ..Limits::default()
        })?)?;
        let table = store.table::<Row>("rows")?;
        let before = store.database().snapshot().revision();
        let ids: Vec<_> = (1..=17).collect();
        let result =
            store.write(|w| w.migrate_ids(&table, &ids, |_, _, _| panic!("no row exists")));
        let expected = if expected_limit {
            weekend_db_typed::engine::Error::Limit
        } else {
            weekend_db_typed::engine::Error::NotFound
        };
        assert_eq!(result, Err(Error::Engine(expected)));
        assert_eq!(store.database().snapshot().revision(), before);
    }
    Ok(())
}

#[test]
fn scalar_macro_instantiations_and_presence_dispatch_are_exercised() -> Result<()> {
    let db = Database::new()?;
    macro_rules! signed {($($t:ty),*)=>{$(
        for n in [<$t>::MIN,<$t>::MAX] {assert_eq!(<$t>::decode(&n.encode(&db)?).unwrap(),n);}
        for n in [i64::from(<$t>::MIN)-1,i64::from(<$t>::MAX)+1] {assert!(<$t>::decode(&db.integer(n)?).is_err());}
        assert!(<$t>::decode(&db.boolean(true)?).is_err());
    )*};}
    macro_rules! unsigned {($($t:ty),*)=>{$(
        for n in [0,<$t>::MAX] {assert_eq!(<$t>::decode(&n.encode(&db)?).unwrap(),n);}
        assert!(<$t>::decode(&db.unsigned(u64::from(<$t>::MAX)+1)?).is_err());
        assert!(<$t>::decode(&db.integer(-1)?).is_err());
    )*};}
    signed!(i8, i16, i32);
    unsigned!(u8, u16, u32);
    assert!(bool::decode(&true.encode(&db)?).unwrap());
    assert_eq!(i64::decode(&i64::MIN.encode(&db)?).unwrap(), i64::MIN);
    assert_eq!(u64::decode(&u64::MAX.encode(&db)?).unwrap(), u64::MAX);
    let bits = 0x7ff8_0000_0000_0042;
    assert_eq!(
        f64::decode(&f64::from_bits(bits).encode(&db)?)
            .unwrap()
            .to_bits(),
        bits
    );
    for result in [
        bool::decode(&db.bytes(b"x")?).map(|_| ()),
        i64::decode(&db.bytes(b"x")?).map(|_| ()),
        u64::decode(&db.bytes(b"x")?).map(|_| ()),
        f64::decode(&db.bytes(b"x")?).map(|_| ()),
    ] {
        assert!(result.is_err());
    }
    assert!(String::decode(&db.bytes(&[255])?).is_err());
    assert!(Vec::<u8>::decode(&db.none()?).is_err());
    assert!(Vec::<u8>::decode(&db.list()?.list_push(&db.integer(-1)?)?).is_err());
    assert!(Presence::<String>::Missing.encode_field(&db)?.is_none());
    assert!(Presence::<String>::Missing.encode(&db).is_err());
    assert_eq!(
        Presence::<String>::decode(&db.none()?).unwrap(),
        Presence::Null
    );
    assert_eq!(Option::<String>::decode(&db.none()?).unwrap(), None);
    assert_eq!(
        Option::<String>::decode(&db.bytes(b"a")?).unwrap(),
        Some("a".into())
    );
    let base = weekend_db_typed::IndexReport::default();
    assert!(base.is_consistent());
    for changed in [
        weekend_db_typed::IndexReport { missing: 1, ..base },
        weekend_db_typed::IndexReport {
            orphaned: 1,
            ..base
        },
        weekend_db_typed::IndexReport {
            mismatched: 1,
            ..base
        },
    ] {
        assert!(!changed.is_consistent());
    }
    Ok(())
}

#[test]
fn index_corruption_rejects_without_modifying_retained_roots_or_staged_writes() -> Result<()> {
    for case in 0..4 {
        let mut store = Store::memory()?;
        let table = store.table::<Row>("rows")?;
        store.write(|w| {
            w.insert(&table, 1, &Row { slug: "a".into() })?;
            w.insert(&table, 2, &Row { slug: "b".into() })
        })?;
        let retained = store.checkpoint()?;
        let db = store.database();
        let indexes = db.collection_id(3);
        let mut key = table.collection().as_u64().to_le_bytes().to_vec();
        key.extend_from_slice(&1u64.to_le_bytes());
        key.push(Kind::Bytes as u8);
        key.extend_from_slice(b"a");
        let mut tx = db.begin();
        match case {
            0 => tx.put(indexes, &key, &db.unsigned(1)?)?,
            1 => tx.put(indexes, &key, &db.boolean(true)?)?,
            2 => tx.put(indexes, &key, &db.unsigned(2)?)?,
            _ => tx.delete(table.collection(), &1u64.to_le_bytes())?,
        }
        tx.commit()?;
        drop(tx);
        let result = store.read(|r| r.find(&table, Row::SLUG, "a"));
        if case == 0 {
            assert_eq!(result?.unwrap().id(), 1);
        } else {
            assert!(matches!(result, Err(Error::IndexInconsistent)));
        }
        if case == 1 || case == 2 {
            let before = db.snapshot().revision();
            assert!(
                store
                    .write(|w| {
                        w.insert(&table, 3, &Row { slug: "c".into() })?;
                        let _ = w.update(&table, 1, |row| {
                            row.slug = "d".into();
                            Ok(())
                        });
                        Ok(())
                    })
                    .is_err()
            );
            assert_eq!(db.snapshot().revision(), before);
            assert!(store.read(|r| r.get(&table, 3))?.is_none());
        }
        assert_eq!(
            store.read_retained(retained, |r| Ok(r
                .find(&table, Row::SLUG, "a")?
                .unwrap()
                .slug
                .clone()))?,
            "a"
        );
    }
    Ok(())
}

struct Manual<const MODE: u8>;
impl<const MODE: u8> Model for Manual<MODE> {
    const TYPE_ID: u64 = 702;
    const VERSION: u64 = 1;
    const UNIQUE_FIELDS: &'static [u64] = if MODE == 3 {
        &[1, 1]
    } else if MODE == 4 {
        &[2]
    } else {
        &[1]
    };
    fn schema(db: &Database) -> Result<Schema> {
        Ok(Schema::build(
            db,
            Self::TYPE_ID,
            1,
            &[FieldSpec {
                id: 1,
                name: "field",
                kind: match MODE {
                    1 => Kind::Map,
                    2 => Kind::List,
                    _ => Kind::Bytes,
                },
                required: false,
                nullable: false,
                default: None,
            }],
            &[],
        )?)
    }
    fn decode(_: &Record, _: &Schema) -> Result<Self> {
        Ok(Self)
    }
    fn patch(&self, _: &Database, record: &Record) -> Result<Record> {
        Ok(record.clone())
    }
}
#[test]
fn manual_models_exercise_duplicate_container_and_missing_unique_ids() -> Result<()> {
    let mut store = Store::memory()?;
    assert!(store.table::<Manual<0>>("valid").is_ok());
    assert!(matches!(
        store.table::<Manual<1>>("map"),
        Err(Error::NotIndexed(1))
    ));
    assert!(matches!(
        store.table::<Manual<2>>("list"),
        Err(Error::NotIndexed(1))
    ));
    assert!(matches!(
        store.table::<Manual<3>>("duplicate"),
        Err(Error::NotIndexed(1))
    ));
    assert!(matches!(
        store.table::<Manual<4>>("absent"),
        Err(Error::NotIndexed(2))
    ));
    Ok(())
}

#[derive(Model)]
#[weekend(type_id = 701, version = 2)]
struct RowV2 {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2)]
    extra: Option<String>,
}
#[derive(Model)]
#[weekend(type_id = 701, version = 1)]
struct RenamedRow {
    #[weekend(id = 1, unique)]
    renamed: String,
}
#[derive(Model)]
#[weekend(type_id = 701, version = 1)]
struct UnindexedRow {
    #[weekend(id = 1)]
    slug: String,
}
#[test]
fn table_registration_distinguishes_descriptor_presence_and_catalog_version() -> Result<()> {
    for prepublish in [false, true] {
        let mut store = Store::memory()?;
        store.table::<Row>("rows")?;
        assert!(matches!(
            store.table::<RenamedRow>("rows"),
            Err(Error::Schema(
                weekend_db_typed::engine::schema::Error::ImmutableDescriptor
            ))
        ));
        assert!(matches!(
            store.table::<UnindexedRow>("rows"),
            Err(Error::IndexLayoutChanged)
        ));
        if prepublish {
            let db = store.database();
            let mut tx = db.begin();
            RowV2::schema(db)?.publish(&mut tx, db.collection_id(2))?;
            tx.commit()?;
        }
        let before = store.database().snapshot().revision();
        store.table::<RowV2>("rows")?;
        assert_eq!(store.database().snapshot().revision(), before + 1);
        let before = store.database().snapshot().revision();
        store.table::<RowV2>("rows")?;
        store.table::<Row>("rows")?;
        assert_eq!(store.database().snapshot().revision(), before);
    }
    for catalog_version in [1, 2] {
        let mut store = Store::memory()?;
        store.table::<Row>("rows")?;
        let db = store.database();
        let catalog = db.collection_id(1);
        let raw = db
            .snapshot()
            .get(catalog, b"table/rows")?
            .unwrap()
            .map_set(b"version", &db.unsigned(catalog_version)?)?;
        let mut tx = db.begin();
        tx.put(catalog, b"table/rows", &raw)?;
        tx.commit()?;
        drop(tx);
        let before = db.snapshot().revision();
        let result = store.table::<RowV2>("rows");
        if catalog_version == 1 {
            assert!(result.is_ok());
        } else {
            assert!(matches!(
                result,
                Err(Error::Engine(weekend_db_typed::engine::Error::NotFound))
            ));
            assert_eq!(store.database().snapshot().revision(), before);
        }
    }
    Ok(())
}

#[test]
fn table_handle_identity_operands_reject_independently() -> Result<()> {
    for case in 0..3 {
        let mut store = Store::memory()?;
        let table = store.table::<Row>("rows")?;
        store.write(|w| w.insert(&table, 1, &Row { slug: "a".into() }))?;
        let db = store.database();
        let catalog = db.collection_id(1);
        let original = db.snapshot().get(catalog, b"table/rows")?.unwrap();
        let altered = match case {
            0 => original,
            1 => original.map_set(b"type", &db.unsigned(Row::TYPE_ID + 1)?)?,
            _ => {
                let other = db.create_collection()?;
                original.map_set(b"records", &db.unsigned(other.as_u64())?)?
            }
        };
        let mut tx = db.begin();
        tx.put(catalog, b"table/rows", &altered)?;
        tx.commit()?;
        drop(tx);
        let before = db.snapshot().revision();
        let result = store.read(|r| r.get(&table, 1));
        if case == 0 {
            assert_eq!(result?.unwrap().slug, "a");
        } else {
            assert!(matches!(result, Err(Error::TypeMismatch)));
        }
        assert_eq!(db.snapshot().revision(), before);
    }
    Ok(())
}

#[test]
fn error_display_dispatch_preserves_context_and_fallbacks() {
    use weekend_db_typed::engine::{Error as CoreError, schema::Error as SchemaError};
    let core = CoreError::Limit;
    let schema = SchemaError::ImmutableDescriptor;
    let cases = [
        (Error::Engine(core), core.to_string()),
        (Error::Schema(schema.clone()), schema.to_string()),
        (
            Error::UniqueViolation(17),
            "Unique field 17 is already in use".into(),
        ),
        (
            Error::InvalidField {
                id: 23,
                reason: "bad bytes",
            },
            "Invalid field 23: bad bytes".into(),
        ),
        (
            Error::MigrationRequired,
            "Older records require an explicit migration".into(),
        ),
        (Error::InvalidCatalog, "InvalidCatalog".into()),
        (Error::NotInitialized, "NotInitialized".into()),
        (Error::TypeMismatch, "TypeMismatch".into()),
        (Error::NotIndexed(29), "NotIndexed(29)".into()),
        (Error::IndexLayoutChanged, "IndexLayoutChanged".into()),
        (Error::IndexInconsistent, "IndexInconsistent".into()),
        (Error::UnsupportedVersion, "UnsupportedVersion".into()),
        (Error::InvalidId, "InvalidId".into()),
    ];
    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn generated_model_patch_rejects_wrong_type_before_touching_the_record() -> Result<()> {
    let db = Database::new()?;
    let row = Row {
        slug: "updated".into(),
    };
    let own_schema = Row::schema(&db)?;
    let other_schema = Schema::build(&db, Row::TYPE_ID + 1, 1, &[], &[])?;
    let unknown = db.bytes(b"preserve")?;
    let own = Record::new(&db, &own_schema)?.set(99, &unknown)?;
    let other = Record::new(&db, &other_schema)?.set(99, &unknown)?;
    let changed = row.patch(&db, &own)?;
    assert_eq!(Row::decode(&changed, &own_schema)?.slug, "updated");
    assert_eq!(changed.field(99)?.unwrap().as_bytes()?, b"preserve");
    assert!(matches!(row.patch(&db, &other), Err(Error::TypeMismatch)));
    assert_eq!(other.type_id(), Row::TYPE_ID + 1);
    assert!(other.field(1)?.is_none());
    assert_eq!(other.field(99)?.unwrap().as_bytes()?, b"preserve");
    assert!(own.field(1)?.is_none());
    Ok(())
}
