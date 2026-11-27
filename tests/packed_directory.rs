use std::time::Instant;
use weekend_db::{
    Database, DatabaseOptions, Error, Limits, ReadOnlyDatabase,
    storage::{self, SimulatedDisk},
};

fn limits() -> Limits {
    Limits {
        max_entries: 1_000_000,
        max_live_bytes: 64 << 20,
        max_writes: 2048,
        ..Limits::default()
    }
}
fn options() -> DatabaseOptions {
    DatabaseOptions::from(limits()).with_packed_pages()
}
fn payload(k: u32) -> [u8; 16] {
    let mut b = [0; 16];
    b[..4].copy_from_slice(&k.to_be_bytes());
    b[4..8].copy_from_slice(&(!k).to_be_bytes());
    b[8..].copy_from_slice(&(u64::from(k) * 714025).to_le_bytes());
    b
}
fn fill(db: &Database, n: u32) {
    let c = db.create_collection().unwrap();
    for first in (0..n).step_by(2048) {
        let mut t = db.begin();
        for k in first..(first + 2048).min(n) {
            t.put(c, &k.to_be_bytes(), &db.bytes(&payload(k)).unwrap())
                .unwrap();
        }
        t.commit().unwrap();
    }
}
fn check_all(s: &weekend_db::Snapshot<'_>, c: weekend_db::CollectionId, n: u32, edited: bool) {
    assert_eq!(s.count(c).unwrap(), n as usize);
    let mut seen = 0u32;
    s.visit(c, |key, v| {
        assert_eq!(key, seen.to_be_bytes());
        if edited && seen == n / 2 {
            assert_eq!(v.as_bytes()?, b"changed");
        } else {
            assert_eq!(v.as_bytes()?, payload(seen));
        }
        seen += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(seen, n);
}
#[test]
fn retained_packed_rows_reopen_after_inserts_deletes_and_size_changes() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(disk.clone(), [27; 16], options()).unwrap();
    fill(&db, 1000);
    let c = db.collection_id(1);
    let old = db.snapshot();
    let id = db.retain_snapshot(&old).unwrap();
    let big = db.bytes(&[77; 16384]).unwrap();
    let nested = db
        .map()
        .unwrap()
        .map_set(b"list", &db.list().unwrap().list_push(&big).unwrap())
        .unwrap();
    let mut t = db.begin();
    t.put(c, &500u32.to_be_bytes(), &nested).unwrap();
    t.delete(c, &250u32.to_be_bytes()).unwrap();
    t.put(c, b"", &db.none().unwrap()).unwrap();
    t.put(c, b"zz", &big).unwrap();
    t.commit().unwrap();
    check_all(&old, c, 1000, false);
    drop(t);
    drop(old);
    drop(db);
    let db = Database::open_on(disk.clone(), limits()).unwrap();
    let c = db.collection_id(1);
    check_all(&db.retained_snapshot(id).unwrap(), c, 1000, false);
    let current = db.snapshot();
    assert_eq!(current.count(c), Ok(1001));
    assert!(current.get(c, &250u32.to_be_bytes()).unwrap().is_none());
    assert_eq!(
        current.get(c, &500u32.to_be_bytes()).unwrap().unwrap(),
        nested
    );
    assert_eq!(current.get(c, b"zz").unwrap().unwrap(), big);
    drop(current);
    drop(db);
    assert_eq!(
        storage::inspect_on_with_options(disk, options())
            .unwrap()
            .format_version,
        3
    );
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
#[ignore = "native million-key capacity measurement; run in release mode"]
fn million_distinct_packed_values() {
    let n = 1_000_000u32;
    let path = std::env::temp_dir().join(format!("wdb-packed-million-{}.db", std::process::id()));
    let db = Database::create_file_with_options(&path, options()).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    let t = Instant::now();
    fill(&db, n);
    let fill = t.elapsed();
    let writer_peak = db.memory_usage().peak_bytes;
    drop(db);
    let t = Instant::now();
    let db = Database::open_file(&path, limits()).unwrap();
    let open = t.elapsed();
    let c = db.collection_id(1);
    let orig = db.snapshot().revision();
    let retained_id = db.retain_snapshot(&db.snapshot()).unwrap();
    let before = std::fs::metadata(&path).unwrap().len();
    let mut tx = db.begin();
    tx.put(c, &(n / 2).to_be_bytes(), &db.bytes(b"changed").unwrap())
        .unwrap();
    let t = Instant::now();
    let rev = tx.commit().unwrap();
    let commit = t.elapsed();
    let after = std::fs::metadata(&path).unwrap().len();
    let append = after - before;
    assert_eq!(append, 4 * 4096);
    drop(tx);
    drop(db);
    let t = Instant::now();
    let view = ReadOnlyDatabase::open_file(&path, options()).unwrap();
    let reopen = t.elapsed();
    let c = view.collection_id(1);
    let t = Instant::now();
    view.check().unwrap();
    let full = t.elapsed();
    let old = view.retained_snapshot(retained_id).unwrap();
    assert_eq!(old.revision(), orig);
    assert_eq!(view.snapshot().revision(), rev);
    let current = view.snapshot();
    let mut times = Vec::with_capacity(1001);
    let mut k = n / 2;
    for _ in 0..1001 {
        let key = k.to_be_bytes();
        let t = Instant::now();
        let value = current.get(c, &key).unwrap().unwrap();
        times.push(t.elapsed());
        if k == n / 2 {
            assert_eq!(value.as_bytes().unwrap(), b"changed");
        } else {
            assert_eq!(value.as_bytes().unwrap(), payload(k));
        }
        assert_eq!(
            old.get(c, &key).unwrap().unwrap().as_bytes().unwrap(),
            payload(k)
        );
        k = (k + 65537) % n;
    }
    drop(current);
    times.sort_unstable();
    println!(
        "packed point reads p50={:?} p95={:?}",
        times[500], times[950]
    );
    let t = Instant::now();
    check_all(&old, c, n, false);
    check_all(&view.snapshot(), c, n, true);
    let scans = t.elapsed();
    let usage = view.memory_usage();
    assert!(usage.peak_bytes <= 64 << 20);
    println!(
        "packed rows={n} before={before} after={after} append={append} fill={fill:?} open={open:?} commit={commit:?} reopen={reopen:?} check={full:?} both_scans={scans:?} reader_live={} reader_peak={} writer_peak={writer_peak} old_rev={orig} new_rev={rev}",
        usage.live_bytes, usage.peak_bytes
    );
    drop(old);
    drop(view);
    let copy = path.with_extension("backup");
    let t = Instant::now();
    let stats = storage::backup_file(&path, &copy, limits()).unwrap();
    let _copy_cleanup = Cleanup(copy.clone());
    assert!(stats.after_bytes < before);
    let verified = ReadOnlyDatabase::open_file(&copy, options()).unwrap();
    let c = verified.collection_id(1);
    check_all(&verified.snapshot(), c, n, true);
    check_all(
        &verified.retained_snapshot(retained_id).unwrap(),
        c,
        n,
        false,
    );
    println!(
        "packed verified backup bytes={} elapsed={:?}",
        stats.after_bytes,
        t.elapsed()
    );
    drop(verified);
    #[cfg(feature = "replication")]
    {
        let area = path.with_extension("follower-test");
        capacity_follower::roundtrip(
            &path,
            &area,
            limits(),
            retained_id,
            64 << 20,
            |view, c, edited| check_all(view, c, n, edited),
        );
        std::fs::remove_dir_all(area).unwrap();
    }
}

#[test]
fn paged_commit_cuts_publish_one_complete_generation() {
    use storage::{Crash, Fault, IoKind};
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(disk.clone(), [10; 16], options()).unwrap();
    fill(&db, 300);
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let before = db.snapshot().revision();
    drop(db);
    let base = disk.durable_image().unwrap();
    let run = |cut: usize| {
        let disk = SimulatedDisk::from_image(&base).unwrap();
        let db = Database::open_on(disk.clone(), limits()).unwrap();
        let c = db.collection_id(1);
        let mut tx = db.begin();
        tx.put(c, &150u32.to_be_bytes(), &db.bytes(b"changed").unwrap())
            .unwrap();
        disk.arm(cut, Fault::Error);
        let commit = tx.commit();
        let trace = disk.trace().unwrap();
        drop(tx);
        drop(db);
        disk.crash(Crash::DropAll).unwrap();
        let view = ReadOnlyDatabase::open_on(disk, options()).unwrap();
        let c = view.collection_id(1);
        let rev = view.snapshot().revision();
        check_all(&view.retained_snapshot(retained).unwrap(), c, 300, false);
        check_all(&view.snapshot(), c, 300, rev != before);
        (commit, rev, trace)
    };
    let (ok, _, trace) = run(usize::MAX);
    assert_eq!(ok, Ok(before + 1));
    let barriers: Vec<_> = trace
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == IoKind::Sync)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(barriers.len(), 3);
    for cut in 0..trace.len() {
        let (result, rev, _) = run(cut);
        assert!(result.is_err());
        assert_eq!(rev, before + u64::from(cut > barriers[1]), "cut {cut}");
    }
}
#[test]
fn corrupt_retained_only_leaf_is_rejected_before_exposing_current() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(disk.clone(), [11; 16], options()).unwrap();
    fill(&db, 600);
    db.retain_snapshot(&db.snapshot()).unwrap();
    let c = db.collection_id(1);
    let mut t = db.begin();
    t.put(c, &100u32.to_be_bytes(), &db.unsigned(42).unwrap())
        .unwrap();
    t.commit().unwrap();
    drop(t);
    drop(db);
    let mut image = disk.durable_image().unwrap();
    let needle = payload(100);
    let found: Vec<_> = image
        .windows(16)
        .enumerate()
        .filter_map(|(i, b)| (b == needle).then_some(i))
        .collect();
    assert_eq!(found.len(), 1);
    image[found[0]] ^= 1;
    let disk = SimulatedDisk::from_image(&image).unwrap();
    assert_eq!(
        Database::open_on(disk.clone(), limits()).err(),
        Some(Error::Corrupt)
    );
    assert_eq!(
        ReadOnlyDatabase::open_on(disk, options()).err(),
        Some(Error::Corrupt)
    );
}

