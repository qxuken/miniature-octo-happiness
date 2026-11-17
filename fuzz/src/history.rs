use super::fixture_image;
use std::collections::BTreeMap;
use weekend_db::{
    Database, Error, Limits, Snapshot,
    storage::{Crash, Fault, IoKind, REGION, SimulatedDisk},
};

type Rows = BTreeMap<u8, i64>;
#[derive(Clone)]
struct State {
    revision: u64,
    data_revision: u64,
    rows: Rows,
}
struct Model {
    current: State,
    retained: BTreeMap<u64, State>,
    next_id: u64,
}
impl Model {
    fn fixture() -> Self {
        Self {
            current: State {
                revision: 4,
                data_revision: 4,
                rows: Rows::from([(0, 21), (1, 12)]),
            },
            retained: BTreeMap::from([(
                1,
                State {
                    revision: 2,
                    data_revision: 2,
                    rows: Rows::from([(0, 11), (1, 12)]),
                },
            )]),
            next_id: 2,
        }
    }
    fn check(&self, db: &Database) {
        self.current.check(&db.snapshot(), db);
        assert_eq!(
            db.retained_ids().unwrap(),
            self.retained.keys().copied().collect::<Vec<_>>()
        );
        for (&id, state) in &self.retained {
            state.check(&db.retained_snapshot(id).unwrap(), db);
        }
    }
}
impl State {
    fn check(&self, snapshot: &Snapshot<'_>, db: &Database) {
        assert_eq!(snapshot.revision(), self.revision);
        assert_eq!(snapshot.data_revision(), self.data_revision);
        let rows = snapshot
            .scan(db.collection_id(1))
            .unwrap()
            .into_iter()
            .map(|(key, value)| {
                assert_eq!(key.len(), 1);
                (key[0], value.as_i64().unwrap())
            })
            .collect::<Rows>();
        assert_eq!(rows, self.rows);
    }
}
fn open(disk: &SimulatedDisk) -> Database {
    Database::open_on(disk.clone(), Limits::default()).unwrap()
}
fn program(
    mut db: Database,
    mut disk: SimulatedDisk,
    model: &mut Model,
    input: &[u8],
) -> (Database, SimulatedDisk) {
    for &[op, key, value] in input.as_chunks::<3>().0.iter().take(12) {
        let key = key % 8;
        match op % 7 {
            mode @ 0..=1 => {
                let mut tx = db.begin();
                if mode == 0 {
                    tx.put(
                        db.collection_id(1),
                        &[key],
                        &db.integer(i64::from(value as i8)).unwrap(),
                    )
                    .unwrap();
                    model.current.rows.insert(key, i64::from(value as i8));
                } else {
                    tx.delete(db.collection_id(1), &[key]).unwrap();
                    model.current.rows.remove(&key);
                }
                model.current.revision += 1;
                model.current.data_revision = model.current.revision;
                assert_eq!(tx.commit(), Ok(model.current.revision));
            }
            2 => {
                let id = db.retain_snapshot(&db.snapshot()).unwrap();
                assert_eq!(id, model.next_id);
                model.retained.insert(id, model.current.clone());
                model.next_id += 1;
                model.current.revision += 1;
            }
            3 => {
                let id = u64::from(value) % model.next_id;
                let expected = if model.retained.remove(&id).is_some() {
                    model.current.revision += 1;
                    Ok(())
                } else {
                    Err(Error::NotFound)
                };
                assert_eq!(db.release_retained(id), expected);
            }
            4 => {
                model.current.revision += 1;
                assert_eq!(db.begin().commit(), Ok(model.current.revision));
            }
            5 => {
                drop(db);
                disk.crash(Crash::DropAll).unwrap();
                db = open(&disk);
            }
            _ => {
                let before = disk.durable_image().unwrap();
                let destination = SimulatedDisk::new().unwrap();
                db.compact_to(destination.clone()).unwrap();
                assert_eq!(disk.durable_image().unwrap(), before);
                drop(db);
                destination.crash(Crash::DropAll).unwrap();
                disk = destination;
                db = open(&disk);
            }
        }
        model.check(&db);
    }
    (db, disk)
}

