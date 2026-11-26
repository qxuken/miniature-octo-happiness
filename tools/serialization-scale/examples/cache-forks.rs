//! Near-cap fixture and oracle for fresh opens and the first publication.
use std::{env, path::Path, time::Instant};
use weekend_db::{Database, Error, Limits, Snapshot, Value, storage};

const BULK: u64 = 65;
const BULK_BYTES: usize = 888 * 1024;
const BATCH: usize = 8;
const WIDTH: u64 = 12;
const CELL_BYTES: usize = 1024;
const EPOCHS: u64 = 64;
const BASE: u64 = 2 + BULK.div_ceil(BATCH as u64);
const SEED_REVISION: u64 = BASE + 2 * EPOCHS - 1;
const LIVE_CAP: usize = 160 * 1024 * 1024;

fn limits() -> Limits {
    Limits {
        max_live_bytes: LIVE_CAP,
        ..Limits::default()
    }
}

fn payload(id: u64, size: usize) -> Vec<u8> {
    let mut bytes: Vec<_> = (0..size)
        .map(|i| ((id * 13 + i as u64) % 251) as u8)
        .collect();
    bytes[..8].copy_from_slice(&id.to_le_bytes());
    bytes
}

fn cell(db: &Database, epoch: u64) -> Value {
    let child = db.bytes(&payload(epoch, CELL_BYTES)).unwrap();
    db.list()
        .unwrap()
        .list_push(&child)
        .unwrap()
        .list_push(&child)
        .unwrap()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Publication {
    Metadata,
    ReuseValue,
    NewValue,
}

impl Publication {
    fn parse(name: &str) -> Self {
        match name {
            "metadata" => Self::Metadata,
            "reuse-value" => Self::ReuseValue,
            "new-value" => Self::NewValue,
            _ => panic!("unknown publication {name}"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::ReuseValue => "reuse-value",
            Self::NewValue => "new-value",
        }
    }
}

fn seed(path: &Path, shared: bool) {
    let db = Database::create_file(path, limits()).unwrap();
    let bulk = db.create_collection().unwrap();
    let history = db.create_collection().unwrap();
    let keys: Vec<_> = (0..BULK).collect();
    for batch in keys.chunks(BATCH) {
        let mut tx = db.begin();
        for key in batch {
            let value = db.bytes(&payload(100 + key, BULK_BYTES)).unwrap();
            tx.put(bulk, &key.to_be_bytes(), &value).unwrap();
        }
        tx.commit().unwrap();
    }
    assert_eq!(db.snapshot().revision(), BASE);
    for epoch in 1..=EPOCHS {
        let mut tx = db.begin();
        let shared_cell = shared.then(|| cell(&db, epoch));
        for key in 0..WIDTH {
            let value = shared_cell.clone().unwrap_or_else(|| cell(&db, epoch));
            tx.put(history, &key.to_be_bytes(), &value).unwrap();
        }
        assert_eq!(tx.commit(), Ok(BASE + 2 * epoch - 1));
        if epoch < EPOCHS {
            assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(epoch));
        }
    }
    verify(&db, None);
    let size = db.storage_space().unwrap().physical_bytes;
    assert!(size >= (storage::MAX_IMAGE_BYTES * 9 / 10) as u64);
    println!(
        "{{\"event\":\"seed\",\"shared\":{shared},\"revision\":{SEED_REVISION},\"retained\":{},\"bytes\":{size}}}",
        EPOCHS - 1
    );
}

fn assert_cell(db: &Database, snapshot: &Snapshot<'_>, key: u64, expected: &[u8]) {
    let value = snapshot
        .get(db.collection_id(2), &key.to_be_bytes())
        .unwrap()
        .unwrap();
    assert_eq!(value.len(), Ok(2));
    for index in 0..2 {
        assert_eq!(value.list_get(index).unwrap().as_bytes().unwrap(), expected);
    }
}

fn verify(db: &Database, publication: Option<Publication>) {
    let metadata = publication == Some(Publication::Metadata);
    let extra_row = publication.is_some() && !metadata;
    let retained = EPOCHS - 1 + u64::from(metadata);
    let current = db.snapshot();
    assert_eq!(
        current.revision(),
        SEED_REVISION + u64::from(publication.is_some())
    );
    assert_eq!(
        db.retained_ids().unwrap(),
        (1..=retained).collect::<Vec<_>>()
    );
    let mut roots = Vec::new();
    roots.push((EPOCHS, current, extra_row));
    for epoch in 1..=retained {
        let snapshot = db.retained_snapshot(epoch).unwrap();
        assert_eq!(snapshot.revision(), BASE + 2 * epoch - 1);
        roots.push((epoch, snapshot, false));
    }
    for (epoch, snapshot, extra_row) in &roots {
        assert_eq!(snapshot.count(db.collection_id(1)), Ok(BULK as usize));
        assert_eq!(
            snapshot.count(db.collection_id(2)),
            Ok(WIDTH as usize + usize::from(*extra_row))
        );
        let expected = payload(*epoch, CELL_BYTES);
        for key in 0..WIDTH + u64::from(*extra_row) {
            assert_cell(db, snapshot, key, &expected);
        }
    }
    for key in 0..BULK {
        let key_bytes = key.to_be_bytes();
        let expected = payload(100 + key, BULK_BYTES);
        for (_, snapshot, _) in &roots {
            let actual = snapshot
                .get(db.collection_id(1), &key_bytes)
                .unwrap()
                .unwrap();
            assert_eq!(actual.as_bytes().unwrap(), expected);
        }
    }
    assert!(!db.requires_recovery());
}

#[cfg(feature = "probe")]
fn counters() -> [usize; 6] {
    storage::cache_fork_probe()
}

#[cfg(not(feature = "probe"))]
fn counters() -> [usize; 6] {
    [0; 6]
}

fn assert_counters(shared: bool, publication: Publication, open: [usize; 6], commit: [usize; 6]) {
    if !cfg!(feature = "probe") {
        return;
    }
    // Each epoch owns a list and one bytes node per distinct cell. The
    // current and retained roots account for one DATA index entry per epoch.
    let cells = if shared { 1 } else { WIDTH as usize };
    let values = BULK as usize + EPOCHS as usize * cells * 2;
    let data = EPOCHS as usize;
    assert_eq!(open, [values, data, 0, 0, 0, 0]);
    let mut expected = open;
    if publication != Publication::Metadata {
        expected[4] = data;
        expected[5] = data;
    }
    if publication == Publication::NewValue {
        expected[2] = values;
        expected[3] = (values * 2).next_power_of_two();
    }
    assert_eq!(commit, expected);
}

#[cfg(target_os = "linux")]
fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .unwrap();
    Some(value.split_whitespace().next().unwrap().parse().unwrap())
}

