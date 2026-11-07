use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use weekend_db::{
    CollectionId, Database, Error, Limits, Snapshot, Transaction, Validation, Value,
    storage::{ByteStore, MemoryStore, SimulatedDisk},
};

fn number(s: &Snapshot<'_>, c: CollectionId, key: &[u8]) -> u64 {
    s.get(c, key).unwrap().unwrap().as_u64().unwrap()
}
fn seed(db: &Database, writers: usize) -> CollectionId {
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    for i in 0..writers {
        for side in 0..2 {
            tx.put(c, &[i as u8, side], &db.unsigned(0).unwrap())
                .unwrap();
        }
    }
    tx.commit().unwrap();
    c
}

// Each round prepares against unchanged record data before any writer publishes.
// Readers exercise both shared historical handles and newly acquired heads.
fn stress(db: &Database, rounds: usize) {
    const WRITERS: usize = 4;
    const READERS: usize = 4;
    let c = seed(db, WRITERS);
    let pinned = db.snapshot();
    let retained = db.retain_snapshot(&pinned).unwrap();
    let start = Barrier::new(WRITERS + READERS + 1);
    let prepared = Barrier::new(WRITERS);
    let committed = Barrier::new(WRITERS);
    let initial_revision = pinned.revision() + 1;
    thread::scope(|scope| {
        let mut writers = Vec::new();
        for i in 0..WRITERS {
            let (start, prepared, committed) = (&start, &prepared, &committed);
            writers.push(scope.spawn(move || {
                start.wait();
                let mut revisions = Vec::new();
                for n in 1..=rounds {
                    let mut tx = db.begin();
                    for side in 0..2 {
                        assert_eq!(
                            tx.get(c, &[i as u8, side]).unwrap().unwrap().as_u64(),
                            Ok((n - 1) as u64)
                        );
                        tx.put(c, &[i as u8, side], &db.unsigned(n as u64).unwrap())
                            .unwrap();
                    }
                    prepared.wait();
                    revisions.push(tx.commit().unwrap());
                    committed.wait();
                }
                revisions
            }));
        }
        for _ in 0..READERS {
            let (start, pinned) = (&start, &pinned);
            scope.spawn(move || {
                start.wait();
                let mut revision = 0;
                for _ in 0..1000 {
                    let s = db.snapshot();
                    assert!(s.revision() >= revision);
                    revision = s.revision();
                    for i in 0..WRITERS {
                        assert_eq!(number(&s, c, &[i as u8, 0]), number(&s, c, &[i as u8, 1]));
                        assert_eq!(number(&pinned.clone(), c, &[i as u8, 0]), 0);
                    }
                    assert_eq!(s.scan(c).unwrap().len(), WRITERS * 2);
                    thread::yield_now();
                }
            });
        }
        scope.spawn(|| {
            start.wait();
            for _ in 0..rounds {
                let s = db.snapshot();
                let id = db.retain_snapshot(&s).unwrap();
                let live = db.retained_snapshot(id).unwrap();
                db.release_retained(id).unwrap();
                assert_eq!(live.revision(), s.revision());
                assert_eq!(live.scan(c).unwrap(), s.scan(c).unwrap());
            }
        });
        let mut revisions: Vec<_> = writers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect();
        revisions.sort_unstable();
        revisions.dedup();
        assert_eq!(revisions.len(), WRITERS * rounds);
    });
    assert_eq!(
        db.snapshot().revision(),
        initial_revision + ((WRITERS + 2) * rounds) as u64
    );
    for i in 0..WRITERS {
        assert_eq!(number(&db.snapshot(), c, &[i as u8, 0]), rounds as u64);
        assert_eq!(
            number(&db.retained_snapshot(retained).unwrap(), c, &[i as u8, 1]),
            0
        );
    }
}

#[test]
fn native_traits_transfer_and_owned_values_outlive_database() {
    fn shared<T: Send + Sync>() {}
    fn movable<T: Send>() {}
    shared::<Database>();
    shared::<Snapshot<'_>>();
    shared::<Value>();
    shared::<weekend_db::Rows>();
    shared::<weekend_db::schema::Record>();
    shared::<weekend_db::schema::Schema>();
    shared::<MemoryStore>();
    shared::<SimulatedDisk>();
    movable::<Transaction<'_>>();
    let value = {
        let db = Database::new().unwrap();
        let c = seed(&db, 1);
        let mut tx = db.begin();
        tx.put(c, &[0, 0], &db.unsigned(17).unwrap()).unwrap();
        thread::scope(|s| s.spawn(move || tx.commit().unwrap()).join().unwrap());
        let value = db
            .map()
            .unwrap()
            .map_set(
                b"nested",
                &db.list()
                    .unwrap()
                    .list_push(&db.bytes(b"shared").unwrap())
                    .unwrap(),
            )
            .unwrap();
        thread::scope(|s| {
            for _ in 0..8 {
                let value = &value;
                s.spawn(move || {
                    for _ in 0..2000 {
                        let copy = value.clone();
                        let list = copy.map_get(b"nested").unwrap().unwrap();
                        assert_eq!(
                            list.list_get(0).unwrap().as_bytes(),
                            Ok(b"shared".as_slice())
                        );
                        let edited = list.list_push(&list.list_get(0).unwrap()).unwrap();
                        assert_eq!(edited.len(), Ok(2));
                    }
                });
            }
        });
        assert_eq!(number(&db.snapshot(), c, &[0, 0]), 17);
        value
    };
    thread::spawn(move || assert_eq!(value.map_entries().unwrap().len(), 1))
        .join()
        .unwrap();
}