pub fn image_and_ops(input: &[u8]) {
    let Some((&mode, commands)) = input.split_first() else {
        return;
    };
    let disk = SimulatedDisk::from_image(fixture_image()).unwrap();
    let mut model = Model::fixture();
    let split = (commands.len() / 6).min(12) * 3;
    let (db, disk) = program(open(&disk), disk, &mut model, &commands[..split]);
    drop(db);
    let mut image = disk.durable_image().unwrap();
    let slot = (1 + (model.current.revision & 1) as usize) * REGION;
    let mode = mode % 5;
    match mode {
        1 => image.extend_from_slice(&input[..input.len().min(64)]),
        2 => {
            let witness = (3 + (model.current.revision & 1) as usize) * REGION;
            image[witness..witness + REGION].fill(0);
        }
        3 => {
            image.pop();
        }
        4 => {
            let descriptor =
                u64::from_le_bytes(image[slot + 40..slot + 48].try_into().unwrap()) as usize;
            image[descriptor + 64] ^= 1;
        }
        _ => {}
    }
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let result = Database::open_on(disk.clone(), Limits::default());
    if mode >= 3 {
        assert!(matches!(result, Err(Error::Corrupt)));
        assert_eq!(disk.durable_image().unwrap(), image);
        assert!(
            disk.trace()
                .unwrap()
                .iter()
                .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
        );
        return;
    }
    let db = result.unwrap();
    model.check(&db);
    let (db, disk) = program(db, disk, &mut model, &commands[split..]);
    drop(db);
    disk.crash(Crash::DropAll).unwrap();
    model.check(&open(&disk));
}

pub fn retention_compact_crash(input: &[u8]) {
    let &[at, mode, power, ref commands @ ..] = input else {
        return;
    };
    let disk = SimulatedDisk::from_image(fixture_image()).unwrap();
    let mut model = Model::fixture();
    let (mut db, disk) = program(open(&disk), disk, &mut model, commands);
    let source = disk.durable_image().unwrap();
    let clean = SimulatedDisk::new().unwrap();
    db.compact_to(clean.clone()).unwrap();
    let trace = clean.trace().unwrap();
    let step = usize::from(at) % trace.len();
    let event = trace[step];
    let fault = match event.kind {
        IoKind::Write => match mode % 4 {
            0 => Fault::Error,
            1 => Fault::Short(1),
            _ => Fault::Tear {
                prefix: usize::from(power) % REGION,
                durable: mode & 1 != 0,
            },
        },
        IoKind::Sync if mode & 1 != 0 => Fault::SyncError(Crash::PersistAll),
        IoKind::Read if mode & 1 != 0 => Fault::Short(1),
        _ => Fault::Error,
    };
    let target = SimulatedDisk::new().unwrap();
    target.arm(step, fault);
    let result = db.compact_to(target.clone());
    match fault {
        Fault::Short(_) => assert!(result.is_ok()),
        _ => assert!(matches!(result, Err(Error::Io | Error::OutcomeUnknown))),
    }
    assert_eq!(disk.durable_image().unwrap(), source);
    model.check(&db);
    assert!(!db.requires_recovery());
    target.clear_fault();
    target
        .crash(if power & 1 == 0 {
            Crash::DropAll
        } else {
            Crash::PersistAll
        })
        .unwrap();
    match Database::open_on(target, Limits::default()) {
        Ok(copy) => model.check(&copy),
        Err(error) => {
            assert!(result.is_err());
            assert!(matches!(error, Error::Corrupt | Error::UnsupportedFormat));
        }
    }
    // A failed copy leaves a usable source, including its persistent counters.
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    assert_eq!(retained, model.next_id);
    model.retained.insert(retained, model.current.clone());
    model.current.revision += 1;
    drop(db);
    disk.crash(Crash::DropAll).unwrap();
    model.check(&open(&disk));
}
