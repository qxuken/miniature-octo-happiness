#[path = "support/allocator.rs"]
pub mod allocator;
use weekend_db as allocator_engine;
mod support;
use allocator::TestAllocator;
use std::{collections::BTreeMap, sync::Arc};
use support::{Model, Rng};
use weekend_db::{
    Database, DatabaseOptions, Error, Limits, TransactionState,
    storage::{
        APPEND_START, ByteStore, Crash, Fault, IoKind, MemoryStore, REGION, SimulatedDisk,
        inspect_on,
    },
};

#[test]
fn pending_write_limit_rejects_only_writes_that_would_grow_the_queue() {
    let mut disk = SimulatedDisk::new().unwrap();
    disk.lock().unwrap();
    for _ in 0..32_768 {
        assert_eq!(disk.write(0, b"a"), Ok(1));
    }

    let mut byte = [0];
    assert_eq!(disk.read(0, &mut byte), Ok(1));
    assert_eq!(&byte, b"a");
    assert_eq!(disk.write(0, b""), Ok(0));
    disk.arm(0, Fault::Short(0));
    assert_eq!(disk.write(0, b"b"), Ok(0));

    disk.arm(
        0,
        Fault::Tear {
            prefix: 1,
            durable: true,
        },
    );
    assert_eq!(disk.write(0, b"b"), Err(Error::Limit));
    assert!(disk.durable_image().unwrap().is_empty());
    assert_eq!(disk.write(0, b"b"), Err(Error::Limit));
    assert_eq!(disk.read(0, &mut byte), Ok(1));
    assert_eq!(&byte, b"a");

    disk.sync().unwrap();
    assert_eq!(disk.durable_image().unwrap(), b"a");
    assert_eq!(disk.write(0, b"b"), Ok(1));
}

const ID: [u8; 16] = *b"weekend-test-001";
fn open(disk: &SimulatedDisk) -> Database {
    Database::open_on(disk.clone(), Limits::default()).unwrap()
}
fn base_store() -> (SimulatedDisk, Database) {
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
    drop(tx);
    (disk, db)
}
fn base_image() -> Vec<u8> {
    let (disk, _db) = base_store();
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
type Rows = Vec<(Vec<u8>, Model)>;

fn state(db: &Database) -> Rows {
    db.snapshot()
        .scan(db.collection_id(1))
        .unwrap()
        .into_iter()
        .map(|(k, v)| (k, Model::read(&v)))
        .collect()
}
fn retained_rows(db: &Database) -> Vec<(u64, u64, Rows)> {
    db.retained_ids()
        .unwrap()
        .into_iter()
        .map(|id| {
            let view = db.retained_snapshot(id).unwrap();
            let rows = view
                .scan(db.collection_id(1))
                .unwrap()
                .into_iter()
                .map(|(key, value)| (key, Model::read(&value)))
                .collect();
            (id, view.revision(), rows)
        })
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

fn commit_barriers(trace: &[weekend_db::storage::IoEvent]) -> [usize; 3] {
    trace
        .iter()
        .enumerate()
        .filter_map(|(step, event)| (event.kind == IoKind::Sync).then_some(step))
        .collect::<Vec<_>>()
        .try_into()
        .expect("a commit syncs its append, root and witness separately")
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
fn publication_failures_reopen_the_revision_that_survived() {
    let image = base_image();
    let trace = commit_trace(&image);
    let [append_sync, root_sync, _] = commit_barriers(&trace);
    let baseline = SimulatedDisk::from_image(&image).unwrap();
    let db = open(&baseline);
    let rev = db.snapshot().revision();
    let old = state(&db);
    let mut tx = staged(&db);
    assert_eq!(tx.commit(), Ok(rev + 1));
    assert_eq!(db.snapshot().revision(), rev + 1);
    let new = state(&db);
    drop(tx);
    drop(db);
    baseline.crash(Crash::DropAll).unwrap();
    let reopened = open(&baseline);
    assert_eq!(reopened.snapshot().revision(), rev + 1);
    assert_eq!(state(&reopened), new);

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
            let error = if step <= append_sync {
                Error::Io
            } else {
                Error::OutcomeUnknown
            };
            assert_eq!(tx.commit(), Err(error), "step={step}, event={event:?}");
            assert_eq!(disk.trace().unwrap(), trace[..=step], "step={step}");
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
            // At a failed root sync the only pending write is the new root.
            // In these policies a torn prefix of 63 bytes cannot validate it.
            let root_survives = matches!(policy, Crash::PersistAll)
                || matches!(policy, Crash::Mask(mask) if mask & 1 != 0);
            let next = step > root_sync || (step == root_sync && root_survives);
            assert_eq!(
                reopened.snapshot().revision(),
                rev + u64::from(next),
                "step={step}, event={event:?}, policy={policy:?}",
            );
            assert_eq!(
                &state(&reopened),
                if next { &new } else { &old },
                "step={step}, event={event:?}, policy={policy:?}",
            );
        }
    }
}

// Extending the prefix through equal bytes leaves the durable region identical.
fn distinct_prefixes<'a>(old: &'a [u8], new: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    assert_eq!(old.len(), new.len());
    std::iter::once(0).chain(
        old.iter()
            .zip(new)
            .enumerate()
            .filter_map(|(i, (a, b))| (a != b).then_some(i + 1)),
    )
}

