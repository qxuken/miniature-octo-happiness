use weekend_db_typed::{
    Error, Model, Presence, Result, Store,
    engine::{self, storage::MemoryStore},
};

#[derive(Model, Debug, PartialEq)]
#[weekend(type_id = 100, version = 1)]
struct OldTask {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2)]
    title: String,
    #[weekend(id = 3)]
    done: bool,
}
#[derive(Model, Debug, PartialEq)]
#[weekend(type_id = 100, version = 2)]
struct Task {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2)]
    title: String,
    #[weekend(id = 3)]
    done: bool,
    #[weekend(id=4,default=Vec::new())]
    tags: Vec<String>,
    #[weekend(id = 5)]
    note: Option<String>,
    #[weekend(id = 6, default = 0)]
    priority: u8,
    #[weekend(id = 7)]
    detail: Presence<String>,
}
fn old(slug: &str) -> OldTask {
    OldTask {
        slug: slug.into(),
        title: "Read docs".into(),
        done: false,
    }
}

#[test]
fn typed_crud_lookup_unknown_fields_history_restore_and_reopen() -> Result<()> {
    let backend = MemoryStore::new()?;
    let mut store = Store::create_on(backend.clone(), [1; 16], engine::Limits::default())?;
    let legacy = store.table::<OldTask>("tasks")?;
    store.write(|w| {
        w.insert(&legacy, 1, &old("first"))?;
        w.insert(&legacy, 2, &old("second"))
    })?;
    let old_id = store.checkpoint()?;
    let tasks = store.table::<Task>("tasks")?;
    assert_eq!(store.read(|r| Ok(r.get(&tasks, 1)?.unwrap().priority))?, 0);
    assert_eq!(
        store.write(|w| w.update(&tasks, 1, |t| {
            t.done = true;
            Ok(())
        })),
        Err(Error::MigrationRequired)
    );
    assert_eq!(
        store.write(|w| w.migrate(&tasks, |_, task| {
            task.priority = 2;
            task.tags.push("reading".into());
            Ok(())
        }))?,
        2
    );
    store.write(|w| {
        w.update(&tasks, 1, |t| {
            t.note = Some("future field".into());
            t.detail = Presence::Null;
            Ok(())
        })
    })?;
    let v2_id = store.checkpoint()?;
    // An older Rust model patches known fields of a v2 envelope.
    store.write(|w| {
        w.update(&legacy, 1, |t| {
            t.title = "Read carefully".into();
            t.slug = "renamed".into();
            Ok(())
        })
    })?;
    store.read(|r| {
        assert!(r.find(&tasks, Task::SLUG, &"first".to_owned())?.is_none());
        let task = r.find(&tasks, Task::SLUG, &"renamed".to_owned())?.unwrap();
        assert_eq!(task.version(), 2);
        assert_eq!(task.note.as_deref(), Some("future field"));
        assert_eq!(task.tags, vec!["reading"]);
        assert_eq!(task.detail, Presence::Null);
        Ok(())
    })?;
    store.restore_table(&tasks, v2_id)?;
    assert!(
        store
            .read(|r| r.find(&tasks, Task::SLUG, "first"))?
            .is_some()
    );
    store.restore_table(&tasks, old_id)?;
    assert_eq!(store.read(|r| Ok(r.get(&tasks, 1)?.unwrap().version()))?, 1);
    assert_eq!(store.write(|w| w.migrate(&tasks, |_, _| Ok(())))?, 2);
    drop(store);
    let mut store = Store::open_on(backend, engine::Limits::default())?;
    let tasks = store.table::<Task>("tasks")?;
    assert_eq!(store.read(|r| Ok(r.list(&tasks)?.len()))?, 2);
    assert_eq!(
        store.read_retained(v2_id, |r| Ok(r.get(&tasks, 1)?.unwrap().note.clone()))?,
        Some("future field".into())
    );
    Ok(())
}

