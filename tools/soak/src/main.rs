use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Barrier,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use weekend_db::{Database, Limits, Snapshot, storage};
const CAP: usize = 16 * 1024 * 1024;
const WRITES: u64 = 32;
fn limits() -> Limits {
    Limits {
        max_live_bytes: CAP,
        ..Limits::default()
    }
}
fn pair(s: &Snapshot<'_>, db: &Database, writer: u8) -> u64 {
    let c = db.collection_id(1);
    let a = s.get(c, &[writer, 0]).unwrap().unwrap().as_u64().unwrap();
    let b = s.get(c, &[writer, 1]).unwrap().unwrap().as_u64().unwrap();
    assert_eq!(a, b, "torn paired-key transaction");
    a
}
fn verify(db: &Database, expected: u64, retained: &VecDeque<(u64, u64, u64)>, revision: u64) {
    let s = db.snapshot();
    assert_eq!(s.revision(), revision);
    assert_eq!(s.scan(db.collection_id(1)).unwrap().len(), 4);
    for w in 0..2 {
        assert_eq!(pair(&s, db, w), expected);
    }
    assert_eq!(
        db.retained_ids().unwrap(),
        retained.iter().map(|r| r.0).collect::<Vec<_>>()
    );
    for &(id, value, rev) in retained {
        let old = db.retained_snapshot(id).unwrap();
        assert_eq!(old.revision(), rev);
        assert_eq!(old.scan(db.collection_id(1)).unwrap().len(), 4);
        for w in 0..2 {
            assert_eq!(pair(&old, db, w), value);
        }
    }
    let m = db.memory_usage();
    assert!(m.live_bytes <= m.peak_bytes && m.peak_bytes <= CAP);
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        4,
        "usage: soak NEW_DIRECTORY SECONDS MAX_ROUNDS"
    );
    let seconds: u64 = args[2].parse().unwrap();
    let max_rounds: u64 = args[3].parse().unwrap();
    assert!((1..=86400).contains(&seconds) && (1..=100000).contains(&max_rounds));
    let dir = Path::new(&args[1]);
    std::fs::create_dir(dir).expect("output must be new");
    let path = dir.join("live.db");
    let backup = dir.join("backup.db");
    let restore = dir.join("restore.db");
    let db = Database::create_file(&path, limits()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    for w in 0..2 {
        for k in 0..2 {
            tx.put(c, &[w, k], &db.unsigned(0).unwrap()).unwrap();
        }
    }
    tx.commit().unwrap();
    drop(tx);
    drop(db);
    let start = Instant::now();
    let mut retained = VecDeque::new();
    let mut rounds = 0;
    let mut total_reads = 0;
    while rounds < max_rounds && start.elapsed() < Duration::from_secs(seconds) {
        let db = Database::open_file(&path, limits()).unwrap();
        let pinned = db.snapshot();
        let old = rounds * WRITES;
        let barrier = Barrier::new(5);
        let stop = AtomicBool::new(false);
        let reads = AtomicU64::new(0);
        std::thread::scope(|scope| {
            let readers: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        loop {
                            let s = db.snapshot();
                            for w in 0..2 {
                                let value = pair(&s, &db, w);
                                assert!((old..=old + WRITES).contains(&value));
                            }
                            reads.fetch_add(1, Ordering::Relaxed);
                            if stop.load(Ordering::Acquire) {
                                break;
                            }
                        }
                    })
                })
                .collect();
            let writers: Vec<_> = (0..2)
                .map(|w| {
                    scope.spawn({
                        let db = &db;
                        let barrier = &barrier;
                        move || {
                            barrier.wait();
                            for n in 1..=WRITES {
                                let v = db.unsigned(old + n).unwrap();
                                let mut tx = db.begin();
                                for k in 0..2 {
                                    tx.put(db.collection_id(1), &[w, k], &v).unwrap();
                                }
                                tx.commit().unwrap();
                            }
                        }
                    })
                })
                .collect();
            barrier.wait();
            for writer in writers {
                writer.join().unwrap();
            }
            stop.store(true, Ordering::Release);
            for reader in readers {
                reader.join().unwrap();
            }
        });
        rounds += 1;
        total_reads += reads.load(Ordering::Relaxed);
        assert!(reads.load(Ordering::Relaxed) > 0);
        for w in 0..2 {
            assert_eq!(pair(&pinned, &db, w), old);
        }
        drop(pinned);
        let snap = db.snapshot();
        let rev = snap.revision();
        let id = db.retain_snapshot(&snap).unwrap();
        drop(snap);
        retained.push_back((id, rounds * WRITES, rev));
        if retained.len() > 8 {
            let (id, value, _) = retained.pop_front().unwrap();
            let pinned = db.retained_snapshot(id).unwrap();
            db.release_retained(id).unwrap();
            for w in 0..2 {
                assert_eq!(pair(&pinned, &db, w), value);
            }
            drop(pinned);
        }
        let revision = db.snapshot().revision();
        verify(&db, rounds * WRITES, &retained, revision);
        let usage = db.memory_usage();
        drop(db);
        let before = std::fs::metadata(&path).unwrap().len();
        Database::compact_file(&path, limits()).unwrap();
        let after = std::fs::metadata(&path).unwrap().len();
        assert!(after <= before);
        assert!(before < 32 * 1024 * 1024);
        let db = Database::open_file(&path, limits()).unwrap();
        verify(&db, rounds * WRITES, &retained, revision);
        drop(db);
        storage::backup_file(&path, &backup, limits()).unwrap();
        storage::restore_file(&backup, &restore, limits()).unwrap();
        for file in [&backup, &restore] {
            let db = Database::open_file(file, limits()).unwrap();
            verify(&db, rounds * WRITES, &retained, revision);
            drop(db);
        }
        std::fs::remove_file(&backup).unwrap();
        std::fs::remove_file(&restore).unwrap();
        println!(
            "{{\"round\":{rounds},\"elapsed_ms\":{},\"revision\":{revision},\"reads\":{total_reads},\"retained\":{},\"live_bytes\":{},\"peak_bytes\":{},\"append_bytes\":{before},\"compacted_bytes\":{after}}}",
            start.elapsed().as_millis(),
            retained.len(),
            usage.live_bytes,
            usage.peak_bytes
        );
    }
    assert!(rounds > 0);
    println!(
        "{{\"complete\":true,\"rounds\":{rounds},\"elapsed_ms\":{},\"reads\":{total_reads}}}",
        start.elapsed().as_millis()
    );
}
