#![cfg(feature = "crdt")]
#[allow(dead_code)]
#[path = "../examples/crdt_policy_migration.rs"]
mod migration;
use migration::{NEW, OLD, OldProgress, ProgressV2, converge, exchange, review, stage_pair};
use std::io::{self, Cursor, Read, Write};
use weekend_db::{
    Database, Error, Limits,
    crdt::{GrowCounter, Policy, Record, Replica},
    storage::SimulatedDisk,
};

fn enroll_old<'a, 'b>(
    a: &'a Database,
    b: &'b Database,
) -> (Replica<'a, OldProgress>, Replica<'b, OldProgress>) {
    (
        Replica::enroll(a, a.create_collection().unwrap(), OLD, OLD.actors[0]).unwrap(),
        Replica::enroll(b, b.create_collection().unwrap(), OLD, OLD.actors[1]).unwrap(),
    )
}
fn revisions(a: &Database, b: &Database) -> (u64, u64) {
    (a.snapshot().revision(), b.snapshot().revision())
}

#[test]
fn reconciled_history_is_seeded_once_and_changed_policy_converges() {
    let a = Database::new().unwrap();
    let b = Database::new().unwrap();
    let (old_a, old_b) = enroll_old(&a, &b);
    old_a
        .update(|s, actor| {
            s.practice_minutes.increment(actor, 20)?;
            s.completed.insert(1)?;
            s.planned.insert(2, actor)?;
            s.planned.insert(5, actor)?;
            assert!(s.planned.remove(5)?);
            s.weekly_goal.assign(actor, 60)?;
            s.session_limit.assign(actor, 30)
        })
        .unwrap();
    old_b
        .update(|s, actor| {
            s.practice_minutes.increment(actor, 10)?;
            s.completed.insert(3)?;
            s.planned.insert(4, actor)?;
            s.weekly_goal.assign(actor, 90)?;
            s.session_limit.assign(actor, 20)
        })
        .unwrap();
    assert_eq!(
        review(old_a.read().unwrap(), old_b.read().unwrap()),
        Err(Error::Conflict)
    );
    converge(&old_a, &old_b).unwrap();
    let baseline = old_a.read().unwrap();
    let retained = a.retain_snapshot(&a.snapshot()).unwrap();
    let old_revisions = revisions(&a, &b);
    let x = Database::new().unwrap();
    let y = Database::new().unwrap();
    let (new_a, new_b) = stage_pair(
        &x,
        &y,
        OLD,
        NEW,
        review(baseline, old_b.read().unwrap()).unwrap(),
    )
    .unwrap();
    let converted = new_a.read().unwrap();
    assert_eq!(converted, new_b.read().unwrap());
    assert_eq!(converted.practice_seconds.total(), 1800);
    assert_eq!(converted.completed.iter().collect::<Vec<_>>(), [1, 3]);
    assert_eq!(converted.planned.iter().collect::<Vec<_>>(), [2, 4]);
    assert!(!converted.planned.contains(5));
    assert_eq!(converted.weekly_goal.values().collect::<Vec<_>>(), [90]);
    assert_eq!(converted.session_limit.values().collect::<Vec<_>>(), [20]);
    let stable = revisions(&x, &y);
    converge(&new_a, &new_b).unwrap();
    assert_eq!(stable, revisions(&x, &y));
    new_a
        .update(|s, a| {
            s.practice_seconds.increment(a, 60)?;
            s.weekly_goal.assign(a, 120)
        })
        .unwrap();
    new_b
        .update(|s, a| {
            s.practice_seconds.increment(a, 30)?;
            s.weekly_goal.assign(a, 45)
        })
        .unwrap();
    converge(&new_a, &new_b).unwrap();
    assert_eq!(new_a.read().unwrap(), new_b.read().unwrap());
    assert_eq!(new_a.read().unwrap().practice_seconds.total(), 1890);
    assert_eq!(
        new_a
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [45]
    );
    let stable = revisions(&x, &y);
    converge(&new_a, &new_b).unwrap();
    assert_eq!(stable, revisions(&x, &y));
    assert_eq!(revisions(&a, &b), old_revisions);
    assert_eq!(
        old_a
            .read_at(&a.retained_snapshot(retained).unwrap())
            .unwrap(),
        baseline
    );
}

