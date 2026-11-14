use super::*;
use std::collections::BTreeMap;
use weekend_db::{
    Error, Snapshot,
    storage::{APPEND_START, Crash, Fault, IoKind},
};

type Rows = BTreeMap<u8, i64>;

fn rows(db: &Database, view: &Snapshot<'_>) -> Rows {
    view.scan(db.collection_id(1))
        .unwrap()
        .into_iter()
        .map(|(key, value)| {
            assert_eq!(key.len(), 1);
            (key[0], value.as_i64().unwrap())
        })
        .collect()
}

fn check(db: &Database, revision: u64, current: &Rows) {
    let view = db.snapshot();
    assert_eq!(view.revision(), revision);
    assert_eq!(&rows(db, &view), current);
    assert_eq!(db.retained_ids().unwrap(), [RETAINED]);
    let retained = db.retained_snapshot(RETAINED).unwrap();
    assert_eq!(retained.revision(), 2);
    assert_eq!(rows(db, &retained), Rows::from([(0, 11), (1, 12)]));
}

fn open(disk: &SimulatedDisk) -> Database {
    Database::open_on(disk.clone(), Limits::default()).unwrap()
}

fn stage<'db>(db: &'db Database, edits: &[(u8, Option<i64>)]) -> weekend_db::Transaction<'db> {
    let collection = db.collection_id(1);
    let mut tx = db.begin();
    for &(key, value) in edits {
        if let Some(value) = value {
            tx.put(collection, &[key], &db.integer(value).unwrap())
                .unwrap();
        } else {
            tx.delete(collection, &[key]).unwrap();
        }
    }
    tx
}

fn prefix(input: u8) -> usize {
    [0, 63, 64, 72, REGION - 1, REGION][usize::from(input) % 6]
}

fn power_loss(kind: u8, arg: u8) -> Crash {
    match kind % 4 {
        0 => Crash::DropAll,
        1 => Crash::PersistAll,
        2 => Crash::Mask(u64::from(arg)),
        _ => Crash::Torn {
            write: usize::from(arg >> 6),
            prefix: prefix(arg),
        },
    }
}

fn fault_for(kind: IoKind, mode: u8, arg: u8) -> Fault {
    match kind {
        IoKind::Write => match mode % 5 {
            0 => Fault::Error,
            1 => Fault::Short(0),
            2 => Fault::Short(1 + usize::from(arg)),
            3 | 4 => Fault::Tear {
                prefix: prefix(arg),
                durable: mode % 5 == 4,
            },
            _ => unreachable!(),
        },
        IoKind::Sync if mode & 1 != 0 => Fault::SyncError(power_loss(mode >> 1, arg)),
        _ => Fault::Error,
    }
}

// The root is the sole pending write at its barrier. A partial persistence
// may validate, depending on which bytes differ from the previous slot.
fn root_survives(policy: Crash) -> Option<bool> {
    match policy {
        Crash::DropAll => Some(false),
        Crash::PersistAll => Some(true),
        Crash::Mask(mask) => Some(mask & 1 != 0),
        Crash::Torn { write, prefix } if write != 0 || prefix == 0 => Some(false),
        Crash::Torn { prefix, .. } if prefix >= REGION => Some(true),
        Crash::Torn { .. } => None,
    }
}