#[test]
fn one_replacement_appends_only_the_path_in_a_multibranch_directory() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(disk.clone(), [12; 16], options()).unwrap();
    fill(&db, 17_000);
    let c = db.collection_id(1);
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let before = db.storage_space().unwrap().physical_bytes as usize;
    let mut t = db.begin();
    t.put(c, &8500u32.to_be_bytes(), &db.bytes(b"changed").unwrap())
        .unwrap();
    t.commit().unwrap();
    drop(t);
    drop(db);
    let image = disk.durable_image().unwrap();
    assert_eq!(image.len() - before, 16384);
    let kinds: Vec<_> = image[before..]
        .as_chunks::<4096>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(b[8..12].try_into().unwrap()))
        .collect();
    assert_eq!(kinds, [4, 5, 2, 3]);
    let info = storage::inspect_on(disk.clone(), limits()).unwrap();
    assert_eq!(info.format_version, 3);
    let view = ReadOnlyDatabase::open_on(disk, options()).unwrap();
    let c = view.collection_id(1);
    check_all(&view.snapshot(), c, 17_000, true);
    check_all(&view.retained_snapshot(retained).unwrap(), c, 17_000, false);
}
#[test]
fn resealed_branch_cannot_repeat_a_leaf_or_point_into_an_object() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(disk.clone(), [13; 16], options()).unwrap();
    fill(&db, 600);
    drop(db);
    let image = disk.durable_image().unwrap();
    let off = image
        .as_chunks::<4096>()
        .0
        .iter()
        .position(|p| &p[..8] == b"WDBOBJ01" && p[8..12] == 5u32.to_le_bytes())
        .unwrap()
        * 4096;
    let first = u64::from_le_bytes(image[off + 64..off + 72].try_into().unwrap());
    for replacement in [first, first + 1, off as u64] {
        let mut bytes = image.clone();
        let page = &mut bytes[off..off + 4096];
        page[72..80].copy_from_slice(&replacement.to_le_bytes());
        page[48..56].fill(0);
        let mut crc = 0u64;
        for &v in page.iter() {
            crc ^= u64::from(v) << 56;
            for _ in 0..8 {
                crc = (crc << 1) ^ (0x42f0e1eba9ea3693 & 0u64.wrapping_sub(crc >> 63));
            }
        }
        page[48..56].copy_from_slice(&crc.to_le_bytes());
        let bad = SimulatedDisk::from_image(&bytes).unwrap();
        assert_eq!(
            ReadOnlyDatabase::open_on(bad, options()).err(),
            Some(Error::Corrupt)
        );
    }
}

