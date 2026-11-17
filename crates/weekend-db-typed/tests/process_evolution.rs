#![cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::{path::Path, process::Command};
use weekend_db_typed::{
    Error, Model, Store,
    engine::{self, storage},
};

#[derive(Model)]
#[weekend(type_id = 81, version = 1)]
struct Old {
    #[weekend(id = 1, unique)]
    slug: String,
}
#[derive(Model)]
#[weekend(type_id = 81, version = 2)]
struct New {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2, default = 0)]
    score: u64,
}

fn verify(store: &mut Store, versions: [u64; 2], scores: [u64; 2]) {
    let table = store.table::<New>("rows").unwrap();
    store
        .read(|read| {
            for id in 1..=2 {
                let row = read.get(&table, id)?.unwrap();
                assert_eq!(row.version(), versions[id as usize - 1]);
                assert_eq!(row.score, scores[id as usize - 1]);
                assert_eq!(
                    read.find(&table, New::SLUG, &format!("row-{id}"))?
                        .unwrap()
                        .id(),
                    id
                );
                assert_eq!(row.record().field(99)?.unwrap().as_bytes()?, b"future");
            }
            assert!(read.check_indexes(&table)?.is_consistent());
            Ok(())
        })
        .unwrap();
    assert_eq!(store.history().unwrap(), vec![1]);
    store
        .read_retained(1, |read| {
            for id in 1..=2 {
                let row = read.find(&table, New::SLUG, &format!("row-{id}"))?.unwrap();
                assert_eq!(row.version(), 1);
                assert_eq!(row.score, 0);
                assert_eq!(row.record().field(99)?.unwrap().as_bytes()?, b"future");
            }
            assert!(read.check_indexes(&table)?.is_consistent());
            Ok(())
        })
        .unwrap();
}

#[test]
fn evolution_worker() {
    let Some(mode) = std::env::var_os("WEEKEND_EVOLUTION_MODE") else {
        return;
    };
    let path = std::path::PathBuf::from(std::env::var_os("WEEKEND_EVOLUTION_PATH").unwrap());
    match mode.to_str().unwrap() {
        "create" => {
            let mut store = Store::create_file(&path).unwrap();
            let table = store.table::<Old>("rows").unwrap();
            store
                .write(|write| {
                    for id in 1..=2 {
                        write.insert(
                            &table,
                            id,
                            &Old {
                                slug: format!("row-{id}"),
                            },
                        )?;
                    }
                    Ok(())
                })
                .unwrap();
            // A newer application added a field unknown to both compiled models.
            for id in 1..=2u64 {
                let raw = store
                    .read(|read| Ok(read.get(&table, id)?.unwrap().record().clone()))
                    .unwrap();
                let db = store.database();
                let raw = raw.set(99, &db.bytes(b"future").unwrap()).unwrap();
                let mut tx = db.begin();
                tx.put(table.collection(), &id.to_le_bytes(), raw.as_value())
                    .unwrap();
                tx.commit().unwrap();
            }
            assert_eq!(store.checkpoint().unwrap(), 1);
        }
        "migrate" => {
            let mut store = Store::open_file(&path).unwrap();
            let table = store.table::<New>("rows").unwrap();
            assert_eq!(
                store.write(|w| w.migrate_ids(&table, &[1], |_, _, row| {
                    row.score = 8;
                    Ok(())
                })),
                Ok(1)
            );
            verify(&mut store, [2, 1], [8, 0]);
        }
        "verify" => verify(&mut Store::open_file(&path).unwrap(), [2, 1], [8, 0]),
        "backup" => {
            storage::backup_file(
                &path,
                path.with_extension("backup"),
                engine::Limits::default(),
            )
            .unwrap();
        }
        "restore" => {
            storage::restore_file(
                path.with_extension("backup"),
                path.with_extension("restored"),
                engine::Limits::default(),
            )
            .unwrap();
        }
        "rollback" => {
            let mut store = Store::open_file(&path).unwrap();
            let table = store.table::<New>("rows").unwrap();
            store.restore_table(&table, 1).unwrap();
            verify(&mut store, [1, 1], [0, 0]);
            let before = store.database().snapshot().revision();
            assert_eq!(
                store.write(|w| w.update(&table, 1, |row| {
                    row.score = 99;
                    Ok(())
                })),
                Err(Error::MigrationRequired)
            );
            assert_eq!(store.database().snapshot().revision(), before);
        }
        "remigrate" => {
            let mut store = Store::open_file(&path).unwrap();
            verify(&mut store, [1, 1], [0, 0]);
            let table = store.table::<New>("rows").unwrap();
            assert_eq!(
                store.write(|w| w.migrate(&table, |_, row| {
                    row.score = 9;
                    Ok(())
                })),
                Ok(2)
            );
            verify(&mut store, [2, 2], [9, 9]);
        }
        "reject" => assert!(matches!(
            Store::open_file(&path),
            Err(Error::Engine(engine::Error::UnsupportedFormat))
        )),
        _ => panic!("invalid child mode"),
    }
}

fn child(mode: &str, path: &Path) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "evolution_worker", "--nocapture"])
        .env("WEEKEND_EVOLUTION_MODE", mode)
        .env("WEEKEND_EVOLUTION_PATH", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{mode}: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn typed_backup_restore_and_mixed_version_rollback_cross_processes() {
    let root = std::env::temp_dir().join(format!("weekend-evolution-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("source.wdb");
    for phase in ["create", "migrate", "verify", "backup", "restore"] {
        child(phase, &path);
    }
    let restored = path.with_extension("restored");
    child("verify", &restored);
    child("rollback", &restored);
    child("remigrate", &restored);
    // Table rollback on the restored copy never rewinds the backup or source.
    child("verify", &path);
    child("verify", &path.with_extension("backup"));
    let unsupported = root.join("future.wdb");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&unsupported, &bytes).unwrap();
    child("reject", &unsupported);
    assert_eq!(std::fs::read(&unsupported).unwrap(), bytes);
    std::fs::remove_dir_all(root).unwrap();
}
