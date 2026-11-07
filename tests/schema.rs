mod support;
#[path = "../examples/support/users.rs"]
mod users;
use support::Model;
use users::{AGE_MONTHS, AGE_YEARS, Age, NAME, Nickname, NicknameUpdate, USER_TYPE, Users};
use weekend_db::{
    Database, Error as EngineError, Kind, Limits, TransactionState,
    schema::{Error, FieldSpec, Record, Schema, descriptor_key, field_key},
    storage::{Crash, Fault, SimulatedDisk},
};

fn setup(db: &Database) -> Users {
    let users = Users::create(db).unwrap();
    let mut tx = db.begin();
    users.install_v1(db, &mut tx).unwrap();
    users
        .insert_v1(db, &mut tx, 42, "Alice", "alice@example.test", 25)
        .unwrap();
    users
        .insert_v1(db, &mut tx, 99, "Bob", "bob@example.test", 30)
        .unwrap();
    tx.commit().unwrap();
    users
}
fn migrate2(db: &Database, users: Users) {
    let mut tx = db.begin();
    assert_eq!(users.migrate_to_v2(db, &mut tx), Ok(2));
    tx.commit().unwrap();
}
fn raw(db: &Database, users: Users, id: u64) -> Record {
    Record::from_value(
        db.snapshot()
            .get(users.records, &id.to_le_bytes())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
fn load(db: &Database, users: Users, id: u64) -> users::User {
    users.load(&mut db.snapshot(), id).unwrap().unwrap()
}
fn spec(id: u64, name: &str, kind: Kind) -> FieldSpec<'_> {
    FieldSpec {
        id,
        name,
        kind,
        required: true,
        nullable: false,
        default: None,
    }
}

#[test]
fn stable_ids_immutable_descriptors_and_reserved_field_evolution() {
    let db = Database::new().unwrap();
    let registry = db.create_collection().unwrap();
    assert_eq!(
        descriptor_key(1, 2),
        [1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(field_key(9), 9u64.to_le_bytes());
    let first = Schema::build(&db, 12, 1, &[spec(1, "count", Kind::Unsigned)], &[]).unwrap();
    let mut tx = db.begin();
    first.publish(&mut tx, registry).unwrap();
    tx.commit().unwrap();
    let changed = Schema::build(&db, 12, 1, &[spec(1, "renamed", Kind::Unsigned)], &[]).unwrap();
    let mut tx = db.begin();
    assert_eq!(
        changed.publish(&mut tx, registry),
        Err(Error::ImmutableDescriptor)
    );
    assert_eq!(tx.commit(), Err(EngineError::Closed));
    assert_eq!(
        db.snapshot()
            .get(registry, &descriptor_key(12, 1))
            .unwrap()
            .as_ref(),
        Some(first.as_value())
    );
    let rename = Schema::build(&db, 12, 2, &[spec(1, "renamed", Kind::Unsigned)], &[]).unwrap();
    assert_eq!(rename.successor_of(&first), Ok(()));
    let bad = Schema::build(&db, 12, 2, &[spec(1, "text", Kind::Bytes)], &[]).unwrap();
    assert_eq!(bad.successor_of(&first), Err(Error::Incompatible));
    let dropped = Schema::build(&db, 12, 2, &[], &[]).unwrap();
    assert_eq!(dropped.successor_of(&first), Err(Error::Incompatible));
    let retired = Schema::build(&db, 12, 2, &[], &[1]).unwrap();
    let mut tx = db.begin();
    retired.publish(&mut tx, registry).unwrap();
    tx.commit().unwrap();
    let reused = Schema::build(&db, 12, 3, &[spec(1, "different", Kind::Unsigned)], &[]).unwrap();
    assert_eq!(reused.successor_of(&retired), Err(Error::Incompatible));
    let successor = Schema::build(&db, 12, 3, &[spec(2, "new", Kind::Unsigned)], &[1]).unwrap();
    assert_eq!(successor.successor_of(&retired), Ok(()));
}

#[test]
fn malformed_descriptors_and_record_shapes_are_checked() {
    let db = Database::new().unwrap();
    for fields in [
        vec![spec(1, "x", Kind::Bytes), spec(1, "y", Kind::Bytes)],
        vec![spec(1, "x", Kind::Bytes), spec(2, "x", Kind::Bytes)],
        vec![spec(0, "x", Kind::Bytes)],
        vec![spec(1, "x", Kind::None)],
    ] {
        assert!(matches!(
            Schema::build(&db, 1, 1, &fields, &[]),
            Err(Error::InvalidDescriptor)
        ));
    }
    assert!(matches!(
        Schema::build(&db, 1, 1, &[spec(1, "x", Kind::Bytes)], &[1]),
        Err(Error::InvalidDescriptor)
    ));
    assert!(matches!(
        Schema::build(&db, 1, 1, &[], &[1, 1]),
        Err(Error::InvalidDescriptor)
    ));
    let invalid = FieldSpec {
        default: Some(db.integer(3).unwrap()),
        ..spec(1, "x", Kind::Bytes)
    };
    assert!(matches!(
        Schema::build(&db, 1, 1, &[invalid], &[]),
        Err(Error::InvalidDescriptor)
    ));
    let schema = Schema::build(&db, 1, 1, &[spec(1, "x", Kind::Bytes)], &[]).unwrap();
    let record = Record::new(&db, &schema).unwrap();
    assert_eq!(schema.validate(&record), Err(Error::MissingField(1)));
    assert_eq!(
        schema.validate(&record.set(1, &db.none().unwrap()).unwrap()),
        Err(Error::UnexpectedNone(1))
    );
    assert_eq!(
        schema.validate(&record.set(1, &db.integer(2).unwrap()).unwrap()),
        Err(Error::WrongKind(1))
    );
    assert!(matches!(
        record.set(0, &db.none().unwrap()),
        Err(Error::InvalidRecord)
    ));
    assert!(matches!(record.remove(0), Err(Error::InvalidRecord)));
    let malformed = record
        .as_value()
        .map_set(
            b"fields",
            &db.map()
                .unwrap()
                .map_set(b"not-an-id", &db.none().unwrap())
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        Record::from_value(malformed),
        Err(Error::InvalidRecord)
    ));
}

#[test]
fn missing_none_and_default_are_distinct_per_schema_version() {
    let db = Database::new().unwrap();
    let users = setup(&db);
    assert_eq!(load(&db, users, 42).effective_nickname(), None);
    migrate2(&db, users);
    let user = load(&db, users, 42);
    assert_eq!(user.nickname(), Nickname::Missing);
    assert_eq!(user.effective_nickname(), Some("guest"));
    assert!(user.record().field(users::NICKNAME).unwrap().is_none());
    let mut tx = db.begin();
    users
        .set_nickname(&db, &mut tx, 42, NicknameUpdate::None)
        .unwrap();
    tx.commit().unwrap();
    let user = load(&db, users, 42);
    assert_eq!(user.nickname(), Nickname::None);
    assert_eq!(user.effective_nickname(), None);
    let mut tx = db.begin();
    users
        .set_nickname(&db, &mut tx, 42, NicknameUpdate::Missing)
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(load(&db, users, 42).effective_nickname(), Some("guest"));
}

#[test]
fn legacy_update_preserves_new_unknown_and_envelope_fields() {
    let db = Database::new().unwrap();
    let users = setup(&db);
    migrate2(&db, users);
    let unknown = db
        .map()
        .unwrap()
        .map_set(
            b"nested",
            &db.list()
                .unwrap()
                .list_push(&db.bytes(b"\0future").unwrap())
                .unwrap(),
        )
        .unwrap();
    let record = raw(&db, users, 42).set(99, &unknown).unwrap();
    let extension = db.bytes(b"future-envelope").unwrap();
    let extended = record.as_value().map_set(b"extension", &extension).unwrap();
    let mut tx = db.begin();
    tx.put(users.records, &42u64.to_le_bytes(), &extended)
        .unwrap();
    users
        .set_nickname(&db, &mut tx, 42, NicknameUpdate::Text("Al"))
        .unwrap();
    tx.commit().unwrap();
    let mut tx = db.begin();
    users.set_name_v1(&db, &mut tx, 42, "Alicia").unwrap();
    tx.commit().unwrap();
    let user = load(&db, users, 42);
    assert_eq!(user.version(), 2);
    assert_eq!(user.display_name(), "Alicia");
    assert_eq!(user.nickname(), Nickname::Text("Al"));
    assert_eq!(user.record().field(99).unwrap(), Some(unknown));
    assert_eq!(
        user.record().as_value().map_get(b"extension").unwrap(),
        Some(extension)
    );
    assert_eq!(
        user.schema().field(NAME).unwrap().unwrap().name(),
        "display_name"
    );
    assert_eq!(
        Schema::load(&mut db.snapshot(), users.schemas, USER_TYPE, 1)
            .unwrap()
            .field(NAME)
            .unwrap()
            .unwrap()
            .name(),
        "name"
    );
}

#[test]
fn explicit_semantic_migration_retires_id_and_rejects_legacy_writer() {
    let db = Database::new().unwrap();
    let users = setup(&db);
    migrate2(&db, users);
    let mut tx = db.begin();
    assert_eq!(users.migrate_to_v3(&db, &mut tx), Ok(2));
    tx.commit().unwrap();
    let user = load(&db, users, 42);
    assert_eq!(user.age(), Age::Months(300));
    assert!(user.record().field(AGE_YEARS).unwrap().is_none());
    assert_eq!(
        user.record().field(AGE_MONTHS).unwrap().unwrap().as_u64(),
        Ok(300)
    );
    let mut tx = db.begin();
    assert_eq!(
        users.set_name_v1(&db, &mut tx, 42, "invalid"),
        Err(Error::UnsupportedVersion)
    );
    assert_eq!(tx.commit(), Err(EngineError::Closed));
    let mut tx = db.begin();
    assert_eq!(users.migrate_to_v3(&db, &mut tx), Ok(0));
    tx.commit().unwrap();
    assert_eq!(load(&db, users, 42).display_name(), "Alice");
}

#[test]
fn conversion_overflow_or_unknown_target_field_aborts_every_migrated_row() {
    for collision in [false, true] {
        let db = Database::new().unwrap();
        let users = setup(&db);
        migrate2(&db, users);
        let bad = if collision {
            raw(&db, users, 99)
                .set(AGE_MONTHS, &db.bytes(b"unknown").unwrap())
                .unwrap()
        } else {
            raw(&db, users, 99)
                .set(AGE_YEARS, &db.unsigned(u64::MAX).unwrap())
                .unwrap()
        };
        let mut raw_tx = db.begin();
        raw_tx
            .put(users.records, &99u64.to_le_bytes(), bad.as_value())
            .unwrap();
        raw_tx.commit().unwrap();
        let revision = db.snapshot().revision();
        let mut tx = db.begin();
        assert_eq!(
            users.migrate_to_v3(&db, &mut tx),
            Err(if collision {
                Error::MigrationCollision
            } else {
                Error::MigrationOverflow
            })
        );
        assert_eq!(tx.state(), TransactionState::Aborted);
        assert_eq!(tx.commit(), Err(EngineError::Closed));
        assert_eq!(db.snapshot().revision(), revision);
        assert_eq!(load(&db, users, 42).version(), 2);
        assert!(
            db.snapshot()
                .get(users.schemas, &descriptor_key(USER_TYPE, 3))
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn concurrent_email_claims_conflict_and_retry_reports_unique_violation() {
    let db = Database::new().unwrap();
    let users = setup(&db);
    let mut a = db.begin();
    let mut b = db.begin();
    users
        .set_email(&db, &mut a, 42, "same@example.test")
        .unwrap();
    users
        .set_email(&db, &mut b, 99, "same@example.test")
        .unwrap();
    a.commit().unwrap();
    assert_eq!(b.commit(), Err(EngineError::Conflict));
    assert_eq!(load(&db, users, 99).email(), "bob@example.test");
    assert_eq!(
        users
            .find_by_email(&mut db.snapshot(), "same@example.test")
            .unwrap()
            .unwrap()
            .id,
        42
    );
    let mut retry = db.begin();
    assert_eq!(
        users.set_email(&db, &mut retry, 99, "same@example.test"),
        Err(Error::UniqueViolation)
    );
    assert_eq!(retry.commit(), Err(EngineError::Closed));
}

#[test]
fn real_threads_claim_one_email_without_partial_primary_or_index_updates() {
    use std::{sync::Barrier, thread};
    let db = Database::new().unwrap();
    let users = setup(&db);
    let pinned = db.snapshot();
    let ready = Barrier::new(2);
    let results = thread::scope(|scope| {
        let workers: Vec<_> = [42, 99]
            .into_iter()
            .map(|id| {
                let (db, ready) = (&db, &ready);
                scope.spawn(move || {
                    let mut tx = db.begin();
                    users
                        .set_email(db, &mut tx, id, "one@example.test")
                        .unwrap();
                    ready.wait();
                    (id, tx.commit())
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>()
    });
    let winner = results.iter().find(|(_, r)| r.is_ok()).unwrap().0;
    let loser = results
        .iter()
        .find(|(_, r)| *r == Err(EngineError::Conflict))
        .unwrap()
        .0;
    assert_eq!(load(&db, users, winner).email(), "one@example.test");
    let original = if loser == 42 {
        "alice@example.test"
    } else {
        "bob@example.test"
    };
    assert_eq!(load(&db, users, loser).email(), original);
    assert_eq!(
        users
            .find_by_email(&mut db.snapshot(), "one@example.test")
            .unwrap()
            .unwrap()
            .id,
        winner
    );
    assert_eq!(
        users
            .find_by_email(&mut db.snapshot(), original)
            .unwrap()
            .unwrap()
            .id,
        loser
    );
    assert!(
        users
            .find_by_email(&mut pinned.clone(), "one@example.test")
            .unwrap()
            .is_none()
    );
    let mut retry = db.begin();
    assert_eq!(
        users.set_email(&db, &mut retry, loser, "one@example.test"),
        Err(Error::UniqueViolation)
    );
    assert_eq!(retry.state(), TransactionState::Aborted);
}

#[test]
fn independent_typed_updates_merge_but_one_record_conflicts() {
    let db = Database::new().unwrap();
    let users = setup(&db);
    let mut a = db.begin();
    let mut b = db.begin();
    users.set_display_name(&db, &mut a, 42, "Alicia").unwrap();
    users.set_display_name(&db, &mut b, 99, "Robert").unwrap();
    a.commit().unwrap();
    b.commit().unwrap();
    assert_eq!(load(&db, users, 42).display_name(), "Alicia");
    assert_eq!(load(&db, users, 99).display_name(), "Robert");
    let mut a = db.begin();
    let mut b = db.begin();
    users.set_display_name(&db, &mut a, 42, "Alice").unwrap();
    users
        .set_email(&db, &mut b, 42, "new@example.test")
        .unwrap();
    a.commit().unwrap();
    assert_eq!(b.commit(), Err(EngineError::Conflict));
    assert_eq!(load(&db, users, 42).email(), "alice@example.test");
}

#[test]
fn schema_lookup_and_secondary_lookup_reads_are_transaction_dependencies() {
    let db = Database::new().unwrap();
    let users = setup(&db);
    let mut stale = db.begin();
    users
        .set_display_name(&db, &mut stale, 42, "pending")
        .unwrap();
    let old = db
        .snapshot()
        .get(users.schemas, &descriptor_key(USER_TYPE, 1))
        .unwrap()
        .unwrap();
    let mut raw_tx = db.begin();
    raw_tx
        .put(users.schemas, &descriptor_key(USER_TYPE, 1), &old)
        .unwrap();
    raw_tx.commit().unwrap();
    assert_eq!(stale.commit(), Err(EngineError::Conflict));
    assert_eq!(load(&db, users, 42).display_name(), "Alice");
    let mut reader = db.begin();
    assert_eq!(
        users
            .find_by_email(&mut reader, "alice@example.test")
            .unwrap()
            .unwrap()
            .id,
        42
    );
    users
        .set_display_name(&db, &mut reader, 99, "decision")
        .unwrap();
    let mut writer = db.begin();
    users
        .set_email(&db, &mut writer, 42, "other@example.test")
        .unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.commit(), Err(EngineError::Conflict));
    assert_eq!(load(&db, users, 99).display_name(), "Bob");
}

#[test]
fn migration_scan_conflicts_with_intervening_update_without_partial_schema() {
    let db = Database::new().unwrap();
    let users = setup(&db);
    let mut migration = db.begin();
    users.migrate_to_v2(&db, &mut migration).unwrap();
    let mut update = db.begin();
    users
        .set_display_name(&db, &mut update, 99, "Robert")
        .unwrap();
    update.commit().unwrap();
    assert_eq!(migration.commit(), Err(EngineError::Conflict));
    assert_eq!(load(&db, users, 42).version(), 1);
    assert!(
        db.snapshot()
            .get(users.schemas, &descriptor_key(USER_TYPE, 2))
            .unwrap()
            .is_none()
    );
}

#[test]
fn raw_writes_bypass_layer_guarantees_and_inconsistent_lookups_are_reported() {
    let db = Database::new().unwrap();
    let users = setup(&db);
    let mut tx = db.begin();
    tx.put(
        users.emails,
        b"alice@example.test",
        &db.unsigned(99).unwrap(),
    )
    .unwrap();
    tx.commit().unwrap();
    assert!(matches!(
        users.find_by_email(&mut db.snapshot(), "alice@example.test"),
        Err(Error::IndexInconsistent)
    ));
    let mut tx = db.begin();
    assert_eq!(
        users.set_email(&db, &mut tx, 42, "new@example.test"),
        Err(Error::IndexInconsistent)
    );
    assert_eq!(tx.commit(), Err(EngineError::Closed));
    assert_eq!(load(&db, users, 42).email(), "alice@example.test");
}

type State = Vec<Vec<(Vec<u8>, Model)>>;
fn state(snapshot: &weekend_db::Snapshot<'_>, users: Users) -> State {
    [users.schemas, users.records, users.emails]
        .iter()
        .map(|c| {
            snapshot
                .scan(*c)
                .unwrap()
                .into_iter()
                .map(|(k, v)| (k, Model::read(&v)))
                .collect()
        })
        .collect()
}
fn fixture() -> (Vec<u8>, [u64; 3]) {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), *b"schema-test-0001", Limits::default()).unwrap();
    let users = setup(&db);
    migrate2(&db, users);
    db.retain_snapshot(&db.snapshot()).unwrap();
    (disk.durable_image().unwrap(), users.ids())
}

#[test]
fn retained_schemas_data_unknown_fields_and_lookups_survive_reopen() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), *b"schema-history01", Limits::default()).unwrap();
    let users = setup(&db);
    let ids = users.ids();
    let unknown = db.bytes(b"future").unwrap();
    let record = raw(&db, users, 42).set(99, &unknown).unwrap();
    let mut tx = db.begin();
    tx.put(users.records, &42u64.to_le_bytes(), record.as_value())
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    let v1 = db.retain_snapshot(&db.snapshot()).unwrap();
    migrate2(&db, users);
    let mut tx = db.begin();
    users
        .set_nickname(&db, &mut tx, 42, NicknameUpdate::Text("Al"))
        .unwrap();
    users.set_name_v1(&db, &mut tx, 42, "Alicia").unwrap();
    tx.commit().unwrap();
    drop(tx);
    let v2 = db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    users.migrate_to_v3(&db, &mut tx).unwrap();
    users
        .set_email(&db, &mut tx, 42, "alicia@example.test")
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    drop(db);
    disk.crash(Crash::DropAll).unwrap();
    let db = Database::open_on(disk, Limits::default()).unwrap();
    let users = Users::from_ids(&db, ids);
    let mut old = db.retained_snapshot(v1).unwrap();
    let first = users
        .find_by_email(&mut old, "alice@example.test")
        .unwrap()
        .unwrap();
    assert_eq!(first.age(), Age::Years(25));
    assert_eq!(first.schema().field(NAME).unwrap().unwrap().name(), "name");
    assert!(
        old.get(users.schemas, &descriptor_key(USER_TYPE, 2))
            .unwrap()
            .is_none()
    );
    let mut additive = db.retained_snapshot(v2).unwrap();
    let second = users.load(&mut additive, 42).unwrap().unwrap();
    assert_eq!(second.nickname(), Nickname::Text("Al"));
    assert_eq!(second.version(), 2);
    assert!(
        additive
            .get(users.schemas, &descriptor_key(USER_TYPE, 3))
            .unwrap()
            .is_none()
    );
    let current = users
        .find_by_email(&mut db.snapshot(), "alicia@example.test")
        .unwrap()
        .unwrap();
    assert_eq!(current.age(), Age::Months(300));
    assert_eq!(current.record().field(99).unwrap(), Some(unknown));
    assert!(
        users
            .find_by_email(&mut db.snapshot(), "alice@example.test")
            .unwrap()
            .is_none()
    );
    db.release_retained(v1).unwrap();
    assert_eq!(users.load(&mut old, 42).unwrap().unwrap().version(), 1);
}

#[test]
fn storage_faults_never_publish_a_partial_schema_primary_index_batch() {
    let (image, ids) = fixture();
    for migration in [false, true] {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let db = Database::open_on(disk.clone(), Limits::default()).unwrap();
        let users = Users::from_ids(&db, ids);
        let old = state(&db.snapshot(), users);
        let mut tx = db.begin();
        if migration {
            users.migrate_to_v3(&db, &mut tx).unwrap();
        }
        users
            .set_email(&db, &mut tx, 42, "new@example.test")
            .unwrap();
        disk.arm(usize::MAX, Fault::Error);
        tx.commit().unwrap();
        let trace = disk.trace().unwrap();
        let new = state(&db.snapshot(), users);
        drop(tx);
        drop(db);
        for step in 0..trace.len() {
            for policy in [Crash::DropAll, Crash::PersistAll, Crash::Mask(0x5555)] {
                let disk = SimulatedDisk::from_image(&image).unwrap();
                let db = Database::open_on(disk.clone(), Limits::default()).unwrap();
                let users = Users::from_ids(&db, ids);
                let mut tx = db.begin();
                if migration {
                    users.migrate_to_v3(&db, &mut tx).unwrap();
                }
                users
                    .set_email(&db, &mut tx, 42, "new@example.test")
                    .unwrap();
                disk.arm(step, Fault::Error);
                let error = tx.commit().unwrap_err();
                assert!(matches!(
                    error,
                    EngineError::Io | EngineError::OutcomeUnknown
                ));
                assert!(db.requires_recovery());
                assert_eq!(state(&db.snapshot(), users), old);
                disk.crash(policy).unwrap();
                drop(tx);
                drop(db);
                let db = Database::open_on(disk, Limits::default()).unwrap();
                let users = Users::from_ids(&db, ids);
                let actual = state(&db.snapshot(), users);
                assert!(
                    actual == old || actual == new,
                    "migration={migration}, step={step}, policy={policy:?}"
                );
                if error == EngineError::Io {
                    assert_eq!(actual, old);
                }
                assert_eq!(state(&db.retained_snapshot(1).unwrap(), users), old);
                let user = load(&db, users, 42);
                assert_eq!(
                    users
                        .find_by_email(&mut db.snapshot(), user.email())
                        .unwrap()
                        .unwrap()
                        .id,
                    42
                );
            }
        }
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn wrapper_allocation_failures_abort_drafts_before_primary_index_or_migration_can_diverge() {
    let (image, ids) = fixture();
    for operation in 0..3 {
        let mut succeeded = false;
        for budget in 0..1024 {
            let disk = SimulatedDisk::from_image(&image).unwrap();
            let db = Database::open_on(disk.clone(), Limits::default()).unwrap();
            let users = Users::from_ids(&db, ids);
            let old = state(&db.snapshot(), users);
            let mut tx = db.begin();
            db.fail_allocations_after(Some(budget));
            let result = match operation {
                0 => users.set_email(&db, &mut tx, 42, "new@example.test"),
                1 => users.delete(&mut tx, 42),
                _ => users.migrate_to_v3(&db, &mut tx).map(|_| ()),
            };
            db.fail_allocations_after(None);
            match result {
                Ok(()) => {
                    tx.commit().unwrap();
                    succeeded = true;
                    break;
                }
                Err(e) => {
                    assert_eq!(
                        e,
                        Error::Engine(EngineError::OutOfMemory),
                        "operation={operation}, budget={budget}"
                    );
                    assert_eq!(tx.state(), TransactionState::Aborted);
                    assert_eq!(tx.commit(), Err(EngineError::Closed));
                    assert!(!db.requires_recovery());
                    assert_eq!(state(&db.snapshot(), users), old);
                    assert_eq!(disk.durable_image().unwrap(), image);
                }
            }
        }
        assert!(succeeded, "operation={operation}");
    }
}
