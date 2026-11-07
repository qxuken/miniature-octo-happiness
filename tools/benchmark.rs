//! Small reproducible workloads, not a comparative database benchmark.
use std::{hint::black_box, time::Instant};
use weekend_db::{
    Database, Error, Result,
    storage::{ByteStore, MemoryStore},
};

fn timed(name: &str, count: usize, mut run: impl FnMut(usize) -> Result<()>) -> Result<()> {
    let start = Instant::now();
    for i in 0..count {
        run(i)?;
    }
    println!("{name},{count},{},0", start.elapsed().as_nanos());
    Ok(())
}
fn seed(db: &Database) -> Result<weekend_db::CollectionId> {
    let c = db.create_collection()?;
    let mut tx = db.begin();
    for i in 0..32u64 {
        tx.put(c, &i.to_le_bytes(), &db.unsigned(i)?)?;
    }
    tx.commit()?;
    Ok(c)
}
fn main() -> Result<()> {
    println!("case,operations,elapsed_ns,file_bytes");
    let db = Database::new()?;
    let c = seed(&db)?;
    let snapshot = db.snapshot();
    timed("memory_point_read", 100_000, |i| {
        black_box(snapshot.get(c, &(i as u64 % 32).to_le_bytes())?.unwrap());
        Ok(())
    })?;
    let mut nested = db
        .map()?
        .map_set(b"child", &db.map()?.map_set(b"n", &db.unsigned(0)?)?)?;
    timed("memory_nested_edit", 10_000, |i| {
        let child = nested
            .map_get(b"child")?
            .unwrap()
            .map_set(b"n", &db.unsigned(i as u64)?)?;
        nested = black_box(nested.map_set(b"child", &child)?);
        Ok(())
    })?;
    let mut list = db.list()?;
    for i in 0..256 {
        list = list.list_push(&db.unsigned(i)?)?;
    }
    let item = db.unsigned(1)?;
    timed("memory_list_middle_pair", 1000, |_| {
        list = black_box(list.list_insert(128, &item)?.list_remove(128)?);
        Ok(())
    })?;
    timed("memory_batch16_commit", 500, |i| {
        let mut tx = db.begin();
        let value = db.unsigned(i as u64)?;
        for k in 0..16u64 {
            tx.put(c, &k.to_le_bytes(), &value)?;
        }
        black_box(tx.commit()?);
        Ok(())
    })?;
    timed("memory_conflict_pair", 2000, |i| {
        let mut a = db.begin();
        let mut b = db.begin();
        for tx in [&mut a, &mut b] {
            tx.put(c, &0u64.to_le_bytes(), &db.unsigned(i as u64)?)?;
        }
        a.commit()?;
        assert_eq!(b.commit(), Err(Error::Conflict));
        Ok(())
    })?;
    let store = MemoryStore::new()?;
    let db = Database::create_on(store.clone(), *b"benchmark-000001", db.limits())?;
    let c = seed(&db)?;
    timed("codec_memory_commit", 64, |i| {
        let mut tx = db.begin();
        tx.put(c, &0u64.to_le_bytes(), &db.unsigned(i as u64)?)?;
        black_box(tx.commit()?);
        Ok(())
    })?;
    drop(db);
    let mut store = store;
    store.lock()?;
    println!("codec_memory_image,0,0,{}", store.length()?);
    store.unlock();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    files()?;
    Ok(())
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn files() -> Result<()> {
    use std::{fs, path::PathBuf};
    use weekend_db::Limits;
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("benchmark requires a new output directory"),
    );
    fs::create_dir(&directory).map_err(|_| Error::Io)?;
    let path = directory.join("latency.db");
    let db = Database::create_file(&path, Limits::default())?;
    let c = seed(&db)?;
    timed("file_fullsync_commit", 64, |i| {
        let mut tx = db.begin();
        tx.put(c, &0u64.to_le_bytes(), &db.unsigned(i as u64)?)?;
        black_box(tx.commit()?);
        Ok(())
    })?;
    drop(db);
    let db = Database::create_file(directory.join("batch.db"), Limits::default())?;
    let c = seed(&db)?;
    timed("file_fullsync_batch16_commit", 64, |i| {
        let mut tx = db.begin();
        let value = db.unsigned(i as u64)?;
        for k in 0..16u64 {
            tx.put(c, &k.to_le_bytes(), &value)?;
        }
        black_box(tx.commit()?);
        Ok(())
    })?;
    drop(db);
    let path = directory.join("history.db");
    let db = Database::create_file(&path, Limits::default())?;
    let c = db.create_collection()?;
    let mut payload = [0u8; 8192];
    timed("file_retained_history_commit", 128, |i| {
        payload[..8].copy_from_slice(&(i as u64).to_le_bytes());
        let value = db.map()?.map_set(b"blob", &db.bytes(&payload)?)?;
        let mut tx = db.begin();
        tx.put(c, b"record", &value)?;
        tx.commit()?;
        if i == 0 || i == 64 {
            db.retain_snapshot(&db.snapshot())?;
        }
        Ok(())
    })?;
    timed("file_head_read", 10_000, |_| {
        black_box(db.snapshot().get(c, b"record")?.unwrap().len()?);
        Ok(())
    })?;
    drop(db);
    let size = fs::metadata(&path).map_err(|_| Error::Io)?.len();
    println!("file_retained_history_image,0,0,{size}");
    let reopen = |_| {
        let db = Database::open_file(&path, Limits::default())?;
        assert_eq!(db.retained_ids()?, vec![1, 2]);
        for id in [1, 2] {
            black_box(db.retained_snapshot(id)?.scan(db.collection_id(1))?);
        }
        black_box(db.snapshot());
        Ok(())
    };
    timed("file_history_reopen", 3, reopen)?;
    if std::env::args().nth(2).as_deref() == Some("--compact") {
        let start = Instant::now();
        let stats = Database::compact_file(&path, Limits::default())?;
        println!(
            "file_compaction,1,{},{}",
            start.elapsed().as_nanos(),
            stats.after_bytes
        );
        timed("file_compacted_reopen", 3, reopen)?;
        let db = Database::open_file(&path, Limits::default())?;
        let c = db.collection_id(1);
        timed("file_compacted_head_read", 10_000, |_| {
            black_box(db.snapshot().get(c, b"record")?.unwrap().len()?);
            Ok(())
        })?;
        drop(db);
        let db = Database::open_file(&path, Limits::default())?;
        for id in [1, 2] {
            db.release_retained(id)?;
        }
        drop(db);
        let start = Instant::now();
        let stats = Database::compact_file(&path, Limits::default())?;
        println!(
            "file_compaction_without_retention,1,{},{}",
            start.elapsed().as_nanos(),
            stats.after_bytes
        );
    }
    Ok(())
}
