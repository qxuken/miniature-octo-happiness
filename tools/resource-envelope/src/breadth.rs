//! Default width, depth and write-count limits on one byte-backed store.
use super::{CORE, MIB, OFFSET, ROOTS, complete, hash, limits, rss_kib, verify_file_bytes};
use std::{fs, path::Path, time::Instant};
use weekend_db::{
    CollectionId, Database, Error, Snapshot, Transaction, TransactionState, Value,
    storage::{self, Crash, Fault, IoKind, MAX_IMAGE_BYTES, SimulatedDisk},
};

const WIDTH: u32 = 4096;
const WRITES: u32 = 1024;
const DEPTH: usize = 64;
const FULL_EPOCHS: u64 = 9;
const FINAL: u64 = 5 + 2 * ROOTS;
const LEAF: u64 = 0x5749_4445;
const PROBE_EPOCH: u64 = ROOTS + 1;

fn dag(db: &Database) -> Value {
    let leaf = db.unsigned(LEAF).unwrap();
    let mut map = db.map().unwrap();
    for index in 0..WIDTH {
        map = map.map_set(&index.to_be_bytes(), &leaf).unwrap();
    }
    assert!(matches!(
        map.map_set(&WIDTH.to_be_bytes(), &leaf),
        Err(Error::Limit)
    ));
    let mut value = db
        .list()
        .unwrap()
        .list_push(&map)
        .unwrap()
        .list_push(&map)
        .unwrap();
    for _ in 3..DEPTH {
        value = db.list().unwrap().list_push(&value).unwrap();
    }
    assert!(matches!(
        db.list().unwrap().list_push(&value),
        Err(Error::Limit)
    ));
    value
}

fn check_transaction_budget(db: &Database, collection: CollectionId, value: &Value) {
    let mut draft = db.begin();
    // Observations of absent keys share one collection dependency. Repeating
    // this reference counts its full logical weight for each staged write.
    for key in 0..203u32 {
        draft.put(collection, &key.to_be_bytes(), value).unwrap();
    }
    let rejected = 203u32.to_be_bytes();
    assert_eq!(draft.put(collection, &rejected, value), Err(Error::Limit));
    assert!(draft.get(collection, &rejected).unwrap().is_none());
    draft
        .put(collection, &rejected, &db.unsigned(0).unwrap())
        .unwrap();
    draft.abort().unwrap();
}

fn scalar(epoch: u64, index: u32) -> u64 {
    (epoch << 32) | u64::from(index)
}

fn stage(db: &Database, keys: std::ops::Range<u32>, epoch: u64) -> Transaction<'_> {
    let collection = db.collection_id(1);
    let mut draft = db.begin();
    for key in keys {
        draft
            .put(
                collection,
                &key.to_be_bytes(),
                &db.unsigned(scalar(epoch, key)).unwrap(),
            )
            .unwrap();
    }
    draft
}

fn seed(db: &Database) {
    let rules = db.limits();
    assert_eq!(
        (
            rules.max_entries,
            rules.max_writes,
            rules.max_depth,
            rules.max_transaction_bytes
        ),
        (WIDTH as usize, WRITES as usize, DEPTH, 32 * MIB)
    );
    let collection = db.create_collection().unwrap();
    assert_eq!(collection.as_u64(), 1);
    let value = dag(db);
    check_transaction_budget(db, collection, &value);
    for batch in 0..WIDTH / WRITES {
        let start = batch * WRITES;
        let end = start + WRITES;
        let mut draft = stage(db, start..end.min(WIDTH - 1), 0);
        if end == WIDTH {
            draft
                .put(collection, &(WIDTH - 1).to_be_bytes(), &value)
                .unwrap();
        }
        if batch == 0 {
            let extra = WRITES.to_be_bytes();
            assert_eq!(
                draft.put(collection, &extra, &db.unsigned(0).unwrap()),
                Err(Error::Limit)
            );
            assert!(draft.get(collection, &extra).unwrap().is_none());
        }
        assert_eq!(draft.commit(), Ok(2 + u64::from(batch)));
    }
    for epoch in 1..=ROOTS {
        let keys = if epoch <= FULL_EPOCHS {
            0..WRITES
        } else {
            let key = WRITES + epoch as u32;
            key..key + 1
        };
        assert_eq!(stage(db, keys, epoch).commit(), Ok(4 + 2 * epoch));
        assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(epoch));
    }
    assert_eq!(db.retain_snapshot(&db.snapshot()), Err(Error::Limit));
}