#[test]
fn every_distinct_reused_metadata_prefix_preserves_the_acknowledged_floor() {
    for (extra_commit, revision) in [(false, 5), (true, 6)] {
        let (control, db) = base_store();
        db.retain_snapshot(&db.snapshot()).unwrap();
        staged(&db).commit().unwrap();
        db.retain_snapshot(&db.snapshot()).unwrap();
        if extra_commit {
            db.begin().commit().unwrap();
        }
        assert_eq!(db.snapshot().revision(), revision);
        let original = control.durable_image().unwrap();
        let old = state(&db);
        let retained = retained_rows(&db);
        assert_eq!(
            retained.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(),
            [(1, 2), (2, 4)]
        );
        drop(db);

        let db = open(&control);
        control.arm(usize::MAX, Fault::Error);
        assert_eq!(staged(&db).commit(), Ok(revision + 1));
        let new = state(&db);
        assert_ne!(old, new);
        let complete = control.durable_image().unwrap();
        let trace = control.trace().unwrap();
        let [_, root_sync, witness_sync] = commit_barriers(&trace);
        drop(db);

        let parity = (revision + 1) as usize % 2;
        let root = (1 + parity) * REGION..(2 + parity) * REGION;
        let witness = (3 + parity) * REGION..(4 + parity) * REGION;
        for (sync, region) in [(root_sync, root.clone()), (witness_sync, witness.clone())] {
            let write = sync - 1;
            let event = &trace[write];
            assert_eq!(
                (event.kind, event.offset, event.len),
                (IoKind::Write, region.start as u64, REGION)
            );
            let before = &original[region.clone()];
            let after = &complete[region.clone()];
            assert_ne!(before, after);
            assert!(
                before.iter().any(|&byte| byte != 0),
                "exercise a reused region"
            );
            let prefixes: Vec<_> = distinct_prefixes(before, after).collect();
            assert!(prefixes.len() > 2);
            eprintln!(
                "parity {parity}, metadata offset {}: {} distinct byte images, {} fault/crash cases",
                region.start,
                prefixes.len(),
                (prefixes.len() + 1) * 5
            );
            // Include the physical end even when an earlier prefix produced
            // the complete metadata bytes. The simulator also sees a full write.
            for prefix in prefixes.into_iter().chain(std::iter::once(REGION)) {
                let tear = |durable| Fault::Tear { prefix, durable };
                let torn_sync = Fault::SyncError(Crash::Torn { write: 0, prefix });
                for (step, fault, crash, persisted) in [
                    (write, tear(true), Crash::DropAll, prefix),
                    (write, tear(false), Crash::PersistAll, prefix),
                    (write, tear(false), Crash::DropAll, 0),
                    (sync, torn_sync, Crash::DropAll, prefix),
                    (sync, torn_sync, Crash::PersistAll, REGION),
                ] {
                    let case = (parity, region.start, prefix, fault, crash);
                    let disk = SimulatedDisk::from_image(&original).unwrap();
                    let db = open(&disk);
                    let mut tx = staged(&db);
                    disk.arm(step, fault);
                    assert_eq!(tx.commit(), Err(Error::OutcomeUnknown), "{case:?}");
                    assert_eq!(tx.state(), TransactionState::Failed, "{case:?}");
                    assert!(db.requires_recovery(), "{case:?}");
                    assert_eq!(db.snapshot().revision(), revision, "{case:?}");
                    assert_eq!(state(&db), old, "{case:?}");
                    assert_eq!(disk.trace().unwrap(), trace[..=step], "{case:?}");
                    drop(tx);
                    drop(db);
                    disk.clear_fault();
                    disk.crash(crash).unwrap();

                    let mut expected = complete.clone();
                    expected[region.start + persisted..region.end]
                        .copy_from_slice(&before[persisted..]);
                    if sync == root_sync {
                        expected[witness.clone()].copy_from_slice(&original[witness.clone()]);
                    }
                    assert!(
                        disk.durable_image().unwrap() == expected,
                        "durable bytes {case:?}"
                    );
                    let root_committed = expected[root.clone()] == complete[root.clone()];
                    let expected_revision = revision + u64::from(root_committed);
                    let rows = if root_committed { &new } else { &old };
                    let witnessed =
                        !root_committed || expected[witness.clone()] == complete[witness.clone()];
                    let info = inspect_on(disk.clone(), Limits::default()).unwrap();
                    assert_eq!(
                        (info.revision, info.witnessed),
                        (expected_revision, witnessed),
                        "{case:?}"
                    );
                    assert!(
                        disk.durable_image().unwrap() == expected,
                        "inspection changed bytes {case:?}"
                    );
                    let reopened = open(&disk);
                    assert_eq!(
                        reopened.snapshot().revision(),
                        expected_revision,
                        "{case:?}"
                    );
                    assert_eq!(&state(&reopened), rows, "{case:?}");
                    assert_eq!(retained_rows(&reopened), retained, "{case:?}");
                    drop(reopened);
                    disk.crash(Crash::DropAll).unwrap();
                    let stable = inspect_on(disk, Limits::default()).unwrap();
                    assert_eq!(
                        (stable.revision, stable.witnessed),
                        (expected_revision, true),
                        "{case:?}"
                    );
                }
            }
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

const OBJECT_HEADER_BYTES: usize = 64;
const OBJECT_CHECKSUM: std::ops::Range<usize> = 48..56;

fn image_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

fn image_crc64(bytes: &[u8]) -> u64 {
    let mut crc = 0_u64;
    for &byte in bytes {
        crc ^= u64::from(byte) << 56;
        for _ in 0..8 {
            crc = if crc & (1 << 63) == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x42f0e1eba9ea3693
            };
        }
    }
    crc
}

fn resign_object(bytes: &mut [u8]) {
    bytes[OBJECT_CHECKSUM].fill(0);
    let checksum = image_crc64(bytes);
    bytes[OBJECT_CHECKSUM].copy_from_slice(&checksum.to_le_bytes());
}

#[test]
fn object_extents_and_directory_references_reject_before_outside_reads() {
    assert_eq!(image_crc64(b"123456789"), 0x6c40df5f0b497347);
    let (disk, db) = base_store();
    let old = db.snapshot();
    let old_rows = state(&db);
    assert_eq!(old.revision(), 2);
    assert_eq!(staged(&db).commit(), Ok(3));
    assert_eq!(db.retain_snapshot(&old), Ok(1));
    drop(old);
    let revision = db.snapshot().revision();
    assert_eq!(revision, 4);
    let expected = state(&db);
    assert_ne!(expected, old_rows);
    let retained = vec![(1, 2, old_rows)];
    let original = disk.durable_image().unwrap();
    drop(db);
    let index = (revision & 1) as usize;
    let root = image_u64(&original, (1 + index) * REGION + 40) as usize;
    assert_eq!(root + REGION, original.len());
    let mut checked = original[root..root + REGION].to_vec();
    let stored_crc = image_u64(&checked, 48);
    checked[OBJECT_CHECKSUM].fill(0);
    assert_eq!(image_crc64(&checked), stored_crc);
    let healthy_disk = SimulatedDisk::from_image(&original).unwrap();
    let healthy = open(&healthy_disk);
    assert_eq!(healthy.snapshot().revision(), revision);
    assert_eq!(state(&healthy), expected);
    assert_eq!(retained_rows(&healthy), retained);
    drop(healthy);
    let clean_trace = healthy_disk.trace().unwrap();
    let header_at = clean_trace
        .iter()
        .position(|e| (e.kind, e.offset, e.len) == (IoKind::Read, root as u64, OBJECT_HEADER_BYTES))
        .unwrap();
    let full_at = header_at + 1;
    let full_read = clean_trace[full_at];
    assert_eq!(
        (full_read.kind, full_read.offset, full_read.len),
        (IoKind::Read, root as u64, REGION)
    );
    let reject = |case: &str, mut image: Vec<u8>, through: usize| {
        // Isolate graph admission from an independently surviving newest witness.
        image[(3 + index) * REGION..(4 + index) * REGION].fill(0);
        let candidate = SimulatedDisk::from_image(&image).unwrap();
        assert!(
            matches!(
                Database::open_on(candidate.clone(), Limits::default()),
                Err(Error::Corrupt)
            ),
            "{case}"
        );
        // Only the clean prefix may be read. This also excludes writes and syncs.
        assert_eq!(
            candidate.trace().unwrap(),
            clean_trace[..=through],
            "{case}"
        );
        assert_eq!(candidate.durable_image().unwrap(), image, "{case}");
        image[(1 + index) * REGION..(2 + index) * REGION].fill(0);
        let older = open(&SimulatedDisk::from_image(&image).unwrap());
        assert_eq!(older.snapshot().revision(), 3, "{case}");
        assert_eq!(state(&older), expected, "{case}");
        assert!(retained_rows(&older).is_empty(), "{case}");
    };
    assert!(
        clean_trace[..=full_at]
            .iter()
            .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
    );
    for (case, reference) in [
        ("parent itself", root as u64),
        ("physical image end", original.len() as u64),
        (
            "largest aligned offset",
            u64::MAX - u64::MAX % REGION as u64,
        ),
    ] {
        let mut image = original.clone();
        let descriptor = &mut image[root..root + REGION];
        let data_reference = OBJECT_HEADER_BYTES + 8;
        descriptor[data_reference..data_reference + 8].copy_from_slice(&reference.to_le_bytes());
        resign_object(descriptor);
        reject(case, image, full_at);
    }
    // An unselected tail is legal. A descriptor may not extend into it even
    // when a full read, header stability, checksum and padding would all pass.
    let mut extended = original.clone();
    extended.resize(original.len() + REGION, 0);
    let with_tail = open(&SimulatedDisk::from_image(&extended).unwrap());
    assert_eq!(with_tail.snapshot().revision(), revision);
    assert_eq!(state(&with_tail), expected);
    assert_eq!(retained_rows(&with_tail), retained);
    drop(with_tail);
    let descriptor = &mut extended[root..root + 2 * REGION];
    descriptor[16..24].copy_from_slice(&(REGION as u64).to_le_bytes());
    resign_object(descriptor);
    reject(
        "descriptor extends outside selected end",
        extended,
        header_at,
    );
    let restored = open(&SimulatedDisk::from_image(&original).unwrap());
    assert_eq!(restored.snapshot().revision(), revision);
    assert_eq!(state(&restored), expected);
    assert_eq!(retained_rows(&restored), retained);
}

#[test]
fn empty_retained_catalog_still_requires_nonzero_next_id() {
    let mut image = base_image();
    let healthy = open(&SimulatedDisk::from_image(&image).unwrap());
    assert_eq!(healthy.snapshot().revision(), 2);
    assert!(retained_rows(&healthy).is_empty());
    drop(healthy);
    let root = image_u64(&image, REGION + 40) as usize;
    let descriptor = &mut image[root..root + REGION];
    let next_id = OBJECT_HEADER_BYTES + 24;
    assert_eq!(image_u64(descriptor, next_id), 1);
    assert_eq!(image_u64(descriptor, next_id + 8), 0);
    descriptor[next_id..next_id + 8].fill(0);
    resign_object(descriptor);
    image[3 * REGION..4 * REGION].fill(0);

    let disk = SimulatedDisk::from_image(&image).unwrap();
    assert!(matches!(
        Database::open_on(disk.clone(), Limits::default()),
        Err(Error::Corrupt)
    ));
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|event| !matches!(event.kind, IoKind::Write | IoKind::Sync))
    );
    assert_eq!(disk.durable_image().unwrap(), image);
    image[REGION..2 * REGION].fill(0);
    let older = open(&SimulatedDisk::from_image(&image).unwrap());
    assert_eq!(older.snapshot().revision(), 1);
    assert!(state(&older).is_empty());
    assert!(retained_rows(&older).is_empty());
}

