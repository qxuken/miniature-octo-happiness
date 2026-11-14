//! Private durable retained-history comparator. run.py supplies a fresh directory.
use std::{fs, path::Path, time::Instant};
use weekend_db::{Database, Limits, Snapshot, storage};

#[derive(Clone, Copy)]
struct Work {
    commits: u64,
    keys: u64,
    batch: u64,
    payload: usize,
    retain_every: u64,
    reopens: usize,
}
impl Work {
    fn pins(self) -> Vec<u64> {
        (1..=self.commits)
            .filter(|n| n % self.retain_every == 0 || *n == self.commits)
            .collect()
    }
    fn payload(self, key: u64, epoch: u64) -> Vec<u8> {
        let mut bytes: Vec<_> = (0..self.payload)
            .map(|i| ((epoch * 17 + key * 31 + i as u64) % 251) as u8)
            .collect();
        bytes[..8].copy_from_slice(&epoch.to_le_bytes());
        bytes[8..16].copy_from_slice(&key.to_le_bytes());
        bytes
    }
    fn last(self, key: u64, epoch: u64) -> u64 {
        (1..=epoch)
            .rev()
            .find(|n| (0..self.batch).any(|j| (n * self.batch + j) % self.keys == key))
            .unwrap_or(0)
    }
}
fn feed(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
    }
}
fn verify_snapshot(
    work: Work,
    snapshot: Snapshot<'_>,
    collection: weekend_db::CollectionId,
    epoch: u64,
    hash: &mut u64,
) {
    feed(hash, &epoch.to_le_bytes());
    let rows = snapshot.scan(collection).unwrap();
    assert_eq!(rows.len(), work.keys as usize);
    // Keys are little-endian binary strings; compare by key value, not row order.
    let mut observed = std::collections::BTreeMap::new();
    for (key, value) in rows {
        let key = u64::from_le_bytes(key.try_into().expect("8-byte key"));
        assert!(
            observed
                .insert(key, value.as_bytes().unwrap().to_vec())
                .is_none()
        );
    }
    for key in 0..work.keys {
        let bytes = observed.remove(&key).expect("every expected key");
        assert_eq!(bytes, work.payload(key, work.last(key, epoch)));
        feed(hash, &key.to_le_bytes());
        feed(hash, &bytes);
    }
    assert!(observed.is_empty());
}

