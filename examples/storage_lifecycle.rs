//! Native checkpoint-heavy lifecycle measurement, not an application latency SLA.
//! Requires a new output directory and checks every current/retained payload.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn main() -> weekend_db::Result<()> {
    use std::{fs, path::PathBuf, time::Instant};
    use weekend_db::{Database, Error, Limits, MAX_RETAINED_SNAPSHOTS, storage};

    const PAYLOAD: usize = 256 * 1024;
    fn verify(db: &Database, last: u64, retained: &[(u64, u64)]) -> weekend_db::Result<()> {
        let check = |snapshot: weekend_db::Snapshot<'_>, value: u64| -> weekend_db::Result<()> {
            let row = snapshot.get(db.collection_id(1), b"checkpoint")?.unwrap();
            let bytes = row.as_bytes()?;
            assert_eq!(bytes.len(), PAYLOAD);
            assert_eq!(&bytes[..8], &value.to_le_bytes());
            assert!(bytes[8..].iter().all(|b| *b == value as u8));
            Ok(())
        };
        check(db.snapshot(), last)?;
        assert_eq!(
            db.retained_ids()?,
            retained.iter().map(|r| r.0).collect::<Vec<_>>()
        );
        for &(id, value) in retained {
            check(db.retained_snapshot(id)?, value)?;
        }
        Ok(())
    }
    fn report(name: &str, samples: &mut [u128], bytes: u64) {
        assert!(!samples.is_empty());
        samples.sort_unstable();
        let percentile = |p: usize| samples[(samples.len() * p).div_ceil(100).max(1) - 1];
        println!(
            "{name},{},{},{},{},{bytes}",
            samples.len(),
            percentile(50),
            percentile(95),
            percentile(99)
        );
    }
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("provide a new output directory"),
    );
    fs::create_dir(&directory).map_err(|_| Error::Io)?;
    let path = directory.join("live.wdb");
    let limits = Limits::default();
    let db = Database::create_file(&path, limits)?;
    let c = db.create_collection()?;
    let mut retained = Vec::new();
    let mut commits = Vec::new();
    let mut retains = Vec::new();
    let mut growth = Vec::new();
    let mut sequence = 0u64;
    println!("case,samples,p50_ns,p95_ns,p99_ns,file_bytes");
    loop {
        let before = db.storage_space()?.physical_bytes;
        let mut bytes = vec![sequence as u8; PAYLOAD];
        bytes[..8].copy_from_slice(&sequence.to_le_bytes());
        let value = db.bytes(&bytes)?;
        let mut tx = db.begin();
        tx.put(c, b"checkpoint", &value)?;
        let start = Instant::now();
        tx.commit()?;
        commits.push(start.elapsed().as_nanos());
        growth.push(db.storage_space()?.physical_bytes - before);
        if retained.len() < MAX_RETAINED_SNAPSHOTS {
            let start = Instant::now();
            let id = db.retain_snapshot(&db.snapshot())?;
            retains.push(start.elapsed().as_nanos());
            retained.push((id, sequence));
        }
        if db.storage_space()?.remaining_append_bytes < 1024 * 1024 {
            break;
        }
        sequence += 1;
        assert!(sequence < 1024, "workload must stay bounded");
    }
    let space = db.storage_space()?;
    assert_eq!(retained.len(), MAX_RETAINED_SNAPSHOTS);
    assert!(space.physical_bytes > storage::MAX_IMAGE_BYTES as u64 * 98 / 100);
    verify(&db, sequence, &retained)?;
    let revision = db.snapshot().revision();
    report("commit_256k", &mut commits, space.physical_bytes);
    report("retain", &mut retains, space.physical_bytes);
    growth.sort_unstable();
    println!(
        "append_bytes_per_commit,{},{},{},{},{}",
        growth.len(),
        growth[growth.len() / 2],
        growth[(growth.len() * 95).div_ceil(100) - 1],
        growth[(growth.len() * 99).div_ceil(100) - 1],
        space.physical_bytes
    );
    drop(db);

    let mut reopen = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let db = Database::open_file(&path, limits)?;
        reopen.push(start.elapsed().as_nanos());
        assert_eq!(db.snapshot().revision(), revision);
        verify(&db, sequence, &retained)?;
    }
    report("reopen_full_history", &mut reopen, space.physical_bytes);

    let backup = directory.join("backup.wdb");
    let start = Instant::now();
    let stats = storage::backup_file(&path, &backup, limits)?;
    report(
        "verified_backup",
        &mut [start.elapsed().as_nanos()],
        stats.after_bytes,
    );
    let restored = directory.join("restored.wdb");
    let start = Instant::now();
    let stats = storage::restore_file(&backup, &restored, limits)?;
    report(
        "verified_restore",
        &mut [start.elapsed().as_nanos()],
        stats.after_bytes,
    );
    let restored_db = Database::open_file(&restored, limits)?;
    assert_eq!(restored_db.snapshot().revision(), revision);
    verify(&restored_db, sequence, &retained)?;
    drop(restored_db);

    let mut db = Database::open_file(&path, limits)?;
    for &(id, _) in &retained[..32] {
        db.release_retained(id)?;
    }
    retained.drain(..32);
    let compact = directory.join("compact.wdb");
    let start = Instant::now();
    let stats = db.compact_to(storage::FileStore::create(&compact)?)?;
    report(
        "verified_compact_after_release32",
        &mut [start.elapsed().as_nanos()],
        stats.after_bytes,
    );
    drop(db);
    let db = Database::open_file(&compact, limits)?;
    assert_eq!(db.snapshot().revision(), revision + 32);
    verify(&db, sequence, &retained)?;
    assert!(db.storage_space()?.remaining_append_bytes > space.remaining_append_bytes);
    db.begin().commit()?;
    drop(db);
    let db = Database::open_file(&compact, limits)?;
    assert_eq!(db.snapshot().revision(), revision + 33);
    verify(&db, sequence, &retained)?;
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn main() {
    panic!("native file adapter required");
}
