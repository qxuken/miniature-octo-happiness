use super::{
    AppResult, COLLECTION_ROW, enroll,
    protocol::{Enrollment, StudyReplica},
    study::StudyProgress,
    tls::bounded_file,
};
use crate::diagnostics::{self, Phase};
use sha2::{Digest, Sha256};
use std::io::{self, Write};
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

    fn report(&self, output: &mut dyn Write) -> io::Result<()> {
        writeln!(
            output,
            "visible minutes={} completed={:?} planned={:?} goal={:?} limit={:?}",
            self.minutes,
            self.completed.iter().collect::<Vec<_>>(),
            self.planned.iter().collect::<Vec<_>>(),
            self.goal,
            self.limit,
        )
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

fn write_report(
    output: &mut dyn Write,
    emit: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> AppResult<()> {
    diagnostics::at(Phase::LocalCommand, || {
        emit(output)?;
        output.flush()?;
        Ok(())
    })
}

pub(super) fn checkpoint(
    files: [&Path; 2],
    policy: Policy,
    output: &mut dyn Write,
) -> AppResult<()> {
    let a = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(files[0], Limits::default())
    })?;
    let b = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(files[1], Limits::default())
    })?;
    let state = agreed(&a, &b, policy)?;
    let receipt = Receipt::Checkpoint.line([&a, &b], policy, state)?;
    write_report(output, |output| output.write_all(receipt.as_bytes()))
}

pub(super) fn stage(
    old_files: [&Path; 2],
    old: Policy,
    new_files: [&Path; 2],
    new: Policy,
    expected_old: Option<&[u8]>,
    output: &mut dyn Write,
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
    write_report(output, |output| {
        writeln!(
            output,
            "staged generation={} old_revisions={} {}",
            new.generation,
            a.snapshot().revision(),
            b.snapshot().revision()
        )?;
        visible.report(output)?;
        if let Some(receipt) = receipt {
            output.write_all(receipt.as_bytes())?;
        }
        Ok(())
    })
}

