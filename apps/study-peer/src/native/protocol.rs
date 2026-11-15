use super::study::StudyProgress;
use std::ffi::OsStr;
use weekend_db::{
    Database, Error,
    crdt::{Actor, Policy, Replica},
};
type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub(crate) type StudyReplica<'db> = Replica<'db, StudyProgress>;
#[derive(Clone, Copy)]
pub(crate) struct Enrollment {
    pub(crate) policy: Policy,
    pub(crate) actor: [u8; 16],
}

impl Enrollment {
    pub(crate) fn new(actor: &str, collection: [u8; 16]) -> AppResult<Self> {
        let policy = Policy {
            collection,
            generation: 7,
            actors: [[1; 16], [2; 16]],
        };
        let actor = match actor {
            "a" => policy.actors[0],
            "b" => policy.actors[1],
            _ => return Err("actor must be a or b".into()),
        };
        if collection == [0; 16] {
            return Err("collection ID must be nonzero".into());
        }
        Ok(Self { policy, actor })
    }

    pub(crate) fn parse(actor: &OsStr, id: &OsStr) -> AppResult<Self> {
        let id = utf8(id)?;
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("collection ID must be 32 hexadecimal digits".into());
        }
        Self::new(utf8(actor)?, u128::from_str_radix(id, 16)?.to_be_bytes())
    }
}

pub(crate) fn utf8(value: &OsStr) -> AppResult<&str> {
    value
        .to_str()
        .ok_or_else(|| "expected a UTF-8 argument".into())
}

pub(crate) fn command(
    words: &[&str],
    db: &Database,
    replica: &StudyReplica<'_>,
) -> AppResult<String> {
    let revision = match words {
        ["show"] => {
            let snapshot = db.snapshot();
            let state = replica.read_at(&snapshot)?;
            return Ok(format!(
                "state {} {} {:?} {:?} {:?}",
                snapshot.revision(),
                state.practice_minutes.total(),
                state.weekly_goal.values().collect::<Vec<_>>(),
                state.session_limit.values().collect::<Vec<_>>(),
                state.completed.iter().collect::<Vec<_>>(),
            ));
        }
        ["practice", minutes] => {
            let minutes = minutes.parse()?;
            edit(replica, |state, actor| {
                state.practice_minutes.increment(actor, minutes)
            })?
        }
        ["edit", lesson, minutes, goal, limit] => {
            let (lesson, minutes, goal, limit) = (
                lesson.parse()?,
                minutes.parse()?,
                goal.parse()?,
                limit.parse()?,
            );
            edit(replica, |state, actor| {
                state.completed.insert(lesson)?;
                state.practice_minutes.increment(actor, minutes)?;
                state.weekly_goal.assign(actor, goal)?;
                state.session_limit.assign(actor, limit)
            })?
        }
        _ => return Err("unknown command (see study-peer --help)".into()),
    };
    Ok(format!("local {revision}"))
}

pub(crate) fn edit(
    replica: &StudyReplica<'_>,
    edit: impl Fn(&mut StudyProgress, Actor) -> weekend_db::Result<()>,
) -> weekend_db::Result<u64> {
    // These commands only edit the draft. A definite conflict can run again
    // against a fresh snapshot. OutcomeUnknown must reach the caller.
    for _ in 0..2 {
        let result = replica.update(&edit);
        if result != Err(Error::Conflict) {
            return result;
        }
    }
    replica.update(edit)
}

#[cfg(test)]
mod decisions {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn enrollment_commands_and_retry_budget() {
        for (actor, id, good) in [
            ("a", "11111111111111111111111111111111", true),
            ("b", "abcdefABCDEFabcdefABCDEFabcdefAB", true),
            ("c", "11111111111111111111111111111111", false),
            ("a", "00000000000000000000000000000000", false),
            ("a", "1", false),
            ("a", "z1111111111111111111111111111111", false),
        ] {
            assert_eq!(
                Enrollment::parse(OsStr::new(actor), OsStr::new(id)).is_ok(),
                good
            );
        }
        let db = Database::new().unwrap();
        let e = Enrollment::new("a", [1; 16]).unwrap();
        let r =
            StudyReplica::enroll(&db, db.create_collection().unwrap(), e.policy, e.actor).unwrap();
        let initial = db.snapshot().revision();
        for command_line in [
            "",
            "what",
            "practice -1",
            "practice 0",
            "edit 64 1 1 1",
            "edit 1 0 1 1",
            "edit 1 x 1 1",
            "edit 1 1 x 1",
            "edit 1 1 1 x",
        ] {
            assert!(
                command(
                    &command_line.split_whitespace().collect::<Vec<_>>(),
                    &db,
                    &r
                )
                .is_err()
            );
            assert_eq!(db.snapshot().revision(), initial);
        }
        assert_eq!(
            command(&["practice", "5"], &db, &r).unwrap(),
            format!("local {}", initial + 1)
        );
        assert!(
            command(&["show"], &db, &r)
                .unwrap()
                .starts_with(&format!("state {} 5", initial + 1))
        );
        let calls = Cell::new(0);
        assert_eq!(
            edit(&r, |_, _| {
                calls.set(calls.get() + 1);
                Err(Error::Conflict)
            }),
            Err(Error::Conflict)
        );
        assert_eq!(calls.get(), 3);
        for error in [Error::Limit, Error::OutcomeUnknown, Error::RecoveryRequired] {
            calls.set(0);
            assert_eq!(
                edit(&r, |_, _| {
                    calls.set(calls.get() + 1);
                    Err(error)
                }),
                Err(error)
            );
            assert_eq!(calls.get(), 1);
        }
        calls.set(0);
        let before = db.snapshot().revision();
        assert_eq!(
            edit(&r, |state, actor| {
                calls.set(calls.get() + 1);
                state.practice_minutes.increment(actor, 1)?;
                if calls.get() == 1 {
                    Err(Error::Conflict)
                } else {
                    Ok(())
                }
            }),
            Ok(before + 1)
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(r.read().unwrap().practice_minutes.total(), 6);
    }
    #[cfg(unix)]
    #[test]
    fn non_utf8_arguments_are_rejected() {
        use std::os::unix::ffi::OsStrExt;
        assert!(utf8(OsStr::from_bytes(&[255])).is_err());
    }
}
