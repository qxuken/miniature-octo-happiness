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
    const CUTS: [usize; 13] = [
        0,
        63,
        64,
        72,
        REGION - 1,
        REGION,
        65,
        66,
        67,
        68,
        69,
        70,
        71,
    ];
    CUTS[usize::from(input) % CUTS.len()]
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

// A metadata barrier has exactly one pending write at index zero.
fn surviving_prefix(policy: Crash, pending: usize) -> usize {
    match policy {
        Crash::PersistAll => pending,
        Crash::Mask(mask) if mask & 1 != 0 => pending,
        Crash::Torn { write: 0, prefix } => prefix.min(pending),
        _ => 0,
    }
}

fn persisted_metadata(kind: IoKind, fault: Fault, crash: Crash) -> usize {
    match (kind, fault) {
        (IoKind::Write, Fault::Tear { prefix, durable }) => {
            let immediate = if durable { prefix } else { 0 };
            immediate.max(surviving_prefix(crash, prefix))
        }
        (IoKind::Sync, Fault::SyncError(during)) => {
            surviving_prefix(during, REGION).max(surviving_prefix(crash, REGION))
        }
        (IoKind::Sync, _) => surviving_prefix(crash, REGION),
        _ => 0,
    }
}

fn check_inspection(disk: &SimulatedDisk, image: &[u8], revision: u64, witnessed: bool) {
    disk.arm(usize::MAX, Fault::Error);
    let info = weekend_db::storage::inspect_on(disk.clone(), Limits::default()).unwrap();
    disk.clear_fault();
    assert_eq!((info.revision, info.witnessed), (revision, witnessed));
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|io| matches!(io.kind, IoKind::Read | IoKind::Length))
    );
    assert!(disk.durable_image().unwrap() == image);
}

fn interrupt_recovery(
    disk: &SimulatedDisk,
    image: &mut [u8],
    revision: u64,
    expected_witness: &[u8],
    faults: &[[u8; 4]],
    capped_attempts: u8,
) {
    let witness = (3 + (revision & 1) as usize) * REGION;
    let range = witness..witness + REGION;
    check_inspection(
        disk,
        image,
        revision,
        image[range.clone()] == *expected_witness,
    );

    // Discover the I/O positions on a separate medium. The actual
    // medium does not complete an open between generated failures.
    let control = SimulatedDisk::from_image(image).unwrap();
    control.arm(usize::MAX, Fault::Error);
    let opened = open(&control);
    assert_eq!(opened.snapshot().revision(), revision);
    let peak = opened.memory_usage().peak_bytes;
    drop(opened);
    let trace = control.trace().unwrap();
    let stabilize = trace.len() - 3;
    let write = stabilize + 1;
    let sync = stabilize + 2;
    assert_eq!(
        (trace[stabilize].kind, trace[sync].kind),
        (IoKind::Sync, IoKind::Sync)
    );
    assert_eq!(
        (trace[write].kind, trace[write].offset, trace[write].len),
        (IoKind::Write, witness as u64, REGION),
    );
    assert!(
        trace[..stabilize]
            .iter()
            .all(|io| matches!(io.kind, IoKind::Read | IoKind::Length))
    );
    let first = trace.iter().position(|io| io.kind == IoKind::Read).unwrap();
    let last = trace
        .iter()
        .rposition(|io| io.kind == IoKind::Read)
        .unwrap();
    drop(control);

    // One byte below the successful open's peak forces a budget refusal.
    // Calibrate on a second copy so this probe cannot stabilize the test disk.
    let capped = (capped_attempts != 0).then(|| {
        let limits = Limits {
            max_live_bytes: peak - 1,
            ..Limits::default()
        };
        let calibration = SimulatedDisk::from_image(image).unwrap();
        calibration.arm(usize::MAX, Fault::Error);
        assert!(matches!(
            Database::open_on(calibration.clone(), limits),
            Err(Error::Limit)
        ));
        let reads = calibration.trace().unwrap();
        assert!(reads.len() <= stabilize);
        assert_eq!(reads, trace[..reads.len()]);
        assert!(calibration.durable_image().unwrap() == image);
        (limits, reads.len())
    });

    for (attempt, &[cut, mode, power, arg]) in faults.iter().enumerate() {
        let step = [first, last, stabilize, write, sync][usize::from(cut) % 5];
        let fault = match (trace[step].kind, mode % 4) {
            (IoKind::Write, 0) => Fault::Error,
            (IoKind::Write, 1) => Fault::Short(0),
            (IoKind::Write, tear) => Fault::Tear {
                prefix: prefix(arg),
                durable: tear == 3,
            },
            (kind, _) => fault_for(kind, mode, arg),
        };
        let crash = power_loss(power, arg);
        let (limits, stop) = if capped_attempts & (1 << attempt) != 0 {
            let (limits, reads) = capped.unwrap();
            (limits, reads.min(step + 1))
        } else {
            (Limits::default(), step + 1)
        };
        let io_reached = stop > step;
        let expected = if io_reached { Error::Io } else { Error::Limit };
        disk.arm(step, fault);
        assert_eq!(
            Database::open_on(disk.clone(), limits).err(),
            Some(expected),
            "recovery attempt {attempt}, step {step}, fault {fault:?}"
        );
        assert_eq!(disk.trace().unwrap(), trace[..stop]);
        disk.crash(crash).unwrap();

        if io_reached && step >= write {
            let persisted = persisted_metadata(trace[step].kind, fault, crash);
            image[witness..witness + persisted].copy_from_slice(&expected_witness[..persisted]);
        }
        check_inspection(
            disk,
            image,
            revision,
            image[range.clone()] == *expected_witness,
        );
    }
}