pub(super) fn verify(
    files: [&Path; 2],
    policy: Policy,
    expected_staged: Option<&[u8]>,
    output: &mut dyn Write,
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
    write_report(output, |output| {
        writeln!(
            output,
            "verified generation={} revisions={} {}",
            policy.generation,
            a.snapshot().revision(),
            b.snapshot().revision()
        )?;
        visible.report(output)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::tests::Directory;
    use std::{
        fs,
        net::{TcpListener, TcpStream},
        time::Duration,
    };
    use weekend_db::storage::MemoryStore;

    const POLICY: Policy = Policy {
        collection: [9; 16],
        generation: 0x0102_0304_0506_0708,
        actors: [[1; 16], [2; 16]],
    };
    const NEW_POLICY: Policy = Policy {
        collection: [10; 16],
        generation: POLICY.generation + 1,
        actors: [[3; 16], [4; 16]],
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
        let error = stage(
            from,
            POLICY,
            to,
            NEW_POLICY,
            Some(stale.as_bytes()),
            &mut io::sink(),
        )
        .unwrap_err();
        assert_eq!(error.downcast_ref::<Error>(), Some(&Error::Conflict));
        assert!(to.iter().all(|path| !path.exists()));
        let error = verify(from, POLICY, Some(current.as_bytes()), &mut io::sink()).unwrap_err();
        assert_eq!(error.downcast_ref::<Error>(), Some(&Error::Conflict));
        verify(from, POLICY, Some(staged.as_bytes()), &mut io::sink()).unwrap();
        verify(from, POLICY, None, &mut io::sink()).unwrap();
        let mut unguarded = Vec::new();
        stage(from, POLICY, to, NEW_POLICY, None, &mut unguarded).unwrap();
        assert_eq!(String::from_utf8_lossy(&unguarded).lines().count(), 2);
        verify(to, NEW_POLICY, None, &mut io::sink()).unwrap();
        for path in to {
            fs::remove_file(path).unwrap();
        }
        let mut guarded = Vec::new();
        stage(
            from,
            POLICY,
            to,
            NEW_POLICY,
            Some(current.as_bytes()),
            &mut guarded,
        )
        .unwrap();
        let line = guarded.strip_prefix(unguarded.as_slice()).unwrap();
        let [a, b] = to.map(|path| Database::open_file(path, Limits::default()).unwrap());
        assert_eq!(
            line,
            Receipt::Staged
                .line([&a, &b], NEW_POLICY, state)
                .unwrap()
                .as_bytes()
        );
        drop((a, b));
        verify(to, NEW_POLICY, None, &mut io::sink()).unwrap();
        assert_eq!(from.map(|path| fs::read(path).unwrap()), original);
    }

    #[test]
    fn roster_commands_preserve_source_open_and_disagreement_errors() {
        let root = Directory::new();
        let [a, b, different, missing, new_a, new_b] =
            ["a", "b", "different", "missing", "new-a", "new-b"].map(|name| root.0.join(name));
        let from = [a.as_path(), b.as_path()];
        let to = [new_a.as_path(), new_b.as_path()];

        for (name, path) in ["a", "b", "b"].into_iter().zip([&a, &b, &different]) {
            let db = Database::create_file(path, Limits::default()).unwrap();
            let replica = enroll(&db, enrollment(name, POLICY).unwrap()).unwrap();
            if path == &different {
                replica
                    .update(|state, _| {
                        state.completed.insert(42)?;
                        Ok(())
                    })
                    .unwrap();
            }
            db.retain_snapshot(&db.snapshot()).unwrap();
        }
        let original = from.map(|path| fs::read(path).unwrap());
        for (pair, expected) in [
            ([missing.as_path(), b.as_path()], Error::NotFound),
            ([a.as_path(), missing.as_path()], Error::NotFound),
            ([a.as_path(), different.as_path()], Error::Conflict),
        ] {
            for result in [
                checkpoint(pair, POLICY, &mut io::sink()),
                verify(pair, POLICY, None, &mut io::sink()),
                stage(pair, POLICY, to, NEW_POLICY, None, &mut io::sink()),
            ] {
                assert_eq!(result.unwrap_err().downcast_ref::<Error>(), Some(&expected));
            }
            assert!(to.iter().all(|path| !path.exists()));
        }
        checkpoint(from, POLICY, &mut io::sink()).unwrap();
        verify(from, POLICY, None, &mut io::sink()).unwrap();
        stage(from, POLICY, to, NEW_POLICY, None, &mut io::sink()).unwrap();
        verify(to, NEW_POLICY, None, &mut io::sink()).unwrap();
        assert_eq!(from.map(|path| fs::read(path).unwrap()), original);
    }

    #[test]
    fn stage_stops_at_destination_creation_failure_and_can_retry() {
        let root = Directory::new();
        let [a, b, new_a, new_b, occupied] =
            ["a", "b", "new-a", "new-b", "occupied"].map(|name| root.0.join(name));
        let from = [a.as_path(), b.as_path()];
        let to = [new_a.as_path(), new_b.as_path()];

        for (name, path) in ["a", "b"].into_iter().zip(from) {
            let db = Database::create_file(path, Limits::default()).unwrap();
            enroll(&db, enrollment(name, POLICY).unwrap()).unwrap();
            db.retain_snapshot(&db.snapshot()).unwrap();
        }
        let original = from.map(|path| fs::read(path).unwrap());
        let sentinel = b"existing destination";
        fs::write(&occupied, sentinel).unwrap();
        for pair in [[occupied.as_path(), to[1]], [to[0], occupied.as_path()]] {
            let error = stage(from, POLICY, pair, NEW_POLICY, None, &mut io::sink()).unwrap_err();
            assert_eq!(error.downcast_ref::<Error>(), Some(&Error::AlreadyExists));
            assert_eq!(fs::read(&occupied).unwrap(), sentinel);
            assert!(!to[1].exists());
        }
        let partial = Database::open_file(to[0], Limits::default()).unwrap();
        assert_eq!(partial.snapshot().revision(), 0);
        assert!(partial.retained_ids().unwrap().is_empty());
        drop(partial);
        assert_eq!(from.map(|path| fs::read(path).unwrap()), original);
        fs::remove_file(to[0]).unwrap();
        stage(from, POLICY, to, NEW_POLICY, None, &mut io::sink()).unwrap();
        verify(to, NEW_POLICY, None, &mut io::sink()).unwrap();
        assert_eq!(from.map(|path| fs::read(path).unwrap()), original);
    }

    fn exchange(sender: &StudyReplica<'_>, receiver: &StudyReplica<'_>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        for stream in [&client, &server] {
            let timeout = Some(Duration::from_secs(5));
            stream.set_read_timeout(timeout).unwrap();
            stream.set_write_timeout(timeout).unwrap();
        }
        std::thread::scope(|threads| {
            let reply = threads.spawn(move || receiver.serve_authenticated(server));
            sender.push_authenticated(client).unwrap();
            reply.join().unwrap().unwrap();
        });
    }

    #[test]
    fn stage_seeds_the_exact_component_limit_and_rejects_the_next_total() {
        let root = Directory::new();
        let [old_a, old_b, new_a, new_b, refused_a, refused_b] =
            ["a", "b", "new-a", "new-b", "refused-a", "refused-b"].map(|x| root.0.join(x));
        let from = [old_a.as_path(), old_b.as_path()];
        let to = [new_a.as_path(), new_b.as_path()];
        let ceiling = u64::MAX / 2;
        {
            let [db_a, db_b] =
                from.map(|path| Database::create_file(path, Limits::default()).unwrap());
            let a = enroll(&db_a, enrollment("a", POLICY).unwrap()).unwrap();
            let b = enroll(&db_b, enrollment("b", POLICY).unwrap()).unwrap();
            a.update(|state, actor| {
                state.practice_minutes.increment(actor, ceiling)?;
                state.completed.insert(7)?;
                state.planned.insert(11, actor)?;
                state.planned.insert(63, actor)?;
                state.planned.insert(22, actor)?;
                assert!(state.planned.remove(22)?);
                state.weekly_goal.assign(actor, 30)?;
                state.session_limit.assign(actor, 15)
            })
            .unwrap();
            exchange(&a, &b);
            assert_eq!(a.read().unwrap(), b.read().unwrap());
            for db in [&db_a, &db_b] {
                db.retain_snapshot(&db.snapshot()).unwrap();
            }
        }
        let bytes = from.map(|path| fs::read(path).unwrap());
        stage(from, POLICY, to, NEW_POLICY, None, &mut io::sink()).unwrap();
        assert_eq!(from.map(|path| fs::read(path).unwrap()), bytes);
        {
            let [db_a, db_b] = to.map(|path| Database::open_file(path, Limits::default()).unwrap());
            let [a, b] = [&db_a, &db_b].map(|db| db.collection_id(COLLECTION_ROW));
            let a = StudyReplica::open(&db_a, a, NEW_POLICY, NEW_POLICY.actors[0]).unwrap();
            let b = StudyReplica::open(&db_b, b, NEW_POLICY, NEW_POLICY.actors[1]).unwrap();
            let seed = a.read().unwrap();
            let components: [u8; 16] = [ceiling.to_le_bytes(), 0u64.to_le_bytes()]
                .concat()
                .try_into()
                .unwrap();
            assert_eq!(seed.practice_minutes.to_le_bytes(), components);
            assert_eq!(seed.completed.iter().collect::<Vec<_>>(), [7]);
            assert_eq!(seed.planned.iter().collect::<Vec<_>>(), [11, 63]);
            assert_eq!(seed.weekly_goal.effective_value(), Some(30));
            assert_eq!(seed.session_limit.effective_value(), Some(15));
            assert_eq!(b.read().unwrap(), StudyProgress::default());
            exchange(&a, &b);
            assert_eq!(b.read().unwrap(), seed);
        }
        verify(to, NEW_POLICY, None, &mut io::sink()).unwrap();
        {
            let [db_a, db_b] =
                from.map(|path| Database::open_file(path, Limits::default()).unwrap());
            let a = StudyReplica::open(
                &db_a,
                db_a.collection_id(COLLECTION_ROW),
                POLICY,
                POLICY.actors[0],
            )
            .unwrap();
            let b = StudyReplica::open(
                &db_b,
                db_b.collection_id(COLLECTION_ROW),
                POLICY,
                POLICY.actors[1],
            )
            .unwrap();
            b.update(|state, actor| state.practice_minutes.increment(actor, 1))
                .unwrap();
            exchange(&b, &a);
            let over = a.read().unwrap();
            assert_eq!(over, b.read().unwrap());
            assert_eq!(over.practice_minutes.total(), ceiling + 1);
        }
        let over_bytes = from.map(|path| fs::read(path).unwrap());
        let refused = [&refused_a as &Path, &refused_b as &Path];
        let error = stage(from, POLICY, refused, NEW_POLICY, None, &mut io::sink()).unwrap_err();
        assert_eq!(error.downcast_ref::<Error>(), Some(&Error::Limit));
        assert!(refused.iter().all(|path| !path.exists()));
        assert_eq!(from.map(|path| fs::read(path).unwrap()), over_bytes);
    }

    #[test]
    fn seed_independently_copies_minutes_goal_and_limit() {
        let db = Database::new().unwrap();
        let replica = enroll(&db, enrollment("a", POLICY).unwrap()).unwrap();
        replica
            .update(|_, actor| {
                for (minutes, goal, limit) in [
                    (0, None, None),
                    (1, None, None),
                    (0, Some(0), None),
                    (0, None, Some(0)),
                ] {
                    let visible = Visible {
                        minutes,
                        completed: GrowSet::default(),
                        planned: GrowSet::default(),
                        goal,
                        limit,
                    };
                    let mut seeded = StudyProgress::default();
                    visible.seed(&mut seeded, actor).unwrap();
                    assert_eq!(seeded.practice_minutes.total(), minutes);
                    assert_eq!(seeded.weekly_goal.effective_value(), goal);
                    assert_eq!(seeded.session_limit.effective_value(), limit);
                    assert!(seeded.planned.iter().next().is_none());
                    assert!(
                        Visible::read(seeded)
                            .unwrap()
                            .planned
                            .iter()
                            .next()
                            .is_none()
                    );
                }
                let over = Visible {
                    minutes: u64::MAX / 2 + 1,
                    completed: GrowSet::default(),
                    planned: GrowSet::default(),
                    goal: None,
                    limit: None,
                };
                let mut seeded = StudyProgress::default();
                assert_eq!(over.seed(&mut seeded, actor), Err(Error::Limit));
                assert_eq!(seeded.practice_minutes.total(), 0);
                Ok(())
            })
            .unwrap();
    }

    #[derive(Default)]
    struct ReportSink {
        bytes: Vec<u8>,
        fail_at: Option<usize>,
        fail_flush: bool,
        flushes: usize,
    }

    impl Write for ReportSink {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            let count = if let Some(limit) = self.fail_at {
                if self.bytes.len() == limit {
                    self.fail_at = None;
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                input.len().min(limit - self.bytes.len())
            } else {
                input.len()
            };
            self.bytes.extend_from_slice(&input[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                return Err(io::ErrorKind::WriteZero.into());
            }
            Ok(())
        }
    }

    #[test]
    fn cutover_reports_stop_at_the_first_write_error_and_report_flush_failure() {
        let root = Directory::new();
        let old = ["a", "b"].map(|name| root.0.join(name));
        let from = old.each_ref().map(|path| path.as_path());
        let [a, b] = from.map(|path| Database::create_file(path, Limits::default()).unwrap());
        enroll(&a, enrollment("a", POLICY).unwrap()).unwrap();
        enroll(&b, enrollment("b", POLICY).unwrap()).unwrap();
        let receipt = Receipt::Checkpoint
            .line([&a, &b], POLICY, StudyProgress::default())
            .unwrap();
        let header = format!(
            "staged generation={} old_revisions={} {}\n",
            NEW_POLICY.generation,
            a.snapshot().revision(),
            b.snapshot().revision()
        );
        let summary =
            format!("{header}visible minutes=0 completed=[] planned=[] goal=None limit=None\n");
        drop((a, b));
        let before = from.map(|path| fs::read(path).unwrap());
        for (ordinal, error_at) in [Some(0), Some(header.len()), Some(summary.len()), None]
            .into_iter()
            .enumerate()
        {
            let paths = ["a", "b"].map(|slot| root.0.join(format!("{ordinal}-{slot}")));
            let to = paths.each_ref().map(|path| path.as_path());
            let mut out = ReportSink {
                fail_at: error_at,
                fail_flush: error_at.is_none(),
                ..ReportSink::default()
            };
            let error = stage(
                from,
                POLICY,
                to,
                NEW_POLICY,
                Some(receipt.as_bytes()),
                &mut out,
            )
            .unwrap_err();
            let [a, b] = to.map(|path| Database::open_file(path, Limits::default()).unwrap());
            let state = agreed(&a, &b, NEW_POLICY).unwrap();
            let written =
                summary.clone() + &Receipt::Staged.line([&a, &b], NEW_POLICY, state).unwrap();
            drop((a, b));
            match error_at {
                Some(end) => {
                    assert_eq!(
                        error.downcast_ref::<io::Error>().unwrap().kind(),
                        io::ErrorKind::BrokenPipe
                    );
                    assert_eq!(out.bytes, written.as_bytes()[..end]);
                    assert_eq!(out.flushes, 0);
                }
                None => {
                    assert_eq!(
                        error.downcast_ref::<io::Error>().unwrap().kind(),
                        io::ErrorKind::WriteZero
                    );
                    assert_eq!(out.bytes, written.as_bytes());
                    assert_eq!(out.flushes, 1);
                }
            }
            let mut out = ReportSink {
                fail_at: Some(0),
                ..ReportSink::default()
            };
            let error = verify(to, NEW_POLICY, None, &mut out).unwrap_err();
            assert_eq!(
                error.downcast_ref::<io::Error>().unwrap().kind(),
                io::ErrorKind::BrokenPipe
            );
            assert!(out.bytes.is_empty());
            assert_eq!(out.flushes, 0);
        }
        assert_eq!(from.map(|path| fs::read(path).unwrap()), before);
    }
}
