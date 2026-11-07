mod support;
use support::{Model, Rng};
use weekend_db::{
    Database, Error, Limits, TransactionState,
    storage::{APPEND_START, Crash, Fault, IoKind, MemoryStore, REGION, SimulatedDisk},
};

const ID: [u8; 16] = *b"weekend-test-001";
fn open(disk: &SimulatedDisk) -> Database {
    Database::open_on(disk.clone(), Limits::default()).unwrap()
}
fn base_image() -> Vec<u8> {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    let shared = db.bytes(b"shared\0bytes").unwrap();
    let nested = db
        .map()
        .unwrap()
        .map_set(b"list", &db.list().unwrap().list_push(&shared).unwrap())
        .unwrap();
    tx.put(c, b"a", &nested).unwrap();
    tx.put(c, b"b", &db.integer(0).unwrap()).unwrap();
    tx.put(c, b"shared", &shared).unwrap();
    tx.commit().unwrap();
    disk.durable_image().unwrap()
}
fn staged(db: &Database) -> weekend_db::Transaction<'_> {
    let c = db.collection_id(1);
    let mut tx = db.begin();
    let old = tx.get(c, b"a").unwrap().unwrap();
    let list = old
        .map_get(b"list")
        .unwrap()
        .unwrap()
        .list_push(&db.bytes(b"new").unwrap())
        .unwrap();
    tx.put(c, b"a", &old.map_set(b"list", &list).unwrap())
        .unwrap();
    tx.put(c, b"b", &db.integer(9).unwrap()).unwrap();
    tx
}
fn state(db: &Database) -> Vec<(Vec<u8>, Model)> {
    db.snapshot()
        .scan(db.collection_id(1))
        .unwrap()
        .into_iter()
        .map(|(k, v)| (k, Model::read(&v)))
        .collect()
}
fn commit_trace(image: &[u8]) -> Vec<weekend_db::storage::IoEvent> {
    let disk = SimulatedDisk::from_image(image).unwrap();
    let db = open(&disk);
    let mut tx = staged(&db);
    disk.arm(usize::MAX, Fault::Error);
    tx.commit().unwrap();
    disk.trace().unwrap()
}