#[test]
fn commit_descriptor_requires_every_catalog_field_and_exact_payload_end() {
    // Use consecutive real slots with different retained catalogs.
    let (disk, db) = base_store();
    let first = db.snapshot();
    let first_rows = state(&db);
    assert_eq!(first.revision(), 2);
    assert_eq!(db.retain_snapshot(&first), Ok(1));
    assert_eq!(staged(&db).commit(), Ok(4));
    let second = db.snapshot();
    let current_rows = state(&db);
    assert_ne!(first_rows, current_rows);
    assert_eq!(db.retain_snapshot(&second), Ok(2));
    assert_eq!(db.snapshot().revision(), 5);
    let original = disk.durable_image().unwrap();
    drop(first);
    drop(second);
    drop(db);

    let root = image_u64(&original, 2 * REGION + 40) as usize;
    assert_eq!(root + REGION, original.len());
    let payload_len = image_u64(&original, root + 16) as usize;
    // Five descriptor words and four words per retained entry.
    assert_eq!(payload_len, (5 + 2 * 4) * 8);
    let older_catalog = vec![(1, 2, first_rows)];
    let mut current_catalog = older_catalog.clone();
    current_catalog.push((2, 4, current_rows.clone()));
    let healthy = open(&SimulatedDisk::from_image(&original).unwrap());
    assert_eq!(healthy.snapshot().revision(), 5);
    assert_eq!(state(&healthy), current_rows);
    assert_eq!(retained_rows(&healthy), current_catalog);
    drop(healthy);

    for len in (0..payload_len).step_by(8).chain([payload_len + 1]) {
        let mut image = original.clone();
        image[4 * REGION..5 * REGION].fill(0);
        let descriptor = &mut image[root..root + REGION];
        descriptor[16..24].copy_from_slice(&(len as u64).to_le_bytes());
        descriptor[OBJECT_HEADER_BYTES + len..].fill(0);
        resign_object(descriptor);
        let candidate = SimulatedDisk::from_image(&image).unwrap();
        assert!(
            matches!(
                Database::open_on(candidate.clone(), Limits::default()),
                Err(Error::Corrupt)
            ),
            "descriptor payload length {len}"
        );
        assert!(
            candidate
                .trace()
                .unwrap()
                .iter()
                .all(|event| !matches!(event.kind, IoKind::Write | IoKind::Sync)),
            "descriptor payload length {len} wrote or synced"
        );
        assert_eq!(
            candidate.durable_image().unwrap(),
            image,
            "descriptor payload length {len}"
        );
        image[2 * REGION..3 * REGION].fill(0);
        let older = open(&SimulatedDisk::from_image(&image).unwrap());
        assert_eq!(older.snapshot().revision(), 4, "payload length {len}");
        assert_eq!(state(&older), current_rows, "payload length {len}");
        assert_eq!(retained_rows(&older), older_catalog, "payload length {len}");
    }
}

