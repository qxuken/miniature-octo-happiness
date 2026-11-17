#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn main() -> weekend_db::Result<()> {
    use weekend_db::{Database, Limits};
    let path = std::env::args_os()
        .nth(1)
        .expect("usage: cargo run --example file -- NEW_PATH");
    let db = Database::create_file(&path, Limits::default())?;
    let users = db.create_collection()?;
    let mut tx = db.begin();
    tx.put(
        users,
        b"42",
        &db.map()?.map_set(b"name", &db.bytes(b"Alice")?)?,
    )?;
    tx.commit()?;
    drop(tx);
    let old = db.snapshot();
    let id = db.retain_snapshot(&old)?;
    let mut tx = db.begin();
    let current = tx
        .get(users, b"42")?
        .unwrap()
        .map_set(b"name", &db.bytes(b"Alicia")?)?;
    tx.put(users, b"42", &current)?;
    tx.commit()?;
    drop(tx);
    drop(old);
    drop(db);
    let db = Database::open_file(&path, Limits::default())?;
    let old = db.retained_snapshot(id)?;
    assert_eq!(
        old.get(db.collection_id(1), b"42")?
            .unwrap()
            .map_get(b"name")?
            .unwrap()
            .as_bytes()?,
        b"Alice"
    );
    assert_eq!(
        db.snapshot()
            .get(db.collection_id(1), b"42")?
            .unwrap()
            .map_get(b"name")?
            .unwrap()
            .as_bytes()?,
        b"Alicia"
    );
    db.release_retained(id)?;
    assert!(db.retained_ids()?.is_empty());
    // Releasing durable retention leaves this live historical view valid.
    assert_eq!(old.count(db.collection_id(1))?, 1);
    println!("Reopened current Alicia and retained Alice; release preserved live reader.");
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn main() {
    eprintln!("The file adapter currently supports macOS, Linux, or Windows NTFS.");
}