use weekend_db as allocator_engine;
#[path = "support/allocator.rs"]
pub mod allocator;

#[test]
fn allocation_refusal_while_copying_a_path_preserves_old_pages_and_retries() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(disk.clone(), [14; 16], options()).unwrap();
    fill(&db, 250);
    let id = db.retain_snapshot(&db.snapshot()).unwrap();
    drop(db);
    let image = disk.durable_image().unwrap();
    let allocator = std::sync::Arc::new(allocator::TestAllocator::default());
    let db =
        Database::open_on_with_options(disk.clone(), options().with_allocator(allocator.handle()))
            .unwrap();
    let c = db.collection_id(1);
    for allowed in 0..1000 {
        let mut tx = db.begin();
        tx.put(c, &125u32.to_be_bytes(), &db.bytes(b"changed").unwrap())
            .unwrap();
        match allocator.with_refusal(allowed, || tx.commit()) {
            Ok(_) => {
                assert!(allowed > 10);
                check_all(&db.snapshot(), c, 250, true);
                check_all(&db.retained_snapshot(id).unwrap(), c, 250, false);
                return;
            }
            Err(e) => assert_eq!(e, Error::OutOfMemory),
        }
        assert!(disk.durable_image().unwrap() == image);
        assert_eq!(
            db.snapshot()
                .get(c, &125u32.to_be_bytes())
                .unwrap()
                .unwrap()
                .as_bytes()
                .unwrap(),
            payload(125)
        );
    }
    panic!("no successful retry");
}

