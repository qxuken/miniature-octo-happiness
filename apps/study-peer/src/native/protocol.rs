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
    use weekend_db::crdt::{MaxRegister, MinRegister, Record};

    #[test]
    fn enrollment_commands_and_retry_budget() {
        const MIXED: [u8; 16] = [
            0xab, 0xcd, 0xef, 0xab, 0xcd, 0xef, 0xab, 0xcd, 0xef, 0xab, 0xcd, 0xef, 0xab, 0xcd,
            0xef, 0xab,
        ];
        for (actor, id, collection, member) in [
            ("a", "11111111111111111111111111111111", [0x11; 16], [1; 16]),
            ("b", "abcdefABCDEFabcdefABCDEFabcdefAB", MIXED, [2; 16]),
        ] {
            let enrollment = Enrollment::parse(OsStr::new(actor), OsStr::new(id)).unwrap();
            assert_eq!(enrollment.policy.collection, collection, "collection");
            assert_eq!(enrollment.actor, member, "member");
        }
        let digits = "collection ID must be 32 hexadecimal digits";
        for (actor, id, reason) in [
            (
                "c",
                "11111111111111111111111111111111",
                "actor must be a or b",
            ),
            (
                "a",
                "00000000000000000000000000000000",
                "collection ID must be nonzero",
            ),
            ("a", "1", digits),
            ("a", "z1111111111111111111111111111111", digits),
            ("a", "+1111111111111111111111111111111", digits),
        ] {
            match Enrollment::parse(OsStr::new(actor), OsStr::new(id)) {
                Ok(_) => panic!("accepted invalid enrollment {actor} {id}"),
                Err(error) => assert_eq!(error.to_string(), reason),
            }
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

    #[cfg(any(unix, windows))]
    #[test]
    fn non_utf8_arguments_are_rejected() {
        use std::ffi::OsString;
        #[cfg(unix)]
        let malformed = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![0xff])
        };
        #[cfg(windows)]
        let malformed = {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0xd800])
        };
        let actor = OsStr::new("a");
        let id = OsStr::new("11111111111111111111111111111111");
        for (argument, result) in [
            ("collection", Enrollment::parse(actor, &malformed)),
            ("actor", Enrollment::parse(&malformed, id)),
        ] {
            match result {
                Ok(_) => panic!("accepted non-UTF-8 {argument}"),
                Err(error) => {
                    assert_eq!(error.to_string(), "expected a UTF-8 argument", "{argument}")
                }
            }
        }
    }

    #[test]
    fn commands_preserve_parse_errors_and_missing_rows() {
        use std::num::{IntErrorKind, ParseIntError};

        let db = Database::new().unwrap();
        let enrollment = Enrollment::new("a", [1; 16]).unwrap();
        let collection = db.create_collection().unwrap();
        let replica =
            StudyReplica::enroll(&db, collection, enrollment.policy, enrollment.actor).unwrap();
        let before = db.snapshot();
        assert_eq!(
            command(&["show"], &db, &replica).unwrap(),
            format!("state {} 0 [] [] []", before.revision())
        );
        for words in [
            &["practice", "x"][..],
            &["edit", "x", "2", "3", "4"],
            &["edit", "1", "x", "3", "4"],
            &["edit", "1", "2", "x", "4"],
            &["edit", "1", "2", "3", "x"],
        ] {
            let error = command(words, &db, &replica).unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<ParseIntError>()
                    .map(ParseIntError::kind),
                Some(&IntErrorKind::InvalidDigit),
                "{words:?}"
            );
            assert_eq!(db.snapshot().revision(), before.revision());
            assert_eq!(replica.read().unwrap(), StudyProgress::default());
        }
        let mut tx = db.begin();
        tx.delete(collection, StudyProgress::<true>::KEY).unwrap();
        let removed = tx.commit().unwrap();
        let error = command(&["show"], &db, &replica).unwrap_err();
        assert_eq!(
            error.downcast_ref::<Error>(),
            Some(&Error::NotFound),
            "show"
        );
        assert_eq!(db.snapshot().revision(), removed);
        assert_eq!(replica.read_at(&before).unwrap(), StudyProgress::default());
    }

    #[test]
    fn failed_field_updates_discard_the_whole_command() {
        let db = Database::new().unwrap();
        let enrollment = Enrollment::new("a", [1; 16]).unwrap();
        let replica = StudyReplica::enroll(
            &db,
            db.create_collection().unwrap(),
            enrollment.policy,
            enrollment.actor,
        )
        .unwrap();
        let before = db.snapshot().revision();
        assert_eq!(
            command(&["edit", "1", "2", "3", "4"], &db, &replica).unwrap(),
            format!("local {}", before + 1),
        );
        let initial = db.snapshot();
        let initial_progress = replica.read_at(&initial).unwrap();
        assert_eq!(initial_progress.practice_minutes.total(), 2);
        assert!(initial_progress.completed.contains(1));
        assert_eq!(initial_progress.weekly_goal.values().next(), Some(3));
        assert_eq!(initial_progress.session_limit.values().next(), Some(4));
        let reject = |words: &[&str], expected| {
            let snapshot = db.snapshot();
            let previous = replica.read_at(&snapshot).unwrap();
            let error = command(words, &db, &replica).unwrap_err();
            assert_eq!(error.downcast_ref::<Error>(), Some(&expected), "{words:?}");
            assert_eq!(db.snapshot().revision(), snapshot.revision());
            assert_eq!(replica.read().unwrap(), previous);
        };
        reject(&["practice", "0"], Error::Invalid);
        reject(&["edit", "64", "2", "3", "4"], Error::Bounds);
        reject(&["edit", "2", "0", "3", "4"], Error::Invalid);

        // A valid actor-a register entry with the largest representable dot.
        // Seed it through the replica so the row keeps its policy.
        let mut exhausted = [0; MaxRegister::SIZE];
        exhausted[..8].copy_from_slice(&u64::MAX.to_le_bytes());
        exhausted[8] = 1;
        exhausted[9..17].copy_from_slice(&9u64.to_le_bytes());
        replica
            .update(|state, _| {
                state.session_limit = MinRegister::decode(&exhausted)?;
                Ok(())
            })
            .unwrap();
        reject(&["edit", "2", "2", "3", "4"], Error::Exhausted);
        replica
            .update(|state, _| {
                state.weekly_goal = MaxRegister::decode(&exhausted)?;
                Ok(())
            })
            .unwrap();
        reject(&["edit", "3", "2", "3", "4"], Error::Exhausted);
        assert_eq!(replica.read_at(&initial).unwrap(), initial_progress);
    }
}