struct Channel {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
}
impl Read for Channel {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.input.read(b)
    }
}
impl Write for Channel {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn raw<P: Record>(db: &Database) -> Vec<u8> {
    db.snapshot()
        .get(db.collection_id(1), P::KEY)
        .unwrap()
        .unwrap()
        .as_bytes()
        .unwrap()
        .to_vec()
}

#[test]
fn obsolete_wire_generation_collection_and_actor_roster_reject_without_publication() {
    let db = Database::new().unwrap();
    let receiver =
        Replica::<ProgressV2>::enroll(&db, db.create_collection().unwrap(), NEW, NEW.actors[1])
            .unwrap();
    let old = Database::new().unwrap();
    Replica::<OldProgress>::enroll(&old, old.create_collection().unwrap(), OLD, OLD.actors[0])
        .unwrap();
    let mut cases = vec![(raw::<OldProgress>(&old), Error::UnsupportedFormat)];
    for (policy, error) in [
        (
            Policy {
                generation: OLD.generation,
                ..NEW
            },
            Error::Conflict,
        ),
        (
            Policy {
                collection: OLD.collection,
                ..NEW
            },
            Error::ForeignStore,
        ),
        (
            Policy {
                actors: OLD.actors,
                ..NEW
            },
            Error::Conflict,
        ),
    ] {
        let foreign = Database::new().unwrap();
        Replica::<ProgressV2>::enroll(
            &foreign,
            foreign.create_collection().unwrap(),
            policy,
            policy.actors[0],
        )
        .unwrap();
        cases.push((raw::<ProgressV2>(&foreign), error));
    }
    let before = (db.snapshot().revision(), raw::<ProgressV2>(&db));
    for (bytes, error) in cases {
        let mut channel = Channel {
            input: Cursor::new(bytes),
            output: Vec::new(),
        };
        assert_eq!(receiver.serve_authenticated(&mut channel), Err(error));
        assert!(channel.output.is_empty());
        assert_eq!((db.snapshot().revision(), raw::<ProgressV2>(&db)), before);
    }
    assert!(matches!(
        Replica::<ProgressV2>::open(&db, db.collection_id(1), NEW, OLD.actors[0]),
        Err(Error::ForeignStore)
    ));
    assert_eq!((db.snapshot().revision(), raw::<ProgressV2>(&db)), before);
}

#[test]
fn stale_backup_is_inspected_without_resuming_identity_and_recovery_starts_new_history() {
    let limits = Limits::default();
    let disk = SimulatedDisk::new().unwrap();
    let a = Database::create_on(disk.clone(), [21; 16], limits).unwrap();
    let b = Database::new().unwrap();
    let (old_a, old_b) = enroll_old(&a, &b);
    let backup = disk.durable_image().unwrap();
    old_a
        .update(|s, actor| {
            s.practice_minutes.increment(actor, 5)?;
            s.completed.insert(7)?;
            s.weekly_goal.assign(actor, 90)
        })
        .unwrap();
    converge(&old_a, &old_b).unwrap();
    // Actor A is now retired. The surviving peer also has an offline event.
    drop(a);
    old_b
        .update(|s, actor| s.practice_minutes.increment(actor, 3))
        .unwrap();
    let restored = Database::open_on(SimulatedDisk::from_image(&backup).unwrap(), limits).unwrap();
    let stale =
        Replica::<OldProgress>::open(&restored, restored.collection_id(1), OLD, OLD.actors[0])
            .unwrap();
    // Opening alone cannot identify rollback. Test the old sync path, without
    // making any new local edit under the restored actor's identity.
    let before = revisions(&restored, &b);
    let (ack, served) = exchange(&stale, &old_b);
    assert_eq!(served, Ok(()));
    assert_eq!(ack, Err(Error::Conflict));
    assert_eq!(before, revisions(&restored, &b));
    assert_eq!(stale.read().unwrap().practice_minutes.total(), 0);
    let survivor = old_b.read().unwrap();
    let mut reviewed = survivor;
    reviewed.merge(stale.read().unwrap());
    assert_eq!(reviewed, survivor); // survivor covers every event in this backup
    let seed = review(reviewed, reviewed).unwrap();
    let da = SimulatedDisk::new().unwrap();
    let db = SimulatedDisk::new().unwrap();
    let x = Database::create_on(da.clone(), [31; 16], limits).unwrap();
    let y = Database::create_on(db.clone(), [32; 16], limits).unwrap();
    let (new_a, new_b) = stage_pair(&x, &y, OLD, NEW, seed).unwrap();
    assert_eq!(new_a.read().unwrap().practice_seconds.total(), 480);
    assert!(new_a.read().unwrap().completed.contains(7));
    assert_eq!(new_a.read().unwrap(), new_b.read().unwrap());
    drop(x);
    drop(y);
    let x = Database::open_on(da, limits).unwrap();
    let y = Database::open_on(db, limits).unwrap();
    let new_a = Replica::<ProgressV2>::open(&x, x.collection_id(1), NEW, NEW.actors[0]).unwrap();
    let new_b = Replica::<ProgressV2>::open(&y, y.collection_id(1), NEW, NEW.actors[1]).unwrap();
    let stable = revisions(&x, &y);
    converge(&new_a, &new_b).unwrap();
    assert_eq!(stable, revisions(&x, &y));
    new_b
        .update(|s, a| s.practice_seconds.increment(a, 60))
        .unwrap();
    converge(&new_a, &new_b).unwrap();
    assert_eq!(new_a.read().unwrap().practice_seconds.total(), 540);
    let stable = revisions(&x, &y);
    converge(&new_a, &new_b).unwrap();
    assert_eq!(stable, revisions(&x, &y));
    assert_eq!(before, revisions(&restored, &b));
}

#[test]
fn migration_rejects_identity_reuse_and_unrepresentable_values_before_staging() {
    let x = Database::new().unwrap();
    let y = Database::new().unwrap();
    let seed = review(OldProgress::default(), OldProgress::default()).unwrap();
    let before = revisions(&x, &y);
    for policy in [
        OLD,
        Policy {
            actors: OLD.actors,
            ..NEW
        },
        Policy {
            generation: OLD.generation,
            ..NEW
        },
        Policy {
            collection: OLD.collection,
            ..NEW
        },
    ] {
        assert!(matches!(
            stage_pair(&x, &y, OLD, policy, seed),
            Err(Error::Invalid)
        ));
        assert_eq!(before, revisions(&x, &y));
    }
    for (minutes, accepted) in [
        (u64::MAX / 2 / 60, true),
        (u64::MAX / 2 / 60 + 1, false),
        (u64::MAX / 2, false),
    ] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&minutes.to_le_bytes());
        let old = OldProgress {
            practice_minutes: GrowCounter::from_le_bytes(bytes).unwrap(),
            ..Default::default()
        };
        assert_eq!(review(old, old).is_ok(), accepted);
    }
    assert_eq!(before, revisions(&x, &y));
}

