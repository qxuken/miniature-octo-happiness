#[path = "../../apps/write-coordinator/src/native/protocol.rs"]
mod protocol;
use std::{collections::BTreeMap, io::Cursor};
use weekend_db::{Database, Error, Limits, storage::MemoryStore};

pub fn coordinator_protocol(input: &[u8]) {
    let input = &input[..input.len().min(2048)];
    let mut cursor = Cursor::new(input);
    let framed = protocol::read_line_from(&mut cursor, || Ok(()));
    let expected = input
        .iter()
        .position(|&b| b == b'\n')
        .filter(|&n| n < 256)
        .and_then(|n| std::str::from_utf8(&input[..n]).ok());
    assert_eq!(framed.as_deref().ok(), expected);
    let mut cursor = Cursor::new(input);
    assert_eq!(
        protocol::read_line_from(&mut cursor, || Err(std::io::ErrorKind::TimedOut.into()))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    assert_eq!(cursor.position(), 0);
    let secret = [b'a'; 64];
    if let Ok(raw) = std::str::from_utf8(input) {
        let expected = raw.strip_prefix(&format!("auth {} ", "a".repeat(64)));
        assert_eq!(protocol::authorize(&secret, raw), expected);
    }
    let disk = MemoryStore::new().unwrap();
    let mut db = Database::create_on(disk.clone(), [61; 16], Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, protocol::KEY, &db.unsigned(0).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    let (mut revision, mut value) = (2u64, 0u64);
    let mut receipts = BTreeMap::<u8, (u64, u64, u64)>::new();
    for (i, op) in input.chunks(4).take(32).enumerate() {
        let mode = op[0] % 5;
        let id = op.get(1).copied().unwrap_or(0) % 8;
        let val = u64::from(op.get(2).copied().unwrap_or(0));
        let expected = match op.get(3).copied().unwrap_or(0) % 3 {
            0 => revision,
            1 => 0,
            _ => receipts.get(&id).map_or(revision, |x| x.0),
        };
        let command = match mode {
            0 => "get".to_owned(),
            1 => format!("set {expected} {val}"),
            2 => format!("set-id id{id} {expected} {val}"),
            3 => format!("set-id ! {expected} {val}"),
            _ => "set 18446744073709551616 1".to_owned(),
        };
        let authorized = op[0] & 128 == 0;
        let request = format!(
            "auth {} {command}",
            if authorized {
                "a".repeat(64)
            } else {
                "b".repeat(64)
            }
        );
        if let Some(command) = protocol::authorize(&secret, &request) {
            let result = protocol::handle(&mut db, command);
            let want = match mode {
                0 => Ok(format!("value {revision} {value}")),
                1 | 2 => {
                    if mode == 2 && receipts.contains_key(&id) {
                        let &(old_expected, old_val, ack) = receipts.get(&id).unwrap();
                        if (old_expected, old_val) == (expected, val) {
                            Ok(format!("committed {ack}"))
                        } else {
                            Err(Error::Invalid)
                        }
                    } else if expected != revision {
                        Ok(format!("conflict {revision} {value}"))
                    } else {
                        revision += 1;
                        value = val;
                        if mode == 2 {
                            receipts.insert(id, (expected, val, revision));
                        }
                        Ok(format!("committed {revision}"))
                    }
                }
                _ => Err(Error::Invalid),
            };
            assert_eq!(result, want);
        } else {
            assert!(!authorized);
        }
        assert_eq!(protocol::state(&db), Ok((revision, value)));
        if i % 8 == 7 {
            drop(db);
            db = Database::open_on(disk.clone(), Limits::default()).unwrap();
            assert_eq!(protocol::state(&db), Ok((revision, value)));
        }
    }
}
#[test]
fn replay_coordinator() {
    for input in [
        b"get\n".as_slice(),
        b"set-id id0 2 7\n",
        b"\xff\n",
        &[b'a'; 257],
        &[2, 0, 7, 0, 2, 0, 7, 2, 2, 0, 8, 2, 130, 0, 0, 0],
    ] {
        coordinator_protocol(input);
    }
}
