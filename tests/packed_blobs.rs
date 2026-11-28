//! Format-4 payload budgets apply equally to deferred reads and strict checks.
use weekend_db::{
    Database, DatabaseOptions, Error, Limits, ReadOnlyDatabase,
    storage::{self, ByteStore, FileCapacity, SimulatedDisk},
};

struct Short(SimulatedDisk);
impl ByteStore for Short {
    fn lock(&mut self) -> weekend_db::Result<()> {
        self.0.lock()
    }
    fn unlock(&mut self) {
        self.0.unlock()
    }
    fn read(&mut self, pos: u64, out: &mut [u8]) -> weekend_db::Result<usize> {
        let n = out.len().min(997);
        self.0.read(pos, &mut out[..n])
    }
    fn write(&mut self, pos: u64, data: &[u8]) -> weekend_db::Result<usize> {
        self.0.write(pos, data)
    }
    fn sync(&mut self) -> weekend_db::Result<()> {
        self.0.sync()
    }
    fn length(&mut self) -> weekend_db::Result<u64> {
        self.0.length()
    }
}
fn fixture(n: usize, edit: bool) -> (SimulatedDisk, u64) {
    let limits = Limits {
        max_bytes: n,
        ..Limits::default()
    };
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on_with_options(
        disk.clone(),
        [42; 16],
        DatabaseOptions::from(limits).with_file_capacity(FileCapacity::MiB64),
    )
    .unwrap();
    let c = db.create_collection().unwrap();
    let put = |key: &[u8], byte| {
        let mut t = db.begin();
        t.put(c, key, &db.bytes(&vec![byte; n]).unwrap()).unwrap();
        t.commit().unwrap();
    };
    put(b"a", 23);
    let id = db.retain_snapshot(&db.snapshot()).unwrap();
    if edit {
        put(b"a", 29);
    }
    (disk, id)
}
#[test]
fn blob_streaming_and_materialization_obey_independent_limits() {
    let n = 2 << 20;
    let (disk, _) = fixture(n, false);
    let limits = Limits {
        max_bytes: n,
        max_value_bytes: n + 16,
        max_live_bytes: 128 << 10,
        ..Limits::default()
    };
    let db = ReadOnlyDatabase::open_on(Short(disk.clone()), limits.into()).unwrap();
    db.check().unwrap();
    assert_eq!(
        db.snapshot().get(db.collection_id(1), b"a").err(),
        Some(Error::Limit)
    );
    drop(db);
    let db = ReadOnlyDatabase::open_on(
        Short(disk.clone()),
        Limits {
            max_live_bytes: 3 << 20,
            ..limits
        }
        .into(),
    )
    .unwrap();
    let value = db
        .snapshot()
        .get(db.collection_id(1), b"a")
        .unwrap()
        .unwrap();
    assert!(value.as_bytes().unwrap().iter().all(|&b| b == 23));
    drop(db);
    let too_small = Limits {
        max_value_bytes: n + 15,
        ..limits
    };
    assert_eq!(
        ReadOnlyDatabase::open_on(disk.clone(), too_small.into()).err(),
        Some(Error::Limit)
    );
    assert_eq!(Database::open_on(disk, too_small).err(), Some(Error::Limit));
}

#[test]
fn copied_shared_blob_keeps_birth_of_older_retained_row() {
    let (disk, id) = fixture(8000, true);
    let mut image = disk.durable_image().unwrap();
    let leaves: Vec<_> = image
        .as_chunks::<4096>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, p)| &p[..8] == b"WDBOBJ01" && p[8..12] == 4u32.to_le_bytes())
        .map(|(i, _)| 4096 * i)
        .collect();
    assert_eq!(leaves.len(), 2);
    let descriptor = 64 + 4 + 1 + 8 + 4;
    let src = leaves[0] + descriptor;
    let dst = leaves[1] + descriptor;
    assert_eq!(image[src], 8);
    image.copy_within(src..src + 25, dst);
    let page = &mut image[leaves[1]..leaves[1] + 4096];
    page[48..56].fill(0);
    let mut crc = 0u64;
    for &byte in page.iter() {
        crc ^= u64::from(byte) << 56;
        for _ in 0..8 {
            crc = (crc << 1) ^ (0x42f0_e1eb_a9ea_3693 & 0u64.wrapping_sub(crc >> 63));
        }
    }
    page[48..56].copy_from_slice(&crc.to_le_bytes());
    let source = SimulatedDisk::from_image(&image).unwrap();
    let target = SimulatedDisk::new().unwrap();
    storage::backup_on(source, target.clone(), Limits::default()).unwrap();
    let view = ReadOnlyDatabase::open_on(target, Limits::default().into()).unwrap();
    for s in [view.snapshot(), view.retained_snapshot(id).unwrap()] {
        let v = s.get(view.collection_id(1), b"a").unwrap().unwrap();
        assert_eq!(v.as_bytes().unwrap(), &[23; 8000]);
    }
}
