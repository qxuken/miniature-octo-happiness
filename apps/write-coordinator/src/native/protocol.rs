use std::io::{self, Read};
use weekend_db::{CollectionId, Database, Error, Result as DbResult, Snapshot};
#[path = "sequenced.rs"]
pub(crate) mod sequenced;
pub(crate) const COLLECTION: u64 = 1;
pub(crate) const KEY: &[u8] = b"counter";
const MAX_LINE: usize = 256;
pub(crate) const MAX_RECEIPTS: usize = 1024;

pub(crate) fn authorize<'a>(secret: &[u8; 64], request: &'a str) -> Option<&'a str> {
    let (candidate, command) = request.strip_prefix("auth ")?.split_once(' ')?;
    if candidate.len() != secret.len() {
        return None;
    }
    let difference = secret
        .iter()
        .zip(candidate.bytes())
        .fold(0, |diff, (a, b)| diff | std::hint::black_box(a ^ b));
    (difference == 0).then_some(command)
}

pub(crate) fn read_line_from(
    reader: &mut impl Read,
    mut before_read: impl FnMut() -> io::Result<()>,
) -> io::Result<String> {
    let mut bytes = Vec::new();
    while bytes.len() < MAX_LINE {
        before_read()?;
        let mut byte = [0];
        if reader.read(&mut byte)? == 0 {
            break;
        }
        if byte[0] == b'\n' {
            return String::from_utf8(bytes)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid text"));
        }
        bytes.push(byte[0]);
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "missing or oversized line",
    ))
}

pub(crate) fn state(db: &Database) -> DbResult<(u64, u64)> {
    state_in(db, COLLECTION)
}

fn state_in(db: &Database, collection: u64) -> DbResult<(u64, u64)> {
    let snapshot = db.snapshot();
    Ok((
        snapshot.revision(),
        counter_at(&snapshot, db.collection_id(collection))?,
    ))
}

fn counter_at(snapshot: &Snapshot<'_>, collection: CollectionId) -> DbResult<u64> {
    snapshot
        .get(collection, KEY)?
        .ok_or(Error::NotFound)?
        .as_u64()
}

pub(crate) fn handle(db: &mut Database, line: &str) -> DbResult<String> {
    if db.requires_recovery() {
        return Err(Error::RecoveryRequired);
    }
    let words: Vec<_> = line.split_whitespace().collect();
    match words.as_slice() {
        ["get"] => {
            let (revision, value) = state(db)?;
            Ok(format!("value {revision} {value}"))
        }
        ["set", expected, value] => {
            let expected = expected.parse::<u64>().map_err(|_| Error::Invalid)?;
            let value = value.parse::<u64>().map_err(|_| Error::Invalid)?;
            // The same owner handles the comparison and commit before accepting
            // another request. This demo uses the revision of its one-record DB.
            let (revision, current) = state(db)?;
            if revision != expected {
                return Ok(format!("conflict {revision} {current}"));
            }
            let mut tx = db.begin();
            tx.put(db.collection_id(COLLECTION), KEY, &db.unsigned(value)?)?;
            Ok(format!("committed {}", tx.commit()?))
        }
        ["set-id", id, expected, value] => {
            let expected = expected.parse::<u64>().map_err(|_| Error::Invalid)?;
            let value = value.parse::<u64>().map_err(|_| Error::Invalid)?;
            submit_identified(db, id, expected, value)
        }
        _ => Err(Error::Invalid),
    }
}

