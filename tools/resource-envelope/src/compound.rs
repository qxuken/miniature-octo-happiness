//! Compound finite workload; no production API or limit changes.
use std::{
    fs,
    io::Write,
    path::Path,
    sync::{Barrier, mpsc},
    thread,
    time::Instant,
};
use weekend_db::{
    Database, Error, Limits, Snapshot, TransactionState, Value,
    storage::{self, Crash, Fault, IoKind, SimulatedDisk},
};
const MIB: usize = 1024 * 1024;
const CORE: usize = 96 * MIB;
const BULK: u64 = 48;
const LARGE: usize = 896 * 1024;
const ROOTS: u64 = 64;
const WRITERS: usize = 8;
const ROUNDS: u64 = 4;
const CELL: usize = 64 * 1024;
const BASE: u64 = 3 + BULK + 1;
const FIXTURE: u64 = BASE + 2 * ROOTS;
const FINAL: u64 = FIXTURE + WRITERS as u64 * ROUNDS;
const OFFSET: u64 = 14_695_981_039_346_656_037;
fn limits(cap: usize) -> Limits {
    Limits {
        max_live_bytes: cap,
        ..Limits::default()
    }
}
fn payload(epoch: u64, n: usize) -> Vec<u8> {
    let mut bytes: Vec<_> = (0..n)
        .map(|i| ((epoch * 13 + i as u64) % 251) as u8)
        .collect();
    bytes[..8].copy_from_slice(&epoch.to_le_bytes());
    bytes
}
fn hash(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h = (*h ^ u64::from(*b)).wrapping_mul(1_099_511_628_211);
    }
}
fn rss_kib() -> usize {
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|line| line.starts_with("VmRSS:"))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    }
    #[cfg(target_os = "macos")]
    {
        // Same point-sample method as the existing resource driver. The outer
        // observer bounds and retires this process group, including ps children.
        let result = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .unwrap();
        assert!(result.status.success());
        String::from_utf8(result.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
}
fn nested(db: &Database, epoch: u64) -> Value {
    let mut value = db.bytes(&payload(epoch, 8192)).unwrap();
    for _ in 1..64 {
        value = db.list().unwrap().list_push(&value).unwrap();
    }
    assert_eq!(db.list().unwrap().list_push(&value), Err(Error::Limit));
    value
}
fn row(db: &Database, writer: usize, seq: u64) -> Value {
    db.map()
        .unwrap()
        .map_set(b"seq", &db.unsigned(seq).unwrap())
        .unwrap()
        .map_set(
            b"payload",
            &db.bytes(&payload(1000 + writer as u64 * 10 + seq, CELL))
                .unwrap(),
        )
        .unwrap()
}
fn check_cell(view: &Snapshot<'_>, db: &Database, writer: usize, seq: Option<u64>) -> (u64, Value) {
    let value = view
        .get(db.collection_id(3), &[writer as u8])
        .unwrap()
        .unwrap();
    assert_eq!(value.len(), Ok(2));
    let actual = value.map_get(b"seq").unwrap().unwrap().as_u64().unwrap();
    if let Some(seq) = seq {
        assert_eq!(actual, seq);
    }
    let bytes = value.map_get(b"payload").unwrap().unwrap();
    assert_eq!(
        bytes.as_bytes().unwrap(),
        payload(1000 + writer as u64 * 10 + actual, CELL)
    );
    (actual, bytes)
}
fn verify(db: &Database, revision: u64, seq: u64, changed: bool, catalog: bool) -> [u64; 4] {
    assert_eq!(db.snapshot().revision(), revision);
    if catalog {
        assert_eq!(db.retained_ids().unwrap(), (1..=ROOTS).collect::<Vec<_>>());
    }
    let mut roots = vec![db.snapshot()];
    for id in 1..=ROOTS {
        let s = db.retained_snapshot(id).unwrap();
        assert_eq!(s.revision(), BASE + id * 2 - 1);
        roots.push(s);
    }
    for view in &roots {
        assert_eq!(view.count(db.collection_id(1)), Ok(BULK as usize));
        assert_eq!(view.count(db.collection_id(2)), Ok(1));
        assert_eq!(view.count(db.collection_id(3)), Ok(WRITERS));
    }
    let mut digests = [OFFSET; 4];
    // Full retained bytes compare against the same independently fingerprinted
    // current bulk payload, without hashing 65 copies of its 42 MiB contents.
    for key in 0..BULK {
        let expected = payload(100 + key, LARGE);
        for (i, view) in roots.iter().enumerate() {
            let value = view
                .get(db.collection_id(1), &key.to_be_bytes())
                .unwrap()
                .unwrap();
            assert_eq!(value.as_bytes().unwrap(), expected);
            if i == 0 {
                hash(&mut digests[0], value.as_bytes().unwrap());
            }
        }
    }
    for (i, view) in roots.iter().enumerate() {
        let mut value = view.get(db.collection_id(2), b"nested").unwrap().unwrap();
        for _ in 1..64 {
            assert_eq!(value.len(), Ok(1));
            value = value.list_get(0).unwrap();
        }
        assert_eq!(
            value.as_bytes().unwrap(),
            payload(if i == 0 { 64 } else { i as u64 }, 8192)
        );
        hash(&mut digests[1], value.as_bytes().unwrap());
        for writer in 0..WRITERS {
            let expected = if i == 0 {
                seq + u64::from(changed && writer == 0)
            } else {
                0
            };
            let (actual, bytes) = check_cell(view, db, writer, Some(expected));
            let digest = if i == 0 {
                &mut digests[2]
            } else {
                &mut digests[3]
            };
            hash(digest, &actual.to_le_bytes());
            if i == 0 {
                hash(digest, bytes.as_bytes().unwrap());
            }
        }
    }
    digests
}
#[allow(clippy::too_many_arguments)]
fn event(
    phase: &str,
    db: &Database,
    revision: u64,
    seq: u64,
    changed: bool,
    elapsed: u128,
    detail: &str,
    catalog: bool,
) {
    let digest = verify(db, revision, seq, changed, catalog);
    assert!(!db.requires_recovery());
    let m = db.memory_usage();
    let space = db.storage_space().unwrap();
    assert!(m.live_bytes <= m.peak_bytes && m.peak_bytes <= CORE);
    println!(
        "{{\"phase\":\"{phase}\",\"revision\":{revision},\"seq\":{seq},\"changed\":{changed},\"roots\":64,\"digests\":{digest:?},\"live_bytes\":{},\"peak_bytes\":{},\"cap_bytes\":{CORE},\"rss_kib\":{},\"file_bytes\":{},\"elapsed_ns\":{elapsed},\"detail\":{detail}}}",
        m.live_bytes,
        m.peak_bytes,
        rss_kib(),
        space.physical_bytes
    );
    std::io::stdout().flush().unwrap();
}
fn pressure(db: &Database) {
    let baseline = db.memory_usage().live_bytes;
    let mut candidate = db.begin();
    candidate
        .put(db.collection_id(3), &[0], &row(db, 0, 1))
        .unwrap();
    let before_file = db.storage_space().unwrap().physical_bytes;
    let start = Barrier::new(WRITERS + 1);
    thread::scope(|scope| {
        let (ready, receive) = mpsc::channel();
        let mut release = Vec::new();
        let mut workers = Vec::new();
        for writer in 0..WRITERS {
            let start = &start;
            let ready = ready.clone();
            let (send, wait) = mpsc::channel();
            release.push(send);
            workers.push(scope.spawn(move || {
                start.wait();
                let now = Instant::now();
                let mut tx = db.begin();
                for key in [writer as u8, 100 + writer as u8] {
                    tx.put(
                        db.collection_id(3),
                        &[key],
                        &db.bytes(&payload(500 + writer as u64, MIB)).unwrap(),
                    )
                    .unwrap();
                }
                ready.send(now.elapsed().as_nanos()).unwrap();
                wait.recv().unwrap();
                drop(tx);
            }));
        }
        start.wait();
        let timings: Vec<_> = (0..WRITERS).map(|_| receive.recv().unwrap()).collect();
        event(
            "drafts_ready",
            db,
            FIXTURE,
            0,
            false,
            0,
            &format!("{{\"drafts\":8,\"prepare_ns\":{timings:?}}}"),
            true,
        );
        let mut fillers = Vec::new();
        for size in [256 * 1024, 512] {
            let bytes = payload(9999, size);
            loop {
                match db.bytes(&bytes) {
                    Ok(value) => fillers.push(value),
                    Err(Error::Limit) => break,
                    other => panic!("filler: {other:?}"),
                }
            }
        }
        assert!(db.memory_usage().live_bytes > CORE * 99 / 100);
        let now = Instant::now();
        assert_eq!(candidate.commit(), Err(Error::Limit));
        let elapsed = now.elapsed().as_nanos();
        assert_eq!(candidate.state(), TransactionState::Failed);
        assert_eq!(candidate.commit(), Err(Error::Closed));
        assert_eq!(db.storage_space().unwrap().physical_bytes, before_file);
        event(
            "core_limit_rejected",
            db,
            FIXTURE,
            0,
            false,
            elapsed,
            "{\"error\":\"Limit\",\"drafts\":8}",
            false,
        );
        drop(fillers);
        drop(candidate);
        for send in release {
            send.send(()).unwrap();
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    assert_eq!(db.memory_usage().live_bytes, baseline);
    drop(db.bytes(&payload(10000, MIB)).unwrap());
    event(
        "drafts_released",
        db,
        FIXTURE,
        0,
        false,
        0,
        &format!("{{\"baseline_live\":{baseline}}}"),
        true,
    );
}
fn contended(db: &Database) {
    let start = Barrier::new(WRITERS + 3);
    let prepared = Barrier::new(WRITERS);
    let done = Barrier::new(WRITERS);
    let (commits, readers) = thread::scope(|scope| {
        let mut writers = Vec::new();
        let mut readers = Vec::new();
        for writer in 0..WRITERS {
            let (start, prepared, done) = (&start, &prepared, &done);
            writers.push(scope.spawn(move || {
                start.wait();
                let mut samples = Vec::new();
                for seq in 1..=ROUNDS {
                    let mut tx = db.begin();
                    tx.put(db.collection_id(3), &[writer as u8], &row(db, writer, seq))
                        .unwrap();
                    prepared.wait();
                    let now = Instant::now();
                    let revision = tx.commit().unwrap();
                    samples.push((revision, now.elapsed().as_nanos()));
                    done.wait();
                }
                samples
            }));
        }
        for _ in 0..2 {
            let start = &start;
            readers.push(scope.spawn(move || {
                let old = db.retained_snapshot(1).unwrap();
                start.wait();
                let mut ns = Vec::new();
                for _ in 0..128 {
                    let now = Instant::now();
                    let view = db.snapshot();
                    for writer in 0..WRITERS {
                        let (seq, _) = check_cell(&view, db, writer, None);
                        assert!(seq <= ROUNDS);
                        check_cell(&old, db, writer, Some(0));
                    }
                    ns.push(now.elapsed().as_nanos());
                    thread::yield_now();
                }
                ns
            }));
        }
        start.wait();
        (
            writers
                .into_iter()
                .flat_map(|w| w.join().unwrap())
                .collect::<Vec<_>>(),
            readers
                .into_iter()
                .flat_map(|r| r.join().unwrap())
                .collect::<Vec<_>>(),
        )
    });
    let mut revisions: Vec<_> = commits.iter().map(|sample| sample.0).collect();
    revisions.sort_unstable();
    assert_eq!(revisions, (FIXTURE + 1..=FINAL).collect::<Vec<_>>());
    let commits: Vec<_> = commits.into_iter().map(|sample| sample.1).collect();
    event(
        "contended_commits",
        db,
        FINAL,
        ROUNDS,
        false,
        0,
        &format!(
            "{{\"commit_ns\":{commits:?},\"reader_ns\":{readers:?},\"writers\":8,\"rounds\":4}}"
        ),
        true,
    );
}
fn native(dir: &Path) {
    fs::create_dir(dir).unwrap();
    let path = dir.join("source.wdb");
    let db = Database::create_file(&path, limits(CORE)).unwrap();
    for id in 1..=3 {
        assert_eq!(db.create_collection().unwrap().as_u64(), id);
    }
    for key in 0..BULK {
        let mut tx = db.begin();
        tx.put(
            db.collection_id(1),
            &key.to_be_bytes(),
            &db.bytes(&payload(100 + key, LARGE)).unwrap(),
        )
        .unwrap();
        tx.commit().unwrap();
    }
    let mut tx = db.begin();
    for writer in 0..WRITERS {
        tx.put(db.collection_id(3), &[writer as u8], &row(&db, writer, 0))
            .unwrap();
    }
    tx.commit().unwrap();
    drop(tx);
    for epoch in 1..=ROOTS {
        let mut tx = db.begin();
        tx.put(db.collection_id(2), b"nested", &nested(&db, epoch))
            .unwrap();
        assert_eq!(tx.commit(), Ok(BASE + 2 * epoch - 1));
        drop(tx);
        assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(epoch));
    }
    assert_eq!(db.retain_snapshot(&db.snapshot()), Err(Error::Limit));
    assert!(
        db.storage_space().unwrap().physical_bytes > (storage::MAX_IMAGE_BYTES * 9 / 10) as u64
    );
    drop(db);
    let db = Database::open_file(&path, limits(CORE)).unwrap();
    event("fixture", &db, FIXTURE, 0, false, 0, "{}", true);
    pressure(&db);
    contended(&db);
    let mut value = db.list().unwrap();
    for key in 0..4 {
        value = value
            .list_push(&db.bytes(&payload(9000 + key, MIB)).unwrap())
            .unwrap();
    }
    let before = db.storage_space().unwrap();
    assert!(before.remaining_append_bytes < (4 * MIB) as u64);
    let mut tx = db.begin();
    tx.put(db.collection_id(3), b"image-limit", &value).unwrap();
    let now = Instant::now();
    assert_eq!(tx.commit(), Err(Error::Limit));
    let elapsed = now.elapsed().as_nanos();
    drop(tx);
    drop(value);
    assert_eq!(db.storage_space().unwrap(), before);
    event(
        "image_limit_rejected",
        &db,
        FINAL,
        ROUNDS,
        false,
        elapsed,
        "{\"error\":\"Limit\",\"new_payload_bytes\":4194304}",
        true,
    );
    drop(db);
    let now = Instant::now();
    let db = Database::open_file(&path, limits(CORE)).unwrap();
    let ns = now.elapsed().as_nanos();
    event("reopen", &db, FINAL, ROUNDS, false, ns, "{}", true);
    drop(db);
    let rejected = dir.join("rejected-backup.wdb");
    let original = fs::read(&path).unwrap();
    let now = Instant::now();
    assert_eq!(
        storage::backup_file(&path, &rejected, limits(CORE)),
        Err(Error::Limit)
    );
    let ns = now.elapsed().as_nanos();
    assert_eq!(fs::metadata(&rejected).unwrap().len(), 0);
    assert_eq!(fs::read(&path).unwrap(), original);
    drop(original);
    let db = Database::open_file(&path, limits(CORE)).unwrap();
    event(
        "backup_budget_rejected",
        &db,
        FINAL,
        ROUNDS,
        false,
        ns,
        "{\"copy_budget\":100663296,\"error\":\"Limit\"}",
        true,
    );
    drop(db);
    let backup = dir.join("backup.wdb");
    let restored = dir.join("restored.wdb");
    let compact = dir.join("compact.wdb");
    for (phase, destination) in [
        ("backup", &backup),
        ("restore", &restored),
        ("compact", &compact),
    ] {
        if phase == "compact" {
            fs::copy(&path, destination).unwrap();
        }
        let now = Instant::now();
        match phase {
            "backup" => {
                storage::backup_file(&path, destination, limits(256 * MIB)).unwrap();
            }
            "restore" => {
                storage::restore_file(&backup, destination, limits(256 * MIB)).unwrap();
            }
            _ => {
                Database::compact_file(destination, limits(256 * MIB)).unwrap();
            }
        }
        let ns = now.elapsed().as_nanos();
        let db = Database::open_file(destination, limits(CORE)).unwrap();
        event(
            phase,
            &db,
            FINAL,
            ROUNDS,
            false,
            ns,
            "{\"copy_budget\":268435456}",
            true,
        );
    }
    complete("native");
}
fn probe(db: &Database) -> weekend_db::Transaction<'_> {
    let mut tx = db.begin();
    tx.put(db.collection_id(3), &[0], &row(db, 0, ROUNDS + 1))
        .unwrap();
    tx
}
fn faults(dir: &Path, source: &Path) {
    fs::create_dir(dir).unwrap();
    let image = fs::read(source).unwrap();
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
    disk.arm(usize::MAX, Fault::Error);
    probe(&db).commit().unwrap();
    let syncs: Vec<_> = disk
        .trace()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(i, e)| (e.kind == IoKind::Sync).then_some(i))
        .collect();
    assert_eq!(syncs.len(), 3);
    drop(db);
    drop(disk);
    let cases = [
        (
            "allocation",
            None,
            Crash::DropAll,
            Some(Error::OutOfMemory),
            false,
        ),
        (
            "append_drop",
            Some((syncs[0], Fault::Error)),
            Crash::DropAll,
            Some(Error::Io),
            false,
        ),
        (
            "append_persist",
            Some((syncs[0], Fault::SyncError(Crash::PersistAll))),
            Crash::DropAll,
            Some(Error::Io),
            false,
        ),
        (
            "root_drop",
            Some((syncs[1], Fault::Error)),
            Crash::DropAll,
            Some(Error::OutcomeUnknown),
            false,
        ),
        (
            "root_persist",
            Some((syncs[1], Fault::Error)),
            Crash::PersistAll,
            Some(Error::OutcomeUnknown),
            true,
        ),
        (
            "witness_drop",
            Some((syncs[2], Fault::Error)),
            Crash::DropAll,
            Some(Error::OutcomeUnknown),
            true,
        ),
        (
            "witness_persist",
            Some((syncs[2], Fault::SyncError(Crash::PersistAll))),
            Crash::DropAll,
            Some(Error::OutcomeUnknown),
            true,
        ),
        ("acknowledged", None, Crash::DropAll, None, true),
    ];
    for (name, fault, crash, error, survives) in cases {
        let now = Instant::now();
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let db = Database::open_on(disk.clone(), limits(CORE)).unwrap();
        let mut tx = probe(&db);
        let pressure = thread::scope(|scope| {
            let (ready, receive) = mpsc::channel();
            let mut release = Vec::new();
            let mut workers = Vec::new();
            for writer in 1..=2u8 {
                let ready = ready.clone();
                let (send, wait) = mpsc::channel();
                release.push(send);
                let db = &db;
                workers.push(scope.spawn(move || {
                    let mut pending = db.begin();
                    pending
                        .put(
                            db.collection_id(3),
                            &[100 + writer],
                            &db.bytes(&payload(700 + u64::from(writer), MIB)).unwrap(),
                        )
                        .unwrap();
                    ready.send(()).unwrap();
                    wait.recv().unwrap();
                    if db.requires_recovery() {
                        assert_eq!(pending.commit(), Err(Error::RecoveryRequired));
                    }
                }));
            }
            for _ in 0..2 {
                receive.recv().unwrap();
            }
            let mut fillers = Vec::new();
            while db.memory_usage().live_bytes < CORE * 9 / 10 {
                fillers.push(db.bytes(&payload(800, 512 * 1024)).unwrap());
            }
            let pressure = db.memory_usage().live_bytes;
            assert!(pressure > CORE * 89 / 100);
            if name == "allocation" {
                db.fail_allocations_after(Some(0));
            }
            if let Some((step, fault)) = fault {
                disk.arm(step, fault);
            }
            assert_eq!(tx.commit(), error.map_or(Ok(FINAL + 1), Err));
            db.fail_allocations_after(None);
            assert_eq!(
                db.requires_recovery(),
                error.is_some_and(|e| e != Error::OutOfMemory)
            );
            assert_eq!(
                db.snapshot().revision(),
                if error.is_none() { FINAL + 1 } else { FINAL }
            );
            if error.is_some() {
                assert_eq!(tx.commit(), Err(Error::Closed));
            }
            drop(fillers);
            for send in release {
                send.send(()).unwrap();
            }
            for worker in workers {
                worker.join().unwrap();
            }
            pressure
        });
        drop(tx);
        drop(db);
        disk.clear_fault();
        disk.crash(crash).unwrap();
        let reopened = Database::open_on(disk.clone(), limits(CORE)).unwrap();
        let revision = FINAL + u64::from(survives);
        verify(&reopened, revision, ROUNDS, survives, true);
        if survives {
            assert_eq!(reopened.begin().commit(), Ok(revision + 1));
        } else {
            assert_eq!(probe(&reopened).commit(), Ok(revision + 1));
        }
        drop(reopened);
        disk.crash(Crash::DropAll).unwrap();
        let reopened = Database::open_on(disk, limits(CORE)).unwrap();
        event(
            name,
            &reopened,
            revision + 1,
            ROUNDS,
            true,
            now.elapsed().as_nanos(),
            &format!(
                "{{\"reopened_revision\":{revision},\"pressure_live_bytes\":{pressure},\"pending_drafts\":2,\"error\":\"{}\"}}",
                error.map_or("Ok".to_owned(), |e| format!("{e:?}"))
            ),
            true,
        );
    }
    complete("faults");
}
fn complete(profile: &str) {
    println!(
        "{{\"complete\":true,\"profile\":\"{profile}\",\"source_id\":\"{}\",\"features\":{},\"abi\":{},\"format\":{}}}",
        weekend_db::version::SOURCE_ID,
        weekend_db::version::FEATURES,
        weekend_db::version::ABI_VERSION,
        weekend_db::version::FORMAT_VERSION
    );
}
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    assert!((2..=4).contains(&args.len()));
    let dir = Path::new(&args[1]);
    match args.len() {
        2 => native(dir),
        3 => faults(dir, Path::new(&args[2])),
        _ => {
            assert_eq!(args[3], "inspect");
            fs::create_dir(dir).unwrap();
            let now = Instant::now();
            let db = Database::open_file(Path::new(&args[2]), limits(CORE)).unwrap();
            let ns = now.elapsed().as_nanos();
            event(
                "fresh_process_reopen",
                &db,
                FINAL,
                ROUNDS,
                false,
                ns,
                "{}",
                true,
            );
            complete("inspect");
        }
    }
}
