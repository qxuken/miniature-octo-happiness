//! Finite resource qualification; independent deterministic byte/state oracle.
use std::{fs, io::Write, path::Path, time::Instant};
use weekend_db::{Database, Error, Limits, Snapshot, Value, storage};

const MIB: usize = 1024 * 1024;
const CORE: usize = 64 * MIB;
const LARGE_CORE: usize = 256 * MIB;
const ROOTS: u64 = 64;
const DEPTH: usize = 64;
const NESTED_BYTES: usize = 8192;
const LARGE_BYTES: usize = 896 * 1024;
fn limits(cap: usize) -> Limits {
    Limits {
        max_live_bytes: cap,
        ..Limits::default()
    }
}
fn bytes(epoch: u64, length: usize) -> Vec<u8> {
    let mut data: Vec<_> = (0..length)
        .map(|i| ((epoch * 13 + i as u64) % 251) as u8)
        .collect();
    data[..8].copy_from_slice(&epoch.to_le_bytes());
    data
}
fn nested(db: &Database, epoch: u64) -> Value {
    let mut value = db.bytes(&bytes(epoch, NESTED_BYTES)).unwrap();
    for _ in 1..DEPTH {
        value = db.list().unwrap().list_push(&value).unwrap();
    }
    assert_eq!(db.list().unwrap().list_push(&value), Err(Error::Limit));
    value
}
fn check_nested(mut value: Value, epoch: u64) {
    for _ in 1..DEPTH {
        assert_eq!(value.len().unwrap(), 1);
        value = value.list_get(0).unwrap();
    }
    assert_eq!(value.as_bytes().unwrap(), bytes(epoch, NESTED_BYTES));
}
fn check_root(db: &Database, snapshot: Snapshot<'_>, epoch: u64) {
    let c = db.collection_id(1);
    assert_eq!(snapshot.count(c).unwrap(), 1);
    check_nested(snapshot.get(c, b"nested").unwrap().unwrap(), epoch);
}
fn verify_nested(db: &Database, retained: bool, revision: u64) {
    verify_nested_known(db, retained, revision, true);
}
fn verify_nested_known(db: &Database, retained: bool, revision: u64, read_catalog: bool) {
    assert_eq!(db.snapshot().revision(), revision);
    check_root(db, db.snapshot(), ROOTS);
    let ids = if retained {
        (1..=ROOTS).collect::<Vec<_>>()
    } else {
        vec![]
    };
    // Enumeration needs temporary charged space. At total saturation use the
    // already checked immutable IDs, and recheck enumeration after release.
    if read_catalog {
        assert_eq!(db.retained_ids().unwrap(), ids);
    }
    for id in ids {
        let snapshot = db.retained_snapshot(id).unwrap();
        assert_eq!(snapshot.revision(), id * 2);
        check_root(db, snapshot, id);
    }
    assert!(!db.requires_recovery());
}
fn rss() -> usize {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").unwrap();
        status
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
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
}
fn digest(db: &Database, large: bool, retained_count: u64) -> u64 {
    let mut hash = 14_695_981_039_346_656_037u64;
    let mut snapshots = vec![db.snapshot()];
    snapshots.extend((1..=retained_count).map(|id| db.retained_snapshot(id).unwrap()));
    let c = db.collection_id(1);
    for snapshot in snapshots {
        let count = snapshot.count(c).unwrap();
        for key in 1..=count as u64 {
            let mut value = if large {
                snapshot.get(c, &key.to_le_bytes()).unwrap().unwrap()
            } else {
                snapshot.get(c, b"nested").unwrap().unwrap()
            };
            if !large {
                for _ in 1..DEPTH {
                    value = value.list_get(0).unwrap();
                }
            }
            for byte in value.as_bytes().unwrap() {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
            }
        }
    }
    hash
}
fn event(phase: &str, db: &Database, path: &Path, elapsed: u128, detail: &str) {
    let m = db.memory_usage();
    assert!(m.live_bytes <= m.peak_bytes && m.peak_bytes <= m.limit_bytes);
    let large = path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("reachable");
    let retained_count = if phase == "core_limit_rejected" {
        ROOTS
    } else {
        db.retained_ids().unwrap().len() as u64
    };
    let oracle = digest(db, large, retained_count);
    let kind = if large {
        "large"
    } else if phase == "bounded_stack_depth64" {
        "stack"
    } else {
        "nested"
    };
    println!(
        "{{\"phase\":\"{phase}\",\"live_bytes\":{},\"peak_bytes\":{},\"cap_bytes\":{},\"rss_kib\":{},\"file_bytes\":{},\"elapsed_ns\":{elapsed},\"retained_count\":{},\"current_count\":{},\"revision\":{},\"oracle_kind\":\"{kind}\",\"oracle_digest\":{oracle},\"detail\":{detail}}}",
        m.live_bytes,
        m.peak_bytes,
        m.limit_bytes,
        rss(),
        fs::metadata(path).unwrap().len(),
        retained_count,
        db.snapshot().count(db.collection_id(1)).unwrap(),
        db.snapshot().revision()
    );
    std::io::stdout().flush().unwrap();
}
fn nested_case(dir: &Path) {
    let path = dir.join("nested.wdb");
    let zero = dir.join("zero-tail.wdb");
    let db = Database::create_file(&path, limits(CORE)).unwrap();
    let c = db.create_collection().unwrap();
    let mut commits = vec![];
    let mut pins = vec![];
    for epoch in 1..=ROOTS {
        let value = nested(&db, epoch);
        let mut tx = db.begin();
        tx.put(c, b"nested", &value).unwrap();
        let started = Instant::now();
        assert_eq!(tx.commit().unwrap(), epoch * 2);
        commits.push(started.elapsed().as_nanos());
        let started = Instant::now();
        assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), epoch);
        pins.push(started.elapsed().as_nanos());
        if epoch == 1 {
            event("nested_first_root", &db, &path, 0, "{}");
        }
    }
    verify_nested(&db, true, 129);
    assert_eq!(db.retain_snapshot(&db.snapshot()), Err(Error::Limit));
    event(
        "nested_64_roots",
        &db,
        &path,
        0,
        &format!("{{\"commit_ns\":{commits:?},\"pin_ns\":{pins:?}}}"),
    );
    drop(db);
    // Reuse the public verified compact-copy API to freeze the zero-tail fixture.
    storage::backup_file(&path, &zero, limits(CORE)).unwrap();
    for sample in 0..3 {
        let started = Instant::now();
        let db = Database::open_file(&path, limits(CORE)).unwrap();
        let open = started.elapsed().as_nanos();
        verify_nested(&db, true, 129);
        event(
            "nested_reopen",
            &db,
            &path,
            open,
            &format!(
                "{{\"sample\":{sample},\"open_verify_ns\":{}}}",
                started.elapsed().as_nanos()
            ),
        );
    }
    let db = Database::open_file(&path, limits(CORE)).unwrap();
    let c = db.collection_id(1);
    let before = db.memory_usage().live_bytes;
    let mut pending = db.begin();
    pending
        .put(c, b"must-not-publish", &db.unsigned(99).unwrap())
        .unwrap();
    let mut drafts = vec![];
    let input = vec![0x5a; 256 * 1024];
    for _ in 0..512 {
        let mut draft = db.begin();
        let result = db
            .bytes(&input)
            .and_then(|value| draft.put(c, b"draft", &value));
        match result {
            Ok(()) => drafts.push(draft),
            Err(error) => {
                assert_eq!(error, Error::Limit);
                break;
            }
        }
    }
    assert!(!drafts.is_empty() && drafts.len() < 512);
    event(
        "draft_pressure",
        &db,
        &path,
        0,
        &format!(
            "{{\"drafts\":{},\"pre_draft_live\":{before}}}",
            drafts.len()
        ),
    );
    let mut fillers = vec![];
    for length in [4096, 512, 64, 1] {
        let input = vec![0xa5; length];
        loop {
            assert!(fillers.len() < 2048);
            match db.bytes(&input) {
                Ok(value) => fillers.push(value),
                Err(error) => {
                    assert_eq!(error, Error::Limit);
                    break;
                }
            }
        }
    }
    let pressured = db.memory_usage().live_bytes;
    assert!(pressured > CORE * 95 / 100);
    let length = fs::metadata(&path).unwrap().len();
    let started = Instant::now();
    assert_eq!(pending.commit(), Err(Error::Limit));
    assert_eq!(fs::metadata(&path).unwrap().len(), length);
    verify_nested_known(&db, true, 129, false);
    assert!(db.snapshot().get(c, b"must-not-publish").unwrap().is_none());
    event(
        "core_limit_rejected",
        &db,
        &path,
        started.elapsed().as_nanos(),
        &format!(
            "{{\"drafts\":{},\"owned_fillers\":{}}}",
            drafts.len(),
            fillers.len()
        ),
    );
    drop(pending);
    drop(fillers);
    drop(drafts);
    assert!(db.memory_usage().live_bytes < pressured / 2);
    let admission = db.bytes(&input).unwrap();
    assert_eq!(admission.as_bytes().unwrap(), input);
    drop(admission);
    event("drafts_released_admission_recovers", &db, &path, 0, "{}");
    let before_release = db.memory_usage().live_bytes;
    for id in 1..=ROOTS {
        db.release_retained(id).unwrap();
    }
    assert!(db.memory_usage().live_bytes < before_release);
    verify_nested(&db, false, 193);
    assert!(db.bytes(&vec![0; 1024 * 1024]).is_ok());
    event("pins_released_admission_recovers", &db, &path, 0, "{}");
    let after_release = db.memory_usage().live_bytes;
    drop(db);
    let db = Database::open_file(&path, limits(CORE)).unwrap();
    verify_nested(&db, false, 193);
    assert!(db.memory_usage().live_bytes < after_release);
    event(
        "pins_released_reopen_reclaims_cache",
        &db,
        &path,
        0,
        &format!("{{\"before_reopen_live\":{after_release}}}"),
    );
    drop(db);

    // Deliberately synthetic, physically written zero tail: exact boundary only.
    let mut file = fs::OpenOptions::new().append(true).open(&zero).unwrap();
    let end = storage::MAX_IMAGE_BYTES - storage::REGION;
    let old = file.metadata().unwrap().len() as usize;
    assert!(old < end);
    let block = vec![0u8; MIB];
    let mut left = end - old;
    while left != 0 {
        let count = left.min(block.len());
        file.write_all(&block[..count]).unwrap();
        left -= count;
    }
    file.sync_all().unwrap();
    drop(file);
    let db = Database::open_file(&zero, limits(CORE)).unwrap();
    verify_nested(&db, true, 129);
    assert_eq!(
        db.storage_space().unwrap().remaining_append_bytes,
        storage::REGION as u64
    );
    assert_eq!(db.begin().commit(), Ok(130));
    assert_eq!(db.storage_space().unwrap().remaining_append_bytes, 0);
    assert_eq!(db.begin().commit(), Err(Error::Limit));
    verify_nested(&db, true, 130);
    event(
        "synthetic_exact_image_boundary",
        &db,
        &zero,
        0,
        "{\"tail_kind\":\"dense_zero_tail\"}",
    );
    drop(db);
    let started = Instant::now();
    Database::compact_file(&zero, limits(CORE)).unwrap();
    let db = Database::open_file(&zero, limits(CORE)).unwrap();
    verify_nested(&db, true, 130);
    assert!(db.storage_space().unwrap().remaining_append_bytes > 0);
    event(
        "synthetic_compact_reopens_admission",
        &db,
        &zero,
        started.elapsed().as_nanos(),
        "{}",
    );
}
fn verify_large(db: &Database, count: u64, revision: u64) {
    assert_eq!(db.snapshot().revision(), revision);
    let c = db.collection_id(1);
    let check = |snapshot: Snapshot<'_>, expected: u64| {
        assert_eq!(snapshot.count(c).unwrap(), expected as usize);
        for key in 1..=expected {
            assert_eq!(
                snapshot
                    .get(c, &key.to_le_bytes())
                    .unwrap()
                    .unwrap()
                    .as_bytes()
                    .unwrap(),
                bytes(key, LARGE_BYTES)
            );
        }
    };
    check(db.snapshot(), count);
    assert_eq!(db.retained_ids().unwrap(), (1..=8).collect::<Vec<_>>());
    for id in 1..=8 {
        let snapshot = db.retained_snapshot(id).unwrap();
        assert_eq!(snapshot.revision(), id * 2);
        check(snapshot, id);
    }
    assert!(!db.requires_recovery());
}
fn large_case(dir: &Path) {
    let path = dir.join("reachable.wdb");
    let db = Database::create_file(&path, limits(LARGE_CORE)).unwrap();
    let c = db.create_collection().unwrap();
    let mut accepted = 0;
    let mut commits = vec![];
    let mut rejected = false;
    for key in 1..=80u64 {
        let value = db.bytes(&bytes(key, LARGE_BYTES)).unwrap();
        let mut tx = db.begin();
        tx.put(c, &key.to_le_bytes(), &value).unwrap();
        let revision = db.snapshot().revision();
        let length = fs::metadata(&path).unwrap().len();
        let started = Instant::now();
        match tx.commit() {
            Ok(_) => {
                accepted = key;
                commits.push(started.elapsed().as_nanos());
                if key <= 8 {
                    db.retain_snapshot(&db.snapshot()).unwrap();
                }
            }
            Err(error) => {
                assert_eq!(error, Error::Limit);
                assert_eq!(db.snapshot().revision(), revision);
                assert_eq!(fs::metadata(&path).unwrap().len(), length);
                assert!(db.snapshot().get(c, &key.to_le_bytes()).unwrap().is_none());
                rejected = true;
                break;
            }
        }
    }
    assert!(rejected && accepted >= 8);
    let revision = 1 + accepted + 8;
    verify_large(&db, accepted, revision);
    let space = db.storage_space().unwrap();
    assert!(space.physical_bytes > storage::MAX_IMAGE_BYTES as u64 * 90 / 100);
    assert!(space.remaining_append_bytes < LARGE_BYTES as u64);
    event(
        "reachable_natural_image_limit",
        &db,
        &path,
        0,
        &format!(
            "{{\"accepted_keys\":{accepted},\"reachable_payload_bytes\":{},\"remaining_append_bytes\":{},\"commit_ns\":{commits:?}}}",
            accepted * LARGE_BYTES as u64,
            space.remaining_append_bytes
        ),
    );
    drop(db);
    for sample in 0..3 {
        let started = Instant::now();
        let db = Database::open_file(&path, limits(LARGE_CORE)).unwrap();
        let open = started.elapsed().as_nanos();
        verify_large(&db, accepted, revision);
        event(
            "reachable_reopen",
            &db,
            &path,
            open,
            &format!(
                "{{\"sample\":{sample},\"open_verify_ns\":{}}}",
                started.elapsed().as_nanos()
            ),
        );
    }
    let backup = dir.join("reachable.backup.wdb");
    let started = Instant::now();
    storage::backup_file(&path, &backup, limits(LARGE_CORE)).unwrap();
    let db = Database::open_file(&backup, limits(LARGE_CORE)).unwrap();
    verify_large(&db, accepted, revision);
    event(
        "reachable_backup",
        &db,
        &backup,
        started.elapsed().as_nanos(),
        "{}",
    );
    drop(db);
    let restored = dir.join("reachable.restored.wdb");
    let started = Instant::now();
    storage::restore_file(&backup, &restored, limits(LARGE_CORE)).unwrap();
    let db = Database::open_file(&restored, limits(LARGE_CORE)).unwrap();
    verify_large(&db, accepted, revision);
    event(
        "reachable_restore",
        &db,
        &restored,
        started.elapsed().as_nanos(),
        "{}",
    );
    drop(db);
    let started = Instant::now();
    Database::compact_file(&path, limits(LARGE_CORE)).unwrap();
    let db = Database::open_file(&path, limits(LARGE_CORE)).unwrap();
    verify_large(&db, accepted, revision);
    event(
        "reachable_compact",
        &db,
        &path,
        started.elapsed().as_nanos(),
        "{}",
    );
}
fn stack_case(dir: &Path) {
    fs::create_dir(dir).unwrap();
    let dir = dir.to_path_buf();
    std::thread::Builder::new().name("depth64-1MiB".into()).stack_size(MIB).spawn(move || {
        let path = dir.join("depth.wdb");
        let db = Database::create_file(&path, limits(CORE)).unwrap();
        let c = db.create_collection().unwrap();
        let value = nested(&db, 1);
        let mut tx = db.begin();
        tx.put(c,b"nested",&value).unwrap();
        tx.commit().unwrap();
        drop(tx);
        db.retain_snapshot(&db.snapshot()).unwrap();
        drop(value);
        drop(db);
        let logical = storage::export_logical_file(&path, limits(CORE)).unwrap();
        let target = storage::MemoryStore::new().unwrap();
        assert_eq!(storage::import_logical_on(&logical,target.clone(),Limits { max_depth: 63,..limits(CORE) }),Err(Error::Limit));
        storage::import_logical_on(&logical,target.clone(),limits(CORE)).unwrap();
        let decoded = Database::open_on(target,limits(CORE)).unwrap();
        check_root(&decoded,decoded.snapshot(),1);
        check_root(&decoded,decoded.retained_snapshot(1).unwrap(),1);
        drop(decoded);
        let reopened = Database::open_file(&path,limits(CORE)).unwrap();
        check_root(&reopened,reopened.snapshot(),1);
        event("bounded_stack_depth64",&reopened,&path,0,"{\"requested_stack_bytes\":1048576,\"valid_depth\":64,\"rejected_wrapper_depth\":65,\"parser_rejection_limit\":63}");
    }).unwrap().join().unwrap();
}
fn main() {
    assert_eq!(weekend_db::version::FEATURES, 0);
    assert_eq!(
        weekend_db::version::SOURCE_ID,
        std::env::var("WEEKEND_DB_SOURCE_ID").unwrap()
    );
    let args: Vec<_> = std::env::args_os().collect();
    assert_eq!(args.len(), 2, "usage: resource-envelope NEW_DIRECTORY");
    let dir = Path::new(&args[1]);
    if std::env::var_os("WEEKEND_RESOURCE_STACK_CHILD").is_some() {
        stack_case(dir);
        return;
    }
    fs::create_dir(dir).expect("output must not exist");
    nested_case(dir);
    large_case(dir);
    // A dedicated process isolates any stack overflow from qualification output.
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .env("WEEKEND_RESOURCE_STACK_CHILD", "1")
        .arg(dir.join("stack-worker"))
        .status();
    assert!(status.unwrap().success());
    println!("{{\"complete\":true,\"cases\":4}}");
}
