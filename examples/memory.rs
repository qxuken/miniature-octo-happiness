use weekend_db::{Database, Result};

fn main() -> Result<()> {
    let db = Database::new()?;
    let users = db.create_collection()?;
    let alice = db
        .map()?
        .map_set(b"name", &db.bytes(b"Alice")?)?
        .map_set(b"tags", &db.list()?.list_push(&db.bytes(b"reader")?)?)?;
    let mut seed = db.begin();
    seed.put(users, b"42", &alice)?;
    seed.put(
        users,
        b"99",
        &db.map()?.map_set(b"name", &db.bytes(b"Bob")?)?,
    )?;
    seed.commit()?;
    let historical = db.snapshot();

    let mut first = db.begin();
    let mut second = db.begin();
    let alice = first
        .get(users, b"42")?
        .unwrap()
        .map_set(b"name", &db.bytes(b"Alicia")?)?;
    let bob = second
        .get(users, b"99")?
        .unwrap()
        .map_set(b"name", &db.bytes(b"Robert")?)?;
    first.put(users, b"42", &alice)?;
    second.put(users, b"99", &bob)?;
    first.commit()?;
    second.commit()?; // Independent existing records merge.
    let old = historical
        .get(users, b"42")?
        .unwrap()
        .map_get(b"name")?
        .unwrap();
    let new = db
        .snapshot()
        .get(users, b"42")?
        .unwrap()
        .map_get(b"name")?
        .unwrap();
    assert_eq!(old.as_bytes()?, b"Alice");
    assert_eq!(new.as_bytes()?, b"Alicia");
    println!("Historical Alice, current Alicia; independent updates committed.");
    Ok(())
}