fn submit_identified(db: &Database, id: &str, expected: u64, value: u64) -> DbResult<String> {
    if id.is_empty()
        || id.len() > 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::Invalid);
    }
    let collection = db.collection_id(COLLECTION);
    let key = format!("receipt/{id}");
    let mut tx = db.begin_with_validation(weekend_db::Validation::WholeDatabase);
    if let Some(receipt) = tx.get(collection, key.as_bytes())? {
        let bytes = receipt.as_bytes().map_err(|_| Error::Corrupt)?;
        if bytes.len() != 25 || bytes[0] != 1 {
            return Err(Error::Corrupt);
        }
        let original_expected = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
        let original_value = u64::from_le_bytes(bytes[9..17].try_into().unwrap());
        let acknowledged = u64::from_le_bytes(bytes[17..25].try_into().unwrap());
        if original_expected.checked_add(1) != Some(acknowledged)
            || acknowledged > db.snapshot().revision()
        {
            return Err(Error::Corrupt);
        }
        if original_expected != expected || original_value != value {
            return Err(Error::Invalid);
        }
        return Ok(format!("committed {acknowledged}"));
    }
    let (revision, current) = state(db)?;
    if revision != expected {
        return Ok(format!("conflict {revision} {current}"));
    }
    // Receipts never expire or get evicted. Exhaustion rejects new IDs while
    // retaining every existing retry result.
    if tx.count(collection)? > MAX_RECEIPTS {
        return Err(Error::Limit);
    }
    let acknowledged = revision.checked_add(1).ok_or(Error::Exhausted)?;
    let mut receipt = [0u8; 25];
    receipt[0] = 1;
    receipt[1..9].copy_from_slice(&expected.to_le_bytes());
    receipt[9..17].copy_from_slice(&value.to_le_bytes());
    receipt[17..25].copy_from_slice(&acknowledged.to_le_bytes());
    tx.put(collection, KEY, &db.unsigned(value)?)?;
    tx.put(collection, key.as_bytes(), &db.bytes(&receipt)?)?;
    assert_eq!(tx.commit()?, acknowledged);
    Ok(format!("committed {acknowledged}"))
}

