use std::collections::BTreeMap;
use weekend_db_typed::{
    Error, Model, Store,
    engine::{self, storage::MemoryStore},
};

#[derive(Model)]
#[weekend(type_id = 71, version = 1)]
struct Old {
    #[weekend(id = 1, unique)]
    slug: u64,
}
#[derive(Model)]
#[weekend(type_id = 71, version = 2)]
struct New {
    #[weekend(id = 1, unique)]
    slug: u64,
    #[weekend(id = 2, default = 0)]
    score: u64,
}

/// A separate map is the primary/index oracle; typed index-check output is not
/// used to decide expected ownership or whether a transaction should succeed.
pub fn schema_index_migration(input: &[u8]) {
    let disk = MemoryStore::new().unwrap();
    let mut store = Store::create_on(disk.clone(), [71; 16], engine::Limits::default()).unwrap();
    let old = store.table::<Old>("rows").unwrap();
    let mut expected = BTreeMap::new();
    for id in 1..=4 {
        store
            .write(|w| w.insert(&old, id, &Old { slug: id - 1 }))
            .unwrap();
        expected.insert(id, (id - 1, 0, 1));
    }
    let mut table = store.table::<New>("rows").unwrap();
    for op in input.chunks(4).take(48) {
        let kind = op[0] % 7;
        let id = u64::from(op.get(1).copied().unwrap_or(0) % 8) + 1;
        let slug = u64::from(op.get(2).copied().unwrap_or(0) % 8);
        let score = u64::from(op.get(3).copied().unwrap_or(0));
        match kind {
            0 => {
                // Single selected-row migration with optional callback failure.
                let prior = expected.get(&id).copied();
                let fail = score == 255;
                let result = store.write(|w| {
                    w.migrate_ids(&table, &[id], |_, _, row| {
                        row.score = score;
                        if fail { Err(Error::InvalidId) } else { Ok(()) }
                    })
                });
                match prior {
                    None => assert_eq!(result, Err(Error::Engine(engine::Error::NotFound))),
                    Some((_, _, 2)) => assert_eq!(result, Ok(0)),
                    Some(_) if fail => assert_eq!(result, Err(Error::InvalidId)),
                    Some((key, _, _)) => {
                        assert_eq!(result, Ok(1));
                        expected.insert(id, (key, score, 2));
                    }
                }
            }
            1 if !expected.contains_key(&id) => {
                let occupied = expected.values().any(|&(key, _, _)| key == slug);
                let result = store.write(|w| w.insert(&table, id, &New { slug, score }));
                if occupied {
                    assert_eq!(result, Err(Error::UniqueViolation(1)));
                } else {
                    result.unwrap();
                    expected.insert(id, (slug, score, 2));
                }
            }
            2 if expected.get(&id).is_some_and(|row| row.2 == 2) => {
                let occupied = expected
                    .iter()
                    .any(|(&other, &(key, _, _))| other != id && key == slug);
                let result = store.write(|w| {
                    w.update(&table, id, |row| {
                        row.slug = slug;
                        row.score = score;
                        Ok(())
                    })
                });
                if occupied {
                    assert_eq!(result, Err(Error::UniqueViolation(1)));
                } else {
                    result.unwrap();
                    expected.insert(id, (slug, score, 2));
                }
            }
            3 if expected.contains_key(&id) => {
                store.write(|w| w.delete(&table, id)).unwrap();
                expected.remove(&id);
            }
            4 => {
                store.write(|w| w.rebuild_indexes(&table)).unwrap();
            }
            5 => {
                drop(store);
                store = Store::open_on(disk.clone(), engine::Limits::default()).unwrap();
                table = store.table::<New>("rows").unwrap();
            }
            6 => {
                let before = store.database().snapshot().revision();
                assert_eq!(
                    store.write(|w| w
                        .migrate_ids(&table, &[id, id], |_, _, _| panic!("duplicate callback"))),
                    Err(Error::InvalidId)
                );
                assert_eq!(before, store.database().snapshot().revision());
            }
            _ => {}
        }
        store
            .read(|r| {
                let rows = r.list(&table)?;
                assert_eq!(rows.len(), expected.len());
                for row in rows {
                    assert_eq!(
                        Some(&(row.slug, row.score, row.version())),
                        expected.get(&row.id())
                    );
                }
                for key in 0..8u64 {
                    let owner = expected
                        .iter()
                        .find_map(|(&id, &(slug, _, _))| (slug == key).then_some(id));
                    assert_eq!(r.find(&table, New::SLUG, &key)?.map(|row| row.id()), owner);
                }
                assert!(r.check_indexes(&table)?.is_consistent());
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn replay_typed_schedules() {
    schema_index_migration(&[
        0, 0, 0, 8, 2, 0, 1, 3, 1, 7, 7, 9, 3, 1, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0,
    ]);
    schema_index_migration(&[0, 0, 0, 255, 0, 0, 0, 7, 0, 0, 0, 255, 1, 4, 0, 0]);
    for a in 0..32u8 {
        schema_index_migration(
            &(0..128u8)
                .map(|b| b.wrapping_mul(a).wrapping_add(7))
                .collect::<Vec<_>>(),
        );
    }
}