#[test]
fn ignored_write_errors_and_caught_panics_abort_every_preceding_change() -> Result<()> {
    let mut store = Store::memory()?;
    let tasks = store.table::<OldTask>("tasks")?;
    store.write(|w| {
        w.insert(&tasks, 1, &old("a"))?;
        w.insert(&tasks, 2, &old("b"))
    })?;
    let error = store.write(|w| {
        w.update(&tasks, 1, |t| {
            t.title = "must roll back".into();
            Ok(())
        })?;
        assert_eq!(
            w.update(&tasks, 2, |t| {
                t.slug = "a".into();
                Ok(())
            }),
            Err(Error::UniqueViolation(1))
        );
        Ok(())
    });
    assert_eq!(error, Err(Error::Engine(engine::Error::Closed)));
    let error = store.write(|w| {
        w.update(&tasks, 1, |t| {
            t.title = "also rolls back".into();
            Ok(())
        })?;
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            w.update(&tasks, 2, |_| panic!("intentional callback panic"))
        }));
        assert!(panic.is_err());
        Ok(())
    });
    assert_eq!(error, Err(Error::Engine(engine::Error::Closed)));
    assert_eq!(
        store.read(|r| Ok(r.get(&tasks, 1)?.unwrap().title.clone()))?,
        "Read docs"
    );
    assert!(
        store
            .read(|r| r.find(&tasks, OldTask::SLUG, "b"))?
            .is_some()
    );
    Ok(())
}