#[cfg(test)]
mod decisions {
    use super::*;
    use weekend_db::{
        Limits,
        storage::{Crash, Fault, IoKind, SimulatedDisk},
    };
    fn seed_counter(db: &Database) {
        let c = db.create_collection().unwrap();
        let mut t = db.begin();
        t.put(c, KEY, &db.unsigned(0).unwrap()).unwrap();
        assert_eq!(t.commit(), Ok(2));
    }
    fn database() -> Database {
        let db = Database::new().unwrap();
        seed_counter(&db);
        db
    }
    #[test]
    fn authentication_and_line_bounds() {
        let secret = [b'a'; 64];
        let token = "a".repeat(64);
        assert_eq!(
            authorize(&secret, &format!("auth {token} get")),
            Some("get")
        );
        for request in [
            "get".to_owned(),
            format!("auth {token}"),
            "auth a get".to_owned(),
            format!("auth {} get", "b".repeat(64)),
        ] {
            assert_eq!(authorize(&secret, &request), None);
        }
        let longest = "a".repeat(255);
        for (bytes, expected) in [
            (b"\n".to_vec(), Ok("")),
            (b"get\n".to_vec(), Ok("get")),
            (
                [vec![b'a'; 255], vec![b'\n']].concat(),
                Ok(longest.as_str()),
            ),
            (
                [vec![b'a'; 256], vec![b'\n']].concat(),
                Err("missing or oversized line"),
            ),
            (b"get".to_vec(), Err("missing or oversized line")),
            (vec![255, b'\n'], Err("invalid text")),
        ] {
            assert_eq!(
                read_line_from(&mut bytes.as_slice(), || Ok(()))
                    .as_deref()
                    .map_err(|error| {
                        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
                        error.to_string()
                    }),
                expected.map_err(str::to_owned)
            );
        }
        let mut frames = &b"get\nset 2 20\n"[..];
        assert_eq!(read_line_from(&mut frames, || Ok(())).unwrap(), "get");
        assert_eq!(frames, b"set 2 20\n");
        let mut polls = 0;
        let error = read_line_from(&mut &b"get"[..], || {
            polls += 1;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(polls, 4);
        let mut calls = 0;
        let mut unread = &b"get\n"[..];
        let error = read_line_from(&mut unread, || {
            calls += 1;
            Err(io::ErrorKind::TimedOut.into())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(calls, 1);
        assert_eq!(unread, b"get\n");
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "fixture read failure",
                ))
            }
        }
        let error = read_line_from(&mut (&b"ge"[..]).chain(Broken), || Ok(())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
        assert_eq!(error.to_string(), "fixture read failure");
    }
    #[test]
    fn request_identifier_and_recovery_admission() {
        for id in ["A0z", "-", "_", &"x".repeat(32)] {
            let db = database();
            assert_eq!(submit_identified(&db, id, 2, 4).unwrap(), "committed 3");
        }
        for id in ["", &"x".repeat(33), "/"] {
            let db = database();
            assert_eq!(submit_identified(&db, id, 2, 4), Err(Error::Invalid));
            assert_eq!(state(&db), Ok((2, 0)));
        }
        let disk = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(disk.clone(), [1; 16], Limits::default()).unwrap();
        let c = db.create_collection().unwrap();
        let mut t = db.begin();
        t.put(c, KEY, &db.unsigned(0).unwrap()).unwrap();
        t.commit().unwrap();
        drop(t);
        disk.arm(0, Fault::Error);
        assert_eq!(handle(&mut db, "set-id uncertain 2 7"), Err(Error::Io));
        assert_eq!(handle(&mut db, "get"), Err(Error::RecoveryRequired));
        assert_eq!(
            handle(&mut db, "set-id uncertain 2 7"),
            Err(Error::RecoveryRequired)
        );
    }

    #[test]
    fn legacy_commands_preserve_missing_rows_and_parse_errors() {
        let mut db = Database::new().unwrap();
        for missing in ["collection", "counter"] {
            if missing == "counter" {
                db.create_collection().unwrap();
            }
            let before = db.snapshot().revision();
            for command in ["get", "set 1 20", "set-id next 1 20"] {
                assert_eq!(
                    handle(&mut db, command),
                    Err(Error::NotFound),
                    "{missing}: {command}"
                );
                assert_eq!(db.snapshot().revision(), before);
            }
        }
        let mut db = database();
        let key = db.collection_id(COLLECTION);
        for command in [
            "",
            "unrecognized",
            "get extra",
            "set 2",
            "set invalid 20",
            "set 2 invalid",
            "set-id next invalid 20",
            "set-id next 2 invalid",
        ] {
            assert_eq!(handle(&mut db, command), Err(Error::Invalid), "{command}");
            assert_eq!(state(&db), Ok((2, 0)));
            assert_eq!(db.snapshot().count(key), Ok(1));
        }
        assert_eq!(handle(&mut db, "set 1 20"), Ok("conflict 2 0".into()));
        assert_eq!(state(&db), Ok((2, 0)));
        assert_eq!(handle(&mut db, "set 2 20"), Ok("committed 3".into()));
        assert_eq!(handle(&mut db, "get"), Ok("value 3 20".into()));
        assert_eq!(
            handle(&mut db, "set-id next 3 21"),
            Ok("committed 4".into())
        );
    }

    #[test]
    fn legacy_selected_allocator_refusals_preserve_image_and_retry() {
        use super::allocator::TestAllocator;
        use std::sync::Arc;
        use weekend_db::Validation;

        let allocator = Arc::new(TestAllocator::default());
        let disk = SimulatedDisk::new().unwrap();
        let mut db =
            Database::create_on_with_options(disk.clone(), [5; 16], allocator.options()).unwrap();
        seed_counter(&db);
        assert_eq!(
            handle(&mut db, "set-id prior 2 12"),
            Ok("committed 3".into())
        );

        let collection = db.collection_id(COLLECTION);
        let start = allocator.requests();
        let mut probe = db.begin_with_validation(Validation::WholeDatabase);
        assert!(probe.get(collection, b"receipt/next").unwrap().is_none());
        let before_count = allocator.requests() - start;
        assert_eq!(probe.count(collection), Ok(2));
        let before_unsigned = allocator.requests() - start;
        assert!(before_count > 0);
        assert!(before_unsigned > before_count);
        drop(probe);

        let image = disk.durable_image().unwrap();
        let live_bytes = allocator.live_bytes();
        let next = "set-id next 3 30";
        for (stage, request, allowance) in [
            ("plain counter value", "set 3 30", 0),
            ("receipt capacity count", next, before_count),
            ("identified counter value", next, before_unsigned),
        ] {
            let (result, requests) =
                allocator.count_with_refusal(allowance, || handle(&mut db, request));
            assert_eq!(requests, allowance + 1, "{stage}");
            assert_eq!(result, Err(Error::OutOfMemory), "{stage}");
            assert_eq!(allocator.live_bytes(), live_bytes);
            assert_eq!(state(&db), Ok((3, 12)));
            assert!(
                db.snapshot()
                    .get(collection, b"receipt/next")
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                handle(&mut db, "set-id prior 2 12"),
                Ok("committed 3".into())
            );
            assert_eq!(disk.durable_image().unwrap(), image);
        }
        assert_eq!(handle(&mut db, next), Ok("committed 4".into()));
        drop(db);
        let mut db = Database::open_on(disk, Limits::default()).unwrap();
        assert_eq!(state(&db), Ok((4, 30)));
        assert_eq!(handle(&mut db, next), Ok("committed 4".into()));
        assert_eq!(state(&db), Ok((4, 30)));
    }

    #[test]
    fn legacy_identified_write_refuses_exhausted_revision() {
        use weekend_db::storage::{export_logical_on, import_logical_on};

        let limits = Limits::default();
        let seed = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(seed.clone(), [5; 16], limits).unwrap();
        seed_counter(&db);
        assert_eq!(
            handle(&mut db, "set-id prior 2 12"),
            Ok("committed 3".into())
        );
        drop(db);
        let mut logical = export_logical_on(seed, limits).unwrap();
        assert_eq!(logical[..8], *b"WDBLOG01");
        assert_eq!(logical[32..40], 3u64.to_le_bytes());
        logical[32..40].copy_from_slice(&u64::MAX.to_le_bytes());
        super::logical_checksum::sign(&mut logical);
        let disk = SimulatedDisk::new().unwrap();
        import_logical_on(&logical, disk.clone(), limits).unwrap();
        let mut db = Database::open_on(disk.clone(), limits).unwrap();
        let image = disk.durable_image().unwrap();
        let next = format!("set-id next {} 30", u64::MAX);
        assert_eq!(handle(&mut db, &next), Err(Error::Exhausted));
        assert_eq!(state(&db), Ok((u64::MAX, 12)));
        let collection = db.collection_id(COLLECTION);
        assert!(
            db.snapshot()
                .get(collection, b"receipt/next")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            handle(&mut db, "set-id prior 2 12"),
            Ok("committed 3".into())
        );
        assert_eq!(disk.durable_image().unwrap(), image);
    }

    #[test]
    fn legacy_draft_budget_failures_preserve_counter_and_receipt() {
        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), [1; 16], Limits::default()).unwrap();
        seed_counter(&db);
        drop(db);
        let mut plain = Database::open_on(
            disk,
            Limits {
                max_transaction_bytes: 0,
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!(handle(&mut plain, "set 2 20"), Err(Error::Limit));
        assert_eq!(state(&plain), Ok((2, 0)));
        let request = "set-id next 2 20";
        for (stage, limits) in [
            (
                "observe counter",
                Limits {
                    max_observations: 1,
                    ..Limits::default()
                },
            ),
            (
                "build receipt bytes",
                Limits {
                    max_bytes: 24,
                    ..Limits::default()
                },
            ),
            (
                "insert receipt",
                Limits {
                    max_writes: 1,
                    ..Limits::default()
                },
            ),
        ] {
            let mut db = Database::with_limits(limits).unwrap();
            seed_counter(&db);
            let collection = db.collection_id(COLLECTION);
            assert_eq!(handle(&mut db, request), Err(Error::Limit), "{stage}");
            assert_eq!(state(&db), Ok((2, 0)), "{stage}");
            let snapshot = db.snapshot();
            assert_eq!(snapshot.count(collection), Ok(1), "{stage}");
            assert!(snapshot.get(collection, b"receipt/next").unwrap().is_none());
        }
    }

    #[test]
    fn legacy_receipt_commit_faults_reopen_atomically_and_retry_once() {
        let original = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(original.clone(), [1; 16], Limits::default()).unwrap();
        seed_counter(&db);
        assert_eq!(
            handle(&mut db, "set-id prior 2 12"),
            Ok("committed 3".into())
        );
        let retained_id = db.retain_snapshot(&db.snapshot()).unwrap();
        assert_eq!(state(&db), Ok((4, 12)));
        let collection = db.collection_id(COLLECTION);
        let prior_receipt = db
            .snapshot()
            .get(collection, b"receipt/prior")
            .unwrap()
            .unwrap()
            .as_bytes()
            .unwrap()
            .to_vec();
        drop(db);
        let image = original.durable_image().unwrap();

        let next = "set-id next 4 30";
        let open =
            |disk: &SimulatedDisk| Database::open_on(disk.clone(), Limits::default()).unwrap();
        let mut control = open(&original);
        original.arm(usize::MAX, Fault::Error);
        assert_eq!(handle(&mut control, next), Ok("committed 5".into()));
        let trace = original.trace().unwrap();
        let syncs: Vec<_> = trace
            .iter()
            .enumerate()
            .filter_map(|(step, event)| (event.kind == IoKind::Sync).then_some(step))
            .collect();
        let [append, root, _]: [usize; 3] = syncs.try_into().unwrap();
        drop(control);

        for step in 0..trace.len() {
            for crash in [Crash::DropAll, Crash::PersistAll] {
                let disk = SimulatedDisk::from_image(&image).unwrap();
                let mut db = open(&disk);
                disk.arm(step, Fault::Error);
                let expected = if step <= append {
                    Error::Io
                } else {
                    Error::OutcomeUnknown
                };
                assert_eq!(handle(&mut db, next), Err(expected), "step {step}");
                assert_eq!(disk.trace().unwrap(), trace[..=step]);
                assert_eq!(state(&db), Ok((4, 12)));
                assert_eq!(handle(&mut db, "get"), Err(Error::RecoveryRequired));
                drop(db);
                disk.crash(crash).unwrap();

                let mut reopened = open(&disk);
                let published = step > root || (step == root && matches!(crash, Crash::PersistAll));
                assert_eq!(
                    state(&reopened),
                    if published { Ok((5, 30)) } else { Ok((4, 12)) }
                );
                let c = reopened.collection_id(COLLECTION);
                assert_eq!(
                    reopened
                        .snapshot()
                        .get(c, b"receipt/next")
                        .unwrap()
                        .is_some(),
                    published
                );
                assert_eq!(
                    handle(&mut reopened, "set-id prior 2 12"),
                    Ok("committed 3".into())
                );
                assert_eq!(handle(&mut reopened, next), Ok("committed 5".into()));
                drop(reopened);
                disk.crash(Crash::DropAll).unwrap();
                let mut reopened = open(&disk);
                assert_eq!(state(&reopened), Ok((5, 30)));
                assert_eq!(handle(&mut reopened, next), Ok("committed 5".into()));
                assert_eq!(state(&reopened), Ok((5, 30)));
                let c = reopened.collection_id(COLLECTION);
                let retained = reopened.retained_snapshot(retained_id).unwrap();
                assert_eq!(retained.revision(), 3);
                assert_eq!(counter_at(&retained, c), Ok(12));
                let record = retained.get(c, b"receipt/prior").unwrap().unwrap();
                assert_eq!(record.as_bytes().unwrap(), prior_receipt);
                assert!(retained.get(c, b"receipt/next").unwrap().is_none());
            }
        }
    }

    #[test]
    fn malformed_receipts_cannot_acknowledge_or_change_state() {
        for (scenario, size, version, expected, acknowledged, result) in [
            ("valid", 25, 1, 2, 3u64, Ok("committed 3")),
            ("short", 24, 1, 2, 3, Err(Error::Corrupt)),
            ("long", 26, 1, 2, 3, Err(Error::Corrupt)),
            ("version", 25, 2, 2, 3, Err(Error::Corrupt)),
            ("sequence", 25, 1, 1, 3, Err(Error::Corrupt)),
            ("ahead", 25, 1, 9, 10, Err(Error::Corrupt)),
            ("overflow", 25, 1, u64::MAX, 0, Err(Error::Corrupt)),
            ("nonbytes", 25, 1, 2, 3, Err(Error::Corrupt)),
        ] {
            let mut db = database();
            let collection = db.collection_id(COLLECTION);
            let key = b"receipt/test";
            let mut bytes = [0u8; 26];
            bytes[0] = version;
            bytes[1..9].copy_from_slice(&expected.to_le_bytes());
            bytes[17..25].copy_from_slice(&acknowledged.to_le_bytes());
            {
                let mut tx = db.begin();
                let value = if scenario == "nonbytes" {
                    db.unsigned(0).unwrap()
                } else {
                    db.bytes(&bytes[..size]).unwrap()
                };
                tx.put(collection, key, &value).unwrap();
                tx.commit().unwrap();
            }
            assert_eq!(
                handle(&mut db, &format!("set-id test {expected} 0")),
                result.map(str::to_owned),
                "{scenario}"
            );
            assert_eq!(state(&db), Ok((3, 0)), "{scenario}");
            let snapshot = db.snapshot();
            let receipt = snapshot.get(collection, key).unwrap().unwrap();
            if scenario == "nonbytes" {
                assert_eq!(receipt.as_u64(), Ok(0));
            } else {
                assert_eq!(receipt.as_bytes().unwrap(), &bytes[..size]);
            }
        }
    }

    #[test]
    fn receipts_replay_original_ack_and_never_expire_at_capacity() {
        let mut db = database();
        assert_eq!(handle(&mut db, "set-id first 2 10").unwrap(), "committed 3");
        assert_eq!(handle(&mut db, "set 3 20").unwrap(), "committed 4");
        assert_eq!(handle(&mut db, "set-id first 2 10").unwrap(), "committed 3");
        assert_eq!(
            handle(&mut db, "set-id fresh 3 20"),
            Ok("conflict 4 20".into())
        );
        let snapshot = db.snapshot();
        assert!(
            snapshot
                .get(db.collection_id(COLLECTION), b"receipt/fresh")
                .unwrap()
                .is_none()
        );
        drop(snapshot);
        assert_eq!(handle(&mut db, "set-id first 2 11"), Err(Error::Invalid));
        assert_eq!(handle(&mut db, "set-id first 4 10"), Err(Error::Invalid));
        assert_eq!(state(&db), Ok((4, 20)));
        // Fill the remaining receipt slots using the same real request path.
        for n in 1..MAX_RECEIPTS {
            let revision = db.snapshot().revision();
            assert_eq!(
                handle(&mut db, &format!("set-id r{n} {revision} {n}")).unwrap(),
                format!("committed {}", revision + 1)
            );
        }
        let revision = db.snapshot().revision();
        assert_eq!(
            handle(&mut db, &format!("set-id full {revision} 8")),
            Err(Error::Limit)
        );
        assert_eq!(handle(&mut db, "set-id first 2 10").unwrap(), "committed 3");
        assert_eq!(db.snapshot().revision(), revision);
        for bad in [
            "set-id x/y 1 2",
            "set-id a 1 2 extra",
            "set-id a -1 2",
            "set-id a 1 nope",
        ] {
            assert_eq!(handle(&mut db, bad), Err(Error::Invalid));
        }
    }
}

#[cfg(test)]
use weekend_db as allocator_engine;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../tests/support/allocator.rs"]
mod allocator;
#[cfg(test)]
#[path = "../../../../tests/support/logical_checksum.rs"]
mod logical_checksum;
