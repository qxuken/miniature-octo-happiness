#![cfg(feature = "crdt")]

#[path = "../examples/support/study.rs"]
mod study;

use study::StudyProgress;

use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time::Duration,
};
use weekend_db::{
    Database, Error, Limits,
    crdt::{AddWinsSet, Counter, CounterPolicy, MaxRegister, MinRegister, Policy, Record, Replica},
    storage::{Crash, Fault, SimulatedDisk},
};

const POLICY: CounterPolicy = CounterPolicy {
    collection: [9; 16],
    generation: 1,
    actors: [[1; 16], [2; 16]],
};

fn exchange<T, U>(
    sender: impl FnOnce(TcpStream) -> T,
    receiver: impl FnOnce(TcpStream) -> U + Send,
) -> (T, U)
where
    U: Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let configure = |stream: TcpStream| {
        let timeout = Some(Duration::from_secs(10));
        stream.set_read_timeout(timeout).unwrap();
        stream.set_write_timeout(timeout).unwrap();
        stream
    };
    // Test-owned loopback transport. Applications authenticate both peers.
    std::thread::scope(|scope| {
        let receiving = scope.spawn(move || receiver(configure(listener.accept().unwrap().0)));
        let sending = sender(configure(TcpStream::connect(address).unwrap()));
        (sending, receiving.join().unwrap())
    })
}

struct RecordOffer<'a>(TcpStream, &'a mut Vec<u8>);
impl Read for RecordOffer<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}
impl Write for RecordOffer<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.0.write(bytes)?;
        self.1.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[test]