#[test]
fn packed_encoding_round_trips_each_value_kind_and_oversize_keys() {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(disk.clone(), [15; 16], options()).unwrap();
    let c = db.create_collection().unwrap();
    let empty = db.create_collection().unwrap();
    let floats = [0x8000_0000_0000_0000, 0x7ff8_0000_0000_0017];
    let mut t = db.begin();
    let long = vec![0xff; 8192];
    let inner = db
        .list()
        .unwrap()
        .list_push(&db.boolean(false).unwrap())
        .unwrap()
        .list_push(&db.integer(i64::MIN).unwrap())
        .unwrap();
    let values = [
        db.none().unwrap(),
        db.boolean(true).unwrap(),
        db.integer(i64::MAX).unwrap(),
        db.unsigned(u64::MAX).unwrap(),
        db.bytes(b"\0\xff").unwrap(),
        inner.clone(),
        db.map().unwrap().map_set(b"\xff", &inner).unwrap(),
    ];
    for (i, v) in values.iter().enumerate() {
        t.put(c, &[i as u8], v).unwrap();
    }
    for (i, bits) in floats.iter().enumerate() {
        t.put(
            c,
            &[10 + i as u8],
            &db.float(f64::from_bits(*bits)).unwrap(),
        )
        .unwrap();
    }
    t.put(c, &long, &inner).unwrap();
    t.commit().unwrap();
    drop(t);
    drop(db);
    let db = Database::open_on(disk, limits()).unwrap();
    let s = db.snapshot();
    let c = db.collection_id(c.as_u64());
    assert_eq!(s.count(db.collection_id(empty.as_u64())), Ok(0));
    for (i, v) in values.iter().enumerate() {
        assert_eq!(s.get(c, &[i as u8]).unwrap().as_ref(), Some(v));
    }
    for (i, bits) in floats.iter().enumerate() {
        assert_eq!(
            s.get(c, &[10 + i as u8])
                .unwrap()
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            *bits
        );
    }
    assert_eq!(s.get(c, &long).unwrap(), Some(inner));
}

#[cfg(all(
    feature = "replication",
    any(target_os = "linux", target_os = "macos", windows)
))]
#[path = "support/capacity_follower.rs"]
mod capacity_follower;