pub fn commit_crash(input: &[u8]) {
    let &[at, mode, power, arg, flags, ref program @ ..] = input else {
        return;
    };
    // The first two flags retain their original meanings. The next two
    // give the number of four-byte recovery faults before the edit stream.
    // Bits four through six also cap the corresponding recovery attempt.
    let recovery_bytes = usize::from((flags >> 2) & 3) * 4;
    let Some((recovery, program)) = program.split_at_checked(recovery_bytes) else {
        return;
    };
    let (recovery, []) = recovery.as_chunks::<4>() else {
        unreachable!();
    };
    let capped_attempts = (flags >> 4) & ((1 << recovery.len()) - 1);
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
    drop(db);
    let complete = clean.durable_image().unwrap();
    drop(clean);
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
    // The control supplies the exact before/after record bytes. At every
    // supported cut, the body ends up wholly old or wholly new. A torn
    // checksum may equal the complete checksum because some bytes agree.
    let mut metadata = image[..APPEND_START].to_vec();
    let [root, witness] = [1 + index, 3 + index].map(|region| region * REGION);
    assert!(image[root..root + REGION] != complete[root..root + REGION]);
    if result.is_ok() || step > append_sync {
        let (root_len, witness_len) = if result.is_ok() {
            (REGION, REGION)
        } else if step <= root_sync {
            (persisted_metadata(trace[step].kind, fault, crash), 0)
        } else {
            (REGION, persisted_metadata(trace[step].kind, fault, crash))
        };
        for (start, len) in [(root, root_len), (witness, witness_len)] {
            metadata[start..start + len].copy_from_slice(&complete[start..start + len]);
        }
    }
    let mut durable = disk.durable_image().unwrap();
    assert!(
        durable[..APPEND_START] == metadata,
        "metadata differs at step {step}, {fault:?}, {crash:?}"
    );
    let next = metadata[root..root + REGION] == complete[root..root + REGION];
    let recovered = revision + u64::from(next);
    let selected_witness = (3 + (recovered & 1) as usize) * REGION;
    let reference = if next { &complete } else { &image };
    let expected_witness = &reference[selected_witness..selected_witness + REGION];

    if flags & 2 != 0 {
        disk.arm(0, Fault::Error);
        assert!(matches!(
            Database::open_on(disk.clone(), Limits::default()),
            Err(Error::Io)
        ));
        disk.clear_fault();
    }
    if !recovery.is_empty() {
        interrupt_recovery(
            &disk,
            &mut durable,
            recovered,
            expected_witness,
            recovery,
            capped_attempts,
        );
    }
    let db = open(&disk);
    durable[selected_witness..selected_witness + REGION].copy_from_slice(expected_witness);
    assert!(disk.durable_image().unwrap() == durable);
    let current = if recovered == revision { &old } else { &new };
    check(&db, recovered, current);
    assert_eq!(db.begin().commit(), Ok(recovered + 1));
    drop(db);
    disk.crash(Crash::DropAll).unwrap();
    check(&open(&disk), recovered + 1, current);
}
