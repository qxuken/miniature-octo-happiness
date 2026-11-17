//! Default width, depth and write-count limits on one byte-backed store.
use super::{CORE, MIB, OFFSET, ROOTS, complete, hash, limits, rss_kib, verify_file_bytes};
use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use weekend_db::{
    CollectionId, Database, Error, Snapshot, Transaction, TransactionState, Value,
    storage::{
        self, ByteStore, Crash, Fault, FileStore, IoEvent, IoKind, MAX_IMAGE_BYTES, SimulatedDisk,
    },
};
#[path = "breadth_sustained.rs"]
pub(super) mod sustained;

const WIDTH: u32 = 4096;
const WRITES: u32 = 1024;
const DEPTH: usize = 64;
const FULL_EPOCHS: u64 = 9;
const FINAL: u64 = 5 + 2 * ROOTS;
const LEAF: u64 = 0x5749_4445;
const PROBE_EPOCH: u64 = ROOTS + 1;
const SHARED: u64 = 0x5348_4152_4544;
const DISJOINT: std::ops::Range<u32> = 2 * WRITES..3 * WRITES;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Updates {
    Seed,
    First,
    Both,
}
impl Updates {
    fn revision(self) -> u64 {
        FINAL
            + match self {
                Self::Seed => 0,
                Self::First => 1,
                Self::Both => 2,
            }
    }
}

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

fn verify_view(
    view: &Snapshot<'_>,
    collection: CollectionId,
    epoch: u64,
    updates: Updates,
    digest: &mut u64,
) {
    verify_values(view, collection, digest, |index| {
        if updates != Updates::Seed && index < WRITES {
            scalar(PROBE_EPOCH, index)
        } else if updates == Updates::Both && DISJOINT.contains(&index) {
            SHARED
        } else {
            seed_bits(epoch, index)
        }
    });
}

fn seed_bits(epoch: u64, index: u32) -> u64 {
    let update = u64::from(index.saturating_sub(WRITES));
    let version = if index < WRITES {
        epoch.min(FULL_EPOCHS)
    } else if (FULL_EPOCHS + 1..=epoch.min(ROOTS)).contains(&update) {
        update
    } else {
        0
    };
    scalar(version, index)
}

