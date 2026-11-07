use weekend_db::{Database, Error, Result};

fn main() -> Result<()> {
    let db = Database::new()?;
    let tasks = db.create_collection()?; // task ID bytes -> task map
    let by_slug = db.create_collection()?; // slug bytes -> unsigned task ID
    let id = 1u64;
    let key = id.to_le_bytes();
    let slug = b"learn";

    let task = db
        .map()?
        .map_set(b"slug", &db.bytes(slug)?)?
        .map_set(b"title", &db.bytes(b"Build something")?)?
        .map_set(b"done", &db.boolean(false)?)?;

    let mut insert = db.begin();
    if insert.get(tasks, &key)?.is_some() || insert.get(by_slug, slug)?.is_some() {
        return Err(Error::AlreadyExists);
    }
    insert.put(tasks, &key, &task)?;
    insert.put(by_slug, slug, &db.unsigned(id)?)?;
    insert.commit()?;

    let before = db.retain_snapshot(&db.snapshot())?;

    // Editing done leaves the slug index valid.
    let mut edit = db.begin();
    let task = edit.get(tasks, &key)?.ok_or(Error::NotFound)?;
    edit.put(tasks, &key, &task.map_set(b"done", &db.boolean(true)?)?)?;
    edit.commit()?;

    let now = db.snapshot();
    let found = now.get(by_slug, slug)?.ok_or(Error::NotFound)?.as_u64()?;
    let task = now
        .get(tasks, &found.to_le_bytes())?
        .ok_or(Error::NotFound)?;
    assert!(task.map_get(b"done")?.ok_or(Error::NotFound)?.as_bool()?);

    // Both versions have the same slug, so restoring the task keeps its index valid.
    let old = db.retained_snapshot(before)?;
    let old_task = old.get(tasks, &key)?.ok_or(Error::NotFound)?;
    let mut restore = db.begin();
    restore.put(tasks, &key, &old_task)?;
    restore.commit()?;

    let task = db.snapshot().get(tasks, &key)?.ok_or(Error::NotFound)?;
    assert!(!task.map_get(b"done")?.ok_or(Error::NotFound)?.as_bool()?);
    println!("Indexed lookup found task {found}. Restored done=false from checkpoint {before}.");
    Ok(())
}