#[test]
fn real_codec_roundtrip_scalars_binary_and_shared_nested_values() {
    let store = MemoryStore::new().unwrap();
    let db = Database::create_on(store.clone(), ID, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    let scalars = [
        db.none().unwrap(),
        db.boolean(true).unwrap(),
        db.integer(i64::MIN).unwrap(),
        db.unsigned(u64::MAX).unwrap(),
        db.float(f64::from_bits(0xfff8000000000001)).unwrap(),
        db.bytes(b"\0a\0").unwrap(),
    ];
    let mut list = db.list().unwrap();
    for v in &scalars {
        list = list.list_push(v).unwrap();
    }
    let map = db
        .map()
        .unwrap()
        .map_set(b"", &list)
        .unwrap()
        .map_set(b"\0", &list)
        .unwrap();
    tx.put(c, b"\0", &map).unwrap();
    tx.put(c, b"empty", &db.bytes(b"").unwrap()).unwrap();
    tx.commit().unwrap();
    let expected = Model::read(&map);
    let revision = db.snapshot().revision();
    assert!(matches!(
        Database::open_on(store.clone(), Limits::default()),
        Err(Error::Busy)
    ));
    drop(tx);
    drop(db);
    let db = Database::open_on(store, Limits::default()).unwrap();
    assert_eq!(db.snapshot().revision(), revision);
    assert_eq!(
        Model::read(
            &db.snapshot()
                .get(db.collection_id(c.as_u64()), b"\0")
                .unwrap()
                .unwrap()
        ),
        expected
    );
    assert_eq!(db.snapshot().get(c, b"\0"), Err(Error::ForeignStore));
}

#[test]
fn failures_at_every_io_preserve_an_allowed_whole_state() {
    let image = base_image();
    let trace = commit_trace(&image);
    let baseline = SimulatedDisk::from_image(&image).unwrap();
    let db = open(&baseline);
    let old = state(&db);
    let mut tx = staged(&db);
    tx.commit().unwrap();
    let new = state(&db);
    drop(tx);
    drop(db);
    let policies = [
        Crash::DropAll,
        Crash::PersistAll,
        Crash::Mask(0x5555),
        Crash::Mask(0xaaaa),
        Crash::Torn {
            write: 0,
            prefix: 63,
        },
    ];
    for (step, event) in trace.iter().enumerate() {
        for policy in policies {
            let disk = SimulatedDisk::from_image(&image).unwrap();
            let db = open(&disk);
            let rev = db.snapshot().revision();
            let historical = db.snapshot();
            let mut tx = staged(&db);
            disk.arm(step, Fault::Error);
            let error = tx.commit().unwrap_err();
            assert!(
                matches!(error, Error::Io | Error::OutcomeUnknown),
                "step={step}, event={:?}, error={error:?}",
                event
            );
            assert!(db.requires_recovery());
            assert_eq!(db.snapshot().revision(), rev);
            assert_eq!(state(&db), old);
            assert_eq!(historical.revision(), rev);
            assert_eq!(
                tx.state(),
                if error == Error::Io {
                    TransactionState::Aborted
                } else {
                    TransactionState::Failed
                }
            );
            assert_eq!(db.begin().commit(), Err(Error::RecoveryRequired));
            assert_eq!(db.create_collection(), Err(Error::RecoveryRequired));
            disk.clear_fault();
            disk.crash(policy).unwrap();
            drop(tx);
            drop(historical);
            drop(db);
            let reopened = open(&disk);
            let actual = state(&reopened);
            if error == Error::Io {
                assert_eq!(
                    actual, old,
                    "prepublication failure accidentally published trailing objects"
                );
            }
            assert!(
                actual == old || actual == new,
                "step={step}, policy={policy:?}, actual={actual:?}"
            );
            assert_eq!(
                reopened.snapshot().revision(),
                if actual == old { rev } else { rev + 1 }
            );
        }
    }
}

#[test]
fn torn_writes_short_writes_and_failed_syncs() {
    let image = base_image();
    let trace = commit_trace(&image);
    let old_disk = SimulatedDisk::from_image(&image).unwrap();
    let old_db = open(&old_disk);
    let old = state(&old_db);
    drop(old_db);
    let new_disk = SimulatedDisk::from_image(&image).unwrap();
    let new_db = open(&new_disk);
    staged(&new_db).commit().unwrap();
    let new = state(&new_db);
    drop(new_db);
    for (step, event) in trace.iter().enumerate() {
        let mut faults = Vec::new();
        match event.kind {
            IoKind::Write => {
                for prefix in [0, 1, 47, 63, 64, REGION - 1, REGION] {
                    for durable in [false, true] {
                        faults.push(Fault::Tear { prefix, durable });
                    }
                }
                for n in [0, 1, 17] {
                    faults.push(Fault::Short(n));
                }
            }
            IoKind::Sync => {
                for policy in [
                    Crash::DropAll,
                    Crash::PersistAll,
                    Crash::Mask(0x5555),
                    Crash::Torn {
                        write: 0,
                        prefix: 71,
                    },
                ] {
                    faults.push(Fault::SyncError(policy));
                }
            }
            _ => continue,
        }
        for fault in faults {
            for policy in [Crash::DropAll, Crash::PersistAll] {
                let disk = SimulatedDisk::from_image(&image).unwrap();
                let db = open(&disk);
                let mut tx = staged(&db);
                disk.arm(step, fault);
                let result = tx.commit();
                let acknowledged = result.is_ok();
                let definitely_unpublished = result == Err(Error::Io);
                if let Err(e) = result {
                    assert!(matches!(e, Error::Io | Error::OutcomeUnknown));
                    assert!(db.requires_recovery());
                } else {
                    assert!(!db.requires_recovery());
                }
                for event in disk
                    .trace()
                    .unwrap()
                    .iter()
                    .filter(|e| e.kind == IoKind::Write)
                {
                    assert!(
                        (event.offset % REGION as u64) as usize + event.len <= REGION,
                        "write crosses a protected region"
                    );
                }
                disk.clear_fault();
                disk.crash(policy).unwrap();
                drop(tx);
                drop(db);
                let reopened = open(&disk);
                let actual = state(&reopened);
                if definitely_unpublished {
                    assert_eq!(
                        actual, old,
                        "unpublished append was selected during recovery"
                    );
                }
                assert!(
                    actual == old || actual == new,
                    "step={step}, fault={fault:?}, policy={policy:?}"
                );
                if acknowledged {
                    assert_eq!(
                        actual, new,
                        "lost acknowledged commit: step={step}, fault={fault:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn recovery_errors_and_short_io_do_not_damage_a_clean_image() {
    let image = base_image();
    let disk = SimulatedDisk::from_image(&image).unwrap();
    disk.arm(usize::MAX, Fault::Error);
    let db = open(&disk);
    let expected = state(&db);
    drop(db);
    let trace = disk.trace().unwrap();
    // trace includes the subsequent snapshot read only in memory, no more byte I/O.
    for (step, event) in trace.iter().enumerate() {
        for fault in [Fault::Error, Fault::Short(0), Fault::Short(17)] {
            let disk = SimulatedDisk::from_image(&image).unwrap();
            disk.arm(step, fault);
            let result = Database::open_on(disk.clone(), Limits::default());
            if matches!(fault, Fault::Short(17))
                && matches!(event.kind, IoKind::Read | IoKind::Write)
            {
                assert_eq!(state(&result.unwrap()), expected);
            } else {
                assert!(matches!(result, Err(Error::Io)));
            }
            assert_eq!(disk.durable_image().unwrap(), image);
            disk.clear_fault();
            let reopened = open(&disk);
            assert_eq!(state(&reopened), expected);
        }
    }
}

#[test]
fn acknowledged_metadata_or_payload_corruption_is_not_a_silent_rollback() {
    let image = base_image();
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let db = open(&disk);
    staged(&db).commit().unwrap();
    let rev = db.snapshot().revision();
    drop(db);
    let image = disk.durable_image().unwrap();
    // At least one valid witness still proves the newest acknowledged revision.
    let newest_slot = if rev.is_multiple_of(2) { 1 } else { 2 };
    let shared = image
        .windows(b"shared\0bytes".len())
        .position(|w| w == b"shared\0bytes")
        .unwrap();
    for offset in [newest_slot * REGION, image.len() - REGION + 64, shared] {
        let mut damaged = image.clone();
        damaged[offset] ^= 1;
        let disk = SimulatedDisk::from_image(&damaged).unwrap();
        assert!(
            matches!(
                Database::open_on(disk, Limits::default()),
                Err(Error::Corrupt)
            ),
            "offset={offset}"
        );
    }
    // Damage to one witness alone leaves the complete latest root recoverable.
    let mut damaged = image.clone();
    damaged[(newest_slot + 2) * REGION] ^= 1;
    let disk = SimulatedDisk::from_image(&damaged).unwrap();
    assert_eq!(open(&disk).snapshot().revision(), rev);
    // Unreachable old descriptors are intentionally not part of the accepted graph.
    let mut garbage = image.clone();
    garbage[APPEND_START + 64] ^= 1;
    let disk = SimulatedDisk::from_image(&garbage).unwrap();
    assert_eq!(open(&disk).snapshot().revision(), rev);
}

#[test]
fn operation_fuzz_with_reopens_and_acknowledged_crashes() {
    for seed in 1..=12 {
        let mut rng = Rng(seed);
        let disk = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
        let c = db.create_collection().unwrap();
        let mut model = std::collections::BTreeMap::new();
        for step in 0..50 {
            let key = vec![rng.n(10) as u8, 0];
            let mut tx = db.begin();
            let c = db.collection_id(c.as_u64());
            if rng.n(4) == 0 {
                tx.delete(c, &key).unwrap();
                model.remove(&key);
            } else {
                let value = rng.next() as i64;
                tx.put(c, &key, &db.integer(value).unwrap()).unwrap();
                model.insert(key, value);
            }
            tx.commit().unwrap();
            drop(tx);
            if step % 7 == 0 {
                let rev = db.snapshot().revision();
                disk.crash(Crash::Mask(rng.next())).unwrap();
                drop(db);
                db = open(&disk);
                assert_eq!(db.snapshot().revision(), rev);
            }
            let actual: std::collections::BTreeMap<_, _> = db
                .snapshot()
                .scan(db.collection_id(1))
                .unwrap()
                .into_iter()
                .map(|(k, v)| (k, v.as_i64().unwrap()))
                .collect();
            assert_eq!(actual, model, "seed={seed}, step={step}");
        }
    }
}

#[test]
fn catalogue_identity_survives_reopen_and_old_leases_cannot_unlock_a_new_writer() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
    let a = db.create_collection().unwrap();
    db.drop_collection(a).unwrap();
    let b = db.create_collection().unwrap();
    assert!(b.as_u64() > a.as_u64());
    disk.crash(Crash::DropAll).unwrap();
    let new = open(&disk);
    assert_eq!(db.begin().commit(), Err(Error::Io));
    assert!(db.requires_recovery());
    drop(db);
    assert!(matches!(
        Database::open_on(disk.clone(), Limits::default()),
        Err(Error::Busy)
    ));
    assert_eq!(
        new.snapshot().count(new.collection_id(a.as_u64())),
        Err(Error::NotFound)
    );
    assert!(new.create_collection().unwrap().as_u64() > b.as_u64());
    drop(new);
    let image = disk.durable_image().unwrap();
    assert!(matches!(
        Database::create_on(disk.clone(), ID, Limits::default()),
        Err(Error::Invalid)
    ));
    assert_eq!(disk.durable_image().unwrap(), image);
    drop(open(&disk));
}

#[test]
fn interrupted_initial_creation_has_no_prior_acknowledged_state() {
    let reference = SimulatedDisk::new().unwrap();
    reference.arm(usize::MAX, Fault::Error);
    drop(Database::create_on(reference.clone(), ID, Limits::default()).unwrap());
    let trace = reference.trace().unwrap();
    for step in 0..trace.len() {
        for policy in [Crash::DropAll, Crash::PersistAll] {
            let disk = SimulatedDisk::new().unwrap();
            disk.arm(step, Fault::Error);
            assert!(Database::create_on(disk.clone(), ID, Limits::default()).is_err());
            disk.crash(policy).unwrap();
            match Database::open_on(disk, Limits::default()) {
                Ok(db) => assert_eq!(db.snapshot().revision(), 0),
                Err(e) => assert!(matches!(e, Error::Corrupt | Error::UnsupportedFormat)),
            }
        }
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn preparation_and_backend_allocation_failures_preserve_the_acknowledged_state() {
    let image = base_image();
    let old = state(&open(&SimulatedDisk::from_image(&image).unwrap()));
    for backend_failure in [false, true] {
        let mut succeeded = false;
        let mut failures = 0;
        for budget in 0..512 {
            let disk = SimulatedDisk::from_image(&image).unwrap();
            let db = open(&disk);
            let mut tx = staged(&db);
            if backend_failure {
                disk.fail_allocations_after(Some(budget));
            } else {
                db.fail_allocations_after(Some(budget));
            }
            let result = tx.commit();
            disk.fail_allocations_after(None);
            db.fail_allocations_after(None);
            match result {
                Ok(_) => {
                    succeeded = true;
                    break;
                }
                Err(e) => {
                    failures += 1;
                    assert_eq!(
                        state(&db),
                        old,
                        "backend={backend_failure}, budget={budget}"
                    );
                    if backend_failure {
                        assert!(matches!(e, Error::Io | Error::OutcomeUnknown));
                        assert!(db.requires_recovery());
                    } else {
                        assert_eq!(e, Error::OutOfMemory);
                        assert!(!db.requires_recovery());
                        assert_eq!(disk.durable_image().unwrap(), image);
                    }
                    disk.crash(Crash::DropAll).unwrap();
                    drop(tx);
                    drop(db);
                    let recovered = open(&disk);
                    if !backend_failure || e == Error::Io {
                        assert_eq!(state(&recovered), old);
                    }
                }
            }
        }
        assert!(succeeded && failures > 0, "backend={backend_failure}");
    }
}

#[test]
fn reopening_an_uncertain_volatile_root_stabilizes_it_before_exposure() {
    let image = base_image();
    let trace = commit_trace(&image);
    let root_sync = trace
        .iter()
        .enumerate()
        .filter(|(_, event)| event.kind == IoKind::Sync)
        .nth(1)
        .unwrap()
        .0;
    let make_uncertain = || {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let db = open(&disk);
        let mut tx = staged(&db);
        disk.arm(root_sync, Fault::SyncError(Crash::DropAll));
        assert_eq!(tx.commit(), Err(Error::OutcomeUnknown));
        drop(tx);
        drop(db);
        disk
    };
    let disk = make_uncertain();
    disk.arm(usize::MAX, Fault::Error);
    let recovered = open(&disk);
    let expected = state(&recovered);
    let revision = recovered.snapshot().revision();
    drop(recovered);
    let recovery_trace = disk.trace().unwrap();
    disk.crash(Crash::DropAll).unwrap();
    assert_eq!(state(&open(&disk)), expected);
    let original = state(&open(&SimulatedDisk::from_image(&image).unwrap()));
    for (step, event) in recovery_trace
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, IoKind::Sync | IoKind::Write))
    {
        let fault = if event.kind == IoKind::Sync {
            Fault::SyncError(Crash::Mask(0x5555))
        } else {
            Fault::Tear {
                prefix: 63,
                durable: true,
            }
        };
        for policy in [Crash::DropAll, Crash::PersistAll] {
            let disk = make_uncertain();
            disk.arm(step, fault);
            assert!(matches!(
                Database::open_on(disk.clone(), Limits::default()),
                Err(Error::Io)
            ));
            disk.clear_fault();
            disk.crash(policy).unwrap();
            let recovered = open(&disk);
            let actual = state(&recovered);
            assert!(
                actual == original || actual == expected,
                "recovery step={step}, fault={fault:?}, crash={policy:?}"
            );
            if actual == expected {
                assert_eq!(recovered.snapshot().revision(), revision);
            }
        }
    }
}

#[test]
fn abandoned_synced_or_torn_append_is_not_published_and_can_be_skipped() {
    let image = base_image();
    let trace = commit_trace(&image);
    let append_sync = trace.iter().position(|e| e.kind == IoKind::Sync).unwrap();
    for policy in [
        Crash::PersistAll,
        Crash::Torn {
            write: 0,
            prefix: 777,
        },
    ] {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let db = open(&disk);
        let original = state(&db);
        let mut tx = staged(&db);
        disk.arm(append_sync, Fault::Error);
        assert_eq!(tx.commit(), Err(Error::Io));
        disk.crash(policy).unwrap();
        drop(tx);
        drop(db);
        let db = open(&disk);
        assert_eq!(state(&db), original);
        let mut tx = staged(&db);
        tx.commit().unwrap();
        let expected = state(&db);
        drop(tx);
        drop(db);
        disk.crash(Crash::DropAll).unwrap();
        assert_eq!(state(&open(&disk)), expected);
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn backend_allocation_failure_during_recovery_returns_no_handle_or_partial_state() {
    let image = base_image();
    let mut succeeded = false;
    for budget in 0..256 {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        disk.fail_allocations_after(Some(budget));
        let result = Database::open_on(disk.clone(), Limits::default());
        disk.fail_allocations_after(None);
        match result {
            Ok(db) => {
                drop(db);
                succeeded = true;
                break;
            }
            Err(e) => assert!(
                matches!(e, Error::OutOfMemory | Error::Io),
                "budget={budget}, error={e:?}"
            ),
        }
        assert_eq!(disk.durable_image().unwrap(), image);
        disk.crash(Crash::DropAll).unwrap();
        drop(open(&disk));
    }
    assert!(succeeded);
}
