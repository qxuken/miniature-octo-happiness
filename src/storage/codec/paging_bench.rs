//! Generate real v2 images with the shipping encoder. Build immutable heads directly
//! to exclude quadratic transaction staging and the writer's allocations from read timing.
use super::*;
use crate::{
    Database, DatabaseOptions, Limits,
    storage::{self, FileStore, Storage},
};
use std::{env, path::Path, time::Instant};

#[test]
#[ignore = "native capacity benchmark; set WDB_PAGING_CASE, WDB_PAGING_ACTION and WDB_PAGING_FILE"]
fn native_read_scaling() {
    let case = env::var("WDB_PAGING_CASE").unwrap();
    let action = env::var("WDB_PAGING_ACTION").unwrap();
    let path = env::var_os("WDB_PAGING_FILE").unwrap();
    let path = Path::new(&path);
    let (n, size, shared) = match case.as_str() {
        "distinct" => (10_000u32, 16, false),
        "million" => (1_000_000, 16, true),
        "blobs" => (64, 768 << 10, false),
        _ => panic!("unknown fixture"),
    };
    let limits = Limits {
        max_entries: 1_000_000,
        max_live_bytes: 64 << 20,
        ..Limits::default()
    };
    let bytes = |k: u32| {
        let mut v = vec![47; size];
        if !shared {
            v[..4].copy_from_slice(&k.to_le_bytes());
        }
        v
    };
    if action == "seed" {
        let mem = Memory::new(Limits {
            max_live_bytes: usize::MAX,
            ..limits
        })
        .unwrap();
        let one = Value::bytes_new(&mem, &bytes(0)).unwrap();
        let mut records = Map::new();
        for k in 0..n {
            let value = if shared {
                one.clone()
            } else {
                Value::bytes_new(&mem, &bytes(k)).unwrap()
            };
            records = records
                .set(&mem, &k.to_be_bytes(), Record { revision: 1, value })
                .unwrap();
        }
        let c = Collection {
            revision: 1,
            records,
            base: None,
        };
        let head = Head {
            revision: 1,
            data_revision: 1,
            collections: Map::new().set(&mem, &1u64.to_le_bytes(), c).unwrap(),
        };
        let retention = Retention {
            next_id: 1,
            entries: ChargedVec::new(),
        };
        Storage::create(
            FileStore::create(path).unwrap(),
            &mem,
            &head,
            2,
            [47; 16],
            &retention,
        )
        .unwrap();
        return;
    }
    if action == "edit" {
        assert!(shared);
        let copy = path.with_extension("edit-wdb");
        let mut dest = std::fs::File::create_new(&copy).unwrap();
        std::io::copy(&mut std::fs::File::open(path).unwrap(), &mut dest).unwrap();
        drop(dest);
        let opts = || DatabaseOptions::from(limits).with_paged_reads();
        let db = Database::open_file_with_options(&copy, opts()).unwrap();
        let c = db.collection_id(1);
        let start = Instant::now();
        let mut count = 0u32;
        db.snapshot()
            .visit(c, |key, value| {
                assert_eq!(key, count.to_be_bytes());
                assert_eq!(value.as_bytes()?, [47; 16]);
                count += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count, n);
        let scan = start.elapsed();
        let retained = db.retain_snapshot(&db.snapshot()).unwrap();
        let mut tx = db.begin();
        tx.put(c, &0u32.to_be_bytes(), &db.unsigned(99).unwrap())
            .unwrap();
        let start = Instant::now();
        let revision = tx.commit().unwrap();
        let elapsed = start.elapsed();
        let peak = db.memory_usage().peak_bytes;
        drop(tx);
        drop(db);
        let db = Database::open_file_with_options(&copy, opts()).unwrap();
        let c = db.collection_id(1);
        assert_eq!(db.snapshot().revision(), revision);
        assert_eq!(
            db.snapshot()
                .get(c, &0u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .as_u64(),
            Ok(99)
        );
        assert_eq!(
            db.retained_snapshot(retained)
                .unwrap()
                .get(c, &0u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .as_bytes()
                .unwrap(),
            [47; 16]
        );
        drop(db);
        println!(
            "PAGING million edit scan_ms={} commit_ms={} peak={} bytes={}",
            scan.as_millis(),
            elapsed.as_millis(),
            peak,
            std::fs::metadata(&copy).unwrap().len()
        );
        std::fs::remove_file(copy).unwrap();
        return;
    }
    let options = || match action.as_str() {
        "eager" => DatabaseOptions::from(limits),
        "paged" | "fast" => DatabaseOptions::from(limits).with_paged_reads(),
        _ => panic!("unknown read mode"),
    };
    let mut opened = Vec::new();
    let mut checked = Vec::new();
    let mut lookup = Vec::new();
    let (mut peak, mut live) = (0, 0);
    for trial in 0..5 {
        let start = Instant::now();
        let writable;
        let reader;
        let (snapshot, id) = if action == "fast" {
            reader = Some(crate::ReadOnlyDatabase::open_file(path, options()).unwrap());
            writable = None;
            let db = reader.as_ref().unwrap();
            (db.snapshot(), db.collection_id(1))
        } else {
            reader = None;
            writable = match Database::open_file_with_options(path, options()) {
                Ok(db) => Some(db),
                Err(Error::Limit) => {
                    assert_eq!(
                        storage::inspect_file_with_options(path, options()),
                        Err(Error::Limit)
                    );
                    assert_eq!((case.as_str(), action.as_str()), ("million", "eager"));
                    println!("PAGING {case} {action} open_and_check=Limit budget=64MiB");
                    return;
                }
                Err(e) => panic!("{e:?}"),
            };
            let db = writable.as_ref().unwrap();
            (db.snapshot(), db.collection_id(1))
        };
        opened.push(start.elapsed().as_micros());
        assert_eq!(snapshot.count(id), Ok(n as usize));
        let usage = || {
            reader.as_ref().map_or_else(
                || writable.as_ref().unwrap().memory_usage(),
                |db| db.memory_usage(),
            )
        };
        live = usage().live_bytes;
        let mut random = 123456789u32 + trial;
        for _ in 0..if size > 4096 { 64 } else { 1000 } {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let k = random % n;
            let start = Instant::now();
            let value = snapshot.get(id, &k.to_be_bytes()).unwrap().unwrap();
            lookup.push(start.elapsed().as_nanos());
            let v = value.as_bytes().unwrap();
            assert_eq!(v.len(), size);
            assert_eq!(v[size - 1], 47);
            if !shared {
                assert_eq!(v[..4], k.to_le_bytes());
            }
        }
        peak = peak.max(usage().peak_bytes);
        drop(snapshot);
        drop(writable);
        let start = Instant::now();
        if let Some(view) = reader {
            view.check().unwrap();
            peak = peak.max(view.memory_usage().peak_bytes);
        } else {
            let info = storage::inspect_file_with_options(path, options()).unwrap();
            assert_eq!(info.revision, 1);
        }
        checked.push(start.elapsed().as_micros());
    }
    opened.sort_unstable();
    checked.sort_unstable();
    lookup.sort_unstable();
    println!(
        "PAGING {case} {action} bytes={} open_us={} check_us={} lookup_p50_ns={} lookup_p95_ns={} peak={} live={}",
        std::fs::metadata(path).unwrap().len(),
        opened[2],
        checked[2],
        lookup[lookup.len() / 2],
        lookup[lookup.len() * 95 / 100],
        peak,
        live
    );
}