fn verify_values(
    view: &Snapshot<'_>,
    collection: CollectionId,
    digest: &mut u64,
    expected: impl Fn(u32) -> u64,
) {
    let rows = view.scan(collection).unwrap();
    assert_eq!(rows.len(), WIDTH as usize);
    hash(digest, &view.revision().to_le_bytes());
    for (index, (key, value)) in (0..WIDTH).zip(rows) {
        assert_eq!(key, index.to_be_bytes());
        if index != WIDTH - 1 {
            let bits = value.as_u64().unwrap();
            assert_eq!(bits, expected(index));
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

fn retained_views(db: &Database) -> Vec<Snapshot<'_>> {
    let ids = db.retained_ids().unwrap();
    assert!(ids.iter().copied().eq(1..=ROOTS));
    ids.into_iter()
        .map(|id| db.retained_snapshot(id).unwrap())
        .collect()
}

fn verify_views(
    current: &Snapshot<'_>,
    retained: &[Snapshot<'_>],
    collection: CollectionId,
    updates: Updates,
) -> u64 {
    let mut digest = OFFSET;
    let revision = updates.revision();
    assert_eq!(current.revision(), revision);
    let data_revision = if updates == Updates::Seed {
        FINAL - 1
    } else {
        revision
    };
    assert_eq!(current.data_revision(), data_revision);
    verify_view(current, collection, ROOTS, updates, &mut digest);
    verify_retained(retained, collection, &mut digest);
    digest
}

fn verify_retained(retained: &[Snapshot<'_>], collection: CollectionId, digest: &mut u64) {
    for (id, view) in (1..=ROOTS).zip(retained) {
        assert_eq!(view.revision(), 4 + 2 * id);
        assert_eq!(view.data_revision(), view.revision());
        verify_view(view, collection, id, Updates::Seed, digest);
    }
}

fn state(
    phase: &str,
    db: &Database,
    updates: Updates,
    elapsed: Duration,
    inspected_witness: Option<bool>,
) {
    let digest = verify_views(
        &db.snapshot(),
        &retained_views(db),
        db.collection_id(1),
        updates,
    );
    let revision = updates.revision();
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

fn recover(phase: &str, disk: &SimulatedDisk, updates: Updates, witnessed: bool, start: Instant) {
    disk.crash(Crash::DropAll).unwrap();
    let inspected = storage::inspect_on(disk.clone(), limits(CORE)).unwrap();
    assert_eq!(inspected.witnessed, witnessed);
    let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
    assert_eq!(inspected.revision, db.snapshot().revision());
    state(phase, &db, updates, start.elapsed(), Some(witnessed));
    drop(db);
    assert_eq!(
        storage::inspect_on(disk.clone(), limits(CORE)).unwrap(),
        storage::ImageInfo {
            witnessed: true,
            ..inspected
        },
    );
}

fn retry_torn_append(
    image: &[u8],
    append: &[u8],
    publication: &[IoEvent],
    append_sync: usize,
    witness_sync: usize,
) {
    let last_fit = MAX_IMAGE_BYTES - image.len() - append.len();
    assert!(last_fit > 0 && last_fit + 1 < append.len());
    assert_eq!(last_fit % storage::REGION, 0);
    for (prefix, old_phase, recovered_phase) in [
        (last_fit - 1, "tail_before_fit", "tail_before_recovered"),
        (last_fit, "tail_exact_fit", "tail_exact_recovered"),
        (last_fit + 1, "tail_limit_rejected", "tail_compact_retry"),
    ] {
        let start = Instant::now();
        let disk = SimulatedDisk::from_image(image).unwrap();
        let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
        let physical = image.len() + prefix;
        let (cut, write) = publication[..append_sync]
            .iter()
            .enumerate()
            .find(|(_, io)| {
                io.kind == IoKind::Write
                    && (io.offset as usize..=io.offset as usize + io.len).contains(&physical)
            })
            .unwrap();
        disk.arm(
            cut,
            Fault::Tear {
                prefix: physical - write.offset as usize,
                durable: false,
            },
        );
        let mut draft = stage(&db, 0..WRITES, PROBE_EPOCH);
        assert_eq!(draft.commit(), Err(Error::Io));
        assert_eq!(draft.state(), TransactionState::Aborted);
        assert!(db.requires_recovery());
        assert_eq!(disk.trace().unwrap(), publication[..=cut]);
        drop(draft);
        drop(db);
        // Keep every completed segment and the final partial segment.
        disk.crash(Crash::PersistAll).unwrap();

        let next = physical.next_multiple_of(storage::REGION);
        let bytes = disk.durable_image().unwrap();
        assert_eq!(bytes.len(), physical);
        assert!(bytes[..image.len()] == *image && bytes[image.len()..] == append[..prefix]);
        let old = storage::inspect_on(disk.clone(), limits(CORE)).unwrap();
        assert_eq!(old.revision, FINAL);
        assert!(old.witnessed);
        let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
        let space = db.storage_space().unwrap();
        assert_eq!(space.committed_bytes, image.len() as u64);
        assert_eq!(space.physical_bytes, physical as u64);
        assert_eq!(space.next_append_offset, next as u64);
        assert!(disk.durable_image().unwrap() == bytes);

        if prefix > last_fit {
            disk.arm(usize::MAX, Fault::Error);
            let mut retry = stage(&db, 0..WRITES, PROBE_EPOCH);
            assert_eq!(retry.commit(), Err(Error::Limit));
            assert_eq!(retry.state(), TransactionState::Failed);
            assert!(
                disk.trace()
                    .unwrap()
                    .iter()
                    .all(|io| io.kind == IoKind::Length)
            );
            assert!(!db.requires_recovery());
            assert!(disk.durable_image().unwrap() == bytes);
            state(old_phase, &db, Updates::Seed, start.elapsed(), Some(true));
            drop(retry);
            drop(db);

            let mut db = Database::open_on(disk.clone(), limits(256 * MIB)).unwrap();
            let compact = SimulatedDisk::new().unwrap();
            db.compact_to(compact.clone()).unwrap();
            drop(db);
            assert!(disk.durable_image().unwrap() == bytes);
            assert!(compact.durable_image().unwrap() == image);
            drop(bytes);
            drop(disk);
            let db = Database::open_on(compact.clone(), limits(CORE)).unwrap();
            assert_eq!(stage(&db, 0..WRITES, PROBE_EPOCH).commit(), Ok(FINAL + 1));
            drop(db);
            recover(recovered_phase, &compact, Updates::First, true, start);
            continue;
        }

        state(old_phase, &db, Updates::Seed, start.elapsed(), Some(true));
        disk.arm(
            witness_sync,
            Fault::SyncError(Crash::Torn {
                write: 0,
                prefix: 64,
            }),
        );
        let mut retry = stage(&db, 0..WRITES, PROBE_EPOCH);
        assert_eq!(retry.commit(), Err(Error::OutcomeUnknown));
        assert_eq!(retry.state(), TransactionState::Failed);
        assert_eq!(db.snapshot().revision(), FINAL);
        assert!(db.requires_recovery());
        let trace = disk.trace().unwrap();
        assert_eq!(trace.len(), witness_sync + 1);
        let first_write = trace.iter().find(|io| io.kind == IoKind::Write).unwrap();
        assert_eq!(first_write.offset, next as u64);
        let last_write = &trace[append_sync - 1];
        assert_eq!(
            (last_write.kind, last_write.offset + last_write.len as u64),
            (IoKind::Write, MAX_IMAGE_BYTES as u64)
        );
        drop(retry);
        drop(db);
        let durable = disk.durable_image().unwrap();
        assert_eq!(durable.len(), MAX_IMAGE_BYTES);
        assert!(durable[image.len()..physical] == bytes[image.len()..]);
        assert!(durable[physical..next].iter().all(|&byte| byte == 0));
        drop(durable);
        drop(bytes);
        recover(recovered_phase, &disk, Updates::First, false, start);
    }
}

fn simulate(compacted: &Path) {
    let image = fs::read(compacted).unwrap();
    let start = Instant::now();
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
    disk.arm(usize::MAX, Fault::Error);
    assert_eq!(stage(&db, 0..WRITES, PROBE_EPOCH).commit(), Ok(FINAL + 1));
    let trace = disk.trace().unwrap();
    let syncs: Vec<_> = trace
        .iter()
        .enumerate()
        .filter_map(|(index, event)| (event.kind == IoKind::Sync).then_some(index))
        .collect();
    let [append_sync, slot_sync, witness_sync] = syncs.try_into().unwrap();
    let first_write = trace.iter().find(|io| io.kind == IoKind::Write).unwrap();
    assert_eq!(first_write.offset, image.len() as u64);
    let append_bytes = disk.durable_image().unwrap()[image.len()..].to_vec();
    drop(db);
    recover("acknowledged", &disk, Updates::First, true, start);
    drop(disk);

    let torn = |prefix| Fault::SyncError(Crash::Torn { write: 0, prefix });
    for (phase, sync, fault, updates, witnessed) in [
        ("slot_drop", slot_sync, Fault::Error, Updates::Seed, true),
        (
            "witness_drop",
            witness_sync,
            Fault::Error,
            Updates::First,
            false,
        ),
        (
            "append_torn",
            append_sync,
            torn(storage::REGION),
            Updates::Seed,
            true,
        ),
        // Version 2's checksum occupies bytes 64 through 71. The rest of
        // either metadata region is zero in both the previous and new image.
        (
            "slot_before_checksum",
            slot_sync,
            torn(64),
            Updates::Seed,
            true,
        ),
        (
            "slot_through_checksum",
            slot_sync,
            torn(72),
            Updates::First,
            false,
        ),
        (
            "witness_before_checksum",
            witness_sync,
            torn(64),
            Updates::First,
            false,
        ),
        (
            "witness_through_checksum",
            witness_sync,
            torn(72),
            Updates::First,
            true,
        ),
    ] {
        let start = Instant::now();
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
        disk.arm(sync, fault);
        let expected = if sync == append_sync {
            Error::Io
        } else {
            Error::OutcomeUnknown
        };
        assert_eq!(stage(&db, 0..WRITES, PROBE_EPOCH).commit(), Err(expected));
        assert_eq!(disk.trace().unwrap().len(), sync + 1);
        assert!(db.requires_recovery());
        drop(db);
        recover(phase, &disk, updates, witnessed, start);
    }
    retry_torn_append(&image, &append_bytes, &trace, append_sync, witness_sync);
}

// Arm after open's stabilization syncs. The next writer pauses after its
// real append sync, with all native writes and leases still in effect.
struct PauseAfterSync {
    file: FileStore,
    armed: Arc<AtomicBool>,
    pause: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
}

impl ByteStore for PauseAfterSync {
    fn lock(&mut self) -> weekend_db::Result<()> {
        self.file.lock()
    }
    fn unlock(&mut self) {
        self.file.unlock();
    }
    fn length(&mut self) -> weekend_db::Result<u64> {
        self.file.length()
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> weekend_db::Result<usize> {
        self.file.read(offset, out)
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> weekend_db::Result<usize> {
        self.file.write(offset, bytes)
    }
    fn sync(&mut self) -> weekend_db::Result<()> {
        self.file.sync()?;
        if self.armed.swap(false, Ordering::Relaxed) {
            let (arrived, resume) = self.pause.take().unwrap();
            arrived.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(30)).unwrap();
        }
        Ok(())
    }
}

fn concurrent(dir: &Path, compacted: &Path, identity: [u8; 16]) {
    let concurrent = dir.join("concurrent.wdb");
    let before_limit = dir.join("before-limit.wdb");
    fs::copy(compacted, &concurrent).unwrap();
    let (arrived, at_sync) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let armed = Arc::new(AtomicBool::new(false));
    let db = Database::open_on(
        PauseAfterSync {
            file: FileStore::open(&concurrent).unwrap(),
            armed: armed.clone(),
            pause: Some((arrived, resumed)),
        },
        limits(CORE),
    )
    .unwrap();
    let initial = db.storage_space().unwrap();
    let collection = db.collection_id(1);
    let pinned = db.snapshot();
    let baseline_live = db.memory_usage().live_bytes;
    let mut first = stage(&db, 0..WRITES, PROBE_EPOCH);
    let mut conflicting = stage(&db, 0..WRITES, PROBE_EPOCH + 1);
    let mut disjoint = stage(&db, DISJOINT, PROBE_EPOCH + 2);
    let draft_live = db.memory_usage().live_bytes;
    assert!(draft_live > baseline_live);
    armed.store(true, Ordering::Relaxed);
    let (digest, acquire_ns, reader_ns, first_ns, conflict_ns) = thread::scope(|scope| {
        let first = scope.spawn(move || {
            let start = Instant::now();
            assert_eq!(first.commit(), Ok(Updates::First.revision()));
            start.elapsed().as_nanos()
        });
        at_sync.recv_timeout(Duration::from_secs(30)).unwrap();
        let (entered, contending) = mpsc::channel();
        let conflict = scope.spawn(move || {
            let start = Instant::now();
            entered.send(()).unwrap();
            assert_eq!(conflicting.commit(), Err(Error::Conflict));
            assert_eq!(conflicting.state(), TransactionState::Aborted);
            start.elapsed().as_nanos()
        });
        contending.recv_timeout(Duration::from_secs(30)).unwrap();
        let start = Instant::now();
        let fresh = db.snapshot();
        let retained = retained_views(&db);
        let acquire_ns = start.elapsed().as_nanos();
        let digest = verify_views(&fresh, &retained, collection, Updates::Seed);
        let reader_ns = start.elapsed().as_nanos();
        resume.send(()).unwrap();
        (
            digest,
            acquire_ns,
            reader_ns,
            first.join().unwrap(),
            conflict.join().unwrap(),
        )
    });
    println!(
        "{{\"phase\":\"overlapping_drafts\",\"digest\":{digest},\"drafts\":3,\"writes_per_draft\":{WRITES},\"baseline_live_bytes\":{baseline_live},\"draft_live_bytes\":{draft_live},\"acquire_ns\":{acquire_ns},\"reader_ns\":{reader_ns},\"first_ns\":{first_ns},\"conflict_ns\":{conflict_ns}}}"
    );

    let acknowledged = db.storage_space().unwrap();
    assert!(
        acknowledged.physical_bytes - initial.physical_bytes > acknowledged.remaining_append_bytes
    );
    fs::copy(&concurrent, &before_limit).unwrap();
    let start = Instant::now();
    assert_eq!(disjoint.commit(), Err(Error::Limit));
    assert_eq!(disjoint.state(), TransactionState::Failed);
    let elapsed = start.elapsed();
    drop(disjoint);
    assert!(!db.requires_recovery());
    assert_eq!(db.storage_space().unwrap(), acknowledged);
    verify_file_bytes(&concurrent, &before_limit);
    fs::remove_file(before_limit).unwrap();
    state("concurrent_limit", &db, Updates::First, elapsed, None);

    let start = Instant::now();
    let shared = db.unsigned(SHARED).unwrap();
    let mut retry = db.begin();
    for index in DISJOINT {
        retry
            .put(collection, &index.to_be_bytes(), &shared)
            .unwrap();
    }
    assert_eq!(retry.commit(), Ok(Updates::Both.revision()));
    let elapsed = start.elapsed();
    assert_eq!(pinned.revision(), FINAL);
    let mut pinned_digest = OFFSET;
    verify_view(
        &pinned,
        collection,
        ROOTS,
        Updates::Seed,
        &mut pinned_digest,
    );
    state("concurrent_retry", &db, Updates::Both, elapsed, None);
    drop(retry);
    drop(pinned);
    drop(db);

    let info = storage::inspect_file(&concurrent, limits(CORE)).unwrap();
    assert_eq!(info.identity, identity);
    assert!(info.witnessed);
    let start = Instant::now();
    let db = Database::open_file(&concurrent, limits(CORE)).unwrap();
    state(
        "concurrent_reopen",
        &db,
        Updates::Both,
        start.elapsed(),
        None,
    );
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
    state("near_cap", &db, Updates::Seed, now.elapsed(), None);
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
    state("image_limit", &db, Updates::Seed, elapsed, None);
    drop(db);

    let now = Instant::now();
    let db = Database::open_file(&source, limits(CORE)).unwrap();
    state("reopen", &db, Updates::Seed, now.elapsed(), None);
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
        state(phase, &db, Updates::Seed, elapsed, None);
    }
    verify_file_bytes(&backup, &restored);
    verify_file_bytes(&backup, &compacted);
    concurrent(dir, &compacted, identity);
    simulate(&compacted);
    complete("breadth");
}