struct RetainedHistory {
    image: Vec<u8>,
    current: Rows,
    prior: Rows,
    retained: Vec<(u64, u64, Rows)>,
}

impl RetainedHistory {
    fn fixture() -> Self {
        let (disk, db) = base_store();
        let old = db.snapshot();
        let prior = state(&db);
        assert_eq!(old.revision(), 2);
        assert_eq!(db.retain_snapshot(&old), Ok(1));
        assert_eq!(staged(&db).commit(), Ok(4));
        let current = state(&db);
        assert_ne!(prior, current);
        let retained = vec![(1, 2, prior.clone())];
        let image = disk.durable_image().unwrap();
        Self {
            image,
            current,
            prior,
            retained,
        }
    }

    fn current_data_offset(&self) -> usize {
        let root = image_u64(&self.image, REGION + 40) as usize;
        image_u64(&self.image, root + OBJECT_HEADER_BYTES + 8) as usize
    }

    fn assert_current_reopens(&self) {
        let mut unwitnessed = self.image.clone();
        unwitnessed[3 * REGION..4 * REGION].fill(0);
        for (case, image) in [
            ("witness present", &self.image),
            ("witness cleared", &unwitnessed),
        ] {
            let healthy = open(&SimulatedDisk::from_image(image).unwrap());
            assert_eq!(healthy.snapshot().revision(), 4, "{case}");
            assert_eq!(state(&healthy), self.current, "{case}");
            assert_eq!(retained_rows(&healthy), self.retained, "{case}");
        }
    }

    fn assert_newest_rejects(&self, case: &str, mut image: Vec<u8>, error: Error) {
        // Revision 4 is in the even slot. Clear its witness to expose any
        // fallback to revision 3 while the damaged newest slot is present.
        image[3 * REGION..4 * REGION].fill(0);
        let candidate = SimulatedDisk::from_image(&image).unwrap();
        assert_eq!(
            Database::open_on(candidate.clone(), Limits::default()).err(),
            Some(error),
            "{case}"
        );
        assert!(
            candidate
                .trace()
                .unwrap()
                .iter()
                .all(|event| !matches!(event.kind, IoKind::Write | IoKind::Sync)),
            "{case} wrote or synced"
        );
        assert_eq!(candidate.durable_image().unwrap(), image, "{case}");
        image[REGION..2 * REGION].fill(0);
        let older = open(&SimulatedDisk::from_image(&image).unwrap());
        assert_eq!(older.snapshot().revision(), 3, "{case}");
        assert_eq!(state(&older), self.prior, "{case}");
        assert_eq!(retained_rows(&older), self.retained, "{case}");
    }
}

#[test]
fn data_directory_requires_complete_sorted_records() {
    let fixture = RetainedHistory::fixture();
    let original = &fixture.image;
    let data = fixture.current_data_offset();
    let payload_len = image_u64(original, data + 16) as usize;
    assert_eq!(payload_len, 128);
    fixture.assert_current_reopens();

    let reject = |case: &str, mut image: Vec<u8>, error: Error| {
        resign_object(&mut image[data..data + REGION]);
        fixture.assert_newest_rejects(case, image, error);
    };

    for len in (0..payload_len).chain([payload_len + 1]) {
        let mut image = original.clone();
        let object = &mut image[data..data + REGION];
        object[16..24].copy_from_slice(&(len as u64).to_le_bytes());
        object[OBJECT_HEADER_BYTES + len..].fill(0);
        reject(&format!("DATA payload length {len}"), image, Error::Corrupt);
    }

    // DATA and collection headers each have three words. A record has a
    // length word, key bytes, write revision and value offset.
    let payload = data + OBJECT_HEADER_BYTES;
    let first_key_len = payload + 6 * 8;
    let second_key = first_key_len + 4 * 8 + 1;
    assert_eq!(image_u64(original, first_key_len), 1);
    assert_eq!(original[first_key_len + 8], b'a');
    assert_eq!(original[second_key], b'b');
    for (case, key) in [("duplicate key", b'a'), ("descending key", b'A')] {
        let mut image = original.clone();
        image[second_key] = key;
        reject(case, image, Error::Corrupt);
    }
    let limits = Limits::default();
    for (case, pos, maximum) in [
        ("collection count", payload + 16, limits.max_entries),
        ("record count", payload + 40, limits.max_entries),
        ("key length", first_key_len, limits.max_bytes),
    ] {
        let mut image = original.clone();
        image[pos..pos + 8].copy_from_slice(&(maximum as u64 + 1).to_le_bytes());
        reject(case, image, Error::Limit);
    }
}

