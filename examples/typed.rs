#[path = "support/users.rs"]
mod users;
use users::{Age, Nickname, NicknameUpdate, Users};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use weekend_db::Limits;
use weekend_db::{Database, schema};

fn main() -> schema::Result<()> {
    let path = std::env::args_os().nth(1);
    let db = if let Some(path) = &path {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            Database::create_file(path, Limits::default())?
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = path;
            return Err(schema::Error::Engine(
                weekend_db::Error::UnsupportedPlatform,
            ));
        }
    } else {
        Database::new()?
    };
    let users = Users::create(&db)?;
    let ids = users.ids();
    let mut tx = db.begin();
    users.install_v1(&db, &mut tx)?;
    users.insert_v1(&db, &mut tx, 42, "Alice", "alice@example.test", 25)?;
    tx.commit()?;
    drop(tx);
    let original = db.snapshot();
    let original_id = db.retain_snapshot(&original)?;
    let mut tx = db.begin();
    assert_eq!(users.migrate_to_v2(&db, &mut tx)?, 1);
    tx.commit()?;
    drop(tx);
    let mut tx = db.begin();
    users.set_nickname(&db, &mut tx, 42, NicknameUpdate::Text("Al"))?;
    tx.commit()?;
    drop(tx);
    let mut tx = db.begin();
    users.set_name_v1(&db, &mut tx, 42, "Alicia")?;
    tx.commit()?;
    drop(tx);
    let mut current = db.snapshot();
    let user = users.load(&mut current, 42)?.unwrap();
    assert_eq!(user.nickname(), Nickname::Text("Al"));
    assert_eq!(
        user.schema().field(users::NAME)?.unwrap().name(),
        "display_name"
    );
    let additive_id = db.retain_snapshot(&current)?;
    let mut tx = db.begin();
    assert_eq!(users.migrate_to_v3(&db, &mut tx)?, 1);
    users.set_email(&db, &mut tx, 42, "alicia@example.test")?;
    tx.commit()?;
    drop(tx);
    let mut now = db.snapshot();
    let current = users
        .find_by_email(&mut now, "alicia@example.test")?
        .unwrap();
    assert_eq!(current.age(), Age::Months(300));
    let mut original = original;
    let old = users.load(&mut original, 42)?.unwrap();
    assert_eq!(old.display_name(), "Alice");
    assert_eq!(old.age(), Age::Years(25));
    assert_eq!(old.schema().field(users::NAME)?.unwrap().name(), "name");
    drop(original);
    drop(now);
    drop(db);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(path) = path {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            Database::compact_file(&path, Limits::default())?;
            let db = Database::open_file(path, Limits::default())?;
            let users = Users::from_ids(&db, ids);
            let mut old = db.retained_snapshot(original_id)?;
            assert_eq!(
                users
                    .find_by_email(&mut old, "alice@example.test")?
                    .unwrap()
                    .version(),
                1
            );
            let mut additive = db.retained_snapshot(additive_id)?;
            let user = users.load(&mut additive, 42)?.unwrap();
            assert_eq!(user.version(), 2);
            assert_eq!(user.nickname(), Nickname::Text("Al"));
            let mut current = db.snapshot();
            assert_eq!(
                users
                    .find_by_email(&mut current, "alicia@example.test")?
                    .unwrap()
                    .age(),
                Age::Months(300)
            );
        }
    }
    let _ = (ids, original_id, additive_id);
    println!(
        "Typed users: stable IDs, preserved nickname, explicit years-to-months migration, historical schemas and email lookup passed."
    );
    Ok(())
}