fn local_edit_racing_an_incoming_merge_conflicts_and_can_retry() {
    let a = Database::new().unwrap();
    let b = Database::new().unwrap();
    let a_study = Replica::<StudyProgress>::enroll(
        &a,
        a.create_collection().unwrap(),
        POLICY,
        POLICY.actors[0],
    )
    .unwrap();
    let b_study = Replica::<StudyProgress>::enroll(
        &b,
        b.create_collection().unwrap(),
        POLICY,
        POLICY.actors[1],
    )
    .unwrap();
    b_study
        .update(|state, actor| state.practice_minutes.increment(actor, 15))
        .unwrap();
    let before = a.snapshot().revision();
    let edit = |state: &mut StudyProgress, actor| state.practice_minutes.increment(actor, 20);

    std::thread::scope(|scope| {
        let (editing, started) = mpsc::channel();
        let (resume, continue_edit) = mpsc::channel();
        let a_study = &a_study;
        let local = scope.spawn(move || {
            a_study.update(|state, actor| {
                edit(state, actor)?;
                editing.send(()).unwrap();
                continue_edit.recv_timeout(Duration::from_secs(10)).unwrap();
                Ok(())
            })
        });
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        let (ack, served) = exchange(
            |stream| b_study.push_authenticated(stream),
            |stream| a_study.serve_authenticated(stream),
        );
        resume.send(()).unwrap();
        assert_eq!(local.join().unwrap(), Err(Error::Conflict));
        served.unwrap();
        assert_eq!(ack.unwrap().state.practice_minutes.total(), 15);
    });
    assert_eq!(a.snapshot().revision(), before + 1);
    assert_eq!(a_study.update(edit), Ok(before + 2));

    let (ack, served) = exchange(
        |stream| a_study.push_authenticated(stream),
        |stream| b_study.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(ack.unwrap().state.practice_minutes.total(), 35);
    assert_eq!(a_study.read(), b_study.read());
}

#[test]
fn concurrent_add_wins_and_a_later_remove_survives_reopen_and_replay() {
    let limits = Limits::default();
    let a = Database::create_on(SimulatedDisk::new().unwrap(), [10; 16], limits).unwrap();
    let disk = SimulatedDisk::new().unwrap();
    let b = Database::create_on(disk.clone(), [20; 16], limits).unwrap();
    let a_plan =
        Replica::<AddWinsSet>::enroll(&a, a.create_collection().unwrap(), POLICY, POLICY.actors[0])
            .unwrap();
    let b_id = b.create_collection().unwrap();
    let b_plan = Replica::<AddWinsSet>::enroll(&b, b_id, POLICY, POLICY.actors[1]).unwrap();
    let lesson = 7;

    a_plan
        .update(|set, actor| set.insert(lesson, actor))
        .unwrap();
    let mut old_offer = Vec::new();
    let (ack, served) = exchange(
        |stream| a_plan.push_authenticated(RecordOffer(stream, &mut old_offer)),
        |stream| b_plan.serve_authenticated(stream),
    );
    served.unwrap();
    assert!(ack.unwrap().state.contains(lesson));

    a_plan
        .update(|set, _| {
            assert!(set.remove(lesson)?);
            Ok(())
        })
        .unwrap();
    b_plan
        .update(|set, actor| set.insert(lesson, actor))
        .unwrap();
    assert!(!a_plan.read().unwrap().contains(lesson));
    assert!(b_plan.read().unwrap().contains(lesson));

    let (ack, served) = exchange(
        |stream| a_plan.push_authenticated(stream),
        |stream| b_plan.serve_authenticated(stream),
    );
    served.unwrap();
    assert!(ack.unwrap().state.contains(lesson));
    let (ack, served) = exchange(
        |stream| b_plan.push_authenticated(stream),
        |stream| a_plan.serve_authenticated(stream),
    );
    served.unwrap();
    assert!(ack.unwrap().state.contains(lesson));
    assert_eq!(a_plan.read(), b_plan.read());

    a_plan
        .update(|set, _| {
            assert!(set.remove(lesson)?);
            Ok(())
        })
        .unwrap();
    let (ack, served) = exchange(
        |stream| a_plan.push_authenticated(stream),
        |stream| b_plan.serve_authenticated(stream),
    );
    served.unwrap();
    assert!(!ack.unwrap().state.contains(lesson));
    assert_eq!(a_plan.read(), b_plan.read());

    drop(b);
    disk.crash(Crash::DropAll).unwrap();
    let b = Database::open_on(disk, limits).unwrap();
    let b_plan =
        Replica::<AddWinsSet>::open(&b, b.collection_id(b_id.as_u64()), POLICY, POLICY.actors[1])
            .unwrap();
    assert!(!b_plan.read().unwrap().contains(lesson));
    let revision = b.snapshot().revision();
    let (reply, served) = exchange(
        |mut stream| {
            stream.write_all(&old_offer).unwrap();
            let mut reply = Vec::new();
            stream.read_to_end(&mut reply).unwrap();
            reply
        },
        |stream| b_plan.serve_authenticated(stream),
    );
    served.unwrap();
    assert!(!reply.is_empty());
    assert!(!b_plan.read().unwrap().contains(lesson));
    assert_eq!(b.snapshot().revision(), revision);

    a_plan
        .update(|set, actor| set.insert(lesson, actor))
        .unwrap();
    let (ack, served) = exchange(
        |stream| a_plan.push_authenticated(stream),
        |stream| b_plan.serve_authenticated(stream),
    );
    served.unwrap();
    assert!(ack.unwrap().state.contains(lesson));
    assert_eq!(a_plan.read(), b_plan.read());
}

#[test]
fn sync_resolves_both_fields_in_one_commit_and_repeat_sync_is_read_only() {
    let limits = Limits::default();
    let a = Database::create_on(SimulatedDisk::new().unwrap(), [10; 16], limits).unwrap();
    let disk = SimulatedDisk::new().unwrap();
    let b = Database::create_on(disk.clone(), [20; 16], limits).unwrap();
    let a_goal = Replica::<StudyProgress>::enroll(
        &a,
        a.create_collection().unwrap(),
        POLICY,
        POLICY.actors[0],
    )
    .unwrap();
    let b_id = b.create_collection().unwrap();
    let b_goal = Replica::<StudyProgress>::enroll(&b, b_id, POLICY, POLICY.actors[1]).unwrap();
    let a_local = a_goal
        .update(|state, actor| {
            state.completed.insert(1)?;
            state.practice_minutes.increment(actor, 20)?;
            state.planned.insert(2, actor)?;
            state.weekly_goal.assign(actor, 60)?;
            state.session_limit.assign(actor, 20)
        })
        .unwrap();
    let b_local = b_goal
        .update(|state, actor| {
            state.weekly_goal.assign(actor, 90)?;
            state.session_limit.assign(actor, 30)
        })
        .unwrap();

    let mut offer = Vec::new();
    let (ack, served) = exchange(
        |stream| a_goal.push_authenticated(RecordOffer(stream, &mut offer)),
        |stream| b_goal.serve_authenticated(stream),
    );
    served.unwrap();
    let mut expected = [0; 2205];
    expected[..8].copy_from_slice(b"COURSES\x06");
    expected[8..24].copy_from_slice(&POLICY.collection);
    expected[24..32].copy_from_slice(&POLICY.generation.to_le_bytes());
    expected[32..48].copy_from_slice(&POLICY.actors[0]);
    expected[48..64].copy_from_slice(&POLICY.actors[1]);
    let payload = &mut expected[65..];
    payload[..8].copy_from_slice(&2u64.to_le_bytes()); // completed bitmap
    payload[8..16].copy_from_slice(&20u64.to_le_bytes()); // A's counter
    payload[88..96].copy_from_slice(&1u64.to_le_bytes()); // A's add of lesson 2
    for (offset, value) in [(2072, 60u64), (2106, 20)] {
        payload[offset..offset + 8].copy_from_slice(&1u64.to_le_bytes());
        payload[offset + 8] = 1;
        payload[offset + 9..offset + 17].copy_from_slice(&value.to_le_bytes());
    }
    assert_eq!(offer, expected);

    let resolved = ack.unwrap().state;
    assert_eq!(resolved.weekly_goal.values().collect::<Vec<_>>(), [90]);
    assert_eq!(resolved.session_limit.values().collect::<Vec<_>>(), [20]);
    assert_eq!(b.snapshot().revision(), b_local + 1);
    assert_eq!(a.snapshot().revision(), a_local);

    drop(b);
    disk.crash(Crash::DropAll).unwrap();
    let b = Database::open_on(disk, limits).unwrap();
    let b_goal = Replica::<StudyProgress>::open(
        &b,
        b.collection_id(b_id.as_u64()),
        POLICY,
        POLICY.actors[1],
    )
    .unwrap();
    assert_eq!(b_goal.read(), Ok(resolved));
    let (ack, served) = exchange(
        |stream| b_goal.push_authenticated(stream),
        |stream| a_goal.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(ack.unwrap().state, resolved);
    assert_eq!(a_goal.read(), b_goal.read());
    assert_eq!(a.snapshot().revision(), a_local + 1);

    for (sender, receiver) in [(&a_goal, &b_goal), (&b_goal, &a_goal)] {
        let (ack, served) = exchange(
            |stream| sender.push_authenticated(stream),
            |stream| receiver.serve_authenticated(stream),
        );
        served.unwrap();
        assert_eq!(ack.unwrap().state, resolved);
    }
    assert_eq!(a.snapshot().revision(), a_local + 1);
    assert_eq!(b.snapshot().revision(), b_local + 1);
}

#[test]
fn peers_commit_the_same_policy_result_offline_and_converge_on_one_goal() {
    let limits = Limits::default();
    let a = Database::create_on(SimulatedDisk::new().unwrap(), [10; 16], limits).unwrap();
    let disk = SimulatedDisk::new().unwrap();
    let b = Database::create_on(disk.clone(), [20; 16], limits).unwrap();
    let a_goal = Replica::<StudyProgress<false>>::enroll(
        &a,
        a.create_collection().unwrap(),
        POLICY,
        POLICY.actors[0],
    )
    .unwrap();
    let b_id = b.create_collection().unwrap();
    let b_goal =
        Replica::<StudyProgress<false>>::enroll(&b, b_id, POLICY, POLICY.actors[1]).unwrap();
    assert_eq!(a_goal.read().unwrap().effective_weekly_goal(), None);
    a_goal
        .update(|state, actor| state.weekly_goal.assign(actor, 60))
        .unwrap();
    b_goal
        .update(|state, actor| state.weekly_goal.assign(actor, 90))
        .unwrap();
    let stale_review = a_goal.read().unwrap().weekly_goal;
    assert_eq!(stale_review.values().collect::<Vec<_>>(), [60]);
    assert_eq!(
        b_goal
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [90]
    );

    let mut old_offer = Vec::new();
    let (ack, served) = exchange(
        |stream| a_goal.push_authenticated(RecordOffer(stream, &mut old_offer)),
        |stream| b_goal.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(
        ack.unwrap().state.weekly_goal.values().collect::<Vec<_>>(),
        [60, 90]
    );
    let (ack, served) = exchange(
        |stream| b_goal.push_authenticated(stream),
        |stream| a_goal.serve_authenticated(stream),
    );
    served.unwrap();
    let reviewed = ack.unwrap().state.weekly_goal;
    assert_eq!(a_goal.read(), b_goal.read());
    assert_eq!(reviewed.values().collect::<Vec<_>>(), [60, 90]);
    let mut encoded = [0; MaxRegister::SIZE];
    reviewed.encode(&mut encoded).unwrap();
    assert_eq!(
        MinRegister::decode(&encoded).unwrap().effective_value(),
        Some(60)
    );
    assert_eq!(a_goal.read().unwrap().effective_weekly_goal(), Some(90));
    assert_eq!(b_goal.read().unwrap().effective_weekly_goal(), Some(90));

    let revision = a.snapshot().revision();
    assert_eq!(
        a_goal.update(|state, actor| state.resolve_goal(actor, stale_review, 75)),
        Err(Error::Conflict)
    );
    assert_eq!(a.snapshot().revision(), revision);
    a_goal
        .update(|state, actor| state.resolve_goal(actor, reviewed, 70))
        .unwrap();
    b_goal
        .update(|state, actor| state.resolve_goal(actor, reviewed, 80))
        .unwrap();
    assert_eq!(
        a_goal
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [70]
    );
    assert_eq!(
        b_goal
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [80]
    );

    let mut old_resolution = Vec::new();
    let (ack, served) = exchange(
        |stream| a_goal.push_authenticated(RecordOffer(stream, &mut old_resolution)),
        |stream| b_goal.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(
        ack.unwrap().state.weekly_goal.values().collect::<Vec<_>>(),
        [70, 80]
    );
    let (ack, served) = exchange(
        |stream| b_goal.push_authenticated(stream),
        |stream| a_goal.serve_authenticated(stream),
    );
    served.unwrap();
    let both_resolutions = ack.unwrap().state.weekly_goal;
    assert_eq!(both_resolutions.values().collect::<Vec<_>>(), [70, 80]);
    assert_eq!(a_goal.read(), b_goal.read());
    assert_eq!(a_goal.read().unwrap().effective_weekly_goal(), Some(80));
    assert_eq!(b_goal.read().unwrap().effective_weekly_goal(), Some(80));
    assert_eq!(
        a_goal.update(|state, actor| state.resolve_goal(actor, reviewed, 75)),
        Err(Error::Conflict)
    );

    let revision_a = a.snapshot().revision();
    let revision_b = b.snapshot().revision();
    assert_eq!(
        a_goal.update(|state, actor| state.weekly_goal.resolve(actor)),
        Ok(revision_a + 1)
    );
    assert_eq!(
        b_goal.update(|state, actor| state.weekly_goal.resolve(actor)),
        Ok(revision_b + 1)
    );
    assert_eq!(
        a_goal
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [80]
    );
    assert_eq!(
        b_goal
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [80]
    );

    drop(b);
    disk.crash(Crash::DropAll).unwrap();
    let b = Database::open_on(disk.clone(), limits).unwrap();
    let b_goal = Replica::<StudyProgress<false>>::open(
        &b,
        b.collection_id(b_id.as_u64()),
        POLICY,
        POLICY.actors[1],
    )
    .unwrap();
    assert_eq!(b.snapshot().revision(), revision_b + 1);
    assert_eq!(
        b_goal
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [80]
    );
    let (ack, served) = exchange(
        |stream| a_goal.push_authenticated(stream),
        |stream| b_goal.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(
        ack.unwrap().state.weekly_goal.values().collect::<Vec<_>>(),
        [80]
    );
    let (ack, served) = exchange(
        |stream| b_goal.push_authenticated(stream),
        |stream| a_goal.serve_authenticated(stream),
    );
    served.unwrap();
    let both_policy_commits = ack.unwrap().state.weekly_goal;
    assert_eq!(both_policy_commits.values().collect::<Vec<_>>(), [80]);
    assert_eq!(a_goal.read(), b_goal.read());

    a_goal
        .update(|state, actor| state.resolve_goal(actor, both_policy_commits, 75))
        .unwrap();
    let (ack, served) = exchange(
        |stream| a_goal.push_authenticated(stream),
        |stream| b_goal.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(
        ack.unwrap().state.weekly_goal.values().collect::<Vec<_>>(),
        [75]
    );

    drop(b);
    disk.crash(Crash::DropAll).unwrap();
    let b = Database::open_on(disk, limits).unwrap();
    let b_goal = Replica::<StudyProgress<false>>::open(
        &b,
        b.collection_id(b_id.as_u64()),
        POLICY,
        POLICY.actors[1],
    )
    .unwrap();
    assert_eq!(a_goal.read(), b_goal.read());
    let revision = b.snapshot().revision();
    for offer in [old_offer, old_resolution] {
        let (reply, served) = exchange(
            |mut stream| {
                stream.write_all(&offer).unwrap();
                let mut reply = Vec::new();
                stream.read_to_end(&mut reply).unwrap();
                reply
            },
            |stream| b_goal.serve_authenticated(stream),
        );
        served.unwrap();
        assert!(!reply.is_empty());
    }
    assert_eq!(b.snapshot().revision(), revision);
    assert_eq!(a_goal.read().unwrap().effective_weekly_goal(), Some(75));
    assert_eq!(b_goal.read().unwrap().effective_weekly_goal(), Some(75));
    assert_eq!(
        b_goal
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [75]
    );
}

#[test]
fn offline_peers_converge_on_one_record_with_a_set_and_counter() {
    let policy = Policy {
        collection: [8; 16],
        ..POLICY
    };
    let limits = Limits::default();
    let a = Database::create_on(SimulatedDisk::new().unwrap(), [10; 16], limits).unwrap();
    let disk = SimulatedDisk::new().unwrap();
    let b = Database::create_on(disk.clone(), [20; 16], limits).unwrap();
    let a_progress = Replica::<StudyProgress>::enroll(
        &a,
        a.create_collection().unwrap(),
        policy,
        policy.actors[0],
    )
    .unwrap();
    let b_id = b.create_collection().unwrap();
    let b_progress = Replica::<StudyProgress>::enroll(&b, b_id, policy, policy.actors[1]).unwrap();

    a_progress
        .update(|state, actor| {
            state.completed.insert(1)?;
            state.completed.insert(3)?;
            state.practice_minutes.increment(actor, 20)
        })
        .unwrap();
    let retained = a.retain_snapshot(&a.snapshot()).unwrap();
    b_progress
        .update(|state, actor| {
            state.completed.insert(2)?;
            state.completed.insert(3)?;
            state.practice_minutes.increment(actor, 15)
        })
        .unwrap();
    assert_eq!(
        a_progress
            .read()
            .unwrap()
            .completed
            .iter()
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(
        b_progress
            .read()
            .unwrap()
            .completed
            .iter()
            .collect::<Vec<_>>(),
        [2, 3]
    );

    let before = b.snapshot().revision();
    let (ack, served) = exchange(
        |stream| a_progress.push_authenticated(stream),
        |stream| b_progress.serve_authenticated(stream),
    );
    served.unwrap();
    let merged = ack.unwrap().state;
    assert_eq!(merged.completed.iter().collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(merged.practice_minutes.total(), 35);
    assert_eq!(b.snapshot().revision(), before + 1);

    drop(b);
    disk.crash(Crash::DropAll).unwrap();
    let b = Database::open_on(disk, limits).unwrap();
    let b_progress = Replica::<StudyProgress>::open(
        &b,
        b.collection_id(b_id.as_u64()),
        policy,
        policy.actors[1],
    )
    .unwrap();
    assert_eq!(b_progress.read(), Ok(merged));
    let (ack, served) = exchange(
        |stream| b_progress.push_authenticated(stream),
        |stream| a_progress.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(ack.unwrap().state, merged);
    assert_eq!(a_progress.read(), Ok(merged));
    let old = a_progress
        .read_at(&a.retained_snapshot(retained).unwrap())
        .unwrap();
    assert_eq!(old.completed.iter().collect::<Vec<_>>(), [1, 3]);
    assert_eq!(old.practice_minutes.total(), 20);

    assert_eq!(
        a_progress.update(|state, actor| {
            state.practice_minutes.increment(actor, 10)?;
            state.completed.insert(64)?;
            Ok(())
        }),
        Err(Error::Bounds)
    );
    assert_eq!(a_progress.read(), Ok(merged));
    a_progress
        .update(|state, _| {
            state.completed.insert(63)?;
            Ok(())
        })
        .unwrap();
    let (ack, served) = exchange(
        |stream| a_progress.push_authenticated(stream),
        |stream| b_progress.serve_authenticated(stream),
    );
    served.unwrap();
    let merged = ack.unwrap().state;
    assert_eq!(b_progress.read(), a_progress.read());
    assert!(merged.completed.contains(63));
    assert_eq!(merged.practice_minutes.total(), 35);

    let revision = b.snapshot().revision();
    let (ack, served) = exchange(
        |stream| a_progress.push_authenticated(stream),
        |stream| b_progress.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(ack.unwrap().state, merged);
    assert_eq!(b.snapshot().revision(), revision);
}

#[test]
fn generic_edits_preserve_existing_state_and_the_other_actors_component() {
    let a = Database::new().unwrap();
    let b = Database::new().unwrap();
    let progress = Replica::<StudyProgress>::enroll(
        &a,
        a.create_collection().unwrap(),
        POLICY,
        POLICY.actors[0],
    )
    .unwrap();
    let peer = Replica::<StudyProgress>::enroll(
        &b,
        b.create_collection().unwrap(),
        POLICY,
        POLICY.actors[1],
    )
    .unwrap();
    let mut other_actor = None;
    peer.update(|_, actor| {
        other_actor = Some(actor);
        Ok(())
    })
    .unwrap();

    let revision = progress
        .update(|state, actor| {
            state.completed.insert(1)?;
            state.practice_minutes.increment(actor, 20)
        })
        .unwrap();
    let previous = progress.read().unwrap();
    assert_eq!(
        progress.update(|state, _| {
            state.completed = Default::default();
            Ok(())
        }),
        Err(Error::Conflict)
    );
    assert_eq!(
        progress.update(|state, _| {
            state.completed.insert(2)?;
            state.practice_minutes.increment(other_actor.unwrap(), 1)
        }),
        Err(Error::Conflict)
    );
    assert_eq!(progress.read(), Ok(previous));
    assert_eq!(a.snapshot().revision(), revision);

    let (ack, served) = exchange(
        |stream| progress.push_authenticated(stream),
        |stream| peer.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(ack.unwrap().state, previous);
    assert_eq!(peer.read(), Ok(previous));
}

#[test]
fn offline_increments_converge_and_delayed_offers_leave_newer_state_intact() {
    let limits = Limits::default();
    let a = Database::create_on(SimulatedDisk::new().unwrap(), [10; 16], limits).unwrap();
    let b = Database::create_on(SimulatedDisk::new().unwrap(), [20; 16], limits).unwrap();
    let ac = a.create_collection().unwrap();
    b.create_collection().unwrap(); // Local collection numbers can differ.
    let bc = b.create_collection().unwrap();
    let counter_a = Counter::enroll(&a, ac, POLICY, POLICY.actors[0]).unwrap();
    let counter_b = Counter::enroll(&b, bc, POLICY, POLICY.actors[1]).unwrap();

    counter_a.increment(2).unwrap();
    let before_sync = a.retain_snapshot(&a.snapshot()).unwrap();
    counter_b.increment(3).unwrap();
    assert_eq!(counter_a.count(), Ok(2));
    assert_eq!(counter_b.count(), Ok(3));

    let mut first_offer = Vec::new();
    let (ack, served) = exchange(
        |stream| counter_a.push_authenticated(RecordOffer(stream, &mut first_offer)),
        |stream| counter_b.serve_authenticated(stream),
    );
    served.unwrap();
    let ack = ack.unwrap();
    assert_eq!(ack.actor, POLICY.actors[1]);
    assert_eq!(ack.components, [2, 3]);
    assert_eq!(
        first_offer,
        [
            b"WDBGCTR\x01".as_slice(),
            &POLICY.collection,
            &POLICY.generation.to_le_bytes(),
            &POLICY.actors[0],
            &POLICY.actors[1],
            &[0],
            &2u64.to_le_bytes(),
            &0u64.to_le_bytes(),
        ]
        .concat()
    );
    assert_eq!(counter_a.count(), Ok(2));
    assert_eq!(counter_b.count(), Ok(5));

    counter_a.increment(4).unwrap();
    let (ack, served) = exchange(
        |stream| counter_a.push_authenticated(stream),
        |stream| counter_b.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(ack.unwrap().components, [6, 3]);

    let revision = b.snapshot().revision();
    let (reply, served) = exchange(
        |mut stream| {
            stream.write_all(&first_offer).unwrap();
            let mut reply = Vec::new();
            stream.read_to_end(&mut reply).unwrap();
            reply
        },
        |stream| counter_b.serve_authenticated(stream),
    );
    served.unwrap();
    assert!(!reply.is_empty());
    assert_eq!(b.snapshot().revision(), revision);
    assert_eq!(counter_b.count(), Ok(9));

    let (ack, served) = exchange(
        |stream| counter_b.push_authenticated(stream),
        |stream| counter_a.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(ack.unwrap().components, [6, 3]);
    assert_eq!(counter_a.count(), Ok(9));
    assert_eq!(
        counter_a.count_at(&a.retained_snapshot(before_sync).unwrap()),
        Ok(2)
    );
}

#[test]
fn a_lost_merge_reply_is_recoverable_and_a_retry_needs_no_commit() {
    struct LoseReply(TcpStream);
    impl Read for LoseReply {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.0.read(bytes)
        }
    }
    impl Write for LoseReply {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let limits = Limits::default();
    let a = Database::create_on(SimulatedDisk::new().unwrap(), [10; 16], limits).unwrap();
    let disk = SimulatedDisk::new().unwrap();
    let b = Database::create_on(disk.clone(), [20; 16], limits).unwrap();
    let counter_a =
        Counter::enroll(&a, a.create_collection().unwrap(), POLICY, POLICY.actors[0]).unwrap();
    let id = b.create_collection().unwrap().as_u64();
    let counter_b = Counter::enroll(&b, b.collection_id(id), POLICY, POLICY.actors[1]).unwrap();
    let local = counter_a.increment(7).unwrap();
    let before_merge = b.snapshot().revision();
    disk.arm(0, Fault::Error); // Fail the merge's first storage operation.
    let (reply, served) = exchange(
        |stream| counter_a.push_authenticated(stream),
        |stream| counter_b.serve_authenticated(stream),
    );
    assert_eq!(reply, Err(Error::Io));
    assert_eq!(served, Err(Error::Io));
    assert!(b.requires_recovery());
    drop(b);
    disk.clear_fault();
    disk.crash(Crash::DropAll).unwrap();

    let b = Database::open_on(disk.clone(), limits).unwrap();
    let counter_b = Counter::open(&b, b.collection_id(id), POLICY, POLICY.actors[1]).unwrap();
    assert_eq!(counter_b.count(), Ok(0));
    assert_eq!(b.snapshot().revision(), before_merge);
    let (reply, served) = exchange(
        |stream| counter_a.push_authenticated(stream),
        |stream| counter_b.serve_authenticated(LoseReply(stream)),
    );
    assert_eq!(reply, Err(Error::Io));
    assert_eq!(served, Err(Error::Io));
    assert_eq!(a.snapshot().revision(), local);
    let merged = b.snapshot().revision();
    drop(b);
    disk.crash(Crash::DropAll).unwrap();

    let b = Database::open_on(disk, limits).unwrap();
    let counter_b = Counter::open(&b, b.collection_id(id), POLICY, POLICY.actors[1]).unwrap();
    assert_eq!(counter_b.count(), Ok(7));
    assert_eq!(b.snapshot().revision(), merged);
    let (reply, served) = exchange(
        |stream| counter_a.push_authenticated(stream),
        |stream| counter_b.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(reply.unwrap().components, [7, 0]);
    assert_eq!(b.snapshot().revision(), merged);
}

#[test]
fn a_peer_rejects_a_different_policy_generation_before_writing() {
    let a = Database::new().unwrap();
    let b = Database::new().unwrap();
    let counter_a =
        Counter::enroll(&a, a.create_collection().unwrap(), POLICY, POLICY.actors[0]).unwrap();
    let newer = CounterPolicy {
        generation: 2,
        ..POLICY
    };
    let id = b.create_collection().unwrap();
    let counter_b = Counter::enroll(&b, id, newer, POLICY.actors[1]).unwrap();
    assert_eq!(
        Counter::open(&b, id, POLICY, POLICY.actors[1]).err(),
        Some(Error::Conflict)
    );
    assert_eq!(
        Counter::open(&b, id, newer, POLICY.actors[0]).err(),
        Some(Error::ForeignStore)
    );

    counter_a.increment(2).unwrap();
    let before = b.snapshot().revision();
    let (reply, served) = exchange(
        |stream| counter_a.push_authenticated(stream),
        |stream| counter_b.serve_authenticated(stream),
    );
    assert_eq!(reply, Err(Error::Io));
    assert_eq!(served, Err(Error::Conflict));
    assert_eq!(counter_b.count(), Ok(0));
    assert_eq!(b.snapshot().revision(), before);
}

#[test]
fn a_stale_actor_copy_cannot_claim_the_peers_acknowledgment() {
    let limits = Limits::default();
    let disk = SimulatedDisk::new().unwrap();
    let a = Database::create_on(disk.clone(), [10; 16], limits).unwrap();
    let b = Database::new().unwrap();
    let id = a.create_collection().unwrap().as_u64();
    let counter_a = Counter::enroll(&a, a.collection_id(id), POLICY, POLICY.actors[0]).unwrap();
    let counter_b =
        Counter::enroll(&b, b.create_collection().unwrap(), POLICY, POLICY.actors[1]).unwrap();
    let stale = disk.durable_image().unwrap();
    counter_a.increment(5).unwrap();
    let (reply, served) = exchange(
        |stream| counter_a.push_authenticated(stream),
        |stream| counter_b.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(reply.unwrap().components, [5, 0]);

    let restored = Database::open_on(SimulatedDisk::from_image(&stale).unwrap(), limits).unwrap();
    let stale_actor = Counter::open(
        &restored,
        restored.collection_id(id),
        POLICY,
        POLICY.actors[0],
    )
    .unwrap();
    stale_actor.increment(1).unwrap();
    let (reply, served) = exchange(
        |stream| stale_actor.push_authenticated(stream),
        |stream| counter_b.serve_authenticated(stream),
    );
    served.unwrap();
    assert_eq!(reply, Err(Error::Conflict));
    assert_eq!(stale_actor.count(), Ok(1));
    assert_eq!(counter_b.count(), Ok(5));
}

#[test]
fn a_lossy_application_codec_cannot_publish_an_unreadable_edit() {
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    struct Lossy(u8);
    impl Record for Lossy {
        const KEY: &'static [u8] = b"lossy";
        const FORMAT: &'static [u8; 8] = b"LOSSY001";
        const SIZE: usize = 1;
        fn encode(self, bytes: &mut [u8]) -> weekend_db::Result<()> {
            bytes[0] = 0;
            Ok(())
        }
        fn decode(bytes: &[u8]) -> weekend_db::Result<Self> {
            Ok(Self(bytes[0]))
        }
        fn merge(&mut self, other: Self) {
            self.0 = self.0.max(other.0);
        }
        fn check_peer(self, _: Self, _: weekend_db::crdt::Actor) -> weekend_db::Result<()> {
            Ok(())
        }
    }
    let db = Database::new().unwrap();
    let id = db.create_collection().unwrap();
    let replica = Replica::<Lossy>::enroll(&db, id, POLICY, POLICY.actors[0]).unwrap();
    let before = db.snapshot().revision();
    assert_eq!(
        replica.update(|record, _| {
            record.0 = 1;
            Ok(())
        }),
        Err(Error::Invalid)
    );
    assert_eq!(db.snapshot().revision(), before);
    assert_eq!(replica.read().unwrap().0, 0);
    assert_eq!(
        Replica::<Lossy>::open(&db, id, POLICY, POLICY.actors[0])
            .unwrap()
            .read()
            .unwrap()
            .0,
        0
    );
}
