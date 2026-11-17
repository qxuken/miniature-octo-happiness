//! Explicit migration between two quiescent, application-owned CRDT histories.
#[allow(dead_code)]
#[path = "support/study.rs"]
mod study;
use std::{
    net::{TcpListener, TcpStream},
    time::Duration,
};
pub use study::StudyProgress as OldProgress;
use weekend_db::{
    Database, Error, Result,
    crdt::{
        AddWinsSet, GrowCounter, GrowSet, MinRegister, Policy, Record, Replica, ReplicaAck,
        Resolution,
    },
};

pub const OLD: Policy = Policy {
    collection: [9; 16],
    generation: 7,
    actors: [[1; 16], [2; 16]],
};
pub const NEW: Policy = Policy {
    collection: [10; 16],
    generation: 8,
    actors: [[3; 16], [4; 16]],
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Record, Resolution)]
#[record(
    key = b"course-progress",
    format = b"COURSES\x07",
    fields(completed, practice_seconds, planned, weekly_goal, session_limit),
    resolve = true
)]
pub struct ProgressV2 {
    pub completed: GrowSet,
    pub practice_seconds: GrowCounter,
    pub planned: AddWinsSet,
    #[resolve]
    pub weekly_goal: MinRegister,
    #[resolve]
    pub session_limit: MinRegister,
}

/// Visible values reviewed under the OLD policy. Old dots and actor components
/// are deliberately not copied into the new roster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reviewed {
    completed: GrowSet,
    planned: GrowSet,
    seconds: u64,
    weekly_goal: Option<u64>,
    session_limit: Option<u64>,
}

pub fn review(left: OldProgress, right: OldProgress) -> Result<Reviewed> {
    // Caller has stopped BOTH writers. Equality alone cannot enforce quiescence.
    if left != right {
        return Err(Error::Conflict);
    }
    let seconds = left
        .practice_minutes
        .total()
        .checked_mul(60)
        .filter(|&n| n <= u64::MAX / 2)
        .ok_or(Error::Limit)?;
    let mut planned = GrowSet::default();
    for id in left.planned.iter() {
        planned.insert(id)?;
    }
    Ok(Reviewed {
        completed: left.completed,
        planned,
        seconds,
        weekly_goal: left.weekly_goal.effective_value(),
        session_limit: left.session_limit.effective_value(),
    })
}

pub fn validate_transition(old: Policy, new: Policy) -> Result<()> {
    if new.collection == [0; 16]
        || new.collection == old.collection
        || new.generation <= old.generation
        || new.actors[0] == [0; 16]
        || new.actors[0] >= new.actors[1]
        || new.actors.iter().any(|actor| old.actors.contains(actor))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

/// Stage two NEW stores. On any failure discard both staging stores; this is
/// not an atomic transaction across files. Switch routing only after verification.
pub fn stage_pair<'a>(
    a: &'a Database,
    b: &'a Database,
    old: Policy,
    new: Policy,
    reviewed: Reviewed,
) -> Result<(Replica<'a, ProgressV2>, Replica<'a, ProgressV2>)> {
    validate_transition(old, new)?;
    let left = Replica::enroll(a, a.create_collection()?, new, new.actors[0])?;
    let right = Replica::enroll(b, b.create_collection()?, new, new.actors[1])?;
    // Seed the preserved aggregate ONCE, owned by new actor A. Seeding the same
    // counter aggregate independently on both actors would double-count it.
    left.update(|state: &mut ProgressV2, actor| {
        for id in reviewed.completed.iter() {
            state.completed.insert(id)?;
        }
        for id in reviewed.planned.iter() {
            state.planned.insert(id, actor)?;
        }
        if reviewed.seconds != 0 {
            state.practice_seconds.increment(actor, reviewed.seconds)?;
        }
        if let Some(value) = reviewed.weekly_goal {
            state.weekly_goal.assign(actor, value)?;
        }
        if let Some(value) = reviewed.session_limit {
            state.session_limit.assign(actor, value)?;
        }
        Ok(())
    })?;
    converge(&left, &right)?;
    Ok((left, right))
}

/// Demo-owned loopback endpoints, NOT an authenticated deployment transport.
/// Real routing must bind the new certificate identities to the new actor roster.
pub fn exchange<P: Record + Send + Sync>(
    from: &Replica<'_, P>,
    to: &Replica<'_, P>,
) -> (Result<ReplicaAck<P>>, Result<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let sending = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (receiving, _) = listener.accept().unwrap();
    for socket in [&sending, &receiving] {
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
    }
    std::thread::scope(|scope| {
        let received = scope.spawn(|| to.serve_authenticated(receiving));
        let sent = from.push_authenticated(sending);
        (sent, received.join().unwrap())
    })
}
pub fn sync<P: Record + Send + Sync>(from: &Replica<'_, P>, to: &Replica<'_, P>) -> Result<()> {
    let (sent, received) = exchange(from, to);
    received?;
    sent.map(|_| ())
}
pub fn converge<P: Record + Send + Sync>(a: &Replica<'_, P>, b: &Replica<'_, P>) -> Result<()> {
    for _ in 0..3 {
        sync(a, b)?;
        sync(b, a)?;
        if a.read()? == b.read()? {
            return Ok(());
        }
    }
    Err(Error::Conflict)
}

fn main() -> Result<()> {
    let a = Database::new()?;
    let b = Database::new()?;
    let old_a = Replica::<OldProgress>::enroll(&a, a.create_collection()?, OLD, OLD.actors[0])?;
    let old_b = Replica::<OldProgress>::enroll(&b, b.create_collection()?, OLD, OLD.actors[1])?;
    old_a.update(|s, actor| {
        s.practice_minutes.increment(actor, 20)?;
        s.completed.insert(1)?;
        s.planned.insert(2, actor)?;
        s.weekly_goal.assign(actor, 60)?;
        s.session_limit.assign(actor, 30)
    })?;
    old_b.update(|s, actor| {
        s.practice_minutes.increment(actor, 10)?;
        s.completed.insert(3)?;
        s.weekly_goal.assign(actor, 90)?;
        s.session_limit.assign(actor, 20)
    })?;
    // Stop application writes before reconciling and keep the old pair frozen.
    converge(&old_a, &old_b)?;
    let reviewed = review(old_a.read()?, old_b.read()?)?;
    let new_a = Database::new()?;
    let new_b = Database::new()?;
    let (left, right) = stage_pair(&new_a, &new_b, OLD, NEW, reviewed)?;
    assert_eq!(left.read()?.practice_seconds.total(), 1800);
    assert_eq!(left.read()?.weekly_goal.effective_value(), Some(90));
    left.update(|s, a| s.weekly_goal.assign(a, 120))?;
    right.update(|s, a| s.weekly_goal.assign(a, 45))?;
    converge(&left, &right)?;
    assert_eq!(left.read()?.weekly_goal.values().collect::<Vec<_>>(), [45]);
    let revisions = (new_a.snapshot().revision(), new_b.snapshot().revision());
    converge(&left, &right)?;
    assert_eq!(
        revisions,
        (new_a.snapshot().revision(), new_b.snapshot().revision())
    );
    println!(
        "old generation {}: 30 minutes, reviewed goal 90; new generation {}: 1800 seconds, concurrent goals resolve to 45; repeat sync adds no commits",
        OLD.generation, NEW.generation
    );
    println!(
        "demo logical collection {:?}, actor roster {:?}; allocate unique identities for a real migration",
        NEW.collection, NEW.actors
    );
    Ok(())
}