#[test]
fn overlapping_transactions_conflict_and_unique_claims_never_diverge() -> Result<()> {
    use std::{sync::Barrier, thread};
    let mut store = Store::memory()?;
    let tasks = store.table::<OldTask>("tasks")?;
    let ready = Barrier::new(2);
    let results = thread::scope(|s| {
        let workers: Vec<_> = (1..=2)
            .map(|id| {
                s.spawn({
                    let (store, tasks, ready) = (&store, &tasks, &ready);
                    move || {
                        store.write(|w| {
                            w.insert(tasks, id, &old("one-owner"))?;
                            ready.wait();
                            Ok(())
                        })
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::Engine(engine::Error::Conflict)))
            .count(),
        1
    );
    store.read(|r| {
        assert_eq!(r.list(&tasks)?.len(), 1);
        assert!(r.find(&tasks, OldTask::SLUG, "one-owner")?.is_some());
        Ok(())
    })?;
    Ok(())
}

#[derive(Model)]
#[weekend(type_id = 200, version = 1)]
struct Years {
    #[weekend(id = 1, unique)]
    name: String,
    #[weekend(id = 2)]
    years: u64,
}
#[derive(Model)]
#[weekend(type_id = 200, version = 2, reserved(2))]
struct Months {
    #[weekend(id = 1, unique)]
    name: String,
    #[weekend(id = 3, default = 0)]
    months: u64,
}
#[test]
fn semantic_migration_retires_ids_and_failure_rolls_back_every_record() -> Result<()> {
    let mut store = Store::memory()?;
    let old = store.table::<Years>("ages")?;
    store.write(|w| {
        w.insert(
            &old,
            1,
            &Years {
                name: "a".into(),
                years: 25,
            },
        )?;
        w.insert(
            &old,
            2,
            &Years {
                name: "b".into(),
                years: u64::MAX,
            },
        )
    })?;
    let new = store.table::<Months>("ages")?;
    let migrate = |record: &weekend_db_typed::Record, months: &mut Months| -> Result<()> {
        months.months = record
            .field(2)?
            .unwrap()
            .as_u64()?
            .checked_mul(12)
            .ok_or(Error::Schema(engine::schema::Error::MigrationOverflow))?;
        Ok(())
    };
    assert_eq!(
        store.write(|w| w.migrate(&new, migrate)),
        Err(Error::Schema(engine::schema::Error::MigrationOverflow))
    );
    assert_eq!(store.read(|r| Ok(r.get(&old, 1)?.unwrap().version()))?, 1);
    store.write(|w| {
        w.update(&old, 2, |v| {
            v.years = 30;
            Ok(())
        })
    })?;
    assert_eq!(store.write(|w| w.migrate(&new, migrate))?, 2);
    store.read(|r| {
        let row = r.find(&new, Months::NAME, "a")?.unwrap();
        assert_eq!(row.months, 300);
        assert!(row.record().field(2)?.is_none());
        Ok(())
    })?;
    Ok(())
}

#[test]
fn prospective_known_id_collision_is_never_silently_overwritten() -> Result<()> {
    let mut store = Store::memory()?;
    let old = store.table::<OldTask>("tasks")?;
    store.write(|w| w.insert(&old, 1, &old_task()))?;
    let record = store.read(|r| Ok(r.get(&old, 1)?.unwrap().record().clone()))?;
    let raw = record.set(6, &store.database().bytes(b"future-but-different")?)?;
    let mut tx = store.database().begin();
    tx.put(old.collection(), &1u64.to_le_bytes(), raw.as_value())?;
    tx.commit()?;
    drop(tx);
    let table = store.table::<Task>("tasks")?;
    assert_eq!(
        store.write(|w| w.migrate(&table, |_, _| Ok(()))),
        Err(Error::Schema(engine::schema::Error::MigrationCollision))
    );
    assert_eq!(
        store.read(|r| Ok(r
            .get(&old, 1)?
            .unwrap()
            .record()
            .field(6)?
            .unwrap()
            .as_bytes()?
            .to_vec()))?,
        b"future-but-different"
    );
    Ok(())
}
fn old_task() -> OldTask {
    old("task")
}

#[derive(Model)]
#[weekend(type_id = 300, version = 1)]
struct DefaultsV1 {
    #[weekend(id=1,default=Some("old".to_owned()))]
    note: Option<String>,
    #[weekend(id = 2)]
    detail: Presence<String>,
    #[weekend(id = 3)]
    number: u8,
}
#[derive(Model)]
#[weekend(type_id = 300, version = 2)]
struct DefaultsV2 {
    #[weekend(id=1,default=Some("new".to_owned()))]
    note: Option<String>,
    #[weekend(id = 2)]
    detail: Presence<String>,
    #[weekend(id = 3)]
    number: u8,
}
#[test]
fn defaults_belong_to_stored_schema_and_presence_range_and_envelope_survive() -> Result<()> {
    let mut store = Store::memory()?;
    let old = store.table::<DefaultsV1>("defaults")?;
    store.write(|w| {
        w.insert(
            &old,
            1,
            &DefaultsV1 {
                note: None,
                detail: Presence::Missing,
                number: 3,
            },
        )
    })?;
    let record = store.read(|r| Ok(r.get(&old, 1)?.unwrap().record().clone()))?;
    let raw = record
        .remove(1)?
        .as_value()
        .map_set(b"extension", &store.database().bytes(b"keep")?)?;
    let mut tx = store.database().begin();
    tx.put(old.collection(), &1u64.to_le_bytes(), &raw)?;
    tx.commit()?;
    drop(tx);
    let new = store.table::<DefaultsV2>("defaults")?;
    store.read(|r| {
        let row = r.get(&new, 1)?.unwrap();
        assert_eq!(row.note.as_deref(), Some("old"));
        assert_eq!(row.detail, Presence::Missing);
        assert_eq!(row.number, 3);
        Ok(())
    })?;
    store.write(|w| w.migrate(&new, |_, _| Ok(())))?;
    store.write(|w| {
        w.update(&new, 1, |v| {
            v.detail = Presence::Null;
            Ok(())
        })
    })?;
    store.read(|r| {
        let row = r.get(&new, 1)?.unwrap();
        assert_eq!(row.detail, Presence::Null);
        assert_eq!(
            row.record()
                .as_value()
                .map_get(b"extension")?
                .unwrap()
                .as_bytes()?,
            b"keep"
        );
        Ok(())
    })?;
    let record = store.read(|r| {
        Ok(r.get(&new, 1)?
            .unwrap()
            .record()
            .set(3, &store.database().unsigned(256)?)?)
    })?;
    let mut tx = store.database().begin();
    tx.put(new.collection(), &1u64.to_le_bytes(), record.as_value())?;
    tx.commit()?;
    drop(tx);
    assert!(matches!(
        store.read(|r| r.get(&new, 1)),
        Err(Error::InvalidField { id: 3, .. })
    ));
    Ok(())
}

#[test]
fn metadata_validation_context_safety_compaction_and_inconsistent_index_detection() -> Result<()> {
    let store_bytes = MemoryStore::new()?;
    let mut store = Store::create_on(store_bytes, [7; 16], engine::Limits::default())?;
    let table = store.table::<OldTask>("tasks")?;
    store.write(|w| w.insert(&table, 1, &old("a")))?;
    assert!(matches!(
        store.read(|r| r.find(&table, weekend_db_typed::Index::<OldTask, u64>::new(1), &42)),
        Err(Error::InvalidField { id: 1, .. })
    ));
    assert!(matches!(
        store.table::<Years>("tasks"),
        Err(Error::TypeMismatch)
    ));
    let checkpoint = store.checkpoint()?;
    let target = MemoryStore::new()?;
    store.compact_to(target.clone())?;
    let mut copy = Store::open_on(target, engine::Limits::default())?;
    assert!(matches!(
        copy.read(|r| r.get(&table, 1)),
        Err(Error::Engine(engine::Error::ForeignStore))
    ));
    let copied = copy.table::<OldTask>("tasks")?;
    assert!(
        copy.read_retained(checkpoint, |r| r.find(&copied, OldTask::SLUG, "a"))?
            .is_some()
    );
    let mut tx = copy.database().begin();
    tx.delete(copied.collection(), &1u64.to_le_bytes())?;
    tx.commit()?;
    drop(tx);
    assert!(matches!(
        copy.read(|r| r.find(&copied, OldTask::SLUG, "a")),
        Err(Error::IndexInconsistent)
    ));
    assert_eq!(
        copy.write(|w| w.insert(&copied, 1, &old("a"))),
        Err(Error::IndexInconsistent)
    );
    Ok(())
}

#[test]
fn callbacks_execute_once_and_schema_reads_are_conflict_dependencies() -> Result<()> {
    let mut store = Store::memory()?;
    let tasks = store.table::<OldTask>("tasks")?;
    store.write(|w| w.insert(&tasks, 1, &old("a")))?;
    let mut calls = 0;
    let result = store.write(|w| {
        calls += 1;
        w.update(&tasks, 1, |t| {
            t.done = true;
            Ok(())
        })?;
        let db = store.database();
        let mut inner = db.begin();
        let key = engine::schema::descriptor_key(100, 1);
        let schema = inner.get(db.collection_id(2), &key)?.unwrap();
        inner.put(db.collection_id(2), &key, &schema)?;
        inner.commit()?;
        Ok(())
    });
    assert_eq!(result, Err(Error::Engine(engine::Error::Conflict)));
    assert_eq!(calls, 1);
    assert!(!store.read(|r| Ok(r.get(&tasks, 1)?.unwrap().done))?);
    Ok(())
}

#[derive(Model)]
#[weekend(type_id = 401, version = 1)]
struct RawPresence {
    #[weekend(id=1,default=Presence::Value("fallback".to_owned()))]
    value: Presence<String>,
}
#[test]
fn presence_keeps_missing_even_when_the_descriptor_supplies_a_default() -> Result<()> {
    let mut store = Store::memory()?;
    let table = store.table::<RawPresence>("presence")?;
    store.write(|w| {
        w.insert(
            &table,
            1,
            &RawPresence {
                value: Presence::Missing,
            },
        )
    })?;
    store.read(|r| {
        let doc = r.get(&table, 1)?.unwrap();
        assert_eq!(doc.value, Presence::Missing);
        assert_eq!(
            doc.schema()
                .read_field(doc.record(), 1)?
                .unwrap()
                .as_bytes()?,
            b"fallback"
        );
        Ok(())
    })?;
    store.write(|w| w.update(&table, 1, |_| Ok(())))?;
    assert!(store.read(|r| Ok(r.get(&table, 1)?.unwrap().record().field(1)?.is_none()))?);
    Ok(())
}

#[derive(Model)]
#[weekend(type_id = 402, version = 1)]
struct ShadowingNames {
    #[weekend(id = 8)]
    version: u64,
    #[weekend(id = 9)]
    type_id: u64,
}
#[test]
fn generated_field_constants_never_shadow_model_identity_in_expansions() -> Result<()> {
    let mut store = Store::memory()?;
    let table = store.table::<ShadowingNames>("shadow")?;
    store.write(|w| {
        w.insert(
            &table,
            1,
            &ShadowingNames {
                version: 50,
                type_id: 60,
            },
        )
    })?;
    store.read(|r| {
        let row = r.get(&table, 1)?.unwrap();
        assert_eq!(row.version, 50);
        assert_eq!(row.type_id, 60);
        assert_eq!(row.version(), 1);
        Ok(())
    })?;
    Ok(())
}

#[cfg(feature = "fault-injection")]
#[test]
fn every_core_allocation_failure_aborts_primary_and_index_updates() -> Result<()> {
    let mut succeeded = false;
    for budget in 0..1024 {
        let mut store = Store::memory()?;
        let table = store.table::<OldTask>("tasks")?;
        store.write(|w| {
            w.insert(&table, 1, &old("a"))?;
            w.insert(&table, 2, &old("b"))
        })?;
        store.database().fail_allocations_after(Some(budget));
        let result = store.write(|w| {
            w.update(&table, 1, |t| {
                t.title = "new title".into();
                Ok(())
            })?;
            w.update(&table, 2, |t| {
                t.slug = "new-slug".into();
                Ok(())
            })
        });
        store.database().fail_allocations_after(None);
        if result.is_ok() {
            succeeded = true;
            break;
        }
        assert_eq!(
            result,
            Err(Error::Engine(engine::Error::OutOfMemory)),
            "budget={budget}"
        );
        store.read(|r| {
            assert_eq!(r.get(&table, 1)?.unwrap().title, "Read docs");
            assert!(r.find(&table, OldTask::SLUG, "b")?.is_some());
            assert!(r.find(&table, OldTask::SLUG, "new-slug")?.is_none());
            Ok(())
        })?;
    }
    assert!(succeeded);
    Ok(())
}

#[test]
fn storage_faults_reopen_complete_primary_index_and_retained_states() -> Result<()> {
    use engine::storage::{Crash, Fault, IoKind, SimulatedDisk};
    let disk = SimulatedDisk::new()?;
    let mut store = Store::create_on(disk.clone(), [8; 16], engine::Limits::default())?;
    let table = store.table::<OldTask>("tasks")?;
    store.write(|w| w.insert(&table, 1, &old("original")))?;
    let snapshot = store.checkpoint()?;
    drop(store);
    let image = disk.durable_image()?;
    let prepare = |disk: SimulatedDisk| -> Result<(Store, weekend_db_typed::Table<OldTask>)> {
        let mut s = Store::open_on(disk, engine::Limits::default())?;
        let t = s.table::<OldTask>("tasks")?;
        Ok((s, t))
    };
    let (store, table) = prepare(disk.clone())?;
    disk.arm(usize::MAX, Fault::Error);
    store.write(|w| {
        w.update(&table, 1, |t| {
            t.slug = "changed".into();
            t.title = "Changed".into();
            Ok(())
        })
    })?;
    let trace = disk.trace()?;
    drop(store);
    for (step, event) in trace.iter().enumerate() {
        let disk = SimulatedDisk::from_image(&image)?;
        let (store, table) = prepare(disk.clone())?;
        disk.arm(
            step,
            match event.kind {
                IoKind::Sync => Fault::SyncError(Crash::PersistAll),
                IoKind::Write => Fault::Tear {
                    prefix: 1,
                    durable: true,
                },
                _ => Fault::Error,
            },
        );
        assert!(
            store
                .write(|w| w.update(&table, 1, |t| {
                    t.slug = "changed".into();
                    t.title = "Changed".into();
                    Ok(())
                }))
                .is_err()
        );
        drop(store);
        disk.crash(Crash::PersistAll)?;
        let (store, table) = prepare(disk)?;
        store.read(|r| {
            let doc = r.get(&table, 1)?.unwrap();
            let changed = doc.slug == "changed";
            assert_eq!(doc.title, if changed { "Changed" } else { "Read docs" });
            assert_eq!(r.find(&table, OldTask::SLUG, "changed")?.is_some(), changed);
            assert_eq!(
                r.find(&table, OldTask::SLUG, "original")?.is_some(),
                !changed
            );
            Ok(())
        })?;
        store.read_retained(snapshot, |r| {
            assert_eq!(r.get(&table, 1)?.unwrap().slug, "original");
            assert!(r.find(&table, OldTask::SLUG, "original")?.is_some());
            Ok(())
        })?;
    }
    Ok(())
}

struct CannotAllocate;
impl weekend_db_typed::FieldValue for CannotAllocate {
    const KIND: engine::Kind = engine::Kind::Unsigned;
    fn encode(&self, db: &engine::Database) -> Result<engine::Value> {
        Ok(db.unsigned(1)?)
    }
    fn decode(_: &engine::Value) -> std::result::Result<Self, weekend_db_typed::DecodeError> {
        Err(weekend_db_typed::DecodeError::OutOfMemory)
    }
}
#[derive(Model)]
#[weekend(type_id = 900, version = 1)]
struct AllocationFailure {
    #[weekend(id = 1)]
    value: CannotAllocate,
}
#[test]
fn custom_decoder_resource_failure_is_preserved_and_aborts_earlier_writes() -> Result<()> {
    let mut store = Store::memory()?;
    let tasks = store.table::<OldTask>("tasks")?;
    let bad = store.table::<AllocationFailure>("bad")?;
    store.write(|w| {
        w.insert(&tasks, 1, &old("a"))?;
        w.insert(
            &bad,
            1,
            &AllocationFailure {
                value: CannotAllocate,
            },
        )
    })?;
    let result = store.write(|w| {
        w.update(&tasks, 1, |t| {
            t.done = true;
            Ok(())
        })?;
        assert!(matches!(
            w.get(&bad, 1),
            Err(Error::Engine(engine::Error::OutOfMemory))
        ));
        Ok(())
    });
    assert_eq!(result, Err(Error::Engine(engine::Error::Closed)));
    assert!(!store.read(|r| Ok(r.get(&tasks, 1)?.unwrap().done))?);
    Ok(())
}

#[derive(Model)]
#[weekend(type_id = 901, version = 1)]
struct Scalars {
    #[weekend(id = 1)]
    flag: bool,
    #[weekend(id = 2)]
    signed: i8,
    #[weekend(id = 3)]
    counter: u32,
    #[weekend(id = 4, unique)]
    float: f64,
    #[weekend(id = 5, unique)]
    payload: weekend_db_typed::Bytes,
    #[weekend(id = 6)]
    nested: Vec<Option<String>>,
}
#[test]
fn scalar_boundaries_binary_lookup_and_float_bit_identity_roundtrip() -> Result<()> {
    use weekend_db_typed::Bytes;
    let mut store = Store::memory()?;
    let table = store.table::<Scalars>("scalars")?;
    let bits = 0x7ff8_0000_0000_0042;
    store.write(|w| {
        w.insert(
            &table,
            1,
            &Scalars {
                flag: true,
                signed: i8::MIN,
                counter: u32::MAX,
                float: f64::from_bits(bits),
                payload: Bytes(vec![0, 255, 1]),
                nested: vec![Some("café".into()), None],
            },
        )
    })?;
    store.read(|r| {
        let row = r
            .find(&table, Scalars::PAYLOAD, [0, 255, 1].as_slice())?
            .unwrap();
        assert!(row.flag);
        assert_eq!(row.signed, i8::MIN);
        assert_eq!(row.counter, u32::MAX);
        assert_eq!(row.float.to_bits(), bits);
        assert_eq!(row.nested, vec![Some("café".into()), None]);
        assert!(
            r.find(&table, Scalars::FLOAT, &f64::from_bits(bits))?
                .is_some()
        );
        assert!(
            r.find(&table, Scalars::FLOAT, &f64::from_bits(bits + 1))?
                .is_none()
        );
        Ok(())
    })?;
    store.write(|w| {
        w.update(&table, 1, |v| {
            v.float = -0.0;
            Ok(())
        })
    })?;
    store.read(|r| {
        assert!(r.find(&table, Scalars::FLOAT, &-0.0)?.is_some());
        assert!(r.find(&table, Scalars::FLOAT, &0.0)?.is_none());
        Ok(())
    })?;
    Ok(())
}

#[derive(Model)]
#[weekend(type_id = 700, version = 1)]
struct UniqueDefaultV1 {
    #[weekend(id=1,unique,default=Presence::Value("old-default".to_owned()))]
    value: Presence<String>,
}
#[derive(Model)]
#[weekend(type_id = 700, version = 2)]
struct UniqueDefaultV2 {
    #[weekend(id=1,unique,default=Presence::Value("new-default".to_owned()))]
    value: Presence<String>,
}

#[test]
fn missing_unique_defaults_remain_unindexed_through_crud_history_and_reopen() -> Result<()> {
    let bytes = MemoryStore::new()?;
    let mut store = Store::create_on(bytes.clone(), [70; 16], engine::Limits::default())?;
    let table = store.table::<UniqueDefaultV1>("unique-defaults")?;
    let query = Presence::Value("old-default".to_owned());
    store.write(|w| {
        w.insert(
            &table,
            1,
            &UniqueDefaultV1 {
                value: Presence::Missing,
            },
        )?;
        w.insert(
            &table,
            2,
            &UniqueDefaultV1 {
                value: Presence::Missing,
            },
        )?;
        w.insert(
            &table,
            3,
            &UniqueDefaultV1 {
                value: Presence::Null,
            },
        )?;
        w.insert(
            &table,
            4,
            &UniqueDefaultV1 {
                value: Presence::Null,
            },
        )?;
        assert!(w.find(&table, UniqueDefaultV1::VALUE, &query)?.is_none());
        Ok(())
    })?;
    store.read(|r| {
        let doc = r.get(&table, 1)?.unwrap();
        assert_eq!(doc.value, Presence::Missing);
        assert!(doc.record().field(1)?.is_none());
        assert_eq!(
            doc.schema()
                .read_field(doc.record(), 1)?
                .unwrap()
                .as_bytes()?,
            b"old-default"
        );
        assert!(r.find(&table, UniqueDefaultV1::VALUE, &query)?.is_none());
        Ok(())
    })?;
    let missing = store.checkpoint()?;
    store.write(|w| {
        w.update(&table, 1, |row| {
            row.value = query.clone();
            Ok(())
        })
    })?;
    assert_eq!(
        store.read(|r| Ok(r
            .find(&table, UniqueDefaultV1::VALUE, &query)?
            .unwrap()
            .id()))?,
        1
    );
    assert_eq!(
        store.write(|w| w.update(&table, 2, |row| {
            row.value = query.clone();
            Ok(())
        })),
        Err(Error::UniqueViolation(1))
    );
    assert_eq!(
        store.read(|r| Ok(r.get(&table, 2)?.unwrap().value.clone()))?,
        Presence::Missing
    );
    let present = store.checkpoint()?;
    store.write(|w| {
        w.update(&table, 1, |row| {
            row.value = Presence::Missing;
            Ok(())
        })
    })?;
    store.write(|w| {
        w.update(&table, 2, |row| {
            row.value = query.clone();
            Ok(())
        })?;
        assert_eq!(
            w.find(&table, UniqueDefaultV1::VALUE, &query)?
                .unwrap()
                .id(),
            2
        );
        Ok(())
    })?;
    store.write(|w| {
        w.update(&table, 2, |row| {
            row.value = Presence::Null;
            Ok(())
        })
    })?;
    assert!(
        store
            .read(|r| r.find(&table, UniqueDefaultV1::VALUE, &query))?
            .is_none()
    );
    store.write(|w| {
        w.insert(
            &table,
            5,
            &UniqueDefaultV1 {
                value: query.clone(),
            },
        )?;
        w.delete(&table, 1)?; // Deleting absence must not remove another row's key.
        w.delete(&table, 4)?;
        assert_eq!(
            w.find(&table, UniqueDefaultV1::VALUE, &query)?
                .unwrap()
                .id(),
            5
        );
        Ok(())
    })?;
    store.restore_table(&table, missing)?;
    assert!(
        store
            .read(|r| r.find(&table, UniqueDefaultV1::VALUE, &query))?
            .is_none()
    );
    store.restore_table(&table, present)?;
    assert_eq!(
        store.read(|r| Ok(r
            .find(&table, UniqueDefaultV1::VALUE, &query)?
            .unwrap()
            .id()))?,
        1
    );
    drop(store);
    let mut store = Store::open_on(bytes, engine::Limits::default())?;
    let table = store.table::<UniqueDefaultV1>("unique-defaults")?;
    assert_eq!(
        store.read(|r| Ok(r
            .find(&table, UniqueDefaultV1::VALUE, &query)?
            .unwrap()
            .id()))?,
        1
    );
    assert!(
        store
            .read_retained(missing, |r| r.find(&table, UniqueDefaultV1::VALUE, &query))?
            .is_none()
    );
    let compact = MemoryStore::new()?;
    store.compact_to(compact.clone())?;
    let mut copy = Store::open_on(compact, engine::Limits::default())?;
    let copied = copy.table::<UniqueDefaultV1>("unique-defaults")?;
    assert_eq!(
        copy.read(|r| Ok(r
            .find(&copied, UniqueDefaultV1::VALUE, &query)?
            .unwrap()
            .id()))?,
        1
    );
    assert!(
        copy.read_retained(missing, |r| r.find(&copied, UniqueDefaultV1::VALUE, &query))?
            .is_none()
    );
    Ok(())
}

#[test]
fn migrating_missing_unique_fields_does_not_index_either_schema_default() -> Result<()> {
    let mut store = Store::memory()?;
    let old = store.table::<UniqueDefaultV1>("defaults")?;
    store.write(|w| {
        w.insert(
            &old,
            1,
            &UniqueDefaultV1 {
                value: Presence::Missing,
            },
        )?;
        w.insert(
            &old,
            2,
            &UniqueDefaultV1 {
                value: Presence::Missing,
            },
        )?;
        w.insert(
            &old,
            3,
            &UniqueDefaultV1 {
                value: Presence::Value("old-default".into()),
            },
        )
    })?;
    let history = store.checkpoint()?;
    let new = store.table::<UniqueDefaultV2>("defaults")?;
    assert_eq!(store.write(|w| w.migrate(&new, |_, _| Ok(())))?, 3);
    store.read(|r| {
        for id in [1, 2] {
            let doc = r.get(&new, id)?.unwrap();
            assert_eq!(doc.version(), 2);
            assert_eq!(doc.value, Presence::Missing);
            assert!(doc.record().field(1)?.is_none());
            assert_eq!(
                doc.schema()
                    .read_field(doc.record(), 1)?
                    .unwrap()
                    .as_bytes()?,
                b"new-default"
            );
        }
        assert!(
            r.find(
                &new,
                UniqueDefaultV2::VALUE,
                &Presence::Value("new-default".into())
            )?
            .is_none()
        );
        assert_eq!(
            r.find(
                &new,
                UniqueDefaultV2::VALUE,
                &Presence::Value("old-default".into())
            )?
            .unwrap()
            .id(),
            3
        );
        Ok(())
    })?;
    store.restore_table(&new, history)?;
    assert_eq!(store.read(|r| Ok(r.get(&new, 1)?.unwrap().version()))?, 1);
    assert_eq!(store.write(|w| w.migrate(&new, |_, _| Ok(())))?, 3);
    Ok(())
}

#[test]
fn stale_default_index_cannot_resolve_a_physically_missing_field() -> Result<()> {
    let mut store = Store::memory()?;
    let table = store.table::<UniqueDefaultV1>("defaults")?;
    let query = Presence::Value("old-default".to_owned());
    store.write(|w| {
        w.insert(
            &table,
            1,
            &UniqueDefaultV1 {
                value: query.clone(),
            },
        )
    })?;
    let raw = store.read(|r| Ok(r.get(&table, 1)?.unwrap().record().remove(1)?))?;
    // Simulate a default-derived index left by the previous experimental policy
    // (or raw writes that bypass typed index maintenance).
    let mut tx = store.database().begin();
    tx.put(table.collection(), &1u64.to_le_bytes(), raw.as_value())?;
    tx.commit()?;
    drop(tx);
    assert!(matches!(
        store.read(|r| r.find(&table, UniqueDefaultV1::VALUE, &query)),
        Err(Error::IndexInconsistent)
    ));
    assert!(matches!(
        store.write(|w| w.find(&table, UniqueDefaultV1::VALUE, &query)),
        Err(Error::IndexInconsistent)
    ));
    Ok(())
}
