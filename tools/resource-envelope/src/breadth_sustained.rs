//! Repeated native publication with uniform full-width writes and fresh readers.
use super::{
    CORE, FINAL, MIB, OFFSET, ROOTS, WRITES, complete, limits, retained_views, seed, seed_bits,
    verify_retained, verify_values,
};
use std::{
    fs,
    path::Path,
    sync::Barrier,
    thread,
    time::{Duration, Instant},
};
use weekend_db::{
    CollectionId, Database, Snapshot,
    storage::{self, FileStore, MAX_IMAGE_BYTES},
};

const WRITERS: usize = 3;
const READERS: usize = 4;
const ROUNDS: u64 = 16;
const CYCLES: u64 = 8;
const SAMPLES: usize = 4096;
const IDENTITY: [u8; 16] = *b"breadth-load-001";
const TAG: u64 = 0x5355_0000_0000_0000;

fn encoded(writer: usize, sequence: u64) -> u64 {
    TAG | ((writer as u64) << 32) | sequence
}

fn expected(key: u32, sequences: [u64; WRITERS]) -> u64 {
    let writer = (key / WRITES) as usize;
    match sequences.get(writer) {
        Some(&seq) if seq > 0 => encoded(writer, seq),
        _ => seed_bits(ROOTS, key),
    }
}

fn snapshot_sample(
    db: &Database,
    collection: CollectionId,
    last_revision: Option<u64>,
) -> [u64; 5] {
    let start = Instant::now();
    let snapshot = db.snapshot();
    let acquired = u64::try_from(start.elapsed().as_nanos()).unwrap();
    let revision = snapshot.revision();
    assert!(revision >= last_revision.unwrap_or(FINAL));
    let mut sequences = [0; WRITERS];
    for (writer, sequence) in sequences.iter_mut().enumerate() {
        let key = writer as u32 * WRITES;
        let bits = read(&snapshot, collection, key);
        if bits != seed_bits(ROOTS, key) {
            let seq = bits & u64::from(u32::MAX);
            assert!((1..=ROUNDS * CYCLES).contains(&seq));
            assert_eq!(bits, encoded(writer, seq));
            *sequence = seq;
        }
        let last_key = key + WRITES - 1;
        let last_bits = if *sequence == 0 {
            seed_bits(ROOTS, last_key)
        } else {
            encoded(writer, *sequence)
        };
        assert_eq!(read(&snapshot, collection, last_key), last_bits);
    }
    assert_eq!(revision, FINAL + sequences.iter().sum::<u64>());
    if last_revision != Some(revision) {
        let mut digest = OFFSET;
        verify_values(&snapshot, collection, &mut digest, |key| {
            expected(key, sequences)
        });
    }
    [revision, acquired, sequences[0], sequences[1], sequences[2]]
}

fn read(view: &Snapshot<'_>, collection: CollectionId, key: u32) -> u64 {
    view.get(collection, &key.to_be_bytes())
        .unwrap()
        .unwrap()
        .as_u64()
        .unwrap()
}

fn verify(db: &Database, sequence: u64) -> u64 {
    let current = db.snapshot();
    let revision = FINAL + WRITERS as u64 * sequence;
    assert_eq!(current.revision(), revision);
    assert_eq!(
        current.data_revision(),
        if sequence == 0 { FINAL - 1 } else { revision }
    );
    let collection = db.collection_id(1);
    let mut digest = OFFSET;
    verify_values(&current, collection, &mut digest, |key| {
        expected(key, [sequence; WRITERS])
    });
    verify_retained(&retained_views(db), collection, &mut digest);
    digest
}

struct Samples {
    writers: Vec<Vec<[u64; 2]>>,
    readers: Vec<Vec<[u64; 5]>>,
}