#[test]
fn simultaneous_compatible_commits_readers_and_retention() {
    stress(&Database::new().unwrap(), 80);
}

#[test]
fn concurrent_durable_commits_reopen_with_retained_root() {
    let store = MemoryStore::new().unwrap();
    let db = Database::create_on(store.clone(), *b"concurrency-0001", Limits::default()).unwrap();
    stress(&db, 12);
    let revision = db.snapshot().revision();
    drop(db);
    let db = Database::open_on(store, Limits::default()).unwrap();
    assert_eq!(db.snapshot().revision(), revision);
    assert_eq!(number(&db.snapshot(), db.collection_id(1), &[0, 0]), 12);
    assert_eq!(
        number(
            &db.retained_snapshot(1).unwrap(),
            db.collection_id(1),
            &[0, 0]
        ),
        0
    );
}

#[cfg(any(
    target_os = "macos",
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
))]
#[test]
fn concurrent_file_commits_reopen_with_retained_root() {
    use std::{fs, path::PathBuf};
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let path = std::env::temp_dir().join(format!("weekend-db-threads-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let directory = Directory(path);
    let path = directory.0.join("threads.db");
    let db = Database::create_file(&path, Limits::default()).unwrap();
    stress(&db, 12);
    let revision = db.snapshot().revision();
    drop(db);
    let db = Database::open_file(&path, Limits::default()).unwrap();
    assert_eq!(db.snapshot().revision(), revision);
    assert_eq!(number(&db.snapshot(), db.collection_id(1), &[0, 0]), 12);
    assert_eq!(
        number(
            &db.retained_snapshot(1).unwrap(),
            db.collection_id(1),
            &[0, 0]
        ),
        0
    );
}

#[test]
fn contested_commits_retry_explicitly_without_lost_updates() {
    let db = Database::new().unwrap();
    let c = seed(&db, 1);
    let start = Barrier::new(8);
    let conflicts = AtomicUsize::new(0);
    thread::scope(|scope| {
        for _ in 0..8 {
            let (db, start, conflicts) = (&db, &start, &conflicts);
            scope.spawn(move || {
                for round in 0..100 {
                    let mut succeeded = false;
                    for attempt in 0..10_000 {
                        let mut tx = db.begin();
                        let n = tx.get(c, &[0, 0]).unwrap().unwrap().as_u64().unwrap();
                        assert_eq!(tx.get(c, &[0, 1]).unwrap().unwrap().as_u64(), Ok(n));
                        for side in 0..2 {
                            tx.put(c, &[0, side], &db.unsigned(n + 1).unwrap()).unwrap();
                        }
                        if round == 0 && attempt == 0 {
                            start.wait();
                        }
                        match tx.commit() {
                            Ok(_) => {
                                succeeded = true;
                                break;
                            }
                            Err(Error::Conflict) => {
                                conflicts.fetch_add(1, Ordering::Relaxed);
                                thread::yield_now();
                            }
                            other => panic!("unexpected commit: {other:?}"),
                        }
                    }
                    assert!(succeeded, "retry budget exhausted");
                }
            });
        }
    });
    assert!(conflicts.load(Ordering::Relaxed) >= 7);
    for side in 0..2 {
        assert_eq!(number(&db.snapshot(), c, &[0, side]), 800);
    }
    assert_eq!(db.snapshot().revision(), 802);
}

#[test]
fn overlapping_write_skew_scan_absence_and_whole_database_conflict() {
    for mode in 0..4 {
        let db = Database::new().unwrap();
        let c = seed(&db, 1);
        let prepared = Barrier::new(2);
        let outcomes = thread::scope(|scope| {
            let workers: Vec<_> = (0..2)
                .map(|i| {
                    let (db, prepared) = (&db, &prepared);
                    scope.spawn(move || {
                        let mut tx = db.begin_with_validation(if mode == 3 {
                            Validation::WholeDatabase
                        } else {
                            Validation::Records
                        });
                        match mode {
                            0 => {
                                tx.get(c, &[0, 1 - i]).unwrap();
                            }
                            1 => {
                                tx.scan(c).unwrap();
                            }
                            2 => {
                                assert_eq!(tx.get(c, b"missing").unwrap(), None);
                            }
                            _ => {}
                        }
                        tx.put(c, &[0, i], &db.unsigned(1).unwrap()).unwrap();
                        prepared.wait();
                        tx.commit()
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|w| w.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            outcomes.iter().filter(|r| r.is_ok()).count(),
            1,
            "mode={mode}"
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|r| **r == Err(Error::Conflict))
                .count(),
            1
        );
        assert_eq!(
            number(&db.snapshot(), c, &[0, 0]) + number(&db.snapshot(), c, &[0, 1]),
            1
        );
    }
}

struct PausedStore {
    store: MemoryStore,
    armed: Arc<AtomicBool>,
    entered: mpsc::Sender<()>,
    resume: mpsc::Receiver<()>,
    panic: bool,
    syncs_before_pause: usize,
}
impl ByteStore for PausedStore {
    fn lock(&mut self) -> weekend_db::Result<()> {
        self.store.lock()
    }
    fn unlock(&mut self) {
        self.store.unlock();
    }
    fn length(&mut self) -> weekend_db::Result<u64> {
        self.store.length()
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> weekend_db::Result<usize> {
        self.store.read(offset, out)
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> weekend_db::Result<usize> {
        self.store.write(offset, bytes)
    }
    fn sync(&mut self) -> weekend_db::Result<()> {
        self.store.sync()?;
        if self.armed.load(Ordering::Relaxed) {
            if self.syncs_before_pause != 0 {
                self.syncs_before_pause -= 1;
                return Ok(());
            }
            self.armed.store(false, Ordering::Relaxed);
            self.entered.send(()).unwrap();
            self.resume.recv_timeout(Duration::from_secs(30)).unwrap();
            assert!(!self.panic, "intentional adapter unwind after sync");
        }
        Ok(())
    }
}

#[test]
fn pinned_reader_proceeds_during_io_and_adapter_panic_fails_closed() {
    // Unwind after append, root-slot, and witness sync. The disk may contain a
    // new root even though the in-process head was never acknowledged.
    for (panic, syncs_before_pause) in [(false, 0), (true, 0), (true, 1), (true, 2)] {
        let store = MemoryStore::new().unwrap();
        let armed = Arc::new(AtomicBool::new(false));
        let (entered, entry) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let db = Database::create_on(
            PausedStore {
                store: store.clone(),
                armed: armed.clone(),
                entered,
                resume: resumed,
                panic,
                syncs_before_pause,
            },
            *b"concurrency-0002",
            Limits::default(),
        )
        .unwrap();
        let c = seed(&db, 1);
        let pinned = db.snapshot();
        let mut tx = db.begin();
        tx.put(c, &[0, 0], &db.unsigned(1).unwrap()).unwrap();
        armed.store(true, Ordering::Relaxed);
        thread::scope(|s| {
            let writer = s.spawn(move || {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tx.commit()))
            });
            entry.recv_timeout(Duration::from_secs(30)).unwrap();
            // The writer holds the commit mutex here, inside a real sync call.
            assert_eq!(number(&pinned, c, &[0, 0]), 0);
            resume.send(()).unwrap();
            let result = writer.join().unwrap();
            assert_eq!(result.is_err(), panic);
            if !panic {
                result.unwrap().unwrap();
            }
        });
        assert_eq!(db.requires_recovery(), panic);
        assert_eq!(
            number(&db.snapshot(), c, &[0, 0]),
            if panic { 0 } else { 1 }
        );
        if panic {
            assert_eq!(db.begin().commit(), Err(Error::RecoveryRequired));
            assert_eq!(db.create_collection(), Err(Error::RecoveryRequired));
            assert_eq!(db.retain_snapshot(&pinned), Err(Error::RecoveryRequired));
        }
        drop(pinned);
        drop(db);
        let reopened = Database::open_on(store, Limits::default()).unwrap();
        assert_eq!(
            number(&reopened.snapshot(), reopened.collection_id(1), &[0, 0]),
            if panic && syncs_before_pause == 0 {
                0
            } else {
                1
            }
        );
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn shared_allocation_failure_budget_is_atomic() {
    let db = Database::new().unwrap();
    db.fail_allocations_after(Some(100));
    let start = Barrier::new(8);
    let successes = thread::scope(|s| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                s.spawn(|| {
                    start.wait();
                    (0..100)
                        .filter(|_| match db.unsigned(1) {
                            Ok(_) => true,
                            Err(Error::OutOfMemory) => false,
                            other => panic!("{other:?}"),
                        })
                        .count()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .sum::<usize>()
    });
    assert_eq!(successes, 100);
    db.fail_allocations_after(None);
    assert!(db.unsigned(1).is_ok());
}