fn exact_revision(
    acknowledged: bool,
    step: usize,
    append_sync: usize,
    root_sync: usize,
    fault: Fault,
    crash: Crash,
) -> Option<bool> {
    if acknowledged || step > root_sync {
        return Some(true);
    }
    if step <= append_sync {
        return Some(false);
    }
    if step < root_sync {
        // The root write failed. No progress means no new root bytes.
        return match fault {
            Fault::Error | Fault::Short(0) => Some(false),
            Fault::Tear { prefix: 0, .. } => Some(false),
            Fault::Tear { prefix, durable } if prefix >= REGION => {
                if durable {
                    Some(true)
                } else {
                    root_survives(crash)
                }
            }
            _ => None,
        };
    }
    let during_sync = match fault {
        Fault::SyncError(policy) => root_survives(policy),
        _ => Some(false),
    };
    match (during_sync, root_survives(crash)) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

pub fn commit_crash(input: &[u8]) {
    let &[at, mode, power, arg, flags, ref program @ ..] = input else {
        return;
    };
    let edits: Vec<_> = program
        .as_chunks::<2>()
        .0
        .iter()
        .take(12)
        .map(|op| {
            (
                op[0] & 7,
                (op[0] & 0x80 == 0).then_some(i64::from(op[1] as i8)),
            )
        })
        .collect();
    let old = Rows::from([(0, 21), (1, 12)]);
    let mut new = old.clone();
    for &(key, value) in &edits {
        if let Some(value) = value {
            new.insert(key, value);
        } else {
            new.remove(&key);
        }
    }
    let seed = SimulatedDisk::from_image(fixture_image()).unwrap();
    let db = open(&seed);
    let revision = 4 + u64::from(flags & 1);
    if flags & 1 != 0 {
        assert_eq!(db.begin().commit(), Ok(revision));
    }
    check(&db, revision, &old);
    drop(db);
    let image = seed.durable_image().unwrap();

    // The successful trace supplies I/O positions. Logical states come from
    // the copying model above, and the oracle below follows the barriers.
    let clean = SimulatedDisk::from_image(&image).unwrap();
    let db = open(&clean);
    clean.arm(usize::MAX, Fault::Error);
    assert_eq!(stage(&db, &edits).commit(), Ok(revision + 1));
    check(&db, revision + 1, &new);
    let trace = clean.trace().unwrap();
    let syncs: Vec<_> = trace
        .iter()
        .enumerate()
        .filter_map(|(step, event)| (event.kind == IoKind::Sync).then_some(step))
        .collect();
    let [append_sync, root_sync, witness_sync] = syncs[..] else {
        panic!("commit must have three separate barriers");
    };
    let index = ((revision + 1) & 1) as usize;
    assert_eq!(trace[0].kind, IoKind::Length);
    assert!(
        trace[1..append_sync]
            .iter()
            .all(|event| event.kind == IoKind::Write && event.offset >= APPEND_START as u64)
    );
    for (previous, sync, region) in [
        (append_sync, root_sync, 1 + index),
        (root_sync, witness_sync, 3 + index),
    ] {
        assert_eq!(sync, previous + 2);
        let write = trace[previous + 1];
        assert_eq!(
            (write.kind, write.offset, write.len),
            (IoKind::Write, (region * REGION) as u64, REGION)
        );
    }
    let step = syncs
        .get(usize::from(at))
        .copied()
        .unwrap_or_else(|| usize::from(at) % trace.len());
    let fault = fault_for(trace[step].kind, mode, arg);
    let crash = power_loss(power, arg);

    let disk = SimulatedDisk::from_image(&image).unwrap();
    let db = open(&disk);
    disk.arm(step, fault);
    let result = stage(&db, &edits).commit();
    let expected = match fault {
        Fault::Short(n) if n != 0 => Ok(revision + 1),
        _ if step <= append_sync => Err(Error::Io),
        _ => Err(Error::OutcomeUnknown),
    };
    assert_eq!(
        result, expected,
        "step={step}, fault={fault:?}, crash={crash:?}"
    );
    assert_eq!(db.requires_recovery(), result.is_err());
    if result.is_ok() {
        check(&db, revision + 1, &new);
    } else {
        check(&db, revision, &old);
    }
    drop(db);
    disk.crash(crash).unwrap();
    if flags & 2 != 0 {
        disk.arm(0, Fault::Error);
        assert!(matches!(
            Database::open_on(disk.clone(), Limits::default()),
            Err(Error::Io)
        ));
        disk.clear_fault();
    }
    let db = open(&disk);
    let recovered = db.snapshot().revision();
    assert!(recovered == revision || recovered == revision + 1);
    if let Some(next) = exact_revision(result.is_ok(), step, append_sync, root_sync, fault, crash) {
        assert_eq!(
            recovered,
            revision + u64::from(next),
            "step={step}, fault={fault:?}, crash={crash:?}"
        );
    }
    let current = if recovered == revision { &old } else { &new };
    check(&db, recovered, current);
    assert_eq!(db.begin().commit(), Ok(recovered + 1));
    drop(db);
    disk.crash(Crash::DropAll).unwrap();
    check(&open(&disk), recovered + 1, current);
}
