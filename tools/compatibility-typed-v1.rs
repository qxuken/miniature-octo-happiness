//! Historical application. Only the version 1 model is compiled here.
use std::path::Path;
use weekend_db_typed::{
    Error, Model, Store, Table,
    engine::{Limits, storage},
};

#[derive(Model)]
#[weekend(type_id = 81, version = 1)]
struct Task {
    #[weekend(id = 1, unique)]
    slug: String,
}

fn verify_original(store: &Store, table: &Table<Task>) {
    for read_retained in [false, true] {
        let verify = |read: &mut weekend_db_typed::Read<'_>| {
            for (id, slug) in [(1, "alpha"), (2, "beta")] {
                let doc = read.find(table, Task::SLUG, slug)?.unwrap();
                assert_eq!((doc.id(), doc.version()), (id, 1));
                assert_eq!(doc.slug, slug);
                assert_eq!(doc.record().field(99)?.unwrap().as_bytes()?, b"future");
            }
            assert!(read.check_indexes(table)?.is_consistent());
            Ok(())
        };
        if read_retained {
            store.read_retained(1, verify).unwrap();
        } else {
            store.read(verify).unwrap();
        }
    }
    assert_eq!(store.history().unwrap(), vec![1]);
}

fn create(path: &Path) {
    let mut store = Store::create_file(path).unwrap();
    let table = store.table::<Task>("tasks").unwrap();
    store
        .write(|write| {
            for (id, slug) in [(1, "alpha"), (2, "beta")] {
                write.insert(&table, id, &Task { slug: slug.into() })?;
            }
            Ok(())
        })
        .unwrap();
    let records = [1, 2].map(|id| {
        store
            .read(|read| Ok(read.get(&table, id)?.unwrap().record().clone()))
            .unwrap()
    });
    let database = store.database();
    let unknown = database.bytes(b"future").unwrap();
    // This fixture supplies opaque fields while preserving the existing slugs.
    let mut tx = database.begin();
    for (id, record) in [1u64, 2].into_iter().zip(records) {
        let record = record.set(99, &unknown).unwrap();
        tx.put(table.collection(), &id.to_le_bytes(), record.as_value())
            .unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(store.checkpoint().unwrap(), 1);
    verify_original(&store, &table);
}

fn legacy_edit(path: &Path) {
    let mut store = Store::open_file(path).unwrap();
    let table = store.table::<Task>("tasks").unwrap();
    store
        .read(|read| {
            let first = read.find(&table, Task::SLUG, "alpha-v2")?.unwrap();
            assert_eq!((first.id(), first.version()), (1, 2));
            assert_eq!(first.record().field(99)?.unwrap().as_bytes()?, b"future");
            let second = read.find(&table, Task::SLUG, "beta")?.unwrap();
            assert_eq!((second.id(), second.version()), (2, 1));
            Ok(())
        })
        .unwrap();
    let before = store.database().snapshot().revision();
    assert_eq!(
        store.write(|write| write.insert(
            &table,
            3,
            &Task {
                slug: "gamma".into()
            }
        )),
        Err(Error::UnsupportedVersion)
    );
    assert_eq!(store.database().snapshot().revision(), before);
    store
        .write(|write| {
            write.update(&table, 1, |task| {
                task.slug = "alpha-legacy-edit".into();
                Ok(())
            })
        })
        .unwrap();
    assert_eq!(store.database().snapshot().revision(), before + 1);
    store
        .read(|read| {
            let row = read.find(&table, Task::SLUG, "alpha-legacy-edit")?.unwrap();
            assert_eq!((row.id(), row.version()), (1, 2));
            assert_eq!(row.record().field(99)?.unwrap().as_bytes()?, b"future");
            assert!(read.find(&table, Task::SLUG, "alpha-v2")?.is_none());
            assert!(read.check_indexes(&table)?.is_consistent());
            Ok(())
        })
        .unwrap();
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let phase = args[1].to_str().unwrap();
    let source = Path::new(&args[2]);
    match phase {
        "seed" => create(source),
        "backup" => {
            storage::backup_file(source, &args[3], Limits::default()).unwrap();
        }
        "legacy-edit" => legacy_edit(source),
        "verify-v1" => {
            let mut store = Store::open_file(source).unwrap();
            let table = store.table::<Task>("tasks").unwrap();
            verify_original(&store, &table);
        }
        "write-restored-v1" => {
            let mut store = Store::open_file(source).unwrap();
            let table = store.table::<Task>("tasks").unwrap();
            verify_original(&store, &table);
            let before = store.database().snapshot().revision();
            store
                .write(|w| {
                    w.insert(
                        &table,
                        3,
                        &Task {
                            slug: "gamma".into(),
                        },
                    )
                })
                .unwrap();
            assert_eq!(store.database().snapshot().revision(), before + 1);
            drop(store);
            let mut store = Store::open_file(source).unwrap();
            let table = store.table::<Task>("tasks").unwrap();
            store
                .read(|r| {
                    let row = r.find(&table, Task::SLUG, "gamma")?.unwrap();
                    assert_eq!((row.id(), row.version()), (3, 1));
                    assert!(r.check_indexes(&table)?.is_consistent());
                    Ok(())
                })
                .unwrap();
            store
                .read_retained(1, |r| {
                    assert!(r.get(&table, 3)?.is_none());
                    assert!(r.check_indexes(&table)?.is_consistent());
                    Ok(())
                })
                .unwrap();
        }
        _ => panic!("unknown historical phase"),
    }
    println!("{phase}");
}