#[test]
fn value_objects_require_complete_payloads() {
    let fixture = RetainedHistory::fixture();
    let original = &fixture.image;
    let data = fixture.current_data_offset();
    let data_payload = data + OBJECT_HEADER_BYTES;
    // The first two directory records have single-byte keys a and b.
    assert_eq!(original[data_payload + 56], b'a');
    assert_eq!(original[data_payload + 81], b'b');
    let map = image_u64(original, data_payload + 65) as usize;
    let scalar = image_u64(original, data_payload + 90) as usize;
    let map_payload = map + OBJECT_HEADER_BYTES;
    assert_eq!(&original[map_payload + 24..map_payload + 28], b"list");
    let list = image_u64(original, map_payload + 28) as usize;
    let bytes = image_u64(original, list + OBJECT_HEADER_BYTES + 24) as usize;
    fixture.assert_current_reopens();

    let reject = |case: &str, len: u64, offset: usize, mut image: Vec<u8>| {
        let object = &mut image[offset..offset + REGION];
        object[16..24].copy_from_slice(&len.to_le_bytes());
        object[OBJECT_HEADER_BYTES + len as usize..].fill(0);
        resign_object(object);
        fixture.assert_newest_rejects(
            &format!("{case} payload length {len}"),
            image,
            Error::Corrupt,
        );
    };
    for (case, offset, tag, expected_len) in [
        ("integer", scalar, 2, 16),
        ("bytes", bytes, 5, 19),
        ("map", map, 6, 36),
        ("list", list, 7, 32),
    ] {
        assert_eq!(image_u64(original, offset + OBJECT_HEADER_BYTES), tag);
        assert_eq!(image_u64(original, offset + 16), expected_len);
        for len in (0..expected_len).chain([expected_len + 1]) {
            reject(case, len, offset, original.clone());
        }
    }
    let mut unknown = original.clone();
    let tag_at = scalar + OBJECT_HEADER_BYTES;
    unknown[tag_at..tag_at + 8].copy_from_slice(&8_u64.to_le_bytes());
    reject(
        "unknown value tag without trailing data",
        8,
        scalar,
        unknown,
    );
}

