use super::{
    AppResult, COLLECTION_ROW, enroll,
    protocol::{Enrollment, StudyReplica},
    study::StudyProgress,
    tls::bounded_file,
};
use crate::diagnostics::{self, Phase};
use sha2::{Digest, Sha256};
use std::path::Path;
use weekend_db::{
    Database, Error, Limits, Result,
    crdt::{Actor, GrowSet, Policy, Record},
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

fn agreed(a: &Database, b: &Database, policy: Policy) -> Result<StudyProgress> {
    let read = |db: &Database, actor| {
        StudyReplica::open(db, db.collection_id(COLLECTION_ROW), policy, actor)?.read()
    };
    let left = read(a, policy.actors[0])?;
    if left != read(b, policy.actors[1])? {
        return Err(Error::Conflict);
    }
    Ok(left)
}

#[derive(Clone, Copy)]
enum Receipt {
    Checkpoint,
    Staged,
}

impl Receipt {
    fn line(self, pair: [&Database; 2], policy: Policy, state: StudyProgress) -> Result<String> {
        let tag = match self {
            Self::Checkpoint => "study-roster-checkpoint-v1",
            Self::Staged => "study-roster-stage-v1",
        };
        let mut hash = Sha256::new();
        hash.update(tag);
        hash.update(policy.collection);
        hash.update(policy.generation.to_le_bytes());
        for actor in policy.actors {
            hash.update(actor);
        }
        for db in pair {
            hash.update(db.storage_identity()?);
            if matches!(self, Self::Checkpoint) {
                hash.update(db.snapshot().revision().to_le_bytes());
            }
        }
        let mut payload = [0; StudyProgress::<true>::SIZE];
        state.encode(&mut payload)?;
        hash.update(StudyProgress::<true>::FORMAT);
        hash.update(payload);
        Ok(format!("{tag} {:x}\n", hash.finalize()))
    }

    fn require(
        self,
        expected: &[u8],
        pair: [&Database; 2],
        policy: Policy,
        state: StudyProgress,
    ) -> Result<()> {
        if self.line(pair, policy, state)?.as_bytes() != expected {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}

pub(super) fn checkpoint(files: [&Path; 2], policy: Policy) -> AppResult<()> {
    let a = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(files[0], Limits::default())
    })?;
    let b = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(files[1], Limits::default())
    })?;
    let state = agreed(&a, &b, policy)?;
    print!("{}", Receipt::Checkpoint.line([&a, &b], policy, state)?);
    Ok(())
}

pub(super) fn stage(
    old_files: [&Path; 2],
    old: Policy,
    new_files: [&Path; 2],
    new: Policy,
    expected_old: Option<&[u8]>,
) -> AppResult<()> {
    if new.collection == old.collection
        || new.generation <= old.generation
        || new.actors.iter().any(|actor| old.actors.contains(actor))
    {
        return Err(Error::Invalid.into());
    }
    let a = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(old_files[0], Limits::default())
    })?;
    let b = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(old_files[1], Limits::default())
    })?;
    let state = agreed(&a, &b, old)?;
    if let Some(expected) = expected_old {
        Receipt::Checkpoint.require(expected, [&a, &b], old, state)?;
    }
    let visible = Visible::read(state)?;
    if visible.minutes > u64::MAX / 2 {
        return Err(Error::Limit.into());
    }
    let left = diagnostics::storage(Phase::StorageCreate, || {
        Database::create_file(new_files[0], Limits::default())
    })?;
    let right = diagnostics::storage(Phase::StorageCreate, || {
        Database::create_file(new_files[1], Limits::default())
    })?;
    let seeded = enroll(&left, enrollment("a", new)?)?;
    enroll(&right, enrollment("b", new)?)?;
    // Seed A only. Authenticated staging sync brings the same causal state to B.
    seeded.update(|state, actor| visible.seed(state, actor))?;
    let seed = seeded.read()?;
    if Visible::read(seed)? != visible {
        return Err(Error::Corrupt.into());
    }
    let receipt = expected_old
        .map(|_| Receipt::Staged.line([&left, &right], new, seed))
        .transpose()?;
    println!(
        "staged generation={} old_revisions={} {}",
        new.generation,
        a.snapshot().revision(),
        b.snapshot().revision()
    );
    visible.report();
    if let Some(receipt) = receipt {
        print!("{receipt}");
    }
    Ok(())
}