fn verify_view(view: &Snapshot<'_>, collection: CollectionId, epoch: u64, digest: &mut u64) {
    let rows = view.scan(collection).unwrap();
    assert_eq!(rows.len(), WIDTH as usize);
    hash(digest, &view.revision().to_le_bytes());
    for (index, (key, value)) in (0..WIDTH).zip(rows) {
        assert_eq!(key, index.to_be_bytes());
        if index != WIDTH - 1 {
            let update = u64::from(index.saturating_sub(WRITES));
            let version = if index < WRITES {
                if epoch == PROBE_EPOCH {
                    epoch
                } else {
                    epoch.min(FULL_EPOCHS)
                }
            } else if (FULL_EPOCHS + 1..=epoch.min(ROOTS)).contains(&update) {
                update
            } else {
                0
            };
            let bits = value.as_u64().unwrap();
            assert_eq!(bits, scalar(version, index));
            hash(digest, &bits.to_le_bytes());
            continue;
        }
        let mut branch = value;
        for _ in 3..DEPTH {
            assert_eq!(branch.len().unwrap(), 1);
            branch = branch.list_get(0).unwrap();
        }
        assert_eq!(branch.len().unwrap(), 2);
        for side in 0..2 {
            let entries = branch.list_get(side).unwrap().map_entries().unwrap();
            assert_eq!(entries.len(), WIDTH as usize);
            for (inner, (key, leaf)) in (0..WIDTH).zip(entries) {
                assert_eq!(key, inner.to_be_bytes());
                let bits = leaf.as_u64().unwrap();
                assert_eq!(bits, LEAF);
                hash(digest, &key);
                hash(digest, &bits.to_le_bytes());
            }
        }
    }
}

fn state(
    phase: &str,
    db: &Database,
    epoch: u64,
    elapsed: std::time::Duration,
    inspected_witness: Option<bool>,
) {
    let collection = db.collection_id(1);
    let mut digest = OFFSET;
    let ids = db.retained_ids().unwrap();
    assert!(ids.iter().copied().eq(1..=ROOTS));
    let current = db.snapshot();
    let revision = FINAL + u64::from(epoch == PROBE_EPOCH);
    assert_eq!(current.revision(), revision);
    let data_revision = if epoch == PROBE_EPOCH {
        revision
    } else {
        FINAL - 1
    };
    assert_eq!(current.data_revision(), data_revision);
    verify_view(&current, collection, epoch, &mut digest);
    for id in ids {
        let view = db.retained_snapshot(id).unwrap();
        assert_eq!(view.revision(), 4 + 2 * id);
        assert_eq!(view.data_revision(), view.revision());
        verify_view(&view, collection, id, &mut digest);
    }
    let space = db.storage_space().unwrap();
    println!(
        "{{\"phase\":\"{phase}\",\"revision\":{revision},\"roots\":{ROOTS},\"width\":{WIDTH},\"depth\":{DEPTH},\"digest\":{digest},\"file_bytes\":{},\"elapsed_ns\":{},\"core_peak_bytes\":{},\"rss_kib\":{},\"inspected_witness\":{}}}",
        space.physical_bytes,
        elapsed.as_nanos(),
        db.memory_usage().peak_bytes,
        rss_kib(),
        inspected_witness.map_or("null", |w| if w { "true" } else { "false" })
    );
}

fn recover(phase: &str, disk: &SimulatedDisk, epoch: u64, witnessed: bool, start: Instant) {
    disk.crash(Crash::DropAll).unwrap();
    let inspected = storage::inspect_on(disk.clone(), limits(CORE)).unwrap();
    assert_eq!(inspected.witnessed, witnessed);
    let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
    assert_eq!(inspected.revision, db.snapshot().revision());
    state(phase, &db, epoch, start.elapsed(), Some(witnessed));
    drop(db);
    assert_eq!(
        storage::inspect_on(disk.clone(), limits(CORE)).unwrap(),
        storage::ImageInfo {
            witnessed: true,
            ..inspected
        },
    );
}