#[cfg(not(target_os = "linux"))]
fn peak_rss_kib() -> Option<u64> {
    None
}

fn measure(path: &Path, shared: bool, publication: Publication) {
    let start = Instant::now();
    let db = Database::open_file(path, limits()).unwrap();
    let open_ns = start.elapsed().as_nanos();
    let open_usage = db.memory_usage();
    let opened = counters();
    assert_eq!(db.snapshot().revision(), SEED_REVISION);
    let initial = db.storage_space().unwrap().physical_bytes;
    let mut drafts = Vec::new();
    for key in 0..8u64 {
        let mut draft = db.begin();
        let value = db.bytes(&payload(1000 + key, 32 * 1024)).unwrap();
        draft
            .put(db.collection_id(1), &key.to_be_bytes(), &value)
            .unwrap();
        drafts.push(draft);
    }

    let (ack, commit_ns) = match publication {
        Publication::Metadata => {
            let snapshot = db.snapshot();
            let start = Instant::now();
            let id = db.retain_snapshot(&snapshot).unwrap();
            (id, start.elapsed().as_nanos())
        }
        Publication::ReuseValue | Publication::NewValue => {
            let mut tx = db.begin();
            let collection = db.collection_id(2);
            let value = match publication {
                Publication::ReuseValue => {
                    tx.get(collection, &0u64.to_be_bytes()).unwrap().unwrap()
                }
                Publication::NewValue => cell(&db, EPOCHS),
                Publication::Metadata => unreachable!(),
            };
            tx.put(collection, &WIDTH.to_be_bytes(), &value).unwrap();
            let start = Instant::now();
            let revision = tx.commit().unwrap();
            (revision, start.elapsed().as_nanos())
        }
    };
    let usage = db.memory_usage();
    let published = counters();
    let final_bytes = db.storage_space().unwrap().physical_bytes;
    let metadata = publication == Publication::Metadata;
    assert_eq!(ack, if metadata { EPOCHS } else { SEED_REVISION + 1 });
    assert!(final_bytes > initial);
    assert!(usage.live_bytes <= usage.peak_bytes && usage.peak_bytes <= LIVE_CAP);
    assert_counters(shared, publication, opened, published);
    for (key, mut draft) in drafts.into_iter().enumerate() {
        let actual = draft
            .get(db.collection_id(1), &(key as u64).to_be_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(
            actual.as_bytes().unwrap(),
            payload(1000 + key as u64, 32 * 1024)
        );
    }
    if metadata {
        assert_eq!(db.retain_snapshot(&db.snapshot()), Err(Error::Limit));
        assert_eq!(db.storage_space().unwrap().physical_bytes, final_bytes);
    }
    verify(&db, Some(publication));
    let rss = peak_rss_kib().map_or_else(|| "null".to_owned(), |value| value.to_string());
    println!(
        concat!(
            "{{\"event\":\"measure\",\"shared\":{},\"publication\":\"{}\",\"probe\":{},",
            "\"open_ns\":{},\"commit_ns\":{},\"open_live_bytes\":{},\"open_peak_bytes\":{},",
            "\"live_bytes\":{},\"peak_bytes\":{},\"before_bytes\":{},\"after_bytes\":{},",
            "\"open_counters\":{:?},\"publication_counters\":{:?},\"peak_rss_kib\":{},\"verified\":true}}"
        ),
        shared,
        publication.name(),
        cfg!(feature = "probe"),
        open_ns,
        commit_ns,
        open_usage.live_bytes,
        open_usage.peak_bytes,
        usage.live_bytes,
        usage.peak_bytes,
        initial,
        final_bytes,
        opened,
        published,
        rss
    );
}

fn shared_shape(shape: &std::ffi::OsStr) -> bool {
    match shape.to_str().unwrap() {
        "shared" => true,
        "independent" => false,
        _ => panic!("unknown shape"),
    }
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    match args.as_slice() {
        [_, command, shape, path] if command == "seed" => {
            let shared = shared_shape(shape);
            seed(Path::new(path), shared);
        }
        [_, command, shape, publication, path] if command == "measure" => {
            let shared = shared_shape(shape);
            measure(
                Path::new(path),
                shared,
                Publication::parse(publication.to_str().unwrap()),
            );
        }
        [_, command, publication, path] if command == "verify" => {
            let publication = Publication::parse(publication.to_str().unwrap());
            let path = Path::new(path);
            let db = Database::open_file(path, limits()).unwrap();
            verify(&db, Some(publication));
            println!(
                "{{\"event\":\"reopen\",\"publication\":\"{}\",\"revision\":{},\"retained\":{},\"verified\":true}}",
                publication.name(),
                db.snapshot().revision(),
                db.retained_ids().unwrap().len(),
            );
        }
        _ => {
            panic!("expected seed <shape> <db>, measure <shape> <mode> <db> or verify <mode> <db>")
        }
    }
}
