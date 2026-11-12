use std::io::{self, Read};
use weekend_db::{Database, Error, Result as DbResult};
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
    let snapshot = db.snapshot();
    let value = snapshot
        .get(db.collection_id(COLLECTION), KEY)?
        .ok_or(Error::NotFound)?
        .as_u64()?;
    Ok((snapshot.revision(), value))
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
        storage::{Fault, SimulatedDisk},
    };
    fn database() -> Database {
        let db = Database::new().unwrap();
        let c = db.create_collection().unwrap();
        let mut t = db.begin();
        t.put(c, KEY, &db.unsigned(0).unwrap()).unwrap();
        t.commit().unwrap();
        drop(t);
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
        for (bytes, expected) in [
            (b"\n".to_vec(), true),
            (b"get\n".to_vec(), true),
            ([vec![b'a'; 255], vec![b'\n']].concat(), true),
            ([vec![b'a'; 256], vec![b'\n']].concat(), false),
            (b"get".to_vec(), false),
            (vec![255, b'\n'], false),
        ] {
            assert_eq!(
                read_line_from(&mut bytes.as_slice(), || Ok(())).is_ok(),
                expected
            );
        }
        let mut calls = 0;
        let error = read_line_from(&mut &b"get\n"[..], || {
            calls += 1;
            Err(io::ErrorKind::TimedOut.into())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(calls, 1);
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::ErrorKind::ConnectionReset.into())
            }
        }
        assert_eq!(
            read_line_from(&mut Broken, || Ok(())).unwrap_err().kind(),
            io::ErrorKind::ConnectionReset
        );
    }
    #[test]
    fn request_identifier_and_recovery_admission() {
        for id in ["a", "-", "_", &"x".repeat(32)] {
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
}