fn run_cycle(db: &Database, cycle: u64) -> Samples {
    let first = 1 + (cycle - 1) * ROUNDS;
    let last = cycle * ROUNDS;
    let start_revision = FINAL + WRITERS as u64 * (first - 1);
    let end_revision = FINAL + WRITERS as u64 * last;
    let collection = db.collection_id(1);
    let start = Barrier::new(WRITERS + READERS);
    thread::scope(|scope| {
        let writers: Vec<_> = (0..WRITERS)
            .map(|writer| {
                let start = &start;
                scope.spawn(move || {
                    let base = writer as u32 * WRITES;
                    let mut samples = Vec::new();
                    for seq in first..=last {
                        let mut draft = db.begin();
                        let value = db.unsigned(encoded(writer, seq)).unwrap();
                        for key in base..base + WRITES {
                            draft.put(collection, &key.to_be_bytes(), &value).unwrap();
                        }
                        if seq == first {
                            start.wait();
                        }
                        let now = Instant::now();
                        let revision = draft.commit().unwrap();
                        samples.push([revision, now.elapsed().as_nanos().try_into().unwrap()]);
                    }
                    samples
                })
            })
            .collect();
        let readers: Vec<_> = (0..READERS)
            .map(|_| {
                let start = &start;
                scope.spawn(move || {
                    let initial = snapshot_sample(db, collection, None);
                    assert_eq!(initial[0], start_revision);
                    let mut samples = vec![initial];
                    start.wait();
                    while samples.last().unwrap()[0] < end_revision {
                        assert!(samples.len() < SAMPLES, "fresh snapshot sample limit");
                        let sample =
                            snapshot_sample(db, collection, Some(samples.last().unwrap()[0]));
                        samples.push(sample);
                        if sample[0] < end_revision {
                            thread::sleep(Duration::from_micros(500));
                        }
                    }
                    assert_eq!(samples.last().unwrap()[0], end_revision);
                    samples
                })
            })
            .collect();
        Samples {
            writers: writers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect(),
            readers: readers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect(),
        }
    })
}

pub(crate) fn run(dir: &Path) {
    fs::create_dir(dir).unwrap();
    let seed_path = dir.join("seed.wdb");
    let active = dir.join("sustained.wdb");
    let db = Database::create_on(
        FileStore::create(&seed_path).unwrap(),
        IDENTITY,
        limits(CORE),
    )
    .unwrap();
    seed(&db);
    drop(db);
    Database::compact_file(&seed_path, limits(256 * MIB)).unwrap();
    fs::copy(&seed_path, &active).unwrap();
    let initial = Database::open_file(&active, limits(CORE)).unwrap();
    let digest = verify(&initial, 0);
    let space = initial.storage_space().unwrap();
    println!(
        "{{\"phase\":\"sustained_seed\",\"revision\":{FINAL},\"digest\":{digest},\"file_bytes\":{}}}",
        space.physical_bytes
    );
    drop(initial);
    for cycle in 1..=CYCLES {
        let db = Database::open_file(&active, limits(CORE)).unwrap();
        let start = Instant::now();
        let samples = run_cycle(&db, cycle);
        let workload_ns = start.elapsed().as_nanos();
        let digest = verify(&db, cycle * ROUNDS);
        let peak = db.memory_usage().peak_bytes;
        let before = db.storage_space().unwrap();
        assert!(before.physical_bytes <= MAX_IMAGE_BYTES as u64);
        drop(db);
        let start = Instant::now();
        let stats = Database::compact_file(&active, limits(256 * MIB)).unwrap();
        let maintenance_ns = start.elapsed().as_nanos();
        assert_eq!(stats.before_bytes, before.physical_bytes);
        let start = Instant::now();
        let reopened = Database::open_file(&active, limits(CORE)).unwrap();
        let reopen_ns = start.elapsed().as_nanos();
        assert_eq!(verify(&reopened, cycle * ROUNDS), digest);
        assert_eq!(
            stats.after_bytes,
            reopened.storage_space().unwrap().physical_bytes
        );
        assert_eq!(reopened.storage_identity(), Ok(IDENTITY));
        drop(reopened);
        let info = storage::inspect_file(&active, limits(CORE)).unwrap();
        assert!(info.witnessed);
        assert_eq!(info.revision, before.revision);
        println!(
            "{{\"phase\":\"sustained_cycle\",\"cycle\":{cycle},\"revision\":{},\"digest\":{digest},\"file_bytes\":{},\"compact_bytes\":{},\"core_peak_bytes\":{peak},\"workload_ns\":{workload_ns},\"maintenance_ns\":{maintenance_ns},\"reopen_ns\":{reopen_ns},\"writers\":{:?},\"readers\":{:?}}}",
            before.revision,
            stats.before_bytes,
            stats.after_bytes,
            samples.writers,
            samples.readers
        );
    }
    complete("breadth-sustained");
}