fn simulate(compacted: &Path) {
    let image = fs::read(compacted).unwrap();
    let start = Instant::now();
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
    disk.arm(usize::MAX, Fault::Error);
    assert_eq!(stage(&db, 0..WRITES, PROBE_EPOCH).commit(), Ok(FINAL + 1));
    let syncs: Vec<_> = disk
        .trace()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(index, event)| (event.kind == IoKind::Sync).then_some(index))
        .collect();
    let [_, slot_sync, witness_sync] = syncs.try_into().unwrap();
    drop(db);
    recover("acknowledged", &disk, PROBE_EPOCH, true, start);
    drop(disk);

    for (phase, sync, epoch, witnessed) in [
        ("slot_drop", slot_sync, ROOTS, true),
        ("witness_drop", witness_sync, PROBE_EPOCH, false),
    ] {
        let start = Instant::now();
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
        disk.arm(sync, Fault::Error);
        assert_eq!(
            stage(&db, 0..WRITES, PROBE_EPOCH).commit(),
            Err(Error::OutcomeUnknown)
        );
        assert_eq!(disk.trace().unwrap().len(), sync + 1);
        assert!(db.requires_recovery());
        drop(db);
        recover(phase, &disk, epoch, witnessed, start);
    }
}

pub(super) fn run(dir: &Path) {
    fs::create_dir(dir).unwrap();
    let source = dir.join("source.wdb");
    let before_reject = dir.join("before-reject.wdb");
    let now = Instant::now();
    let db = Database::create_file(&source, limits(CORE)).unwrap();
    seed(&db);
    let space = db.storage_space().unwrap();
    assert!(space.physical_bytes > (MAX_IMAGE_BYTES as u64 * 9 / 10));
    assert!(space.remaining_append_bytes < u64::from(WRITES) * storage::REGION as u64);
    state("near_cap", &db, ROOTS, now.elapsed(), None);
    fs::copy(&source, &before_reject).unwrap();
    let mut overflow = stage(&db, WIDTH..WIDTH + 1, PROBE_EPOCH);
    assert_eq!(overflow.commit(), Err(Error::Limit));
    assert_eq!(overflow.state(), TransactionState::Failed);
    drop(overflow);
    let now = Instant::now();
    let mut rejected = stage(&db, 0..WRITES, PROBE_EPOCH);
    assert_eq!(rejected.commit(), Err(Error::Limit));
    assert_eq!(rejected.state(), TransactionState::Failed);
    let elapsed = now.elapsed();
    drop(rejected);
    assert!(!db.requires_recovery());
    assert_eq!(db.storage_space().unwrap(), space);
    verify_file_bytes(&source, &before_reject);
    fs::remove_file(before_reject).unwrap();
    state("image_limit", &db, ROOTS, elapsed, None);
    drop(db);

    let now = Instant::now();
    let db = Database::open_file(&source, limits(CORE)).unwrap();
    state("reopen", &db, ROOTS, now.elapsed(), None);
    drop(db);
    let backup = dir.join("backup.wdb");
    let restored = dir.join("restored.wdb");
    let compacted = dir.join("compact.wdb");
    let identity = storage::inspect_file(&source, limits(CORE))
        .unwrap()
        .identity;
    for (phase, destination) in [
        ("backup", &backup),
        ("restore", &restored),
        ("compact", &compacted),
    ] {
        if phase == "compact" {
            fs::copy(&source, destination).unwrap();
        }
        let now = Instant::now();
        let stats = match phase {
            "backup" => storage::backup_file(&source, destination, limits(256 * MIB)),
            "restore" => storage::restore_file(&backup, destination, limits(256 * MIB)),
            _ => Database::compact_file(destination, limits(256 * MIB)),
        }
        .unwrap();
        let elapsed = now.elapsed();
        assert_eq!(stats.after_bytes, fs::metadata(destination).unwrap().len());
        let info = storage::inspect_file(destination, limits(CORE)).unwrap();
        assert_eq!(info.identity, identity);
        assert!(info.witnessed);
        let db = Database::open_file(destination, limits(CORE)).unwrap();
        state(phase, &db, ROOTS, elapsed, None);
    }
    verify_file_bytes(&backup, &restored);
    verify_file_bytes(&backup, &compacted);
    simulate(&compacted);
    complete("breadth");
}