fn verify(db: &Database, work: Work) -> u64 {
    let pins = work.pins();
    assert_eq!(
        db.retained_ids().unwrap(),
        (1..=pins.len() as u64).collect::<Vec<_>>()
    );
    assert_eq!(
        db.snapshot().revision(),
        2 + work.commits + pins.len() as u64
    );
    let mut hash = 14_695_981_039_346_656_037;
    verify_snapshot(
        work,
        db.snapshot(),
        db.collection_id(1),
        work.commits,
        &mut hash,
    );
    for (i, epoch) in pins.into_iter().enumerate() {
        let snapshot = db.retained_snapshot(i as u64 + 1).unwrap();
        assert_eq!(snapshot.revision(), 2 + epoch + i as u64);
        verify_snapshot(work, snapshot, db.collection_id(1), epoch, &mut hash);
    }
    hash
}
fn verify_path(path: &Path, work: Work, expected: u64) {
    assert_eq!(
        verify(&Database::open_file(path, Limits::default()).unwrap(), work),
        expected
    );
}
fn size(path: &Path) -> u64 {
    fs::metadata(path).unwrap().len()
}
fn main() {
    assert_eq!(weekend_db::version::FEATURES, 0);
    assert_eq!(
        weekend_db::version::SOURCE_ID,
        std::env::var("WEEKEND_DB_SOURCE_ID").expect("explicit runtime source identity")
    );
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 8);
    let work = Work {
        commits: args[2].parse().unwrap(),
        keys: args[3].parse().unwrap(),
        batch: args[4].parse().unwrap(),
        payload: args[5].parse().unwrap(),
        retain_every: args[6].parse().unwrap(),
        reopens: args[7].parse().unwrap(),
    };
    assert!((1..=512).contains(&work.commits));
    assert!((1..=256).contains(&work.keys));
    assert!(work.batch > 0 && work.batch <= work.keys);
    assert!((16..=8192).contains(&work.payload));
    assert!(work.keys * work.payload as u64 <= 1024 * 1024);
    assert!(work.commits * work.batch * work.payload as u64 <= 64 * 1024 * 1024);
    assert!(work.retain_every > 0 && work.pins().len() <= 64);
    assert!((1..=128).contains(&work.reopens));
    let directory = Path::new(&args[1]);
    fs::create_dir(directory).expect("output must not exist");
    let path = directory.join("main.wdb");
    let db = Database::create_file(&path, Limits::default()).unwrap();
    let collection = db.create_collection().unwrap();
    let mut seed = db.begin();
    for key in 0..work.keys {
        seed.put(
            collection,
            &key.to_le_bytes(),
            &db.bytes(&work.payload(key, 0)).unwrap(),
        )
        .unwrap();
    }
    seed.commit().unwrap();
    drop(seed);
    let mut commit_ns = Vec::new();
    let mut transaction_ns = Vec::new();
    let mut pin_ns = Vec::new();
    for epoch in 1..=work.commits {
        let writes: Vec<_> = (0..work.batch)
            .map(|j| {
                let key = (epoch * work.batch + j) % work.keys;
                (key.to_le_bytes(), work.payload(key, epoch))
            })
            .collect();
        let started = Instant::now();
        let mut tx = db.begin();
        for (key, bytes) in writes {
            tx.put(collection, &key, &db.bytes(&bytes).unwrap())
                .unwrap();
        }
        let commit = Instant::now();
        tx.commit().unwrap();
        commit_ns.push(commit.elapsed().as_nanos());
        transaction_ns.push(started.elapsed().as_nanos());
        if epoch % work.retain_every == 0 || epoch == work.commits {
            let started = Instant::now();
            db.retain_snapshot(&db.snapshot()).unwrap();
            pin_ns.push(started.elapsed().as_nanos());
        }
    }
    let digest = verify(&db, work);
    let bytes_live = size(&path);
    drop(db);
    let bytes_closed = size(&path);
    let mut reopen_ns = Vec::new();
    let mut reopen_verify_ns = Vec::new();
    for _ in 0..work.reopens {
        let started = Instant::now();
        let db = Database::open_file(&path, Limits::default()).unwrap();
        assert_eq!(db.retained_ids().unwrap().len(), work.pins().len());
        reopen_ns.push(started.elapsed().as_nanos());
        assert_eq!(verify(&db, work), digest);
        reopen_verify_ns.push(started.elapsed().as_nanos());
    }
    let backup = directory.join("backup.wdb");
    let started = Instant::now();
    verify_path(&path, work, digest);
    storage::backup_file(&path, &backup, Limits::default()).unwrap();
    verify_path(&backup, work, digest);
    let backup_ns = started.elapsed().as_nanos();
    let restored = directory.join("restored.wdb");
    let started = Instant::now();
    verify_path(&backup, work, digest);
    storage::restore_file(&backup, &restored, Limits::default()).unwrap();
    verify_path(&restored, work, digest);
    let restore_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    Database::compact_file(&path, Limits::default()).unwrap();
    verify_path(&path, work, digest);
    let compact_ns = started.elapsed().as_nanos();
    println!(
        "{{\"commit_ns\":{commit_ns:?},\"transaction_ns\":{transaction_ns:?},\"pin_ns\":{pin_ns:?},\"reopen_ns\":{reopen_ns:?},\"reopen_verify_ns\":{reopen_verify_ns:?},\"backup_ns\":{backup_ns},\"restore_ns\":{restore_ns},\"compact_ns\":{compact_ns},\"oracle_digest\":{digest},\"history_count\":{},\"bytes_live\":{bytes_live},\"bytes_closed\":{bytes_closed},\"bytes_compacted\":{},\"backup_bytes\":{},\"restored_bytes\":{}}}",
        work.pins().len(),
        size(&path),
        size(&backup),
        size(&restored)
    );
}
