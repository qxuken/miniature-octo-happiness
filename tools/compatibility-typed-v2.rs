//! Successor application. Only the version 2 model is compiled here.
use std::{io::Write as _, path::Path};
use weekend_db_typed::{
    Error, Model, Store, Table,
    engine::{self, Limits, storage},
};

#[derive(Model)]
#[weekend(type_id = 81, version = 2)]
struct Task {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2, default = 0)]
    score: u64,
}

fn verify(store: &Store, table: &Table<Task>, current: [(u64, &str, u64, u64); 2]) {
    let check = |read: &mut weekend_db_typed::Read<'_>, records: [(u64, &str, u64, u64); 2]| {
        for (id, slug, version, score) in records {
            let doc = read.find(table, Task::SLUG, slug)?.unwrap();
            assert_eq!((doc.id(), doc.version(), doc.score), (id, version, score));
            assert_eq!(doc.slug, slug);
            assert_eq!(doc.record().field(99)?.unwrap().as_bytes()?, b"future");
            assert_eq!(doc.record().field(2)?.is_some(), version == 2);
        }
        assert_eq!(read.list(table)?.len(), 2);
        assert!(read.check_indexes(table)?.is_consistent());
        Ok(())
    };
    store.read(|r| check(r, current)).unwrap();
    assert_eq!(store.history().unwrap(), vec![1]);
    store.read_retained(1, |r| check(r, ORIGINAL)).unwrap();
}

const ORIGINAL: [(u64, &str, u64, u64); 2] = [(1, "alpha", 1, 0), (2, "beta", 1, 0)];
const HALF: [(u64, &str, u64, u64); 2] = [(1, "alpha-legacy-edit", 2, 8), (2, "beta", 1, 0)];
const COMPLETE: [(u64, &str, u64, u64); 2] = [(1, "alpha-legacy-edit", 2, 8), (2, "beta", 2, 9)];

fn upgrade(phase: &str, source: &Path) {
    let mut store = Store::open_file(source).unwrap();
    let before = store.database().snapshot().revision();
    let table = store.table::<Task>("tasks").unwrap();
    let registered = store.database().snapshot().revision();
    assert_eq!(registered, before + u64::from(phase == "register-v2"));
    match phase {
        "register-v2" | "verify-interruption" => {
            verify(&store, &table, ORIGINAL);
            assert_eq!(
                store.write(|write| write.update(&table, 1, |_| panic!("migration is required"))),
                Err(Error::MigrationRequired)
            );
        }
        "interrupt-migration" => {
            store
                .write::<()>(|write| {
                    assert_eq!(
                        write.migrate_ids(&table, &[1], |_, _, task| {
                            task.slug = "uncommitted".into();
                            task.score = 81;
                            Ok(())
                        })?,
                        1
                    );
                    let staged = write.find(&table, Task::SLUG, "uncommitted")?.unwrap();
                    assert_eq!((staged.id(), staged.version(), staged.score), (1, 2, 81));
                    println!("migration-staged");
                    std::io::stdout().flush().unwrap();
                    std::process::exit(70);
                })
                .unwrap();
            unreachable!();
        }
        "migrate-first" => {
            // The first conversion is staged before the unknown ID aborts the batch.
            let mut callbacks = 0;
            assert_eq!(
                store.write(|write| write.migrate_ids(&table, &[1, 404], |_, _, task| {
                    callbacks += 1;
                    task.score = 100;
                    Ok(())
                })),
                Err(Error::Engine(engine::Error::NotFound))
            );
            assert_eq!(callbacks, 1);
            assert_eq!(store.database().snapshot().revision(), registered);
            verify(&store, &table, ORIGINAL);
            assert_eq!(
                store.write(|write| write.migrate_ids(&table, &[1], |_, _, task| {
                    task.slug = "alpha-v2".into();
                    task.score = 8;
                    Ok(())
                })),
                Ok(1)
            );
            verify(&store, &table, [(1, "alpha-v2", 2, 8), (2, "beta", 1, 0)]);
        }
        "finish-migration" => {
            verify(&store, &table, HALF);
            assert_eq!(
                store.write(|write| write.migrate_ids(&table, &[1, 2], |id, _, task| {
                    assert_eq!(id, 2);
                    task.score = 9;
                    Ok(())
                })),
                Ok(1)
            );
            verify(&store, &table, COMPLETE);
        }
        "verify-v2" => verify(&store, &table, COMPLETE),
        _ => panic!("unknown successor phase"),
    }
    assert_eq!(
        store.database().snapshot().revision(),
        registered + u64::from(matches!(phase, "migrate-first" | "finish-migration"))
    );
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let phase = args[1].to_str().unwrap();
    let source = Path::new(&args[2]);
    if phase == "restore" {
        storage::restore_file(source, &args[3], Limits::default()).unwrap();
    } else {
        upgrade(phase, source);
    }
    println!("{phase}");
}