pub(super) fn verify(
    files: [&Path; 2],
    policy: Policy,
    expected_staged: Option<&[u8]>,
) -> AppResult<()> {
    let a = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(files[0], Limits::default())
    })?;
    let b = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(files[1], Limits::default())
    })?;
    let state = agreed(&a, &b, policy)?;
    if let Some(expected) = expected_staged {
        Receipt::Staged.require(expected, [&a, &b], policy, state)?;
    }
    let visible = Visible::read(state)?;
    println!(
        "verified generation={} revisions={} {}",
        policy.generation,
        a.snapshot().revision(),
        b.snapshot().revision()
    );
    visible.report();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::tests::Directory;
    use std::fs;
    use weekend_db::storage::MemoryStore;

    const POLICY: Policy = Policy {
        collection: [9; 16],
        generation: 0x0102_0304_0506_0708,
        actors: [[1; 16], [2; 16]],
    };

    #[test]
    fn receipt_preimage_binds_policy_ordered_store_ids_and_state() {
        let a = Database::create_on(MemoryStore::new().unwrap(), [0x41; 16], Limits::default())
            .unwrap();
        let b = Database::create_on(MemoryStore::new().unwrap(), [0x42; 16], Limits::default())
            .unwrap();
        a.create_collection().unwrap();
        b.create_collection().unwrap();
        b.create_collection().unwrap();
        let pair = [&a, &b];
        let mut state = StudyProgress::default();
        state.completed.insert(42).unwrap();
        let mut payload = [0; StudyProgress::<true>::SIZE];
        state.encode(&mut payload).unwrap();

        let preimage = |tag: &str, revisions: bool| {
            let mut bytes = tag.as_bytes().to_vec();
            bytes.extend([9; 16]);
            bytes.extend([8, 7, 6, 5, 4, 3, 2, 1]);
            bytes.extend([1; 16]);
            bytes.extend([2; 16]);
            bytes.extend([0x41; 16]);
            if revisions {
                bytes.extend(1u64.to_le_bytes());
            }
            bytes.extend([0x42; 16]);
            if revisions {
                bytes.extend(2u64.to_le_bytes());
            }
            bytes.extend(b"COURSES\x06");
            bytes.extend(payload);
            format!("{tag} {:x}\n", Sha256::digest(bytes))
        };
        let checkpoint = "study-roster-checkpoint-v1";
        let staged = "study-roster-stage-v1";
        assert_eq!(
            Receipt::Checkpoint.line(pair, POLICY, state).unwrap(),
            preimage(checkpoint, true)
        );
        assert_eq!(
            Receipt::Staged.line(pair, POLICY, state).unwrap(),
            preimage(staged, false)
        );
        for (receipt, tag, revisions) in [
            (Receipt::Checkpoint, checkpoint, true),
            (Receipt::Staged, staged, false),
        ] {
            let expected = preimage(tag, revisions);
            receipt
                .require(expected.as_bytes(), pair, POLICY, state)
                .unwrap();
            assert_eq!(
                receipt.require(expected.trim_end().as_bytes(), pair, POLICY, state),
                Err(Error::Conflict)
            );
            let volatile = Database::new().unwrap();
            assert_eq!(
                receipt.require(expected.as_bytes(), [&a, &volatile], POLICY, state),
                Err(Error::Invalid)
            );
        }
    }

    #[test]
    fn agreed_distinguishes_missing_records_and_disagreement() {
        let a = Database::new().unwrap();
        let b = Database::new().unwrap();
        for db in [&a, &b] {
            db.create_collection().unwrap();
        }
        assert_eq!(agreed(&a, &b, POLICY), Err(Error::NotFound));
        let left = StudyReplica::enroll(
            &a,
            a.collection_id(COLLECTION_ROW),
            POLICY,
            POLICY.actors[0],
        )
        .unwrap();
        assert_eq!(agreed(&a, &b, POLICY), Err(Error::NotFound));
        StudyReplica::enroll(
            &b,
            b.collection_id(COLLECTION_ROW),
            POLICY,
            POLICY.actors[1],
        )
        .unwrap();
        let missing = Database::new().unwrap();
        missing.create_collection().unwrap();
        assert_eq!(agreed(&missing, &b, POLICY), Err(Error::NotFound));
        assert_eq!(agreed(&a, &b, POLICY).unwrap(), StudyProgress::default());
        left.update(|state, _| {
            state.completed.insert(42)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(agreed(&a, &b, POLICY), Err(Error::Conflict));
    }

    #[test]
    fn stage_and_verify_check_receipts_before_admitting_the_pair() {
        let root = Directory::new();
        let old = ["a", "b"].map(|name| root.0.join(name));
        let next = ["new-a", "new-b"].map(|name| root.0.join(name));
        let from = old.each_ref().map(|path| path.as_path());
        let to = next.each_ref().map(|path| path.as_path());
        let new = Policy {
            collection: [10; 16],
            generation: POLICY.generation + 1,
            actors: [[3; 16], [4; 16]],
        };
        let state = StudyProgress::default();
        let [a, b] = from.map(|path| Database::create_file(path, Limits::default()).unwrap());
        enroll(&a, enrollment("a", POLICY).unwrap()).unwrap();
        enroll(&b, enrollment("b", POLICY).unwrap()).unwrap();
        let stale = Receipt::Checkpoint.line([&a, &b], POLICY, state).unwrap();
        let staged = Receipt::Staged.line([&a, &b], POLICY, state).unwrap();
        a.retain_snapshot(&a.snapshot()).unwrap();
        let current = Receipt::Checkpoint.line([&a, &b], POLICY, state).unwrap();
        assert_ne!(stale, current);
        drop((a, b));
        let original = from.map(|path| fs::read(path).unwrap());
        let error = stage(from, POLICY, to, new, Some(stale.as_bytes())).unwrap_err();
        assert_eq!(error.downcast_ref::<Error>(), Some(&Error::Conflict));
        assert!(to.iter().all(|path| !path.exists()));
        let error = verify(from, POLICY, Some(current.as_bytes())).unwrap_err();
        assert_eq!(error.downcast_ref::<Error>(), Some(&Error::Conflict));
        verify(from, POLICY, Some(staged.as_bytes())).unwrap();
        verify(from, POLICY, None).unwrap();
        stage(from, POLICY, to, new, None).unwrap();
        verify(to, new, None).unwrap();
        for path in to {
            fs::remove_file(path).unwrap();
        }
        stage(from, POLICY, to, new, Some(current.as_bytes())).unwrap();
        verify(to, new, None).unwrap();
        assert_eq!(from.map(|path| fs::read(path).unwrap()), original);
    }
}