#[test]
fn recovery_selects_the_newest_available_root_only_if_the_slots_agree() {
    let (disk, db) = base_store();
    let original = disk.durable_image().unwrap();
    assert_eq!(db.snapshot().revision(), 2);
    let collection = db.collection_id(1);
    let mut odd_3 = [0; REGION];
    for revision in 3..=5 {
        let mut tx = db.begin();
        tx.put(collection, b"b", &db.integer(revision).unwrap())
            .unwrap();
        assert_eq!(tx.commit(), Ok(revision as u64));
        if revision == 3 {
            odd_3.copy_from_slice(&disk.durable_image().unwrap()[2 * REGION..3 * REGION]);
        }
    }
    drop(db);
    let latest = disk.durable_image().unwrap();
    let even_2 = &original[REGION..2 * REGION];
    let even_4 = &latest[REGION..2 * REGION];
    let odd_5 = &latest[2 * REGION..3 * REGION];
    let absent = [0; REGION];
    for (case, even, odd, expected) in [
        ("nonconsecutive", even_2, odd_5, Err(Error::Corrupt)),
        ("even absent", &absent, odd_5, Ok((5, 5))),
        ("odd absent", even_2, &absent, Ok((2, 0))),
        ("consecutive, odd newer", even_4, odd_5, Ok((5, 5))),
        ("consecutive, even newer", even_4, &odd_3, Ok((4, 4))),
        ("both absent", &absent, &absent, Err(Error::Corrupt)),
    ] {
        let mut image = latest.clone();
        image[REGION..2 * REGION].copy_from_slice(even);
        image[2 * REGION..3 * REGION].copy_from_slice(odd);
        // Hold the witness check out of these pairs. Every image keeps the
        // same object tail, including objects newer than some candidate roots.
        image[3 * REGION..5 * REGION].fill(0);
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let result = Database::open_on(disk.clone(), Limits::default()).map(|db| {
            let snapshot = db.snapshot();
            let value = snapshot
                .get(db.collection_id(1), b"b")
                .unwrap()
                .unwrap()
                .as_i64()
                .unwrap();
            (snapshot.revision(), value)
        });
        assert_eq!(result, expected, "{case}");
        if expected.is_err() {
            assert_eq!(disk.durable_image().unwrap(), image, "{case}");
            assert!(
                disk.trace()
                    .unwrap()
                    .iter()
                    .all(|event| !matches!(event.kind, IoKind::Write | IoKind::Sync)),
                "{case}"
            );
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

#[test]
fn preparation_allocation_failures_preserve_current_and_retained_roots() {
    fn prepare(db: &Database) -> weekend_db::Transaction<'_> {
        let mut tx = staged(db);
        tx.delete(db.collection_id(1), b"shared").unwrap();
        tx
    }
    let (disk, db) = base_store();
    assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(1));
    let previous_revision = db.snapshot().revision();
    let previous_rows = state(&db);
    let retained = retained_rows(&db);
    let previous_image = disk.durable_image().unwrap();
    drop(db);

    let db = open(&disk);
    let committed_revision = prepare(&db).commit().unwrap();
    assert_eq!(db.snapshot().get(db.collection_id(1), b"shared"), Ok(None));
    let committed_rows = state(&db);
    let committed_image = disk.durable_image().unwrap();
    assert_ne!(committed_rows, previous_rows);
    assert_eq!(retained_rows(&db), retained);
    drop(db);

    for allowance in 0..512 {
        let disk = SimulatedDisk::from_image(&previous_image).unwrap();
        let allocator = Arc::new(TestAllocator::default());
        let options = DatabaseOptions::default().with_allocator(allocator.handle());
        let db = Database::open_on_with_options(disk.clone(), options).unwrap();
        let live = db.memory_usage().live_bytes;
        let allocated = allocator.live_bytes();
        let mut tx = prepare(&db);
        disk.arm(usize::MAX, Fault::Error);
        let result = allocator.with_refusal(allowance, || tx.commit());
        assert_eq!(
            tx.state(),
            if result.is_ok() {
                TransactionState::Committed
            } else {
                TransactionState::Failed
            }
        );
        assert_eq!(tx.commit(), Err(Error::Closed));
        assert_eq!(tx.abort(), Err(Error::Closed));
        drop(tx);
        let (revision, rows, image) = match result {
            Ok(revision) => {
                assert!(allowance > 0);
                assert_eq!(revision, committed_revision);
                (committed_revision, &committed_rows, &committed_image)
            }
            Err(e) => {
                assert_eq!(e, Error::OutOfMemory, "allocation {allowance}");
                assert!(!db.requires_recovery());
                assert_eq!(db.memory_usage().live_bytes, live);
                assert_eq!(allocator.live_bytes(), allocated);
                assert!(
                    disk.trace()
                        .unwrap()
                        .iter()
                        .all(|event| !matches!(event.kind, IoKind::Write | IoKind::Sync))
                );
                (previous_revision, &previous_rows, &previous_image)
            }
        };
        assert_eq!(db.snapshot().revision(), revision);
        assert_eq!(&state(&db), rows);
        assert_eq!(retained_rows(&db), retained);
        assert_eq!(&disk.durable_image().unwrap(), image);
        drop(db);
        assert_eq!(allocator.live(), 0);
        disk.crash(Crash::DropAll).unwrap();
        let recovered = open(&disk);
        assert_eq!(recovered.snapshot().revision(), revision);
        assert_eq!(&state(&recovered), rows);
        assert_eq!(retained_rows(&recovered), retained);
        if result.is_ok() {
            return;
        }
    }
    panic!("commit did not finish within the allocation sweep");
}

#[test]
fn backend_allocation_failures_preserve_the_acknowledged_state() {
    let image = base_image();
    let old = state(&open(&SimulatedDisk::from_image(&image).unwrap()));
    for allowance in 0..512 {
        let allocator = Arc::new(TestAllocator::default());
        let disk = SimulatedDisk::from_image_with_allocator(&image, allocator.handle()).unwrap();
        let db = open(&disk);
        let mut tx = staged(&db);
        let (result, requests) = allocator.count_with_refusal(allowance, || tx.commit());
        assert_eq!(result.is_err(), requests > allowance);
        match result {
            Ok(_) => {
                assert!(allowance > 0);
                return;
            }
            Err(e) => {
                assert_eq!(state(&db), old, "allocation {allowance}");
                assert!(matches!(e, Error::Io | Error::OutcomeUnknown));
                assert!(db.requires_recovery());
                disk.crash(Crash::DropAll).unwrap();
                drop(tx);
                drop(db);
                let recovered = open(&disk);
                if e == Error::Io {
                    assert_eq!(state(&recovered), old);
                }
            }
        }
    }
    panic!("commit did not finish within the backend allocation sweep");
}

#[test]
fn reopening_an_uncertain_volatile_root_stabilizes_it_before_exposure() {
    let image = base_image();
    let trace = commit_trace(&image);
    let [_, root_sync, _] = commit_barriers(&trace);
    let original_db = open(&SimulatedDisk::from_image(&image).unwrap());
    let original_revision = original_db.snapshot().revision();
    let original = state(&original_db);
    drop(original_db);
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
    assert_eq!(revision, original_revision + 1);
    drop(recovered);
    let recovery_trace = disk.trace().unwrap();
    disk.crash(Crash::DropAll).unwrap();
    let reopened = open(&disk);
    assert_eq!(reopened.snapshot().revision(), revision);
    assert_eq!(state(&reopened), expected);
    drop(reopened);

    let barriers: Vec<_> = recovery_trace
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, IoKind::Sync | IoKind::Write))
        .collect();
    let [(stabilize, first), (_, witness), (_, last)] = barriers.as_slice() else {
        panic!("recovery must sync the root, write its witness and sync again");
    };
    assert_eq!(
        [first.kind, witness.kind, last.kind],
        [IoKind::Sync, IoKind::Write, IoKind::Sync]
    );
    for (step, event) in barriers.iter().copied() {
        let faults: &[(Fault, bool)] = if event.kind == IoKind::Sync {
            &[
                (Fault::SyncError(Crash::DropAll), false),
                (Fault::SyncError(Crash::Mask(0x5555)), true),
            ]
        } else {
            &[(
                Fault::Tear {
                    prefix: 63,
                    durable: true,
                },
                false,
            )]
        };
        for &(fault, persists_pending) in faults {
            for policy in [Crash::DropAll, Crash::PersistAll] {
                let disk = make_uncertain();
                disk.arm(step, fault);
                assert!(matches!(
                    Database::open_on(disk.clone(), Limits::default()),
                    Err(Error::Io)
                ));
                assert_eq!(disk.trace().unwrap(), recovery_trace[..=step]);
                disk.clear_fault();
                disk.crash(policy).unwrap();
                let next =
                    step != *stabilize || persists_pending || matches!(policy, Crash::PersistAll);
                let recovered = open(&disk);
                assert_eq!(
                    recovered.snapshot().revision(),
                    if next { revision } else { original_revision },
                    "recovery step={step}, fault={fault:?}, crash={policy:?}",
                );
                assert_eq!(
                    &state(&recovered),
                    if next { &expected } else { &original },
                    "recovery step={step}, fault={fault:?}, crash={policy:?}",
                );
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

#[test]
fn sparse_append_survivors_do_not_publish_and_retries_skip_them() {
    fn stage_append<'db>(db: &'db Database, payload: &[u8]) -> weekend_db::Transaction<'db> {
        let collection = db.collection_id(1);
        let mut tx = db.begin();
        tx.put(collection, b"b", &db.integer(19).unwrap()).unwrap();
        tx.put(collection, b"large", &db.bytes(payload).unwrap())
            .unwrap();
        tx.delete(collection, b"shared").unwrap();
        tx
    }

    let (seed, db) = base_store();
    db.retain_snapshot(&db.snapshot()).unwrap();
    staged(&db).commit().unwrap();
    db.retain_snapshot(&db.snapshot()).unwrap();
    let old_revision = db.snapshot().revision();
    let old_rows = state(&db);
    let old_retained = retained_rows(&db);
    drop(db);
    let original = seed.durable_image().unwrap();
    let original_info = inspect_on(seed, Limits::default()).unwrap();
    assert!(original_info.witnessed);

    let mut rng = Rng(0xb4e7_d9a8_6c31_5f02);
    let payload: Vec<_> = (0..4 * REGION + 137).map(|_| rng.next() as u8).collect();
    let mut expected_rows: BTreeMap<_, _> = old_rows.iter().cloned().collect();
    expected_rows.insert(b"b".to_vec(), Model::Int(19));
    expected_rows.insert(b"large".to_vec(), Model::Bytes(payload.clone()));
    expected_rows.remove(b"shared".as_slice());
    let expected_rows: Rows = expected_rows.into_iter().collect();

    let control = SimulatedDisk::from_image(&original).unwrap();
    let mut db = open(&control);
    control.arm(usize::MAX, Fault::Error);
    assert_eq!(stage_append(&db, &payload).commit(), Ok(old_revision + 1));
    assert_eq!(state(&db), expected_rows);
    assert_eq!(retained_rows(&db), old_retained);
    let control_trace = control.trace().unwrap();
    let control_bytes = control.durable_image().unwrap();
    let [append_sync, _, _] = commit_barriers(&control_trace);
    let append_writes = &control_trace[1..append_sync];
    assert_eq!(control_trace[0].kind, IoKind::Length);
    assert!((4..=8).contains(&append_writes.len()));
    let mut end = original.len().next_multiple_of(REGION);
    for event in append_writes {
        assert_eq!((event.kind, event.offset), (IoKind::Write, end as u64));
        end += event.len;
    }
    assert_eq!(end, control_bytes.len());
    let control_copy = SimulatedDisk::new().unwrap();
    db.compact_to(control_copy.clone()).unwrap();
    let compact_bytes = control_copy.durable_image().unwrap();

    let masks = (0..(1 << append_writes.len())).map(Crash::Mask);
    let tears = append_writes.iter().enumerate().flat_map(|(write, event)| {
        [0, 1, event.len / 2, event.len - 1, event.len]
            .into_iter()
            .map(move |prefix| Crash::Torn { write, prefix })
    });
    let mut cases = 0;
    for policy in masks.chain(tears) {
        cases += 1;
        let mut expected_bytes = original.clone();
        for (index, event) in append_writes.iter().enumerate() {
            let len = match policy {
                Crash::Mask(mask) if mask & (1 << index) != 0 => event.len,
                Crash::Torn { write, prefix } if write == index => prefix,
                _ => 0,
            };
            if len != 0 {
                let start = event.offset as usize;
                let end = start + len;
                expected_bytes.resize(end, 0);
                expected_bytes[start..end].copy_from_slice(&control_bytes[start..end]);
            }
        }

        let disk = SimulatedDisk::from_image(&original).unwrap();
        let db = open(&disk);
        disk.arm(append_sync, Fault::SyncError(policy));
        let mut tx = stage_append(&db, &payload);
        assert_eq!(tx.commit(), Err(Error::Io), "{policy:?}");
        assert_eq!(tx.state(), TransactionState::Aborted, "{policy:?}");
        assert_eq!(disk.trace().unwrap(), control_trace[..=append_sync]);
        assert_eq!(db.begin().commit(), Err(Error::RecoveryRequired));
        assert_eq!(db.snapshot().revision(), old_revision, "{policy:?}");
        assert_eq!(state(&db), old_rows, "{policy:?}");
        assert_eq!(retained_rows(&db), old_retained, "{policy:?}");
        drop(tx);
        drop(db);
        disk.crash(Crash::DropAll).unwrap();
        assert!(
            disk.durable_image().unwrap() == expected_bytes,
            "{policy:?}"
        );

        disk.arm(usize::MAX, Fault::Error);
        let mut expected_info = original_info.clone();
        expected_info.physical_bytes = expected_bytes.len() as u64;
        assert_eq!(
            inspect_on(disk.clone(), Limits::default()).unwrap(),
            expected_info,
            "{policy:?}"
        );
        assert!(
            disk.trace()
                .unwrap()
                .iter()
                .all(|event| matches!(event.kind, IoKind::Length | IoKind::Read))
        );
        assert!(
            disk.durable_image().unwrap() == expected_bytes,
            "{policy:?}"
        );
        let db = open(&disk);
        assert_eq!(db.snapshot().revision(), old_revision, "{policy:?}");
        assert_eq!(state(&db), old_rows, "{policy:?}");
        assert_eq!(retained_rows(&db), old_retained, "{policy:?}");

        disk.arm(usize::MAX, Fault::Error);
        assert_eq!(
            stage_append(&db, &payload).commit(),
            Ok(old_revision + 1),
            "{policy:?}"
        );
        let retry_offset = expected_bytes.len().next_multiple_of(REGION);
        let first_write = disk.trace().unwrap()[1];
        assert_eq!(
            (first_write.kind, first_write.offset),
            (IoKind::Write, retry_offset as u64),
            "{policy:?}"
        );
        let retried = disk.durable_image().unwrap();
        assert!(
            retried[APPEND_START..expected_bytes.len()] == expected_bytes[APPEND_START..],
            "{policy:?}"
        );
        assert!(
            retried[expected_bytes.len()..retry_offset]
                .iter()
                .all(|&byte| byte == 0),
            "{policy:?}"
        );
        drop(db);
        disk.crash(Crash::DropAll).unwrap();
        let mut db = open(&disk);
        assert_eq!(db.snapshot().revision(), old_revision + 1, "{policy:?}");
        assert_eq!(state(&db), expected_rows, "{policy:?}");
        assert_eq!(retained_rows(&db), old_retained, "{policy:?}");
        let copy = SimulatedDisk::new().unwrap();
        db.compact_to(copy.clone()).unwrap();
        assert!(copy.durable_image().unwrap() == compact_bytes, "{policy:?}");
    }
    eprintln!(
        "append writes: {}; sparse and torn crash cases: {cases}",
        append_writes.len()
    );
}

#[test]
fn backend_allocation_failure_during_recovery_returns_no_handle_or_partial_state() {
    let image = base_image();
    let mut succeeded = false;
    for budget in 0..256 {
        let allocator = Arc::new(TestAllocator::default());
        let disk = SimulatedDisk::from_image_with_allocator(&image, allocator.handle()).unwrap();
        let (result, requests) = allocator.count_with_refusal(budget, || {
            Database::open_on(disk.clone(), Limits::default())
        });
        assert_eq!(result.is_err(), requests > budget);
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

#[test]
fn repeated_recovery_failures_preserve_retained_roots_and_acknowledged_floor() {
    let (disk, db) = base_store();
    db.retain_snapshot(&db.snapshot()).unwrap();
    staged(&db).commit().unwrap();
    db.retain_snapshot(&db.snapshot()).unwrap();
    let image = disk.durable_image().unwrap();
    let old_revision = db.snapshot().revision();
    let old = state(&db);
    let retained = retained_rows(&db);
    drop(db);
    let trace = commit_trace(&image);
    let [_, root_sync, _] = commit_barriers(&trace);
    for initial in 0..3 {
        let make = || {
            let disk = SimulatedDisk::from_image(&image).unwrap();
            let db = open(&disk);
            let mut tx = staged(&db);
            if initial != 2 {
                disk.arm(
                    root_sync,
                    Fault::SyncError(if initial == 0 {
                        Crash::DropAll
                    } else {
                        Crash::PersistAll
                    }),
                );
            }
            assert_eq!(
                tx.commit(),
                if initial == 2 {
                    Ok(old_revision + 1)
                } else {
                    Err(Error::OutcomeUnknown)
                }
            );
            assert_eq!(
                db.snapshot().revision(),
                old_revision + u64::from(initial == 2)
            );
            drop(tx);
            drop(db);
            disk.clear_fault();
            disk
        };
        let control = make();
        let before_recovery = control.durable_image().unwrap();
        control.arm(usize::MAX, Fault::Error);
        let expected = state(&open(&control));
        let after_recovery = control.durable_image().unwrap();
        let recovery = control.trace().unwrap();
        let barriers: Vec<_> = recovery
            .iter()
            .enumerate()
            .filter_map(|(i, e)| matches!(e.kind, IoKind::Sync | IoKind::Write).then_some(i))
            .collect();
        assert_eq!(barriers.len(), 3);
        let first_read = recovery
            .iter()
            .position(|e| e.kind == IoKind::Read)
            .unwrap();
        let last_read = recovery
            .iter()
            .rposition(|e| e.kind == IoKind::Read)
            .unwrap();
        drop(control);
        let witness_write = &recovery[barriers[1]];
        assert_eq!(
            (witness_write.kind, witness_write.len),
            (IoKind::Write, REGION)
        );
        let offset = witness_write.offset as usize;
        let witness_region = offset..offset + REGION;
        let before_witness = &before_recovery[witness_region.clone()];
        let after_witness = &after_recovery[witness_region.clone()];
        let prefixes: Vec<_> = distinct_prefixes(before_witness, after_witness).collect();
        assert_eq!(prefixes.len() == 1, initial == 2);
        let other_faults = [
            (first_read, Fault::Error, false),
            (last_read, Fault::Error, false),
            (barriers[0], Fault::SyncError(Crash::DropAll), false),
            (barriers[0], Fault::SyncError(Crash::PersistAll), true),
            (barriers[2], Fault::SyncError(Crash::DropAll), true),
            (barriers[2], Fault::SyncError(Crash::PersistAll), true),
        ];
        let witness_tears = prefixes.iter().copied().chain([REGION]).flat_map(|prefix| {
            [false, true].map(|durable| (barriers[1], Fault::Tear { prefix, durable }, true))
        });
        eprintln!(
            "recovery initial state {initial}: {} witness byte images, {} compound fault cases",
            prefixes.len(),
            2 * (other_faults.len() + 2 * (prefixes.len() + 1))
        );
        for (step, fault, stabilized) in other_faults.into_iter().chain(witness_tears) {
            for persist in [false, true] {
                let disk = make();
                disk.arm(step, fault);
                assert!(matches!(
                    Database::open_on(disk.clone(), Limits::default()),
                    Err(Error::Io)
                ));
                assert_eq!(disk.trace().unwrap(), recovery[..=step]);
                disk.clear_fault();
                disk.crash(if persist {
                    Crash::PersistAll
                } else {
                    Crash::DropAll
                })
                .unwrap();
                let survives = initial != 0 || stabilized || persist;
                let revision = old_revision + u64::from(survives);
                if let Fault::Tear { prefix, durable } = fault {
                    let n = if durable || persist { prefix } else { 0 };
                    let bytes = disk.durable_image().unwrap();
                    let actual = &bytes[witness_region.clone()];
                    assert_eq!(&actual[..n], &after_witness[..n]);
                    assert_eq!(&actual[n..], &before_witness[n..]);
                    let info = inspect_on(disk.clone(), Limits::default()).unwrap();
                    assert_eq!(
                        (info.revision, info.witnessed),
                        (revision, actual == after_witness)
                    );
                    assert!(disk.durable_image().unwrap() == bytes);
                }
                // Discover the next witness barrier in a separate medium, so
                // the real medium sees two failed opens without a successful
                // recovery exposing a handle between them.
                let reference = SimulatedDisk::from_image(&disk.durable_image().unwrap()).unwrap();
                reference.arm(usize::MAX, Fault::Error);
                drop(open(&reference));
                let again = reference
                    .trace()
                    .unwrap()
                    .iter()
                    .rposition(|e| e.kind == IoKind::Sync)
                    .unwrap();
                disk.arm(again, Fault::SyncError(Crash::DropAll));
                assert!(matches!(
                    Database::open_on(disk.clone(), Limits::default()),
                    Err(Error::Io)
                ));
                disk.clear_fault();
                disk.crash(Crash::DropAll).unwrap();
                let reopened = open(&disk);
                assert_eq!(reopened.snapshot().revision(), revision);
                assert_eq!(&state(&reopened), if survives { &expected } else { &old });
                assert_eq!(retained_rows(&reopened), retained);
                reopened.begin().commit().unwrap();
                drop(reopened);
                disk.crash(Crash::DropAll).unwrap();
                let reopened = open(&disk);
                assert_eq!(reopened.snapshot().revision(), revision + 1);
                assert_eq!(&state(&reopened), if survives { &expected } else { &old });
                assert_eq!(retained_rows(&reopened), retained);
            }
        }
    }
}
