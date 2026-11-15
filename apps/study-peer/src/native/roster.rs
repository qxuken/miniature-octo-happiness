use super::{
    AppResult, COLLECTION_ROW, enroll,
    protocol::{Enrollment, StudyReplica},
    study::StudyProgress,
    tls::bounded_file,
};
use std::path::Path;
use weekend_db::{
    Database, Error, Limits, Result,
    crdt::{Actor, GrowSet, Policy},
};

fn parse_id(id: &str) -> AppResult<[u8; 16]> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("identity must be 32 hexadecimal digits".into());
    }
    Ok(u128::from_str_radix(id, 16)?.to_be_bytes())
}

pub(super) fn enrollment(actor: &str, policy: Policy) -> AppResult<Enrollment> {
    let actor = match actor {
        "a" => policy.actors[0],
        "b" => policy.actors[1],
        _ => return Err("actor must be a or b".into()),
    };
    Ok(Enrollment { policy, actor })
}

pub(super) fn policy_file(path: &Path) -> AppResult<Policy> {
    let bytes = bounded_file(path, 256)?;
    let mut words = std::str::from_utf8(&bytes)?.split_ascii_whitespace();
    let (Some("study-policy-v1"), Some(collection), Some(generation), Some(a), Some(b), None) = (
        words.next(),
        words.next(),
        words.next(),
        words.next(),
        words.next(),
        words.next(),
    ) else {
        return Err(Error::UnsupportedFormat.into());
    };
    let policy = Policy {
        collection: parse_id(collection)?,
        generation: generation.parse()?,
        actors: [parse_id(a)?, parse_id(b)?],
    };
    if policy.collection == [0; 16]
        || policy.generation == 0
        || policy.actors[0] == [0; 16]
        || policy.actors[0] >= policy.actors[1]
    {
        return Err(Error::Invalid.into());
    }
    Ok(policy)
}

#[derive(Debug, PartialEq, Eq)]
struct Visible {
    minutes: u64,
    completed: GrowSet,
    planned: GrowSet,
    goal: Option<u64>,
    limit: Option<u64>,
}

impl Visible {
    fn read(state: StudyProgress) -> Result<Self> {
        let mut planned = GrowSet::default();
        for id in state.planned.iter() {
            planned.insert(id)?;
        }
        Ok(Self {
            minutes: state.practice_minutes.total(),
            completed: state.completed,
            planned,
            goal: state.weekly_goal.effective_value(),
            limit: state.session_limit.effective_value(),
        })
    }

    fn seed(&self, state: &mut StudyProgress, actor: Actor) -> Result<()> {
        state.completed = self.completed;
        for id in self.planned.iter() {
            state.planned.insert(id, actor)?;
        }
        if self.minutes != 0 {
            state.practice_minutes.increment(actor, self.minutes)?;
        }
        if let Some(goal) = self.goal {
            state.weekly_goal.assign(actor, goal)?;
        }
        if let Some(limit) = self.limit {
            state.session_limit.assign(actor, limit)?;
        }
        Ok(())
    }

    fn report(&self) {
        println!(
            "visible minutes={} completed={:?} planned={:?} goal={:?} limit={:?}",
            self.minutes,
            self.completed.iter().collect::<Vec<_>>(),
            self.planned.iter().collect::<Vec<_>>(),
            self.goal,
            self.limit,
        );
    }
}

fn agreed(a: &Database, b: &Database, policy: Policy) -> Result<Visible> {
    let read = |db: &Database, actor| {
        StudyReplica::open(db, db.collection_id(COLLECTION_ROW), policy, actor)?.read()
    };
    let left = read(a, policy.actors[0])?;
    if left != read(b, policy.actors[1])? {
        return Err(Error::Conflict);
    }
    Visible::read(left)
}

pub(super) fn stage(
    old_files: [&Path; 2],
    old: Policy,
    new_files: [&Path; 2],
    new: Policy,
) -> AppResult<()> {
    if new.collection == old.collection
        || new.generation <= old.generation
        || new.actors.iter().any(|actor| old.actors.contains(actor))
    {
        return Err(Error::Invalid.into());
    }
    let a = Database::open_file(old_files[0], Limits::default())?;
    let b = Database::open_file(old_files[1], Limits::default())?;
    let visible = agreed(&a, &b, old)?;
    if visible.minutes > u64::MAX / 2 {
        return Err(Error::Limit.into());
    }
    let left = Database::create_file(new_files[0], Limits::default())?;
    let right = Database::create_file(new_files[1], Limits::default())?;
    let seeded = enroll(&left, enrollment("a", new)?)?;
    enroll(&right, enrollment("b", new)?)?;
    // Seed A only. Authenticated staging sync brings the same causal state to B.
    seeded.update(|state, actor| visible.seed(state, actor))?;
    if Visible::read(seeded.read()?)? != visible {
        return Err(Error::Corrupt.into());
    }
    println!(
        "staged generation={} old_revisions={} {}",
        new.generation,
        a.snapshot().revision(),
        b.snapshot().revision()
    );
    visible.report();
    Ok(())
}

pub(super) fn verify(files: [&Path; 2], policy: Policy) -> AppResult<()> {
    let a = Database::open_file(files[0], Limits::default())?;
    let b = Database::open_file(files[1], Limits::default())?;
    let visible = agreed(&a, &b, policy)?;
    println!(
        "verified generation={} revisions={} {}",
        policy.generation,
        a.snapshot().revision(),
        b.snapshot().revision()
    );
    visible.report();
    Ok(())
}