#[cfg(feature = "fault-injection")]
#[test]
fn failed_staging_leaves_old_pair_usable_and_requires_discarding_partial_targets() {
    let a = Database::new().unwrap();
    let b = Database::new().unwrap();
    let (old_a, old_b) = enroll_old(&a, &b);
    old_a
        .update(|s, actor| s.practice_minutes.increment(actor, 2))
        .unwrap();
    converge(&old_a, &old_b).unwrap();
    let payload = old_a.read().unwrap();
    let reviewed = review(payload, old_b.read().unwrap()).unwrap();
    let old_revisions = revisions(&a, &b);
    let x = Database::new().unwrap();
    let y = Database::new().unwrap();
    y.fail_allocations_after(Some(0));
    assert!(matches!(
        stage_pair(&x, &y, OLD, NEW, reviewed),
        Err(Error::OutOfMemory)
    ));
    assert!(x.snapshot().revision() > 0); // cross-file staging is not atomic
    assert_eq!(revisions(&a, &b), old_revisions);
    assert_eq!(old_a.read().unwrap(), payload);
    drop(x);
    drop(y);
    let x = Database::new().unwrap();
    let y = Database::new().unwrap();
    let retry_policy = Policy {
        collection: [11; 16],
        generation: 9,
        actors: [[5; 16], [6; 16]],
    };
    let (left, right) = stage_pair(&x, &y, OLD, retry_policy, reviewed).unwrap();
    assert_eq!(left.read().unwrap().practice_seconds.total(), 120);
    assert_eq!(left.read().unwrap(), right.read().unwrap());
    assert_eq!(revisions(&a, &b), old_revisions);
}
